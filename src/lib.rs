pub mod config;
pub mod layout;
pub mod locale;
pub mod metrics;
pub mod render;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
