//! Instance identity is tied to the live flock, independently of metrics/status files.
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Seek, SeekFrom, Write},
    os::{fd::AsRawFd, unix::fs::MetadataExt},
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
struct Owner {
    version: u32,
    pid: i32,
    start_ticks: u64,
    exe: PathBuf,
}

pub fn start_ticks(pid: i32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(19)?
        .parse()
        .ok()
}

pub fn without_deleted(path: &Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    PathBuf::from(std::ffi::OsString::from_vec(
        bytes.strip_suffix(b" (deleted)").unwrap_or(bytes).to_vec(),
    ))
}

pub fn record(lock: &mut File) -> io::Result<()> {
    let pid = std::process::id() as i32;
    let owner = Owner {
        version: 1,
        pid,
        start_ticks: start_ticks(pid)
            .ok_or_else(|| io::Error::other("Cannot read process start time"))?,
        exe: std::env::current_exe()?,
    };
    let bytes = serde_json::to_vec(&owner)?;
    lock.seek(SeekFrom::Start(0))?;
    lock.set_len(0)?;
    lock.write_all(&bytes)
}

/// A similarly named process is insufficient: it must hold this exact file's flock.
pub fn owns_lock(pid: i32, path: &Path) -> bool {
    let Ok(expected) = fs::metadata(path) else {
        return false;
    };
    let Ok(fds) = fs::read_dir(format!("/proc/{pid}/fd")) else {
        return false;
    };
    fds.flatten().any(|entry| {
        let Ok(actual) = fs::metadata(entry.path()) else {
            return false;
        };
        actual.dev() == expected.dev()
            && actual.ino() == expected.ino()
            && fs::read_to_string(format!(
                "/proc/{pid}/fdinfo/{}",
                entry.file_name().to_string_lossy()
            ))
            .is_ok_and(|text| {
                text.lines().any(|line| {
                    line.starts_with("lock:") && line.contains("FLOCK") && line.contains("WRITE")
                })
            })
    })
}

pub fn owner_pid(path: &Path) -> Option<i32> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() > 8192 {
        return None;
    }
    let owner: Owner = serde_json::from_slice(&bytes).ok()?;
    if owner.version != 1 || owner.pid <= 1 || start_ticks(owner.pid)? != owner.start_ticks {
        return None;
    }
    let actual = fs::read_link(format!("/proc/{}/exe", owner.pid)).ok()?;
    (without_deleted(&actual) == without_deleted(&owner.exe) && owns_lock(owner.pid, path))
        .then_some(owner.pid)
}

pub fn locked(path: &Path) -> io::Result<bool> {
    let file = match fs::OpenOptions::new().read(true).write(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(false);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock {
        Ok(true)
    } else {
        Err(error)
    }
}
