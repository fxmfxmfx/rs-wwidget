//! Host-to-provider events. Only opt-in interactive command widgets get stdin.
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PointerKind {
    Enter,
    Leave,
    Motion,
    Button,
    Scroll,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    Vertical,
    Horizontal,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Custom {
        name: String,
        data: serde_json::Value,
    },
    Init {
        width: i32,
        height: i32,
        keyboard: bool,
        theme: serde_json::Value,
    },
    Pointer {
        kind: PointerKind,
        x: f64,
        y: f64,
        target: Option<String>,
        local: Option<[f64; 2]>,
        button: Option<u32>,
        pressed: Option<bool>,
        axis: Option<Axis>,
        value: Option<f64>,
        drag_handle: bool,
    },
    KeyboardFocus {
        focused: bool,
        keys: Vec<u32>,
    },
    Key {
        key: u32,
        pressed: bool,
        time: u32,
    },
    Modifiers {
        depressed: u32,
        latched: u32,
        locked: u32,
        group: u32,
    },
    Keymap {
        format: &'static str,
        keymap: String,
    },
    RepeatInfo {
        rate: i32,
        delay: i32,
    },
}

impl Event {
    pub(crate) fn encode(&self) -> Vec<u8> {
        #[derive(Serialize)]
        struct Envelope<'a> {
            version: u32,
            #[serde(flatten)]
            event: &'a Event,
        }
        let mut data = serde_json::to_vec(&Envelope {
            version: 2,
            event: self,
        })
        .expect("host event serialization");
        data.push(b'\n');
        data
    }
    pub(crate) fn is_keyboard(&self) -> bool {
        matches!(
            self,
            Self::KeyboardFocus { .. }
                | Self::Key { .. }
                | Self::Modifiers { .. }
                | Self::Keymap { .. }
                | Self::RepeatInfo { .. }
        )
    }
    pub(crate) fn is_motion(&self) -> bool {
        matches!(
            self,
            Self::Pointer {
                kind: PointerKind::Motion,
                ..
            }
        )
    }
}
