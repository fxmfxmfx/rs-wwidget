//! Small same-user datagram control channel. No blocking clients in the host loop.
use crate::{Result, config};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    os::{
        fd::{AsRawFd, RawFd},
        unix::{
            fs::{FileTypeExt, MetadataExt, PermissionsExt},
            net::UnixDatagram,
        },
    },
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
pub const MAX_MESSAGE: usize = 16_384;
pub const MAX_REPLY: usize = 65_536;
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Status {
        id: Option<String>,
        offset: Option<usize>,
        limit: Option<usize>,
    },
    Reload {
        id: String,
    },
    Configure {
        id: String,
        options: serde_json::Value,
    },
    Reset {
        id: String,
    },
    Move {
        id: String,
        x: i32,
        y: i32,
    },
    Lock {
        id: String,
        locked: bool,
    },
    Visible {
        id: String,
        visible: bool,
    },
    Send {
        id: String,
        name: String,
        data: serde_json::Value,
    },
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Override {
    pub locked: Option<bool>,
    pub hidden: bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Overrides(pub BTreeMap<String, Override>);
impl Overrides {
    pub fn load(path: &Path) -> Result<Self> {
        use std::io::Read;
        let mut bytes = Vec::new();
        match fs::File::open(path) {
            Ok(file) => {
                file.take(65_537).read_to_end(&mut bytes)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.into()),
        };
        if bytes.len() > 65_536 {
            return Err("widget controls file exceeds 64 KiB".into());
        }
        let result: Self = serde_json::from_slice(&bytes)?;
        if result.0.len() > 256 || result.0.keys().any(|id| !crate::registry::valid_id(id)) {
            return Err("invalid widget control IDs or more than 256 overrides".into());
        }
        Ok(result)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self)?;
        if bytes.len() > 65_536 || self.0.len() > 256 {
            return Err("too many widget overrides".into());
        }
        config::atomic_write(path, &bytes)?;
        Ok(())
    }
}
pub fn socket_path() -> PathBuf {
    config::state_dir().join("control.sock")
}

pub struct Server {
    socket: UnixDatagram,
    path: PathBuf,
    inode: u64,
}
impl Server {
    /// Call only after obtaining the host's instance lock.
    pub fn bind(path: PathBuf) -> Result<Self> {
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if !meta.file_type().is_socket() || meta.uid() != unsafe { libc::geteuid() } {
                return Err("refusing to remove non-socket/foreign control endpoint".into());
            }
            fs::remove_file(&path)?;
        }
        let socket = UnixDatagram::bind(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        socket.set_nonblocking(true)?;
        let inode = fs::metadata(&path)?.ino();
        Ok(Self {
            socket,
            path,
            inode,
        })
    }
    pub fn fd(&self) -> RawFd {
        self.socket.as_raw_fd()
    }
    pub fn receive(&self) -> Option<(std::result::Result<Request, String>, PathBuf)> {
        let mut bytes = [0; MAX_MESSAGE + 1];
        let (size, peer) = self.socket.recv_from(&mut bytes).ok()?;
        let peer = peer.as_pathname()?.to_path_buf();
        let request = if size > MAX_MESSAGE {
            Err("control request exceeds 16 KiB".into())
        } else {
            serde_json::from_slice(&bytes[..size]).map_err(|e| e.to_string())
        };
        Some((request, peer))
    }
    pub fn reply(&self, peer: &Path, result: std::result::Result<serde_json::Value, String>) {
        let value = match result {
            Ok(value) => serde_json::json!({"ok":true,"result":value}),
            Err(error) => serde_json::json!({"ok":false,"error":error}),
        };
        if let Ok(bytes) = serde_json::to_vec(&value) {
            if bytes.len() <= MAX_REPLY {
                let _ = self.socket.send_to(&bytes, peer);
            } else {
                let _ = self.socket.send_to(
                    br#"{"ok":false,"error":"response too large; query a single widget"}"#,
                    peer,
                );
            }
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if fs::symlink_metadata(&self.path).is_ok_and(|m| m.ino() == self.inode) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
struct Client {
    socket: UnixDatagram,
    path: PathBuf,
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
pub fn call(request: &Request) -> Result<serde_json::Value> {
    let endpoint = socket_path();
    let path = endpoint.with_file_name(format!(
        "client-{}-{}.sock",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    ));
    let client = Client {
        socket: UnixDatagram::bind(&path)?,
        path,
    };
    fs::set_permissions(&client.path, fs::Permissions::from_mode(0o600))?;
    client
        .socket
        .set_read_timeout(Some(Duration::from_secs(2)))?;
    client
        .socket
        .set_write_timeout(Some(Duration::from_secs(1)))?;
    let bytes = serde_json::to_vec(request)?;
    if bytes.len() > MAX_MESSAGE {
        return Err("control request exceeds 16 KiB".into());
    }
    client
        .socket
        .connect(endpoint)
        .map_err(|e| format!("host control unavailable: {e}; start rs-wwidget in this session"))?;
    client.socket.send(&bytes)?;
    let mut response = [0; MAX_REPLY + 1];
    let size = client.socket.recv(&mut response)?;
    if size > MAX_REPLY {
        return Err("oversized control response".into());
    }
    let value: serde_json::Value = serde_json::from_slice(&response[..size])?;
    if value["ok"] != true {
        return Err(value["error"]
            .as_str()
            .unwrap_or("control request failed")
            .to_string()
            .into());
    }
    Ok(value["result"].clone())
}
pub fn cli(args: &[String]) -> Result<()> {
    let request = match args {
        [op] if op == "list" => Request::Status { id: None, offset:None, limit:None },
        [op,offset] if op=="list"=>Request::Status{id:None,offset:Some(offset.parse()?),limit:None},
        [id, op] if op == "status" => Request::Status {
            id: Some(id.clone()), offset:None, limit:None,
        },
        [id,op] if op=="reset"=>Request::Reset{id:id.clone()},
        [id,op,options] if op=="configure"=>Request::Configure{id:id.clone(),options:serde_json::from_str(options)?},
        [id, op] if op == "reload" => Request::Reload { id: id.clone() },
        [id, op] if op == "lock" || op == "unlock" => Request::Lock {
            id: id.clone(),
            locked: op == "lock",
        },
        [id, op] if op == "hide" || op == "show" => Request::Visible {
            id: id.clone(),
            visible: op == "show",
        },
        [id, op, x, y] if op == "move" => Request::Move {
            id: id.clone(),
            x: x.parse()?,
            y: y.parse()?,
        },
        [id, op, name, data] if op == "send" => Request::Send {
            id: id.clone(),
            name: name.clone(),
            data: serde_json::from_str(data)?,
        },
        _ => return Err(
            "widget list [OFFSET] | widget ID status|reload|reset|lock|unlock|hide|show|move X Y|configure JSON|send NAME JSON"
                .into(),
        ),
    };
    println!("{}", serde_json::to_string_pretty(&call(&request)?)?);
    Ok(())
}
