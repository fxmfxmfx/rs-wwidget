use rs_wwidget::{
    Result,
    config::{self, atomic_write},
    instance::{self, without_deleted},
};
use std::{
    fs,
    os::fd::{AsRawFd, FromRawFd},
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
pub fn running_pid() -> Option<i32> {
    let lock_path = config::state_dir().join("lock");
    if let Some(pid) = instance::owner_pid(&lock_path) {
        return Some(pid);
    }
    let pid = i32::try_from(status().ok()?.get("pid")?.as_i64()?)
        .ok()
        .filter(|p| *p > 1)?;
    let exe = without_deleted(&fs::read_link(format!("/proc/{pid}/exe")).ok()?);
    let current = without_deleted(&std::env::current_exe().ok()?);
    let installed = config::home().join(".local/bin/rs-wwidget");
    if (exe == current
        || exe == installed
        || exe.file_name().is_some_and(|name| name == "rs-wwidget"))
        && instance::owns_lock(pid, &lock_path)
    {
        Some(pid)
    } else {
        None
    }
}
pub fn stop() -> Result<()> {
    let Some(pid) = running_pid() else {
        if instance::locked(&config::state_dir().join("lock"))? {
            return Err("An rs-wwidget instance holds the lock, but its owner is not visible in this process session. Run restart from your desktop terminal.".into());
        }
        println!("Widgets are not running in this session.");
        return Ok(());
    };
    // Pin the process before revalidating ownership, so PID reuse cannot redirect SIGTERM.
    let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
    if descriptor < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let process = unsafe { fs::File::from_raw_fd(descriptor as i32) };
    if running_pid() != Some(pid) {
        return Err("Instance ownership changed before shutdown; retry the command.".into());
    }
    if unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            process.as_raw_fd(),
            libc::SIGTERM,
            std::ptr::null::<libc::siginfo_t>(),
            0,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if !instance::locked(&config::state_dir().join("lock"))? && running_pid() != Some(pid) {
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
    if instance::locked(&config::state_dir().join("lock"))? {
        return Err("An rs-wwidget instance already holds the lock, but its owner cannot be inspected. Run restart from your desktop terminal; session.log was kept.".into());
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
            if instance::locked(&state.join("lock"))? {
                return Err("Another rs-wwidget instance holds the lock; it may have been started from a different checkout or process session.".into());
            }
            return Err("Process exited before creating widgets; see session.log".into());
        }
        if running_pid() == Some(child.id() as i32)
            && status().ok().and_then(|info| info["pid"].as_i64()) == Some(child.id() as i64)
        {
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
