//! Bounded single-threaded provider stdin. Kept independent of the host library.
use serde_json::Value;
use std::{
    io::{self, Read},
    os::fd::AsRawFd,
    time::{Duration, Instant},
};
const LIMIT: usize = 262_144;

pub struct Events {
    buffer: Vec<u8>,
    closed: bool,
}
impl Events {
    pub fn new() -> io::Result<Self> {
        let fd = io::stdin().as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            buffer: Vec::new(),
            closed: false,
        })
    }
    pub fn closed(&self) -> bool {
        self.closed && self.buffer.is_empty()
    }
    pub fn poll(&mut self, timeout: Duration) -> Result<Option<Value>, Box<dyn std::error::Error>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
                if end > LIMIT {
                    return Err("event exceeds 256 KiB".into());
                }
                let event: Value = serde_json::from_slice(&self.buffer[..end])?;
                self.buffer.drain(..=end);
                if event.get("version").and_then(Value::as_u64) != Some(2)
                    || !event["type"].is_string()
                {
                    return Err("invalid host event".into());
                }
                return Ok(Some(event));
            }
            if self.buffer.len() > LIMIT {
                return Err("event exceeds 256 KiB".into());
            }
            if self.closed {
                if !self.buffer.is_empty() {
                    return Err("truncated host event".into());
                }
                return Ok(None);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let mut fd = libc::pollfd {
                fd: 0,
                events: libc::POLLIN,
                revents: 0,
            };
            // Round up: sub-millisecond deadlines must not become a busy loop.
            let millis = remaining
                .as_millis()
                .saturating_add(u128::from(!remaining.is_zero()))
                .min(i32::MAX as u128) as i32;
            let result = unsafe { libc::poll(&mut fd, 1, millis) };
            if result < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error.into());
            }
            if result == 0 {
                return Ok(None);
            }
            let mut bytes = [0; 8192];
            match io::stdin().read(&mut bytes) {
                Ok(0) => self.closed = true,
                Ok(n) => self.buffer.extend_from_slice(&bytes[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Ok(None);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
}
