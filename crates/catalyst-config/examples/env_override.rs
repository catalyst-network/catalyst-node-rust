use crate::error::ConfigResult;
use crate::loader::ConfigLoader;
use crate::networks::{NetworkConfig, Network};
use catalyst_utils::{CatalystResult, CatalystError};
use std::collections::HashMap;
use std::env;

/// Environment variable prefix for Catalyst configuration
pub const CATALYST_ENV_PREFIX: &str = "CATALYST_";

/// Environment variable override system
pub struct EnvOverride {
    prefix: String,
    overrides: HashMap<String, String>,
}

impl EnvOverride {
    /// Create new environment override handler
    pub fn new() -> Self {
        Self {
            prefix: CATALYST_ENV_PREFIX.to_string(),
            overrides: HashMap::new(),
        }
    }

    /// Create with custom prefix
    pub fn with_prefix(prefix: &str) -> Self {
        Self {
            prefix: prefix.to_string(),
            overrides: HashMap::new(),
        }
    }

    /// Load all environment variables with the catalyst prefix
    pub fn load_env_vars(&mut self) -> ConfigResult<()> {
        self.overrides.clear();
        
        for (key, value) in env::vars() {
            if key.starts_with(&self.prefix) {
                let config_key = key.strip_prefix(&self.prefix)
                    .unwrap()
                    .to_lowercase()
                    .replace('_', ".");
                self.overrides.insert(config_key, value);
            }
        }
        
        Ok(())
    }

    /// Add manual override (useful for testing)
    pub fn add_override(&mut self, key: &str, value: &str) {
        self.overrides.insert(key.to_string(), value.to_string());
    }

    /// Apply overrides to a network config
    pub fn apply_to_network_config(&self, config: &mut NetworkConfig) -> ConfigResult<()> {
        // Network identification
        if let Some(name) = self.overrides.get("network.name") {
            config.network.name = name.clone();
        }
        
        if let Some(id) = self.overrides.get("network.id") {
            config.network.id = id.parse()
                .map_err(|e| CatalystError::Config(format!("Invalid network.id: {}", e)))?;
        }

        // Node configuration
        if let Some(id) = self.overrides.get("node.id") {
            config.node.id = id.clone();
        }

        if let Some(data_dir) = self.overrides.get("node.data_dir") {
            config.node.data_dir = data_dir.into();
        }

        if let Some(log_level) = self.overrides.get("node.log_level") {
            config.node.log_level = log_level.clone();
        }

        // Network settings
        if let Some(port) = self.overrides.get("network.port") {
            config.network.port = port.parse()
                .map_err(|e| CatalystError::Config(format!("Invalid network.port: {}", e)))?;
        }

        if let Some(addr) = self.overrides.get("network.bind_address") {
            config.network.bind_address = addr.clone();
        }

        if let Some(max_peers) = self.overrides.get("network.max_peers") {
            config.network.max_peers = max_peers.parse()
                .map_err(|e| CatalystError::Config(format!("Invalid network.max_peers: {}", e)))?;
        }

        // Bootstrap peers
        if let Some(peers) = self.overrides.get("network.bootstrap_peers") {
            config.network.bootstrap_peers = peers
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }

        // Consensus settings
        if let Some(enabled) = self.overrides.get("consensus.enabled") {
            config.consensus.enabled = enabled.parse()
                .map_err(|e| CatalystError::Config(format!("Invalid consensus.enabled: {}", e)))?;
        }

        if let Some(cycle_time) = self.overrides.get("consensus.cycle_time_ms") {
            config.consensus.cycle_time_ms = cycle_time.parse()
                .map_err(|e| CatalystError::Config(format!("Invalid consensus.cycle_time_ms: {}", e)))?;
        }

        if let Some(producers) = self.overrides.get("consensus.max_producers") {
            config.consensus.max_producers = producers.parse()
                .map_err(|e| CatalystError::Config(format!("Invalid consensus.max_producers: {}", e)))?;
        }

        // Storage settings
        if let Some(path) = self.overrides.get("storage.path") {
            config.storage.path = path.into();
        }

        if let Some(cache_size) = self.overrides.get("storage.cache_size_mb") {
            config.storage.cache_size_mb = cache_size.parse()
                .map_err(|e| CatalystError::Config(format!("Invalid storage.cache_size_mb: {}", e)))?;
        }

        // RPC settings
        if let Some(enabled) = self.overrides.get("rpc.enabled") {
            config.rpc.enabled = enabled.parse()
                .map_err(|e| CatalystError::Config(format!("Invalid rpc.enabled: {}", e)))?;
        }

        if let Some(port) = self.overrides.get("rpc.port") {
            config.rpc.port = port.parse()
                .map_err(|e| CatalystError::Config(format!("Invalid rpc.port: {}", e)))?;
        }

        if let Some(addr) = self.overrides.get("rpc.bind_address") {
            config.rpc.bind_address = addr.clone();
        }

        // Service Bus settings
        if let Some(enabled) = self.overrides.get("service_bus.enabled") {
            config.service_bus.enabled = enabled.parse()
                .map_err(|e| CatalystError::Config(format!("Invalid service_bus.enabled: {}", e)))?;
        }

        if let Some(port) = self.overrides.get("service_bus.ws_port") {
            config.service_bus.ws_port = port.parse()
                .map_err(|e| CatalystError::Config(format!("Invalid service_bus.ws_port: {}", e)))?;
        }

        Ok(())
    }

    /// Get all loaded overrides
    pub fn get_overrides(&self) -> &HashMap<String, String> {
        &self.overrides
    }

    /// Check if a specific override exists
    pub fn has_override(&self, key: &str) -> bool {
        self.overrides.contains_key(key)
    }

    /// Get a specific override value
    pub fn get_override(&self, key: &str) -> Option<&String> {
        self.overrides.get(key)
    }

    /// Clear all overrides
    pub fn clear(&mut self) {
        self.overrides.clear();
    }
}

impl Default for EnvOverride {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to load config with environment overrides
pub fn load_config_with_env_overrides(
    network: Network,
    config_path: Option<&str>,
) -> ConfigResult<NetworkConfig> {
    let mut loader = ConfigLoader::new();
    let mut config = loader.load_network_config(network, config_path)?;
    
    let mut env_override = EnvOverride::new();
    env_override.load_env_vars()?;
    env_override.apply_to_network_config(&mut config)?;
    
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_env_override_creation() {
        let override_handler = EnvOverride::new();
        assert_eq!(override_handler.prefix, CATALYST_ENV_PREFIX);
        assert!(override_handler.overrides.is_empty());
    }

    #[test]
    fn test_custom_prefix() {
        let override_handler = EnvOverride::with_prefix("TEST_");
        assert_eq!(override_handler.prefix, "TEST_");
    }

    #[test]
    fn test_manual_override() {
        let mut override_handler = EnvOverride::new();
        override_handler.add_override("node.id", "test_node");
        
        assert!(override_handler.has_override("node.id"));
        assert_eq!(override_handler.get_override("node.id"), Some(&"test_node".to_string()));
    }

    #[test]
    fn test_env_var_loading() {
        // Set test environment variables
        env::set_var("CATALYST_NODE_ID", "env_test_node");
        env::set_var("CATALYST_NETWORK_PORT", "9876");
        env::set_var("NOT_CATALYST_VAR", "should_be_ignored");
        
        let mut override_handler = EnvOverride::new();
        override_handler.load_env_vars().unwrap();
        
        assert!(override_handler.has_override("node.id"));
        assert!(override_handler.has_override("network.port"));
        assert!(!override_handler.has_override("not.catalyst.var"));
        
        // Clean up
        env::remove_var("CATALYST_NODE_ID");
        env::remove_var("CATALYST_NETWORK_PORT");
        env::remove_var("NOT_CATALYST_VAR");
    }

    #[test]
    fn test_apply_to_config() {
        let mut config = NetworkConfig::devnet();
        let mut override_handler = EnvOverride::new();
        
        override_handler.add_override("node.id", "overridden_node");
        override_handler.add_override("network.port", "8765");
        override_handler.add_override("consensus.enabled", "false");
        
        override_handler.apply_to_network_config(&mut config).unwrap();
        
        assert_eq!(config.node.id, "overridden_node");
        assert_eq!(config.network.port, 8765);
        assert!(!config.consensus.enabled);
    }

    #[test]
    fn test_invalid_override_value() {
        let mut config = NetworkConfig::devnet();
        let mut override_handler = EnvOverride::new();
        
        override_handler.add_override("network.port", "not_a_number");
        
        let result = override_handler.apply_to_network_config(&mut config);
        assert!(result.is_err());
    }

    #[test]
    fn test_bootstrap_peers_override() {
        let mut config = NetworkConfig::devnet();
        let mut override_handler = EnvOverride::new();
        
        override_handler.add_override("network.bootstrap_peers", "peer1,peer2,peer3");
        override_handler.apply_to_network_config(&mut config).unwrap();
        
        assert_eq!(config.network.bootstrap_peers, vec!["peer1", "peer2", "peer3"]);
    }

    #[test]
    fn test_clear_overrides() {
        let mut override_handler = EnvOverride::new();
        override_handler.add_override("test.key", "test.value");
        
        assert!(!override_handler.overrides.is_empty());
        override_handler.clear();
        assert!(override_handler.overrides.is_empty());
    }
}