//! Opt-in bounded metadata polling; no threads, recursive directory walks or disabled-widget work.
use crate::config::PluginConfig;
use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime},
};
type Stamp = Option<(SystemTime, u64)>;
fn stamp(path: &PathBuf) -> Stamp {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| Some((m.modified().ok()?, m.len())))
}
pub(crate) struct WatchSet {
    paths: Vec<PathBuf>,
    stamps: Vec<Stamp>,
    next: Instant,
}
impl WatchSet {
    pub fn new(config: &PluginConfig, now: Instant) -> Self {
        let mut paths = Vec::new();
        if config.enabled && !config.watch_files.is_empty() {
            paths.extend(config.watch_files.iter().map(|p| config.asset_dir.join(p)));
            if let Some(path) = &config.package_path {
                paths.push(path.clone());
            }
            if let Some(frame) = &config.frame {
                paths.push(config.asset_dir.join(frame));
            }
            paths.sort();
            paths.dedup();
        }
        let stamps = paths.iter().map(stamp).collect();
        Self {
            paths,
            stamps,
            next: now + Duration::from_millis(500),
        }
    }
    pub fn deadline(&self) -> Option<Instant> {
        (!self.paths.is_empty()).then_some(self.next)
    }
    pub fn changed(&mut self, now: Instant) -> bool {
        if self.paths.is_empty() || now < self.next {
            return false;
        }
        self.next = now + Duration::from_millis(500);
        let current: Vec<_> = self.paths.iter().map(stamp).collect();
        let changed = current != self.stamps;
        self.stamps = current;
        changed
    }
}
