use rs_wwidget::{
    Result,
    config::{self, atomic_write},
};
use std::{
    fs,
    os::fd::AsRawFd,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn status_path() -> PathBuf {
    config::state_dir().join("status.json")
}
fn status() -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(&fs::read(status_path())?)?)
}
fn without_deleted(path: &std::path::Path) -> PathBuf {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    let bytes = path.as_os_str().as_bytes();
    PathBuf::from(std::ffi::OsString::from_vec(
        bytes.strip_suffix(b" (deleted)").unwrap_or(bytes).to_vec(),
    ))
}
pub fn running_pid() -> Option<i32> {
    let pid = i32::try_from(status().ok()?.get("pid")?.as_i64()?)
        .ok()
        .filter(|p| *p > 1)?;
    let exe = without_deleted(&fs::read_link(format!("/proc/{pid}/exe")).ok()?);
    let current = without_deleted(&std::env::current_exe().ok()?);
    let installed = config::home().join(".local/bin/rs-wwidget");
    if exe == current || exe == installed {
        Some(pid)
    } else {
        None
    }
}
pub fn stop() -> Result<()> {
    let Some(pid) = running_pid() else {
        println!("Widgets are not running in this session.");
        return Ok(());
    };
    if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if running_pid() != Some(pid) {
            println!("Widgets stopped.");
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err("Could not confirm shutdown; files were kept.".into())
}
pub fn start(config_path: Option<&std::path::Path>) -> Result<()> {
    if running_pid().is_some() {
        println!("Widgets are already running.");
        return Ok(());
    }
    let state = config::state_dir();
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&state)?;
    let log = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(state.join("session.log"))?;
    let mut cmd = Command::new(std::env::current_exe()?);
    cmd.arg("--run");
    if let Some(path) = config_path {
        cmd.arg("--config").arg(fs::canonicalize(path)?);
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log));
    // Create a detached session without leaving a resident controller process.
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let mut child = cmd.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if let Some(exit) = child.try_wait()? {
            if !exit.success() {
                return Err(fs::read_to_string(state.join("session.log"))?.into());
            }
            if running_pid().is_some() {
                println!("Widgets are already running.");
                return Ok(());
            }
            return Err("Process exited before creating widgets; see session.log".into());
        }
        if running_pid() == Some(child.id() as i32) {
            println!("Widgets started.");
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!(
        "Could not confirm startup. See {}",
        state.join("session.log").display()
    )
    .into())
}
pub fn print_status() -> Result<()> {
    let Ok(info) = status() else {
        println!("Widgets are not running; no cached measurements are available.");
        return Ok(());
    };
    let rss = info["rss_kib"].as_f64().unwrap_or(0.0) / 1024.0;
    let pss = info["pss_kib"].as_f64().unwrap_or(0.0) / 1024.0;
    if running_pid().is_some() {
        println!(
            "RSS: {rss:.1} MiB; PSS: {pss:.1} MiB; threads: {}; monitor: {}. Updated once per minute.",
            info["threads"],
            info["monitor"].as_str().unwrap_or("?")
        );
    } else {
        let now = unsafe { libc::time(std::ptr::null_mut()) };
        let age = (now - info["updated"].as_i64().unwrap_or(now)).max(0);
        println!(
            "Process is not available for inspection. Cached measurement: RSS {rss:.1} MiB; PSS {pss:.1} MiB; {age} seconds ago."
        );
    }
    Ok(())
}
pub fn remove() -> Result<()> {
    stop()?;
    let state = config::state_dir();
    let lock = if state.join("lock").exists() {
        let f = fs::OpenOptions::new()
            .write(true)
            .open(state.join("lock"))?;
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err("Widgets are still running; removal cancelled.".into());
        }
        Some(f)
    } else {
        None
    };
    let share = config::home().join(".local/share/rs-wwidget");
    let mango = config::xdg("XDG_CONFIG_HOME", ".config").join("mango/config.conf");
    if mango.exists() {
        let text = fs::read_to_string(&mango)?;
        let source = format!("source = {}", share.join("mango.conf").display());
        let filtered: String = text
            .split_inclusive('\n')
            .filter(|line| line.trim() != source)
            .collect();
        if filtered != text {
            atomic_write(&mango, filtered.as_bytes())?;
        }
    }
    for p in [
        config::home().join(".local/bin/rs-wwidget"),
        config::home().join(".local/share/applications/rs-wwidget.desktop"),
    ] {
        if p.exists() {
            fs::remove_file(p)?;
        }
    }
    if share.exists() {
        fs::remove_dir_all(share)?;
    }
    if state.exists() {
        fs::remove_dir_all(state)?;
    }
    // Keep user-authored config.toml for a later reinstall.
    drop(lock);
    println!("rs-wwidget and its autostart files were removed; user configuration was kept.");
    Ok(())
}
