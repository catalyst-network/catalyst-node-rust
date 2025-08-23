//! Hot reload functionality for configuration changes

pub mod events;
pub mod watcher;

use crate::hot_reload::watcher::FileWatcher;
pub use events::*;
pub use watcher::ConfigWatcher;

pub struct HotReloadManager {
    watcher: FileWatcher,
}

impl HotReloadManager {
    pub fn new() -> Self {
        let (event_sender, _event_receiver) = tokio::sync::mpsc::unbounded_channel();
        let watcher = FileWatcher::new(event_sender);
        Self { watcher }
    }
}
