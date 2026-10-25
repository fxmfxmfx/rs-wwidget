//! Single-threaded host runtime for newline-delimited plugin frames.
use crate::config::PluginConfig;
use serde::Deserialize;
use std::{
    io,
    os::fd::{AsRawFd, RawFd},
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const MAX_FRAME: usize = 65_536;
const READ_BUDGET: usize = 65_536;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    pub version: u32,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub rows: Vec<Row>,
    #[serde(default)]
    pub elements: Vec<crate::canvas::Element>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Row {
    pub label: String,
    pub value: String,
    pub fraction: Option<f64>,
    pub history: Vec<f64>,
}

impl Frame {
    pub fn parse(data: &[u8]) -> crate::Result<Self> {
        if data.len() > MAX_FRAME {
            return Err("plugin frame exceeds 65536 bytes".into());
        }
        let value: serde_json::Value = serde_json::from_slice(data)?;
        let v1_fields = value.get("title").is_some()
            && value.get("rows").is_some()
            && value.get("elements").is_none();
        let frame: Self = serde_json::from_value(value)?;
        if ![1, 2].contains(&frame.version) {
            return Err("plugin frame version must be 1 or 2".into());
        }
        if frame.version == 1 && !v1_fields {
            return Err("v1 requires title and rows; elements require v2".into());
        }
        if frame.elements.len() > 256 {
            return Err("at most 256 elements per frame".into());
        }
        for element in &frame.elements {
            element.validate()?;
        }
        if frame.title.chars().count() > 128 {
            return Err("plugin title exceeds 128 characters".into());
        }
        if frame.rows.len() > 12 {
            return Err("plugin frame has more than 12 rows".into());
        }
        for row in &frame.rows {
            if row.label.chars().count() > 512 || row.value.chars().count() > 512 {
                return Err("plugin row text exceeds 512 characters".into());
            }
            if row
                .fraction
                .is_some_and(|x| !x.is_finite() || !(0.0..=1.0).contains(&x))
            {
                return Err("plugin fraction must be finite and between 0 and 1".into());
            }
            if row.history.len() > 120
                || row
                    .history
                    .iter()
                    .any(|x| !x.is_finite() || !(0.0..=1.0).contains(x))
            {
                return Err(
                    "plugin history must contain at most 120 values between 0 and 1".into(),
                );
            }
        }
        Ok(frame)
    }

    pub fn load(path: &std::path::Path) -> crate::Result<Self> {
        use std::io::Read;
        let mut data = Vec::new();
        std::fs::File::open(path)?
            .take((MAX_FRAME + 1) as u64)
            .read_to_end(&mut data)?;
        Self::parse(&data)
    }

    pub fn placeholder(title: &str, value: &str) -> Self {
        Self {
            version: 1,
            elements: Vec::new(),
            title: title.chars().take(128).collect(),
            rows: vec![Row {
                label: String::new(),
                value: value.chars().take(512).collect(),
                ..Row::default()
            }],
        }
    }
}

struct Running {
    child: Child,
    stdout: std::process::ChildStdout,
    stopping: bool,
}

pub struct Runtime {
    config: PluginConfig,
    frame: Frame,
    running: Option<Running>,
    line: Vec<u8>,
    last_frame: Option<Instant>,
    deadline: Instant,
    failed: bool,
    restart_at: Option<Instant>,
}

impl Runtime {
    pub fn new(config: PluginConfig) -> Self {
        let now = Instant::now();
        let title = config.id.clone();
        let frame = config
            .static_data
            .clone()
            .unwrap_or_else(|| Frame::placeholder(&title, "Starting…"));
        let deadline = if config.frame.is_some() {
            now + Duration::from_secs(86400)
        } else {
            now
        };
        Self {
            config,
            frame,
            running: None,
            line: Vec::new(),
            last_frame: None,
            deadline,
            failed: false,
            restart_at: None,
        }
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    /// Starts due children and enforces liveness. Returns whether display data changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.config.frame.is_some() {
            self.deadline = now + Duration::from_secs(86400);
            return false;
        }
        if !self.config.enabled {
            return false;
        }
        let mut dirty = self.service(now);
        if self.running.as_ref().is_some_and(|r| r.stopping) {
            // Reap killed children promptly, but retain the original restart time.
            self.deadline = now + Duration::from_millis(25);
        } else if self.running.is_some() {
            if self.last_frame.is_some_and(|t| {
                now.saturating_duration_since(t)
                    >= Duration::from_secs(self.config.timeout_seconds.max(1))
            }) {
                dirty |= self.fail("frame timeout", now);
            }
        } else if now >= self.restart_at.unwrap_or(self.deadline) {
            self.restart_at = None;
            if let Err(e) = self.start(now) {
                dirty |= self.fail(&format!("start failed: {e}"), now);
            }
        }
        dirty
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn poll_fd(&self) -> Option<RawFd> {
        self.running
            .as_ref()
            .filter(|r| !r.stopping)
            .map(|r| r.stdout.as_raw_fd())
    }

    /// Drains currently available bytes. Call from the event loop after fd readiness.
    pub fn service(&mut self, now: Instant) -> bool {
        let mut dirty = false;
        let mut budget = READ_BUDGET;
        let mut buf = [0u8; 8192];
        while budget > 0 {
            let read_result = match self.running.as_ref().filter(|r| !r.stopping) {
                Some(r) => unsafe {
                    libc::read(
                        r.stdout.as_raw_fd(),
                        buf.as_mut_ptr().cast(),
                        buf.len().min(budget),
                    )
                },
                None => break,
            };
            if read_result > 0 {
                let n = read_result as usize;
                budget -= n;
                for &byte in &buf[..n] {
                    if byte == b'\n' {
                        if self.line.last() == Some(&b'\r') {
                            self.line.pop();
                        }
                        match Frame::parse(&self.line) {
                            Ok(frame) => {
                                self.last_frame = Some(now);
                                self.deadline =
                                    now + Duration::from_secs(self.config.timeout_seconds.max(1));
                                self.failed = false;
                                if self.frame != frame {
                                    self.frame = frame;
                                    dirty = true;
                                }
                            }
                            Err(e) => {
                                dirty |= self.fail(&format!("invalid frame: {e}"), now);
                                return dirty;
                            }
                        }
                        self.line.clear();
                    } else {
                        if self.line.len() >= MAX_FRAME {
                            dirty |= self.fail("frame exceeds 65536 bytes", now);
                            return dirty;
                        }
                        self.line.push(byte);
                    }
                }
            } else if read_result == 0 {
                dirty |= self.fail("stdout closed", now);
                return dirty;
            } else {
                let e = io::Error::last_os_error();
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::Interrupted {
                    break;
                }
                dirty |= self.fail(&format!("stdout read failed: {e}"), now);
                return dirty;
            }
        }
        if let Some(r) = self.running.as_mut() {
            match r.child.try_wait() {
                Ok(Some(status)) => {
                    // A process may exit while descendants in its process group keep
                    // the pipe open. Terminate the group before dropping the leader.
                    if let Some(r) = self.running.as_ref() {
                        unsafe {
                            libc::kill(-(r.child.id() as i32), libc::SIGKILL);
                        }
                    }
                    self.running = None;
                    self.line.clear();
                    if !self.failed {
                        dirty |= self.fail(&format!("process exited ({status})"), now);
                    } else {
                        self.deadline = self.restart_at.unwrap_or(now);
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    dirty |= self.fail(&format!("process status failed: {e}"), now);
                }
            }
        }
        dirty
    }

    fn start(&mut self, now: Instant) -> io::Result<()> {
        if self.config.command.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty command"));
        }
        let mut command = Command::new(&self.config.command[0]);
        command
            .args(&self.config.command[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env("RS_WWIDGET_PROTOCOL", self.config.protocol.to_string())
            .env("RS_WWIDGET_WIDTH", self.config.width.to_string())
            .env("RS_WWIDGET_HEIGHT", self.config.height.to_string())
            .env("RS_WWIDGET_ASSET_DIR", &self.config.asset_dir);
        command.process_group(0);
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().expect("piped stdout");
        let flags = unsafe { libc::fcntl(stdout.as_raw_fd(), libc::F_GETFL) };
        if flags < 0
            || unsafe { libc::fcntl(stdout.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) }
                < 0
        {
            let e = io::Error::last_os_error();
            self.running = Some(Running {
                child,
                stdout,
                stopping: false,
            });
            return Err(e);
        }
        self.running = Some(Running {
            child,
            stdout,
            stopping: false,
        });
        self.line.clear();
        self.last_frame = Some(now);
        self.failed = false;
        self.deadline = now + Duration::from_secs(self.config.timeout_seconds.max(1));
        Ok(())
    }

    fn fail(&mut self, reason: &str, now: Instant) -> bool {
        if !self.failed {
            eprintln!("plugin {}: {reason}", self.config.id);
            self.failed = true;
        }
        if let Some(r) = self.running.as_mut()
            && !r.stopping
        {
            unsafe {
                libc::kill(-(r.child.id() as i32), libc::SIGKILL);
            }
            r.stopping = true;
        }
        self.line.clear();
        self.last_frame = None;
        let replacement = Frame::placeholder(&self.config.id, "Unavailable");
        let dirty = self.frame != replacement;
        self.frame = replacement;
        let restart_at = now + Duration::from_secs(self.config.restart_seconds.max(1));
        self.restart_at = Some(restart_at);
        self.deadline = if self.running.is_some() {
            now + Duration::from_millis(25)
        } else {
            restart_at
        };
        dirty
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(mut r) = self.running.take() {
            unsafe {
                libc::kill(-(r.child.id() as i32), libc::SIGKILL);
            }
            let _ = r.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(command: &[&str]) -> PluginConfig {
        PluginConfig {
            id: "test-plugin".into(),
            command: command.iter().map(|s| (*s).into()).collect(),
            enabled: true,
            height: 100,
            restart_seconds: 1,
            timeout_seconds: 2,
            ..PluginConfig::default()
        }
    }

    #[test]
    fn parses_valid_frame_and_enforces_limits() {
        let bytes = br#"{"version":1,"title":"T","rows":[{"label":"CPU","value":"42%","fraction":0.42,"history":[0,1]}]}"#;
        let frame = Frame::parse(bytes).unwrap();
        assert_eq!(frame.rows[0].label, "CPU");
        assert!(Frame::parse(br#"{"version":99,"title":"T","rows":[]}"#).is_err());
        assert!(Frame::parse(br#"{"version":1,"title":"T","extra":0,"rows":[]}"#).is_err());
        assert!(
            Frame::parse(
                br#"{"version":1,"title":"T","rows":[{"label":"x","value":"y","fraction":1.1}]}"#
            )
            .is_err()
        );
        assert!(
            Frame::parse(
                format!(r#"{{"version":1,"title":"{}","rows":[]}}"#, "x".repeat(129)).as_bytes()
            )
            .is_err()
        );
        assert!(Frame::parse(format!(r#"{{"version":1,"title":"T","rows":[{{"label":"x","value":"y","history":{}}}]}}"#, serde_json::to_string(&vec![0.5; 121]).unwrap()).as_bytes()).is_err());
        assert!(Frame::parse(&vec![b' '; MAX_FRAME + 1]).is_err());
    }

    #[test]
    fn accepts_fragmented_frame_and_unchanged_heartbeat() {
        let script = r#"printf '{"version":1,'; sleep 0.05; printf '"title":"ok","rows":[]}\n'; sleep 0.05; printf '{"version":1,"title":"ok","rows":[]}\n'; sleep 5"#;
        let mut rt = Runtime::new(config(&["/bin/sh", "-c", script]));
        let now = Instant::now();
        assert!(!rt.tick(now));
        let mut changed = false;
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(10));
            changed |= rt.service(Instant::now());
            if rt.frame.title == "ok" {
                break;
            }
        }
        assert!(changed);
        assert_eq!(rt.frame.title, "ok");
        let first_heartbeat = rt.last_frame;
        let mut heartbeat_dirty = false;
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(10));
            heartbeat_dirty |= rt.service(Instant::now());
            if rt.last_frame != first_heartbeat {
                break;
            }
        }
        assert!(!heartbeat_dirty);
        assert_ne!(
            rt.last_frame, first_heartbeat,
            "second heartbeat was not received"
        );
    }

    #[test]
    fn disabled_runtime_does_not_spawn_and_timeout_schedules_restart() {
        let mut disabled = Runtime::new(PluginConfig {
            enabled: false,
            command: vec!["/bin/false".into()],
            ..PluginConfig::default()
        });
        assert!(!disabled.tick(Instant::now() + Duration::from_secs(10)));
        assert!(disabled.poll_fd().is_none());

        let mut rt = Runtime::new(config(&["/bin/sleep", "10"]));
        let start = Instant::now();
        rt.tick(start);
        assert!(rt.poll_fd().is_some());
        assert!(rt.tick(start + Duration::from_secs(3)));
        assert!(rt.running.as_ref().is_some_and(|r| r.stopping));
        assert!(rt.deadline() > start + Duration::from_secs(3));
    }

    #[test]
    fn leader_exit_kills_descendants_holding_stdout_open() {
        let pid_file =
            std::env::temp_dir().join(format!("rs-wwidget-plugin-child-{}", std::process::id()));
        let script = format!("sleep 10 & echo $! > {}; exit 0", pid_file.display());
        let mut rt = Runtime::new(config(&["/bin/sh", "-c", &script]));
        rt.tick(Instant::now());
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(10));
            rt.service(Instant::now());
            if rt.running.is_none() {
                break;
            }
        }
        assert!(rt.running.is_none());
        assert!(rt.frame.rows[0].value == "Unavailable");
        let child_pid: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(
            unsafe { libc::kill(child_pid, 0) },
            -1,
            "descendant should be gone"
        );
        let _ = std::fs::remove_file(pid_file);
    }

    #[test]
    fn drop_kills_descendants() {
        let pid_file =
            std::env::temp_dir().join(format!("rs-wwidget-plugin-drop-{}", std::process::id()));
        let script = format!("sleep 10 & echo $! > {}; wait", pid_file.display());
        let mut rt = Runtime::new(config(&["/bin/sh", "-c", &script]));
        rt.tick(Instant::now());
        for _ in 0..20 {
            if pid_file.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let child_pid: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        drop(rt);
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(
            unsafe { libc::kill(child_pid, 0) },
            -1,
            "descendant should be gone after drop"
        );
        let _ = std::fs::remove_file(pid_file);
    }

    #[test]
    fn invalid_and_oversized_output_fail_runtime() {
        for script in [
            "printf 'not json\\n'; sleep 10",
            "head -c 70000 /dev/zero; sleep 10",
        ] {
            let mut rt = Runtime::new(config(&["/bin/sh", "-c", script]));
            rt.tick(Instant::now());
            let mut dirty = false;
            for _ in 0..20 {
                dirty |= rt.service(Instant::now());
                if rt.running.as_ref().is_some_and(|r| r.stopping) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(dirty);
            assert_eq!(rt.frame.rows[0].value, "Unavailable");
            assert!(rt.running.as_ref().is_some_and(|r| r.stopping));
        }
    }
}
