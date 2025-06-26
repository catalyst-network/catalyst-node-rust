use crate::error::ConfigResult;
use crate::loader::ConfigLoader;
use crate::networks::{NetworkConfig, Network};
use crate::env_override::EnvOverride;
use crate::events::{ConfigEvent, ConfigEventType};
use crate::watcher::FileWatcher;
use catalyst_utils::{CatalystResult, CatalystError};
use catalyst_utils::logging::{LogCategory, log_info, log_warn, log_error};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;

/// Configuration hot reload manager
pub struct HotReloadManager {
    current_config: Arc<RwLock<NetworkConfig>>,
    config_path: PathBuf,
    network: Network,
    loader: ConfigLoader,
    env_override: EnvOverride,
    file_watcher: Option<FileWatcher>,
    event_sender: Option<mpsc::UnboundedSender<ConfigEvent>>,
    last_reload: Instant,
    reload_debounce: Duration,
    auto_reload_enabled: bool,
}

impl HotReloadManager {
    /// Create new hot reload manager
    pub fn new(
        initial_config: NetworkConfig,
        config_path: PathBuf,
        network: Network,
    ) -> Self {
        Self {
            current_config: Arc::new(RwLock::new(initial_config)),
            config_path,
            network,
            loader: ConfigLoader::new(),
            env_override: EnvOverride::new(),
            file_watcher: None,
            event_sender: None,
            last_reload: Instant::now(),
            reload_debounce: Duration::from_millis(500), // 500ms debounce
            auto_reload_enabled: false,
        }
    }

    /// Start automatic file watching and hot reload
    pub async fn start_auto_reload(&mut self) -> ConfigResult<mpsc::UnboundedReceiver<ConfigEvent>> {
        if self.auto_reload_enabled {
            return Err(CatalystError::Config("Auto reload already enabled".to_string()));
        }

        log_info!(LogCategory::Config, "Starting configuration hot reload for: {:?}", self.config_path);

        // Create event channel
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        self.event_sender = Some(event_sender.clone());

        // Create file watcher
        let mut file_watcher = FileWatcher::new(event_sender.clone());
        file_watcher.watch_file(&self.config_path).await?;
        self.file_watcher = Some(file_watcher);

        // Start reload loop
        let config_path = self.config_path.clone();
        let current_config = Arc::clone(&self.current_config);
        let network = self.network;
        let reload_debounce = self.reload_debounce;
        
        tokio::spawn(async move {
            let mut last_reload = Instant::now();
            let mut loader = ConfigLoader::new();
            let mut env_override = EnvOverride::new();
            
            // Load initial environment overrides
            if let Err(e) = env_override.load_env_vars() {
                log_warn!(LogCategory::Config, "Failed to load env vars for hot reload: {}", e);
            }

            let mut interval = tokio::time::interval(Duration::from_millis(100));
            
            loop {
                interval.tick().await;
                
                // Check if file has been modified recently
                if let Ok(metadata) = std::fs::metadata(&config_path) {
                    if let Ok(modified) = metadata.modified() {
                        let modified_instant = Instant::now() - 
                            Duration::from_secs(
                                std::time::SystemTime::now()
                                    .duration_since(modified)
                                    .unwrap_or_default()
                                    .as_secs()
                            );
                        
                        if modified_instant > last_reload && 
                           Instant::now().duration_since(last_reload) > reload_debounce {
                            
                            match Self::reload_config_internal(
                                &mut loader,
                                &mut env_override,
                                &config_path,
                                network,
                                &current_config,
                                &event_sender,
                            ).await {
                                Ok(_) => {
                                    last_reload = Instant::now();
                                    log_info!(LogCategory::Config, "Configuration reloaded successfully");
                                }
                                Err(e) => {
                                    log_error!(LogCategory::Config, "Failed to reload configuration: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        });

        self.auto_reload_enabled = true;
        Ok(event_receiver)
    }

    /// Stop automatic reload
    pub async fn stop_auto_reload(&mut self) -> ConfigResult<()> {
        if !self.auto_reload_enabled {
            return Ok(());
        }

        log_info!(LogCategory::Config, "Stopping configuration hot reload");

        if let Some(mut file_watcher) = self.file_watcher.take() {
            file_watcher.stop().await?;
        }

        self.event_sender = None;
        self.auto_reload_enabled = false;
        
        Ok(())
    }

    /// Manually trigger a configuration reload
    pub async fn reload_now(&mut self) -> ConfigResult<()> {
        if Instant::now().duration_since(self.last_reload) < self.reload_debounce {
            log_warn!(LogCategory::Config, "Reload requested too soon, ignoring");
            return Ok(());
        }

        log_info!(LogCategory::Config, "Manual configuration reload triggered");

        // Reload environment variables
        self.env_override.load_env_vars()?;

        // Reload configuration
        Self::reload_config_internal(
            &mut self.loader,
            &mut self.env_override,
            &self.config_path,
            self.network,
            &self.current_config,
            &self.event_sender.as_ref(),
        ).await?;

        self.last_reload = Instant::now();
        Ok(())
    }

    /// Internal config reload logic
    async fn reload_config_internal(
        loader: &mut ConfigLoader,
        env_override: &mut EnvOverride,
        config_path: &Path,
        network: Network,
        current_config: &Arc<RwLock<NetworkConfig>>,
        event_sender: &Option<&mpsc::UnboundedSender<ConfigEvent>>,
    ) -> ConfigResult<()> {
        // Load new configuration
        let mut new_config = loader.load_network_config(network, Some(config_path.to_str().unwrap()))?;
        
        // Apply environment overrides
        env_override.apply_to_network_config(&mut new_config)?;

        // Get the old config for comparison
        let old_config = {
            let guard = current_config.read().unwrap();
            guard.clone()
        };

        // Update current config
        {
            let mut guard = current_config.write().unwrap();
            *guard = new_config.clone();
        }

        // Send events for changes
        if let Some(sender) = event_sender {
            Self::detect_and_send_changes(&old_config, &new_config, sender)?;
        }

        Ok(())
    }

    /// Detect configuration changes and send appropriate events
    fn detect_and_send_changes(
        old_config: &NetworkConfig,
        new_config: &NetworkConfig,
        event_sender: &mpsc::UnboundedSender<ConfigEvent>,
    ) -> ConfigResult<()> {
        let mut changes = Vec::new();

        // Check node configuration changes
        if old_config.node != new_config.node {
            changes.push(("node".to_string(), format!("{:?}", new_config.node)));
        }

        // Check network configuration changes
        if old_config.network != new_config.network {
            changes.push(("network".to_string(), format!("{:?}", new_config.network)));
        }

        // Check consensus configuration changes
        if old_config.consensus != new_config.consensus {
            changes.push(("consensus".to_string(), format!("{:?}", new_config.consensus)));
        }

        // Check storage configuration changes
        if old_config.storage != new_config.storage {
            changes.push(("storage".to_string(), format!("{:?}", new_config.storage)));
        }

        // Check RPC configuration changes
        if old_config.rpc != new_config.rpc {
            changes.push(("rpc".to_string(), format!("{:?}", new_config.rpc)));
        }

        // Check service bus configuration changes
        if old_config.service_bus != new_config.service_bus {
            changes.push(("service_bus".to_string(), format!("{:?}", new_config.service_bus)));
        }

        // Send change events
        for (section, new_value) in changes {
            let event = ConfigEvent {
                event_type: ConfigEventType::ConfigChanged,
                section: Some(section),
                old_value: None, // Could implement detailed old value tracking if needed
                new_value: Some(new_value),
                timestamp: std::time::SystemTime::now(),
            };

            if let Err(e) = event_sender.send(event) {
                log_warn!(LogCategory::Config, "Failed to send config change event: {}", e);
            }
        }

        // Send general reload complete event
        let reload_event = ConfigEvent {
            event_type: ConfigEventType::ReloadComplete,
            section: None,
            old_value: None,
            new_value: None,
            timestamp: std::time::SystemTime::now(),
        };

        if let Err(e) = event_sender.send(reload_event) {
            log_warn!(LogCategory::Config, "Failed to send reload complete event: {}", e);
        }

        Ok(())
    }

    /// Get current configuration (thread-safe)
    pub fn get_current_config(&self) -> NetworkConfig {
        let guard = self.current_config.read().unwrap();
        guard.clone()
    }

    /// Update a specific configuration value
    pub async fn update_config_value(&mut self, key: &str, value: &str) -> ConfigResult<()> {
        {
            let mut guard = self.current_config.write().unwrap();
            Self::apply_config_update(&mut *guard, key, value)?;
        }

        // Send update event
        if let Some(sender) = &self.event_sender {
            let event = ConfigEvent {
                event_type: ConfigEventType::ConfigChanged,
                section: Some(key.to_string()),
                old_value: None,
                new_value: Some(value.to_string()),
                timestamp: std::time::SystemTime::now(),
            };

            if let Err(e) = sender.send(event) {
                log_warn!(LogCategory::Config, "Failed to send config update event: {}", e);
            }
        }

        Ok(())
    }

    /// Apply a single configuration update
    fn apply_config_update(config: &mut NetworkConfig, key: &str, value: &str) -> ConfigResult<()> {
        match key {
            "node.id" => config.node.id = value.to_string(),
            "node.log_level" => config.node.log_level = value.to_string(),
            "network.port" => {
                config.network.port = value.parse()
                    .map_err(|e| CatalystError::Config(format!("Invalid port: {}", e)))?;
            }
            "network.bind_address" => config.network.bind_address = value.to_string(),
            "network.max_peers" => {
                config.network.max_peers = value.parse()
                    .map_err(|e| CatalystError::Config(format!("Invalid max_peers: {}", e)))?;
            }
            "consensus.enabled" => {
                config.consensus.enabled = value.parse()
                    .map_err(|e| CatalystError::Config(format!("Invalid consensus.enabled: {}", e)))?;
            }
            "consensus.cycle_time_ms" => {
                config.consensus.cycle_time_ms = value.parse()
                    .map_err(|e| CatalystError::Config(format!("Invalid cycle_time_ms: {}", e)))?;
            }
            "rpc.enabled" => {
                config.rpc.enabled = value.parse()
                    .map_err(|e| CatalystError::Config(format!("Invalid rpc.enabled: {}", e)))?;
            }
            "rpc.port" => {
                config.rpc.port = value.parse()
                    .map_err(|e| CatalystError::Config(format!("Invalid rpc.port: {}", e)))?;
            }
            "service_bus.enabled" => {
                config.service_bus.enabled = value.parse()
                    .map_err(|e| CatalystError::Config(format!("Invalid service_bus.enabled: {}", e)))?;
            }
            _ => {
                return Err(CatalystError::Config(format!("Unknown config key: {}", key)));
            }
        }
        Ok(())
    }

    /// Check if auto reload is enabled
    pub fn is_auto_reload_enabled(&self) -> bool {
        self.auto_reload_enabled
    }

    /// Set reload debounce duration
    pub fn set_reload_debounce(&mut self, duration: Duration) {
        self.reload_debounce = duration;
    }
}

impl Drop for HotReloadManager {
    fn drop(&mut self) {
        if self.auto_reload_enabled {
            log_info!(LogCategory::Config, "Hot reload manager dropping, cleaning up");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_hot_reload_manager_creation() {
        let config = NetworkConfig::devnet();
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");
        
        let manager = HotReloadManager::new(config, config_path, Network::Devnet);
        assert!(!manager.is_auto_reload_enabled());
    }

    #[tokio::test]
    async fn test_manual_reload() {
        let mut config = NetworkConfig::devnet();
        config.node.id = "test_node".to_string();
        
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");
        
        // Write initial config
        let toml_content = r#"
[node]
id = "updated_node"
data_dir = "./data"
log_level = "info"

[network]
name = "devnet"
id = 1
port = 9944
bind_address = "0.0.0.0"
max_peers = 50
bootstrap_peers = []

[consensus]
enabled = true
cycle_time_ms = 30000
max_producers = 100

[storage]
path = "./data/storage"
cache_size_mb = 256

[rpc]
enabled = true
port = 9933
bind_address = "127.0.0.1"

[service_bus]
enabled = true
ws_port = 9222
bind_address = "127.0.0.1"
"#;
        fs::write(&config_path, toml_content).unwrap();
        
        let mut manager = HotReloadManager::new(config, config_path, Network::Devnet);
        
        // Perform manual reload
        manager.reload_now().await.unwrap();
        
        let reloaded_config = manager.get_current_config();
        assert_eq!(reloaded_config.node.id, "updated_node");
    }

    #[tokio::test]
    async fn test_config_value_update() {
        let config = NetworkConfig::devnet();
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");
        
        let mut manager = HotReloadManager::new(config, config_path, Network::Devnet);
        
        // Update a config value
        manager.update_config_value("node.id", "new_node_id").await.unwrap();
        
        let updated_config = manager.get_current_config();
        assert_eq!(updated_config.node.id, "new_node_id");
    }

    #[test]
    fn test_apply_config_update() {
        let mut config = NetworkConfig::devnet();
        
        HotReloadManager::apply_config_update(&mut config, "node.id", "test").unwrap();
        assert_eq!(config.node.id, "test");
        
        HotReloadManager::apply_config_update(&mut config, "network.port", "8080").unwrap();
        assert_eq!(config.network.port, 8080);
        
        // Test invalid values
        let result = HotReloadManager::apply_config_update(&mut config, "network.port", "not_a_number");
        assert!(result.is_err());
    }

    #[test]
    fn test_debounce_duration() {
        let config = NetworkConfig::devnet();
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("test_config.toml");
        
        let mut manager = HotReloadManager::new(config, config_path, Network::Devnet);
        manager.set_reload_debounce(Duration::from_secs(1));
        
        assert_eq!(manager.reload_debounce, Duration::from_secs(1));
    }
}