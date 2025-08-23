use catalyst_utils::{log_error, log_info, log_warn};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::SystemTime;

/// Types of configuration events that can occur
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConfigEventType {
    /// Configuration file was modified
    FileModified,
    /// Configuration was successfully reloaded
    ReloadComplete,
    /// Configuration reload failed
    ReloadFailed,
    /// Specific configuration value changed
    ConfigChanged,
    /// Environment variable override detected
    EnvOverrideDetected,
    /// File watcher started
    WatcherStarted,
    /// File watcher stopped
    WatcherStopped,
    /// File watcher error occurred
    WatcherError,
    /// Configuration validation failed
    ValidationFailed,
    /// Hot reload started
    HotReloadStarted,
    /// Hot reload stopped
    HotReloadStopped,
}

impl fmt::Display for ConfigEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigEventType::FileModified => write!(f, "file_modified"),
            ConfigEventType::ReloadComplete => write!(f, "reload_complete"),
            ConfigEventType::ReloadFailed => write!(f, "reload_failed"),
            ConfigEventType::ConfigChanged => write!(f, "config_changed"),
            ConfigEventType::EnvOverrideDetected => write!(f, "env_override_detected"),
            ConfigEventType::WatcherStarted => write!(f, "watcher_started"),
            ConfigEventType::WatcherStopped => write!(f, "watcher_stopped"),
            ConfigEventType::WatcherError => write!(f, "watcher_error"),
            ConfigEventType::ValidationFailed => write!(f, "validation_failed"),
            ConfigEventType::HotReloadStarted => write!(f, "hot_reload_started"),
            ConfigEventType::HotReloadStopped => write!(f, "hot_reload_stopped"),
        }
    }
}

/// Configuration event containing details about what changed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEvent {
    /// Type of event that occurred
    pub event_type: ConfigEventType,
    /// Configuration section that changed (if applicable)
    pub section: Option<String>,
    /// Previous value (if applicable)
    pub old_value: Option<String>,
    /// New value (if applicable)
    pub new_value: Option<String>,
    /// When the event occurred
    pub timestamp: SystemTime,
}

impl ConfigEvent {
    /// Create a new config event
    pub fn new(event_type: ConfigEventType) -> Self {
        Self {
            event_type,
            section: None,
            old_value: None,
            new_value: None,
            timestamp: SystemTime::now(),
        }
    }

    /// Create a config change event
    pub fn config_changed(section: String, old_value: Option<String>, new_value: String) -> Self {
        Self {
            event_type: ConfigEventType::ConfigChanged,
            section: Some(section),
            old_value,
            new_value: Some(new_value),
            timestamp: SystemTime::now(),
        }
    }

    /// Create a file modified event
    pub fn file_modified() -> Self {
        Self::new(ConfigEventType::FileModified)
    }

    /// Create a reload complete event
    pub fn reload_complete() -> Self {
        Self::new(ConfigEventType::ReloadComplete)
    }

    /// Create a reload failed event
    pub fn reload_failed(error: String) -> Self {
        Self {
            event_type: ConfigEventType::ReloadFailed,
            section: None,
            old_value: None,
            new_value: Some(error),
            timestamp: SystemTime::now(),
        }
    }

    /// Create an environment override detected event
    pub fn env_override_detected(key: String, value: String) -> Self {
        Self {
            event_type: ConfigEventType::EnvOverrideDetected,
            section: Some(key),
            old_value: None,
            new_value: Some(value),
            timestamp: SystemTime::now(),
        }
    }

    /// Create a watcher started event
    pub fn watcher_started(path: String) -> Self {
        Self {
            event_type: ConfigEventType::WatcherStarted,
            section: Some(path),
            old_value: None,
            new_value: None,
            timestamp: SystemTime::now(),
        }
    }

    /// Create a watcher stopped event
    pub fn watcher_stopped() -> Self {
        Self::new(ConfigEventType::WatcherStopped)
    }

    /// Create a watcher error event
    pub fn watcher_error(error: String) -> Self {
        Self {
            event_type: ConfigEventType::WatcherError,
            section: None,
            old_value: None,
            new_value: Some(error),
            timestamp: SystemTime::now(),
        }
    }

    /// Create a validation failed event
    pub fn validation_failed(section: String, error: String) -> Self {
        Self {
            event_type: ConfigEventType::ValidationFailed,
            section: Some(section),
            old_value: None,
            new_value: Some(error),
            timestamp: SystemTime::now(),
        }
    }

    /// Create a hot reload started event
    pub fn hot_reload_started() -> Self {
        Self::new(ConfigEventType::HotReloadStarted)
    }

    /// Create a hot reload stopped event
    pub fn hot_reload_stopped() -> Self {
        Self::new(ConfigEventType::HotReloadStopped)
    }

    /// Get the event age in seconds
    pub fn age_seconds(&self) -> f64 {
        match SystemTime::now().duration_since(self.timestamp) {
            Ok(duration) => duration.as_secs_f64(),
            Err(_) => 0.0,
        }
    }

    /// Check if the event is a specific type
    pub fn is_type(&self, event_type: &ConfigEventType) -> bool {
        &self.event_type == event_type
    }

    /// Check if the event is an error type
    pub fn is_error(&self) -> bool {
        matches!(
            self.event_type,
            ConfigEventType::ReloadFailed
                | ConfigEventType::WatcherError
                | ConfigEventType::ValidationFailed
        )
    }

    /// Get a human-readable description of the event
    pub fn description(&self) -> String {
        match &self.event_type {
            ConfigEventType::FileModified => "Configuration file was modified".to_string(),
            ConfigEventType::ReloadComplete => "Configuration reloaded successfully".to_string(),
            ConfigEventType::ReloadFailed => {
                format!(
                    "Configuration reload failed: {}",
                    self.new_value.as_deref().unwrap_or("unknown error")
                )
            }
            ConfigEventType::ConfigChanged => {
                if let Some(section) = &self.section {
                    format!("Configuration section '{}' changed", section)
                } else {
                    "Configuration changed".to_string()
                }
            }
            ConfigEventType::EnvOverrideDetected => {
                if let Some(key) = &self.section {
                    format!("Environment override detected for '{}'", key)
                } else {
                    "Environment override detected".to_string()
                }
            }
            ConfigEventType::WatcherStarted => {
                if let Some(path) = &self.section {
                    format!("File watcher started for '{}'", path)
                } else {
                    "File watcher started".to_string()
                }
            }
            ConfigEventType::WatcherStopped => "File watcher stopped".to_string(),
            ConfigEventType::WatcherError => {
                format!(
                    "File watcher error: {}",
                    self.new_value.as_deref().unwrap_or("unknown error")
                )
            }
            ConfigEventType::ValidationFailed => {
                if let Some(section) = &self.section {
                    format!(
                        "Validation failed for section '{}': {}",
                        section,
                        self.new_value.as_deref().unwrap_or("unknown error")
                    )
                } else {
                    format!(
                        "Configuration validation failed: {}",
                        self.new_value.as_deref().unwrap_or("unknown error")
                    )
                }
            }
            ConfigEventType::HotReloadStarted => "Hot reload started".to_string(),
            ConfigEventType::HotReloadStopped => "Hot reload stopped".to_string(),
        }
    }
}

impl fmt::Display for ConfigEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.event_type, self.description())
    }
}

/// Event listener trait for handling configuration events
pub trait ConfigEventListener: Send + Sync {
    /// Handle a configuration event
    fn on_event(&self, event: &ConfigEvent);
}

/// Simple event logger that logs events using catalyst-utils logging
pub struct ConfigEventLogger;

impl ConfigEventListener for ConfigEventLogger {
    fn on_event(&self, event: &ConfigEvent) {
        use catalyst_utils::logging::LogCategory;
        use catalyst_utils::{log_error, log_info, log_warn};

        match event.event_type {
            ConfigEventType::ReloadFailed
            | ConfigEventType::WatcherError
            | ConfigEventType::ValidationFailed => {
                log_error!(LogCategory::Config, "Config event: {}", event);
            }
            ConfigEventType::FileModified | ConfigEventType::EnvOverrideDetected => {
                log_warn!(LogCategory::Config, "Config event: {}", event);
            }
            _ => {
                log_info!(LogCategory::Config, "Config event: {}", event);
            }
        }
    }
}

/// Event dispatcher for managing multiple event listeners
pub struct ConfigEventDispatcher {
    listeners: Vec<Box<dyn ConfigEventListener>>,
}

impl ConfigEventDispatcher {
    /// Create a new event dispatcher
    pub fn new() -> Self {
        Self {
            listeners: Vec::new(),
        }
    }

    /// Add an event listener
    pub fn add_listener(&mut self, listener: Box<dyn ConfigEventListener>) {
        self.listeners.push(listener);
    }

    /// Dispatch an event to all listeners
    pub fn dispatch(&self, event: &ConfigEvent) {
        for listener in &self.listeners {
            listener.on_event(event);
        }
    }

    /// Get the number of registered listeners
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }
}

impl Default for ConfigEventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Configuration event history for tracking recent events
pub struct ConfigEventHistory {
    events: Vec<ConfigEvent>,
    max_events: usize,
}

impl ConfigEventHistory {
    /// Create a new event history with maximum capacity
    pub fn new(max_events: usize) -> Self {
        Self {
            events: Vec::with_capacity(max_events),
            max_events,
        }
    }

    /// Add an event to the history
    pub fn add_event(&mut self, event: ConfigEvent) {
        if self.events.len() >= self.max_events {
            self.events.remove(0);
        }
        self.events.push(event);
    }

    /// Get all events in chronological order
    pub fn get_events(&self) -> &[ConfigEvent] {
        &self.events
    }

    /// Get events of a specific type
    pub fn get_events_by_type(&self, event_type: &ConfigEventType) -> Vec<&ConfigEvent> {
        self.events
            .iter()
            .filter(|event| &event.event_type == event_type)
            .collect()
    }

    /// Get the most recent event
    pub fn get_latest_event(&self) -> Option<&ConfigEvent> {
        self.events.last()
    }

    /// Get events within a time range
    pub fn get_events_since(&self, since: SystemTime) -> Vec<&ConfigEvent> {
        self.events
            .iter()
            .filter(|event| event.timestamp >= since)
            .collect()
    }

    /// Clear all events
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Get the number of events in history
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

impl Default for ConfigEventHistory {
    fn default() -> Self {
        Self::new(100) // Default to 100 events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_config_event_creation() {
        let event = ConfigEvent::new(ConfigEventType::FileModified);
        assert_eq!(event.event_type, ConfigEventType::FileModified);
        assert!(event.section.is_none());
        assert!(event.old_value.is_none());
        assert!(event.new_value.is_none());
    }

    #[test]
    fn test_config_changed_event() {
        let event = ConfigEvent::config_changed(
            "network.port".to_string(),
            Some("9944".to_string()),
            "8080".to_string(),
        );

        assert_eq!(event.event_type, ConfigEventType::ConfigChanged);
        assert_eq!(event.section, Some("network.port".to_string()));
        assert_eq!(event.old_value, Some("9944".to_string()));
        assert_eq!(event.new_value, Some("8080".to_string()));
    }

    #[test]
    fn test_event_type_display() {
        assert_eq!(ConfigEventType::FileModified.to_string(), "file_modified");
        assert_eq!(
            ConfigEventType::ReloadComplete.to_string(),
            "reload_complete"
        );
        assert_eq!(ConfigEventType::ConfigChanged.to_string(), "config_changed");
    }

    #[test]
    fn test_event_description() {
        let event =
            ConfigEvent::config_changed("network.port".to_string(), None, "8080".to_string());

        assert_eq!(
            event.description(),
            "Configuration section 'network.port' changed"
        );
    }

    #[test]
    fn test_event_is_error() {
        let reload_failed = ConfigEvent::reload_failed("Test error".to_string());
        assert!(reload_failed.is_error());

        let reload_complete = ConfigEvent::reload_complete();
        assert!(!reload_complete.is_error());
    }

    #[test]
    fn test_event_dispatcher() {
        let mut dispatcher = ConfigEventDispatcher::new();
        assert_eq!(dispatcher.listener_count(), 0);

        dispatcher.add_listener(Box::new(ConfigEventLogger));
        assert_eq!(dispatcher.listener_count(), 1);

        let event = ConfigEvent::reload_complete();
        dispatcher.dispatch(&event); // Should not panic
    }

    #[test]
    fn test_event_history() {
        let mut history = ConfigEventHistory::new(3);
        assert_eq!(history.len(), 0);
        assert!(history.is_empty());

        history.add_event(ConfigEvent::file_modified());
        history.add_event(ConfigEvent::reload_complete());
        history.add_event(ConfigEvent::config_changed(
            "test".to_string(),
            None,
            "value".to_string(),
        ));

        assert_eq!(history.len(), 3);
        assert!(!history.is_empty());

        // Add one more to test capacity limit
        history.add_event(ConfigEvent::reload_failed("error".to_string()));
        assert_eq!(history.len(), 3); // Should still be 3 due to capacity limit

        let latest = history.get_latest_event().unwrap();
        assert_eq!(latest.event_type, ConfigEventType::ReloadFailed);
    }

    #[test]
    fn test_event_history_filtering() {
        let mut history = ConfigEventHistory::new(10);

        history.add_event(ConfigEvent::file_modified());
        history.add_event(ConfigEvent::reload_complete());
        history.add_event(ConfigEvent::file_modified());
        history.add_event(ConfigEvent::config_changed(
            "test".to_string(),
            None,
            "value".to_string(),
        ));

        let file_modified_events = history.get_events_by_type(&ConfigEventType::FileModified);
        assert_eq!(file_modified_events.len(), 2);

        let config_changed_events = history.get_events_by_type(&ConfigEventType::ConfigChanged);
        assert_eq!(config_changed_events.len(), 1);
    }

    #[test]
    fn test_event_age() {
        let event = ConfigEvent::reload_complete();
        std::thread::sleep(Duration::from_millis(10));

        let age = event.age_seconds();
        assert!(age > 0.0);
        assert!(age < 1.0); // Should be less than a second
    }

    #[test]
    fn test_event_history_time_filtering() {
        let mut history = ConfigEventHistory::new(10);
        let start_time = SystemTime::now();

        history.add_event(ConfigEvent::file_modified());
        std::thread::sleep(Duration::from_millis(10));
        let mid_time = SystemTime::now();
        history.add_event(ConfigEvent::reload_complete());

        let recent_events = history.get_events_since(mid_time);
        assert_eq!(recent_events.len(), 1);
        assert_eq!(recent_events[0].event_type, ConfigEventType::ReloadComplete);

        let all_events = history.get_events_since(start_time);
        assert_eq!(all_events.len(), 2);
    }
}
