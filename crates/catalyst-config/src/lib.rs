use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

/// Configuration management for Catalyst Network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalystConfig {
    /// Network configuration
    pub network: NetworkConfig,
    /// Consensus configuration
    pub consensus: ConsensusConfig,
    /// Storage configuration
    pub storage: StorageConfig,
    /// Logging configuration
    pub logging: LoggingConfig,
}

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),
    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Listen addresses for the network
    pub listen_addresses: Vec<String>,
    /// Bootstrap peers to connect to
    pub bootstrap_peers: Vec<String>,
    /// Maximum number of connections
    pub max_connections: usize,
    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,
    /// Enable mDNS discovery
    pub enable_mdns: bool,
    /// Enable Kademlia DHT
    pub enable_kademlia: bool,
    /// Network protocol version
    pub protocol_version: String,
}

/// Consensus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Consensus algorithm type
    pub algorithm: String,
    /// Block time in milliseconds
    pub block_time_ms: u64,
    /// Maximum block size in bytes
    pub max_block_size: usize,
    /// Transaction timeout in milliseconds
    pub transaction_timeout_ms: u64,
    /// Maximum transactions per block
    pub max_transactions_per_block: usize,
    /// Consensus round timeout in milliseconds
    pub round_timeout_ms: u64,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Data directory path
    pub data_dir: String,
    /// Database type (rocksdb, memory, etc.)
    pub database_type: String,
    /// Maximum database size in bytes
    pub max_db_size: u64,
    /// Enable WAL (Write-Ahead Logging)
    pub enable_wal: bool,
    /// Sync writes to disk
    pub sync_writes: bool,
    /// Cache size in bytes
    pub cache_size: usize,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,
    /// Log format (json, pretty, compact)
    pub format: String,
    /// Log to file
    pub log_to_file: bool,
    /// Log file path
    pub log_file_path: Option<String>,
    /// Maximum log file size in MB
    pub max_log_file_size_mb: u64,
    /// Number of log files to keep
    pub max_log_files: usize,
}

impl Default for CatalystConfig {
    fn default() -> Self {
        Self {
            network: NetworkConfig::default(),
            consensus: ConsensusConfig::default(),
            storage: StorageConfig::default(),
            logging: LoggingConfig::default(),
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/0".to_string()],
            bootstrap_peers: Vec::new(),
            max_connections: 50,
            connection_timeout_ms: 10_000,
            enable_mdns: true,
            enable_kademlia: true,
            protocol_version: "catalyst/1.0.0".to_string(),
        }
    }
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            algorithm: "catalyst-consensus".to_string(),
            block_time_ms: 5_000, // 5 seconds
            max_block_size: 1_048_576, // 1MB
            transaction_timeout_ms: 30_000, // 30 seconds
            max_transactions_per_block: 1000,
            round_timeout_ms: 10_000, // 10 seconds
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: "./catalyst-data".to_string(),
            database_type: "rocksdb".to_string(),
            max_db_size: 10_737_418_240, // 10GB
            enable_wal: true,
            sync_writes: false,
            cache_size: 268_435_456, // 256MB
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            format: "pretty".to_string(),
            log_to_file: false,
            log_file_path: None,
            max_log_file_size_mb: 100,
            max_log_files: 5,
        }
    }
}

impl CatalystConfig {
    /// Load configuration from a TOML file
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: CatalystConfig = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    /// Save configuration to a TOML file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Load configuration from environment variables
    pub fn load_from_env() -> Result<Self, ConfigError> {
        let mut config = CatalystConfig::default();
        
        // Network configuration from environment
        if let Ok(listen_addr) = std::env::var("CATALYST_LISTEN_ADDRESS") {
            config.network.listen_addresses = vec![listen_addr];
        }
        
        if let Ok(bootstrap_peers) = std::env::var("CATALYST_BOOTSTRAP_PEERS") {
            config.network.bootstrap_peers = bootstrap_peers
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();
        }
        
        if let Ok(max_connections) = std::env::var("CATALYST_MAX_CONNECTIONS") {
            config.network.max_connections = max_connections.parse()
                .map_err(|e| ConfigError::Validation(format!("Invalid max_connections: {}", e)))?;
        }
        
        // Storage configuration from environment
        if let Ok(data_dir) = std::env::var("CATALYST_DATA_DIR") {
            config.storage.data_dir = data_dir;
        }
        
        if let Ok(db_type) = std::env::var("CATALYST_DATABASE_TYPE") {
            config.storage.database_type = db_type;
        }
        
        // Logging configuration from environment
        if let Ok(log_level) = std::env::var("CATALYST_LOG_LEVEL") {
            config.logging.level = log_level;
        }
        
        if let Ok(log_file) = std::env::var("CATALYST_LOG_FILE") {
            config.logging.log_to_file = true;
            config.logging.log_file_path = Some(log_file);
        }
        
        config.validate()?;
        Ok(config)
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate network configuration
        if self.network.listen_addresses.is_empty() {
            return Err(ConfigError::Validation(
                "At least one listen address is required".to_string()
            ));
        }
        
        if self.network.max_connections == 0 {
            return Err(ConfigError::Validation(
                "max_connections must be greater than 0".to_string()
            ));
        }
        
        // Validate consensus configuration
        if self.consensus.block_time_ms == 0 {
            return Err(ConfigError::Validation(
                "block_time_ms must be greater than 0".to_string()
            ));
        }
        
        if self.consensus.max_block_size == 0 {
            return Err(ConfigError::Validation(
                "max_block_size must be greater than 0".to_string()
            ));
        }
        
        // Validate storage configuration
        if self.storage.data_dir.is_empty() {
            return Err(ConfigError::Validation(
                "data_dir cannot be empty".to_string()
            ));
        }
        
        // Validate logging configuration
        let valid_log_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_log_levels.contains(&self.logging.level.as_str()) {
            return Err(ConfigError::Validation(
                format!("Invalid log level: {}. Must be one of: {:?}", self.logging.level, valid_log_levels)
            ));
        }
        
        let valid_log_formats = ["json", "pretty", "compact"];
        if !valid_log_formats.contains(&self.logging.format.as_str()) {
            return Err(ConfigError::Validation(
                format!("Invalid log format: {}. Must be one of: {:?}", self.logging.format, valid_log_formats)
            ));
        }
        
        Ok(())
    }

    /// Merge configuration with another configuration (other takes precedence)
    pub fn merge(mut self, other: CatalystConfig) -> Self {
        // For simplicity, just replace entire sections
        // In a more sophisticated implementation, you might merge individual fields
        self.network = other.network;
        self.consensus = other.consensus;
        self.storage = other.storage;
        self.logging = other.logging;
        self
    }

    /// Get configuration as JSON string
    pub fn to_json(&self) -> Result<String, ConfigError> {
        let json = serde_json::to_string_pretty(self)?;
        Ok(json)
    }

    /// Load configuration from JSON string
    pub fn from_json(json: &str) -> Result<Self, ConfigError> {
        let config: CatalystConfig = serde_json::from_str(json)?;
        config.validate()?;
        Ok(config)
    }
}

/// Builder pattern for configuration
pub struct ConfigBuilder {
    config: CatalystConfig,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: CatalystConfig::default(),
        }
    }

    pub fn network(mut self, network: NetworkConfig) -> Self {
        self.config.network = network;
        self
    }

    pub fn consensus(mut self, consensus: ConsensusConfig) -> Self {
        self.config.consensus = consensus;
        self
    }

    pub fn storage(mut self, storage: StorageConfig) -> Self {
        self.config.storage = storage;
        self
    }

    pub fn logging(mut self, logging: LoggingConfig) -> Self {
        self.config.logging = logging;
        self
    }

    pub fn build(self) -> Result<CatalystConfig, ConfigError> {
        self.config.validate()?;
        Ok(self.config)
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_default_config() {
        let config = CatalystConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation() {
        let mut config = CatalystConfig::default();
        
        // Test invalid max_connections
        config.network.max_connections = 0;
        assert!(config.validate().is_err());
        
        // Reset and test invalid log level
        config = CatalystConfig::default();
        config.logging.level = "invalid".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_builder() {
        let config = ConfigBuilder::new()
            .network(NetworkConfig {
                listen_addresses: vec!["/ip4/127.0.0.1/tcp/8000".to_string()],
                ..Default::default()
            })
            .build()
            .unwrap();
        
        assert_eq!(config.network.listen_addresses[0], "/ip4/127.0.0.1/tcp/8000");
    }

    #[test]
    fn test_save_and_load_config() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_config.toml");
        
        let original_config = CatalystConfig::default();
        original_config.save_to_file(&file_path).unwrap();
        
        let loaded_config = CatalystConfig::load_from_file(&file_path).unwrap();
        
        assert_eq!(original_config.network.max_connections, loaded_config.network.max_connections);
        assert_eq!(original_config.consensus.block_time_ms, loaded_config.consensus.block_time_ms);
    }

    #[test]
    fn test_json_serialization() {
        let config = CatalystConfig::default();
        let json = config.to_json().unwrap();
        let parsed_config = CatalystConfig::from_json(&json).unwrap();
        
        assert_eq!(config.network.max_connections, parsed_config.network.max_connections);
    }
}