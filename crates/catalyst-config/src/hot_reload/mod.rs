//! Hot reload functionality for configuration changes

pub mod events;
pub mod watcher;

pub use events::*;
pub use watcher::ConfigWatcher;

use crate::hot_reload::watcher::FileWatcher;

/// Manages hot-reload file watching infrastructure.
pub struct HotReloadManager {
    // The watcher will be consumed by runtime wiring later; silence dead-code for now.
    #[allow(dead_code)]
    watcher: FileWatcher,
}

impl HotReloadManager {
    /// Construct a new hot-reload manager.
    pub fn new() -> Self {
        let (event_sender, _event_receiver) = tokio::sync::mpsc::unbounded_channel();
        let watcher = FileWatcher::new(event_sender);
        Self { watcher }
    }
}

impl Default for HotReloadManager {
    fn default() -> Self {
        Self::new()
    }
}
