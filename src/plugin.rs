//! Single-threaded host runtime for newline-delimited plugin frames.
use crate::config::PluginConfig;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io,
    os::fd::{AsRawFd, RawFd},
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const MAX_FRAME: usize = 65_536;
const READ_BUDGET: usize = 65_536;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
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

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Row {
    pub label: String,
    pub value: String,
    pub fraction: Option<f64>,
    pub history: Vec<f64>,
}

// Presence is distinct from null: v1 requires title/rows, and optional v2 fields
// still reject null. Deserialize directly so duplicate fields cannot be hidden.
#[derive(Default)]
struct Field<T>(Option<T>);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Field<T> {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        Ok(Self(Some(T::deserialize(deserializer)?)))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireFrame {
    version: u32,
    #[serde(default)]
    title: Field<String>,
    #[serde(default)]
    rows: Field<Vec<Row>>,
    #[serde(default)]
    elements: Field<Vec<crate::canvas::Element>>,
}

impl Frame {
    pub fn parse(data: &[u8]) -> crate::Result<Self> {
        if data.len() > MAX_FRAME {
            return Err("plugin frame exceeds 65536 bytes".into());
        }
        let raw: WireFrame = serde_json::from_slice(data)?;
        if ![1, 2].contains(&raw.version) {
            return Err("plugin frame version must be 1 or 2".into());
        }
        if raw.version == 1
            && (raw.title.0.is_none() || raw.rows.0.is_none() || raw.elements.0.is_some())
        {
            return Err("v1 requires title and rows; elements require v2".into());
        }
        let frame = Self {
            version: raw.version,
            title: raw.title.0.unwrap_or_default(),
            rows: raw.rows.0.unwrap_or_default(),
            elements: raw.elements.0.unwrap_or_default(),
        };
        if frame.elements.len() > 256 {
            return Err("at most 256 elements per frame".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for (index, element) in frame.elements.iter().enumerate() {
            if let Some(id) = &element.id
                && !ids.insert(id)
            {
                return Err(format!("duplicate element id: {id}").into());
            }
            element
                .validate()
                .map_err(|e| format!("element {index}: {e}"))?;
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

    pub fn target(&self, point: (f64, f64)) -> Option<(String, [f64; 2])> {
        self.elements.iter().rev().find_map(|element| {
            let id = element.id.as_ref()?;
            element.hit(point).map(|local| (id.clone(), local))
        })
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
    stdout: Option<std::process::ChildStdout>,
    stdin: Option<std::process::ChildStdin>,
    stderr: Option<std::process::ChildStderr>,
    stopping: bool,
}

struct PendingEvent {
    data: Vec<u8>,
    offset: usize,
    motion: bool,
}
const MAX_EVENT_BYTES: usize = 262_144;
const MAX_EVENTS: usize = 128;

pub struct Runtime {
    config: PluginConfig,
    frame: Frame,
    running: Option<Running>,
    line: Vec<u8>,
    last_frame: Option<Instant>,
    deadline: Instant,
    failed: bool,
    restart_at: Option<Instant>,
    events: VecDeque<PendingEvent>,
    event_bytes: usize,
    generation: u64,
    config_generation: u64,
    pending_config: Option<PluginConfig>,
    watch: crate::watch::WatchSet,
    last_error: Option<String>,
    stderr_tail: VecDeque<u8>,
    log_wait: Option<Instant>,
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
        let watch = crate::watch::WatchSet::new(&config, now);
        Self {
            config,
            frame,
            running: None,
            line: Vec::new(),
            last_frame: None,
            deadline,
            failed: false,
            restart_at: None,
            events: VecDeque::new(),
            event_bytes: 0,
            generation: 0,
            config_generation: 0,
            pending_config: None,
            watch,
            last_error: None,
            stderr_tail: VecDeque::new(),
            log_wait: None,
        }
    }

    pub fn frame(&self) -> &Frame {
        &self.frame
    }

    /// Starts due children and enforces liveness. Returns whether display data changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        if !self.config.enabled {
            return false;
        }
        if self.pending_config.is_none()
            && self.watch.changed(now)
            && let Err(error) = self.reload(now)
        {
            self.last_error = Some(
                format!("reload rejected: {error}")
                    .chars()
                    .take(1024)
                    .collect(),
            );
            eprintln!(
                "plugin {}: reload rejected, keeping previous configuration: {error}",
                self.config.id
            );
        }
        let mut dirty = self.service(now);
        if self.running.is_none()
            && let Some(config) = self.pending_config.take()
        {
            if let Some(frame) = &config.static_data {
                self.frame = frame.clone();
            }
            self.config = config;
            self.watch = crate::watch::WatchSet::new(&self.config, now);
            self.config_generation += 1;
            self.failed = false;
            self.restart_at = None;
            self.last_frame = None;
            self.deadline = now;
            dirty = true;
        }
        if self.config.frame.is_some() {
            self.deadline = now + Duration::from_secs(86400);
            return dirty;
        }
        if self.running.as_ref().is_some_and(|r| r.stopping) {
            self.deadline = now + Duration::from_millis(25);
        } else if self.running.is_some() {
            if self.config.render_mode == crate::config::RenderMode::External {
                self.deadline = now + Duration::from_secs(1);
            } else if self.last_frame.is_some_and(|t| {
                now.saturating_duration_since(t)
                    >= Duration::from_secs(self.config.timeout_seconds.max(1))
            }) {
                dirty |= self.fail("frame timeout", now);
            }
        } else if now >= self.restart_at.unwrap_or(self.deadline) {
            self.restart_at = None;
            if let Err(error) = self.start(now) {
                dirty |= self.fail(&format!("start failed: {error}"), now);
            }
        }
        dirty
    }

    pub fn is_running(&self) -> bool {
        self.running.as_ref().is_some_and(|r| !r.stopping)
    }
    pub fn diagnostics(&self) -> serde_json::Value {
        let bytes: Vec<u8> = self.stderr_tail.iter().copied().collect();
        serde_json::json!({"render_mode":if self.config.render_mode==crate::config::RenderMode::External {"external"}else{"canvas"},
            "stderr_tail":String::from_utf8_lossy(&bytes),"running":self.is_running(),"pid":self.running.as_ref().map(|r|r.child.id()),
            "generation":self.generation,"config_generation":self.config_generation,"reloading":self.pending_config.is_some(),"last_error":self.last_error})
    }
    pub fn config_generation(&self) -> u64 {
        self.config_generation
    }
    pub fn config(&self) -> &PluginConfig {
        &self.config
    }

    /// Validate first; a bad edit never stops the currently working provider.
    /// Update this package's manifest, validate it, and queue a live reload.
    pub fn configure(&mut self, options: &serde_json::Value, now: Instant) -> crate::Result<()> {
        let path = self
            .config
            .package_path
            .as_ref()
            .ok_or("configure requires a package manifest; edit legacy config directly")?
            .clone();
        let options = options
            .as_object()
            .ok_or("configure expects a JSON object")?;
        if options
            .keys()
            .any(|key| ["id", "version", "enabled"].contains(&key.as_str()))
        {
            return Err("configure cannot change id/version/enabled".into());
        }
        let original = crate::config::read_text(&path, 65_536)?;
        let mut fields: toml::Table = toml::from_str(&original)?;
        for (key, value) in options {
            if value.is_null() {
                fields.remove(key);
            } else {
                fields.insert(key.clone(), toml::Value::try_from(value)?);
            }
        }
        let mut definition = fields.clone();
        definition.remove("version");
        let mut candidate: PluginConfig = toml::Value::Table(definition).try_into()?;
        candidate.validate()?;
        candidate.asset_dir = self.config.asset_dir.clone();
        candidate.load_static()?;
        let updated = toml::to_string_pretty(&fields)?;
        crate::config::atomic_write(&path, updated.as_bytes())?;
        if let Err(error) = self.reload(now) {
            if let Err(rollback) = crate::config::atomic_write(&path, original.as_bytes()) {
                return Err(format!(
                    "reload failed: {error}; manifest rollback failed: {rollback}"
                )
                .into());
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn reload(&mut self, now: Instant) -> crate::Result<()> {
        let mut config = if self.config.package_path.is_some() {
            crate::registry::resolve_directory(&self.config.asset_dir)?
        } else {
            let mut c = self.config.clone();
            c.load_static()?;
            c
        };
        config.enabled = self.config.enabled;
        config.host_theme = self.config.host_theme;
        config.validate()?;
        if config.id != self.config.id {
            return Err("reload cannot change a widget ID".into());
        }
        self.pending_config = Some(config);
        if let Some(running) = self.running.as_mut()
            && !running.stopping
        {
            unsafe {
                libc::kill(-(running.child.id() as i32), libc::SIGKILL);
            }
            running.stopping = true;
        }
        self.events.clear();
        self.event_bytes = 0;
        self.line.clear();
        self.failed = true;
        self.restart_at = Some(now);
        self.deadline = now;
        Ok(())
    }

    pub fn deadline(&self) -> Instant {
        let deadline = self
            .watch
            .deadline()
            .map_or(self.deadline, |watch| watch.min(self.deadline));
        self.log_wait.map_or(deadline, |log| deadline.min(log))
    }

    pub fn poll_stderr_fd(&self) -> Option<RawFd> {
        if self.log_wait.is_some_and(|t| Instant::now() < t) {
            return None;
        }
        self.running
            .as_ref()
            .and_then(|r| r.stderr.as_ref())
            .map(AsRawFd::as_raw_fd)
    }

    fn drain_stderr(&mut self, now: Instant) {
        if self.log_wait.is_some_and(|t| now < t) {
            return;
        }
        self.log_wait = None;
        let Some(fd) = self
            .running
            .as_ref()
            .and_then(|r| r.stderr.as_ref())
            .map(AsRawFd::as_raw_fd)
        else {
            return;
        };
        let mut budget = 16_384;
        let mut bytes = [0u8; 4096];
        while budget > 0 {
            let n = unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len().min(budget)) };
            if n > 0 {
                budget -= n as usize;
                self.stderr_tail.extend(&bytes[..n as usize]);
                while self.stderr_tail.len() > 8192 {
                    self.stderr_tail.pop_front();
                }
            } else if n == 0 {
                if let Some(r) = &mut self.running {
                    r.stderr = None;
                }
                break;
            } else {
                let e = io::Error::last_os_error();
                if e.kind() != io::ErrorKind::WouldBlock
                    && e.kind() != io::ErrorKind::Interrupted
                    && let Some(r) = &mut self.running
                {
                    r.stderr = None;
                }
                break;
            }
        }
        if budget == 0 {
            self.log_wait = Some(now + Duration::from_millis(50));
        }
    }

    pub fn poll_fd(&self) -> Option<RawFd> {
        self.running
            .as_ref()
            .filter(|r| !r.stopping)
            .and_then(|r| r.stdout.as_ref())
            .map(AsRawFd::as_raw_fd)
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn poll_write_fd(&self) -> Option<RawFd> {
        if self.events.is_empty() {
            return None;
        }
        self.running
            .as_ref()
            .filter(|r| !r.stopping)?
            .stdin
            .as_ref()
            .map(AsRawFd::as_raw_fd)
    }

    /// Bounded, nonblocking stdin transport. Motion may coalesce; discrete events
    /// never disappear silently. An unresponsive event consumer is restarted.
    pub fn send_event(&mut self, event: &crate::input::Event, now: Instant) -> bool {
        if (event.is_keyboard() && !self.config.keyboard)
            || self.config.input != crate::config::Input::Interactive
            || self
                .running
                .as_ref()
                .is_none_or(|r| r.stopping || r.stdin.is_none())
        {
            return false;
        }
        let data = event.encode();
        if event.is_motion()
            && self
                .events
                .back()
                .is_some_and(|e| e.motion && e.offset == 0)
        {
            self.event_bytes -= self.events.pop_back().unwrap().data.len();
        }
        if data.len() > MAX_EVENT_BYTES
            || self.event_bytes + data.len() > MAX_EVENT_BYTES
            || self.events.len() >= MAX_EVENTS
        {
            return self.fail("input queue overflow", now);
        }
        self.event_bytes += data.len();
        self.events.push_back(PendingEvent {
            data,
            offset: 0,
            motion: event.is_motion(),
        });
        self.flush_events(now)
    }

    fn flush_events(&mut self, now: Instant) -> bool {
        let Some(fd) = self
            .running
            .as_ref()
            .filter(|r| !r.stopping)
            .and_then(|r| r.stdin.as_ref())
            .map(AsRawFd::as_raw_fd)
        else {
            return false;
        };
        let mut budget = 16_384;
        while budget > 0 {
            let Some(event) = self.events.front_mut() else {
                break;
            };
            let bytes = &event.data[event.offset..];
            let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len().min(budget)) };
            if n > 0 {
                event.offset += n as usize;
                self.event_bytes -= n as usize;
                budget -= n as usize;
                if event.offset == event.data.len() {
                    self.events.pop_front();
                }
            } else if n < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::Interrupted
                {
                    break;
                }
                return self.fail(&format!("input write failed: {error}"), now);
            } else {
                break;
            }
        }
        false
    }

    /// Drains currently available bytes. Call from the event loop after fd readiness.
    pub fn service(&mut self, now: Instant) -> bool {
        self.drain_stderr(now);
        let mut dirty = self.flush_events(now);
        let mut budget = READ_BUDGET;
        let mut buf = [0u8; 8192];
        while budget > 0 {
            let read_result = match self
                .running
                .as_ref()
                .filter(|r| !r.stopping)
                .and_then(|r| r.stdout.as_ref())
            {
                Some(stdout) => unsafe {
                    libc::read(
                        stdout.as_raw_fd(),
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
                                self.last_error = None;
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
                    self.drain_stderr(now);
                    self.running = None;
                    self.log_wait = None;
                    self.events.clear();
                    self.event_bytes = 0;
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
        if let Some(cwd) = &self.config.cwd {
            command.current_dir(cwd);
        }
        command
            .args(&self.config.command[1..])
            .envs(&self.config.env)
            .stdin(if self.config.input == crate::config::Input::Interactive {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(
                if self.config.render_mode == crate::config::RenderMode::External {
                    Stdio::null()
                } else {
                    Stdio::piped()
                },
            )
            .stderr(Stdio::piped())
            .env(
                "RS_WWIDGET_RENDER_MODE",
                if self.config.render_mode == crate::config::RenderMode::External {
                    "external"
                } else {
                    "canvas"
                },
            )
            .env("RS_WWIDGET_ID", &self.config.id)
            .env(
                "RS_WWIDGET_LOCKED",
                if self.config.locked { "1" } else { "0" },
            )
            .env("RS_WWIDGET_PROTOCOL", self.config.protocol.to_string())
            .env("RS_WWIDGET_WIDTH", self.config.width.to_string())
            .env("RS_WWIDGET_HEIGHT", self.config.height.to_string())
            .env("RS_WWIDGET_ASSET_DIR", &self.config.asset_dir)
            .env(
                "RS_WWIDGET_STATE_DIR",
                crate::config::state_dir()
                    .join("widgets")
                    .join(&self.config.id),
            )
            .env(
                "RS_WWIDGET_EVENTS",
                if self.config.input == crate::config::Input::Interactive {
                    "1"
                } else {
                    "0"
                },
            );
        if let Some(sdk) = crate::registry::sdk_directory(&self.config.asset_dir) {
            command.env("RS_WWIDGET_SDK", sdk);
        }
        command.env(
            "RS_WWIDGET_LAYER",
            match self.config.layer {
                crate::config::Layer::Background => "background",
                crate::config::Layer::Bottom => "bottom",
                crate::config::Layer::Top => "top",
                crate::config::Layer::Overlay => "overlay",
            },
        );
        let [x, y] = self.config.position.unwrap_or([64, 96]);
        command
            .env("RS_WWIDGET_X", x.to_string())
            .env("RS_WWIDGET_Y", y.to_string());
        command.env(
            "RS_WWIDGET_THEME",
            self.config
                .host_theme
                .unwrap_or(crate::config::Palette::DEFAULT)
                .as_json()
                .to_string(),
        );
        command.env("RS_WWIDGET_CONTROL_SOCKET", crate::ipc::socket_path());
        command.env(
            "RS_WWIDGET_NAMESPACE",
            self.config.namespace.as_deref().unwrap_or("rs-wwidget"),
        );
        command.process_group(0);
        let mut child = command.spawn()?;
        let stdout = child.stdout.take();
        let stdin = child.stdin.take();
        let stderr = child.stderr.take();
        if let Some(output) = &stdout {
            let flags = unsafe { libc::fcntl(output.as_raw_fd(), libc::F_GETFL) };
            if flags < 0
                || unsafe {
                    libc::fcntl(output.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK)
                } < 0
            {
                let error = io::Error::last_os_error();
                self.running = Some(Running {
                    child,
                    stdout,
                    stdin,
                    stderr,
                    stopping: false,
                });
                return Err(error);
            }
        }
        self.running = Some(Running {
            child,
            stdout,
            stdin,
            stderr,
            stopping: false,
        });
        if let Some(fd) = self
            .running
            .as_ref()
            .and_then(|r| r.stderr.as_ref())
            .map(AsRawFd::as_raw_fd)
        {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        if let Some(fd) = self
            .running
            .as_ref()
            .and_then(|r| r.stdin.as_ref())
            .map(AsRawFd::as_raw_fd)
        {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        self.events.clear();
        self.event_bytes = 0;
        self.generation += 1;
        self.line.clear();
        self.last_frame = Some(now);
        self.failed = false;
        self.last_error = None;
        self.log_wait = None;
        self.deadline = now
            + Duration::from_secs(
                if self.config.render_mode == crate::config::RenderMode::External {
                    1
                } else {
                    self.config.timeout_seconds.max(1)
                },
            );
        if self.send_event(
            &crate::input::Event::Init {
                width: self.config.width,
                height: self.config.height,
                keyboard: self.config.keyboard,
                theme: self
                    .config
                    .host_theme
                    .unwrap_or(crate::config::Palette::DEFAULT)
                    .as_json(),
            },
            now,
        ) {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "initial input handshake failed",
            ));
        }
        Ok(())
    }

    fn fail(&mut self, reason: &str, now: Instant) -> bool {
        self.last_error = Some(reason.chars().take(1024).collect());
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
        self.events.clear();
        self.event_bytes = 0;
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

#[cfg(test)]
mod input_queue_tests {
    use super::*;
    #[test]
    fn unsent_motion_coalesces_without_corrupting_a_partial_event() {
        let mut runtime = Runtime::new(PluginConfig {
            enabled: true,
            input: crate::config::Input::Interactive,
            protocol: 2,
            keyboard: true,
            command: vec!["/bin/sleep".into(), "10".into()],
            ..PluginConfig::default()
        });
        let now = Instant::now();
        runtime.tick(now);
        runtime.send_event(
            &crate::input::Event::Keymap {
                format: "xkb_v1",
                keymap: "x".repeat(200_000),
            },
            now,
        );
        for i in 0..200 {
            runtime.send_event(
                &crate::input::Event::Pointer {
                    kind: crate::input::PointerKind::Motion,
                    x: i as f64,
                    y: 10.,
                    target: None,
                    local: None,
                    button: None,
                    pressed: None,
                    axis: None,
                    value: None,
                    drag_handle: false,
                },
                now,
            );
        }
        assert_eq!(runtime.events.len(), 2);
        assert!(runtime.events.front().unwrap().offset > 0);
        let event: serde_json::Value =
            serde_json::from_slice(&runtime.events.back().unwrap().data).unwrap();
        assert_eq!(event["x"], 199.0);
        assert_eq!(
            runtime.event_bytes,
            runtime
                .events
                .iter()
                .map(|e| e.data.len() - e.offset)
                .sum::<usize>()
        );
    }
}
