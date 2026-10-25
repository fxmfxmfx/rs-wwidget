pub mod author;
pub mod canvas;
pub mod config;
pub mod input;
pub mod instance;
pub mod ipc;
pub mod layout;
pub mod locale;
pub mod metrics;
pub mod plugin;
pub mod poster;
pub mod registry;
pub mod render;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

mod watch;
