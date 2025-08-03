//! Simplified configuration module for CLI
//! Replace or update crates/catalyst-cli/src/config.rs

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Complete node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node identity and role
    pub node: NodeIdentityConfig,
    
    /// Network configuration
    pub network: NetworkConfig,
    
    /// Storage configuration
    pub storage: StorageConfig,
    
    /// Consensus configuration
    pub consensus: ConsensusConfig,
    
    /// RPC server configuration
    pub rpc: RpcConfig,
    
    /// Service bus configuration
    pub service_bus: ServiceBusConfig,
    
    /// Distributed file system configuration
    pub dfs: DfsConfig,
    
    /// Logging configuration
    pub logging: LoggingConfig,
    
    /// Whether to run as validator
    pub validator: bool,
}

/// Node identity and basic settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIdentityConfig {
    /// Node name for identification
    pub name: String,
    
    /// Private key file path
    pub private_key_file: PathBuf,
    
    /// Auto-generate identity if key file doesn't exist
    pub auto_generate_identity: bool,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Listen addresses for P2P networking
    pub listen_addresses: Vec<String>,
    
    /// Bootstrap peer addresses
    pub bootstrap_peers: Vec<String>,
    
    /// Maximum number of peers to connect to
    pub max_peers: u32,
    
    /// Minimum number of peers to maintain
    pub min_peers: u32,
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Data directory for local storage
    pub data_dir: PathBuf,
    
    /// Enable storage provision to network
    pub enabled: bool,
    
    /// Storage capacity to provide in GB
    pub capacity_gb: u64,
}

/// Consensus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Ledger cycle duration in seconds
    pub cycle_duration_seconds: u32,
    
    /// Maximum transactions per block
    pub max_transactions_per_block: u32,
    
    /// Minimum producer count for consensus
    pub min_producer_count: u32,
    
    /// Maximum producer count for consensus
    pub max_producer_count: u32,
}

/// RPC server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    /// Enable RPC server
    pub enabled: bool,
    
    /// RPC server address
    pub address: String,
    
    /// RPC server port
    pub port: u16,
    
    /// Enable CORS
    pub cors_enabled: bool,
    
    /// Allowed CORS origins
    pub cors_origins: Vec<String>,
}

/// Service bus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceBusConfig {
    /// Enable service bus
    pub enabled: bool,
    
    /// WebSocket server port
    pub websocket_port: u16,
    
    /// Maximum number of concurrent connections
    pub max_connections: u32,
}

/// Distributed file system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfsConfig {
    /// Enable DFS
    pub enabled: bool,
    
    /// Local cache directory
    pub cache_dir: PathBuf,
    
    /// Cache size limit in GB
    pub cache_size_gb: u64,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: String,
    
    /// Log format (text, json)
    pub format: String,
    
    /// Log to file
    pub file_enabled: bool,
    
    /// Log file path
    pub file_path: PathBuf,
    
    /// Log to console
    pub console_enabled: bool,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node: NodeIdentityConfig {
                name: "catalyst-node".to_string(),
                private_key_file: PathBuf::from("node.key"),
                auto_generate_identity: true,
            },
            network: NetworkConfig {
                listen_addresses: vec![
                    "/ip4/0.0.0.0/tcp/30333".to_string(),
                ],
                bootstrap_peers: vec![],
                max_peers: 50,
                min_peers: 5,
            },
            storage: StorageConfig {
                data_dir: PathBuf::from("data"),
                enabled: false,
                capacity_gb: 10,
            },
            consensus: ConsensusConfig {
                cycle_duration_seconds: 60,
                max_transactions_per_block: 1000,
                min_producer_count: 3,
                max_producer_count: 100,
            },
            rpc: RpcConfig {
                enabled: false,
                address: "127.0.0.1".to_string(),
                port: 9933,
                cors_enabled: true,
                cors_origins: vec!["*".to_string()],
            },
            service_bus: ServiceBusConfig {
                enabled: false,
                websocket_port: 8546,
                max_connections: 1000,
            },
            dfs: DfsConfig {
                enabled: false,
                cache_dir: PathBuf::from("dfs_cache"),
                cache_size_gb: 5,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "text".to_string(),
                file_enabled: true,
                file_path: PathBuf::from("logs/catalyst.log"),
                console_enabled: true,
            },
            validator: false,
        }
    }
}

impl NodeConfig {
    /// Load configuration from file
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: NodeConfig = toml::from_str(&content)?;
        Ok(config)
    }
    
    /// Save configuration to file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
    
    /// Validate configuration
    pub fn validate(&self) -> anyhow::Result<()> {
        // Basic validation
        if self.network.max_peers < self.network.min_peers {
            anyhow::bail!("max_peers must be >= min_peers");
        }
        
        if self.consensus.cycle_duration_seconds < 10 {
            anyhow::bail!("Cycle duration must be at least 10 seconds");
        }
        
        if self.rpc.enabled && self.rpc.port == 0 {
            anyhow::bail!("RPC port must be specified when RPC is enabled");
        }
        
        Ok(())
    }
    
    /// Create data directory if it doesn't exist
    pub fn ensure_data_dir(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.storage.data_dir)?;
        
        if self.dfs.enabled {
            std::fs::create_dir_all(&self.dfs.cache_dir)?;
        }
        
        if self.logging.file_enabled {
            if let Some(parent) = self.logging.file_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
        }
        
        Ok(())
    }
}