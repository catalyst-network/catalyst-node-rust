use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use anyhow::Result;

/// Complete node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Protocol / chain identity configuration
    pub protocol: ProtocolConfig,
    
    /// Node identity and role
    pub node: NodeIdentityConfig,
    
    /// Network configuration
    pub network: NetworkConfig,
    
    /// Storage configuration
    pub storage: StorageConfig,
    
    /// Consensus configuration
    pub consensus: ConsensusConfig,
    
    /// Runtime configurations
    pub runtimes: RuntimeConfig,
    
    /// Service bus configuration
    pub service_bus: ServiceBusConfig,
    
    /// Distributed file system configuration
    pub dfs: DfsConfig,
    
    /// RPC server configuration
    pub rpc: RpcConfig,
    
    /// Logging configuration
    pub logging: LoggingConfig,
    
    /// Whether to run as validator
    pub validator: bool,
}

/// Protocol and chain identity configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    /// Stable chain id (used for EVM domain separation and client tooling).
    pub chain_id: u64,
    /// Human-readable network id (e.g. "tna_testnet", "local", "mainnet").
    pub network_id: String,

    /// Faucet funding mode for genesis initialization (dev/test convenience).
    ///
    /// - `deterministic`: use the built-in deterministic dev faucet key (`[0xFA; 32]`)
    /// - `configured`: use `faucet_pubkey_hex` and `faucet_balance`
    /// - `disabled`: do not fund any faucet account at genesis
    #[serde(default)]
    pub faucet_mode: FaucetMode,

    /// If false, the node will refuse to use the deterministic faucet key for genesis funding.
    ///
    /// Set this to false for any public network.
    #[serde(default = "default_allow_deterministic_faucet")]
    pub allow_deterministic_faucet: bool,

    /// Faucet pubkey (32 bytes hex) used when `faucet_mode = "configured"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub faucet_pubkey_hex: Option<String>,

    /// Faucet initial balance used when `faucet_mode != "disabled"`.
    #[serde(default = "default_faucet_balance")]
    pub faucet_balance: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FaucetMode {
    Deterministic,
    Configured,
    Disabled,
}

impl Default for FaucetMode {
    fn default() -> Self {
        FaucetMode::Deterministic
    }
}

fn default_allow_deterministic_faucet() -> bool {
    true
}

fn default_faucet_balance() -> i64 {
    1_000_000
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

    /// Optional DNS seed domains (TXT/A/AAAA) used to discover bootstrap peers at runtime.
    ///
    /// TXT records may contain multiaddrs (preferred), IPs, or host:port entries.
    /// Example TXT values:
    /// - `/ip4/45.32.177.248/tcp/30333`
    /// - `45.32.177.248:30333`
    /// - `45.32.177.248`
    #[serde(default)]
    pub dns_seeds: Vec<String>,
    
    /// Maximum number of peers to connect to
    pub max_peers: u32,
    
    /// Minimum number of peers to maintain
    pub min_peers: u32,
    
    /// Network protocol version
    pub protocol_version: String,
    
    /// Enable MDNS peer discovery
    pub mdns_discovery: bool,
    
    /// Enable DHT for peer discovery
    pub dht_enabled: bool,
    
    /// Network timeouts
    pub timeouts: NetworkTimeouts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkTimeouts {
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    
    /// Request timeout in seconds
    pub request_timeout: u64,
    
    /// Keep-alive interval in seconds
    pub keep_alive_interval: u64,
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
    
    /// Database cache size in MB
    pub cache_size_mb: u32,
    
    /// Database write buffer size in MB
    pub write_buffer_size_mb: u32,
    
    /// Maximum number of open files
    pub max_open_files: i32,
    
    /// Enable database compression
    pub compression_enabled: bool,

    /// Enable pruning of historical RPC/indexer metadata (blocks/tx history).
    ///
    /// When enabled, the node will delete old `metadata` keys for cycles older than
    /// `history_keep_cycles` behind the applied head. Authenticated account state is not affected.
    #[serde(default)]
    pub history_prune_enabled: bool,

    /// Number of cycles (seconds) of history to retain behind head when pruning is enabled.
    ///
    /// `0` means "keep all history" (no pruning), even if `history_prune_enabled=true`.
    #[serde(default = "default_history_keep_cycles")]
    pub history_keep_cycles: u64,

    /// Minimum time between prune runs.
    #[serde(default = "default_history_prune_interval_seconds")]
    pub history_prune_interval_seconds: u64,

    /// Maximum number of cycles to prune per run (bounds runtime overhead).
    #[serde(default = "default_history_prune_batch_cycles")]
    pub history_prune_batch_cycles: u64,
}

fn default_history_keep_cycles() -> u64 {
    // Default retention window for pruned nodes: 7 days at 1 cycle/sec.
    7 * 24 * 60 * 60
}

fn default_history_prune_interval_seconds() -> u64 {
    300
}

fn default_history_prune_batch_cycles() -> u64 {
    1_000
}

/// Consensus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Ledger cycle duration in seconds
    pub cycle_duration_seconds: u32,
    
    /// Transaction freeze time in seconds
    pub freeze_time_seconds: u32,
    
    /// Maximum transactions per block
    pub max_transactions_per_block: u32,
    
    /// Minimum producer count for consensus
    pub min_producer_count: u32,
    
    /// Maximum producer count for consensus
    pub max_producer_count: u32,
    
    /// Consensus timeouts for each phase
    pub phase_timeouts: PhaseTimeouts,

    /// Deterministic validator worker pool for producer selection (32-byte hex public keys).
    ///
    /// This is used to derive the producer set without relying on ad-hoc gossip discovery messages.
    /// For local testnet, it can be auto-populated from `testnet/validators.toml`.
    #[serde(default)]
    pub validator_worker_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseTimeouts {
    /// Construction phase timeout in seconds
    pub construction_timeout: u32,
    
    /// Campaigning phase timeout in seconds
    pub campaigning_timeout: u32,
    
    /// Voting phase timeout in seconds
    pub voting_timeout: u32,
    
    /// Synchronization phase timeout in seconds
    pub synchronization_timeout: u32,
}

/// Runtime configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// EVM runtime settings
    pub evm: EvmRuntimeConfig,
    
    /// SVM runtime settings (future)
    pub svm: SvmRuntimeConfig,
    
    /// WASM runtime settings (future)
    pub wasm: WasmRuntimeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvmRuntimeConfig {
    /// Enable EVM runtime
    pub enabled: bool,
    
    /// Gas limit per transaction
    pub gas_limit: u64,
    
    /// Gas price in Wei equivalent
    pub gas_price: u64,
    
    /// Maximum contract code size
    pub max_code_size: u32,
    
    /// Enable EVM debugging
    pub debug_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SvmRuntimeConfig {
    /// Enable SVM runtime
    pub enabled: bool,
    
    /// Compute unit limit per transaction
    pub compute_unit_limit: u64,
    
    /// Maximum account data size
    pub max_account_data_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmRuntimeConfig {
    /// Enable WASM runtime
    pub enabled: bool,
    
    /// Maximum memory pages
    pub max_memory_pages: u32,
    
    /// Execution timeout in milliseconds
    pub execution_timeout_ms: u64,
}

/// Service bus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceBusConfig {
    /// Enable service bus
    pub enabled: bool,
    
    /// WebSocket server address
    pub websocket_address: String,
    
    /// WebSocket server port
    pub websocket_port: u16,
    
    /// Maximum number of concurrent connections
    pub max_connections: u32,
    
    /// Event buffer size
    pub event_buffer_size: u32,
    
    /// Enable webhook support
    pub webhooks_enabled: bool,
    
    /// Webhook timeout in seconds
    pub webhook_timeout: u64,
    
    /// Maximum webhook retries
    pub webhook_max_retries: u32,
}

/// Distributed file system configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfsConfig {
    /// Enable DFS
    pub enabled: bool,
    
    /// IPFS API endpoint
    pub ipfs_api_url: String,
    
    /// IPFS gateway URL
    pub ipfs_gateway_url: String,
    
    /// Local cache directory
    pub cache_dir: PathBuf,
    
    /// Cache size limit in GB
    pub cache_size_gb: u64,
    
    /// File pinning enabled
    pub pinning_enabled: bool,
    
    /// Automatic garbage collection enabled
    pub gc_enabled: bool,
    
    /// Garbage collection interval in hours
    pub gc_interval_hours: u64,
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
    
    /// Enable authentication
    pub auth_enabled: bool,
    
    /// API key for authentication
    pub api_key: Option<String>,
    
    /// Rate limiting (requests per minute)
    pub rate_limit: u32,
    
    /// Request timeout in seconds
    pub request_timeout: u64,
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
    
    /// Maximum log file size in MB
    pub max_file_size_mb: u64,
    
    /// Number of log files to keep
    pub max_files: u32,
    
    /// Log to console
    pub console_enabled: bool,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            protocol: ProtocolConfig {
                chain_id: 31337,
                network_id: "tna_testnet".to_string(),
                faucet_mode: FaucetMode::Deterministic,
                allow_deterministic_faucet: true,
                faucet_pubkey_hex: None,
                faucet_balance: default_faucet_balance(),
            },
            node: NodeIdentityConfig {
                name: "catalyst-node".to_string(),
                private_key_file: PathBuf::from("node.key"),
                auto_generate_identity: true,
            },
            network: NetworkConfig {
                listen_addresses: vec![
                    "/ip4/0.0.0.0/tcp/30333".to_string(),
                    "/ip6/::/tcp/30333".to_string(),
                ],
                bootstrap_peers: vec![],
                dns_seeds: vec![],
                max_peers: 50,
                min_peers: 5,
                protocol_version: "catalyst/1.0".to_string(),
                mdns_discovery: true,
                dht_enabled: true,
                timeouts: NetworkTimeouts {
                    connection_timeout: 30,
                    request_timeout: 10,
                    keep_alive_interval: 30,
                },
            },
            storage: StorageConfig {
                data_dir: PathBuf::from("data"),
                enabled: false,
                capacity_gb: 10,
                cache_size_mb: 256,
                write_buffer_size_mb: 64,
                max_open_files: 1000,
                compression_enabled: true,
                history_prune_enabled: false,
                history_keep_cycles: default_history_keep_cycles(),
                history_prune_interval_seconds: default_history_prune_interval_seconds(),
                history_prune_batch_cycles: default_history_prune_batch_cycles(),
            },
            consensus: ConsensusConfig {
                cycle_duration_seconds: 60,
                freeze_time_seconds: 5,
                max_transactions_per_block: 1000,
                min_producer_count: 3,
                max_producer_count: 100,
                phase_timeouts: PhaseTimeouts {
                    construction_timeout: 15,
                    campaigning_timeout: 15,
                    voting_timeout: 15,
                    synchronization_timeout: 15,
                },
                validator_worker_ids: Vec::new(),
            },
            runtimes: RuntimeConfig {
                evm: EvmRuntimeConfig {
                    enabled: true,
                    gas_limit: 8_000_000,
                    gas_price: 1_000_000_000, // 1 Gwei equivalent
                    max_code_size: 24576, // 24KB
                    debug_enabled: false,
                },
                svm: SvmRuntimeConfig {
                    enabled: false,
                    compute_unit_limit: 200_000,
                    max_account_data_size: 10_485_760, // 10MB
                },
                wasm: WasmRuntimeConfig {
                    enabled: false,
                    max_memory_pages: 1024,
                    execution_timeout_ms: 5000,
                },
            },
            service_bus: ServiceBusConfig {
                enabled: true,
                websocket_address: "0.0.0.0".to_string(),
                websocket_port: 8546,
                max_connections: 1000,
                event_buffer_size: 10000,
                webhooks_enabled: true,
                webhook_timeout: 30,
                webhook_max_retries: 3,
            },
            dfs: DfsConfig {
                enabled: true,
                ipfs_api_url: "http://127.0.0.1:5001".to_string(),
                ipfs_gateway_url: "http://127.0.0.1:8080".to_string(),
                cache_dir: PathBuf::from("dfs_cache"),
                cache_size_gb: 5,
                pinning_enabled: true,
                gc_enabled: true,
                gc_interval_hours: 24,
            },
            rpc: RpcConfig {
                enabled: false,
                address: "127.0.0.1".to_string(),
                port: 8545,
                cors_enabled: true,
                cors_origins: vec!["*".to_string()],
                auth_enabled: false,
                api_key: None,
                rate_limit: 100,
                request_timeout: 30,
            },
            logging: LoggingConfig {
                level: "info".to_string(),
                format: "text".to_string(),
                file_enabled: true,
                file_path: PathBuf::from("logs/catalyst.log"),
                max_file_size_mb: 100,
                max_files: 10,
                console_enabled: true,
            },
            validator: false,
        }
    }
}

impl NodeConfig {
    /// Load configuration from file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path.as_ref())?;

        // Try JSON first (backwards compatible), then TOML (matches Makefile config.toml usage).
        if let Ok(config) = serde_json::from_str::<NodeConfig>(&content) {
            return Ok(config);
        }

        let config: NodeConfig = toml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse config as JSON or TOML: {e}"))?;
        Ok(config)
    }
    
    /// Save configuration to file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let content = match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase()
            .as_str()
        {
            "toml" => toml::to_string_pretty(self)?,
            _ => serde_json::to_string_pretty(self)?,
        };

        std::fs::write(path, content)?;
        Ok(())
    }

    /// Create a reasonable default config for a given config file path.
    ///
    /// Used by `catalyst-cli start --config some/path/config.toml` when the file doesn't exist.
    pub fn default_for_config_path(path: &Path) -> Self {
        let mut cfg = NodeConfig::default();

        // Put node state alongside the config file to avoid collisions in local testnets.
        if let Some(dir) = path.parent() {
            cfg.storage.data_dir = dir.join("data");
            cfg.dfs.cache_dir = dir.join("dfs_cache");
            cfg.logging.file_path = dir.join("logs").join("catalyst.log");
            cfg.node.private_key_file = dir.join("node.key");
        }

        // If the path contains `nodeN` (e.g. testnet/node2/config.toml), derive unique ports.
        if let Some(idx) = extract_node_index(path) {
            cfg.node.name = format!("catalyst-node-{}", idx);

            // P2P listen port: 30333 + (idx-1)
            let p2p_port = 30333u16.saturating_add((idx.saturating_sub(1)) as u16);
            cfg.network.listen_addresses = vec![
                format!("/ip4/0.0.0.0/tcp/{}", p2p_port),
                format!("/ip6/::/tcp/{}", p2p_port),
            ];

            // Service bus websocket port: 8546 + (idx-1)
            cfg.service_bus.websocket_port = 8546u16.saturating_add((idx.saturating_sub(1)) as u16);

            // Default RPC port too (CLI flags can override).
            cfg.rpc.port = 8545u16.saturating_add((idx.saturating_sub(1)) as u16);
        }

        cfg
    }
    
    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        if self.protocol.chain_id == 0 {
            return Err(anyhow::anyhow!("protocol.chain_id must be > 0"));
        }
        if self.protocol.network_id.trim().is_empty() {
            return Err(anyhow::anyhow!("protocol.network_id must be non-empty"));
        }

        // Faucet config validation
        match self.protocol.faucet_mode {
            FaucetMode::Deterministic => {
                if !self.protocol.allow_deterministic_faucet {
                    return Err(anyhow::anyhow!(
                        "protocol.allow_deterministic_faucet=false but protocol.faucet_mode=deterministic"
                    ));
                }
                if self.protocol.faucet_balance <= 0 {
                    return Err(anyhow::anyhow!("protocol.faucet_balance must be > 0"));
                }
            }
            FaucetMode::Configured => {
                let Some(pk) = self.protocol.faucet_pubkey_hex.as_ref() else {
                    return Err(anyhow::anyhow!(
                        "protocol.faucet_pubkey_hex must be set when protocol.faucet_mode=configured"
                    ));
                };
                // Basic format check: 32-byte hex (optionally 0x-prefixed)
                let s = pk.trim().strip_prefix("0x").unwrap_or(pk.trim());
                let bytes = hex::decode(s)
                    .map_err(|_| anyhow::anyhow!("protocol.faucet_pubkey_hex must be valid hex"))?;
                if bytes.len() != 32 {
                    return Err(anyhow::anyhow!(
                        "protocol.faucet_pubkey_hex must be 32 bytes (64 hex chars)"
                    ));
                }
                if self.protocol.faucet_balance <= 0 {
                    return Err(anyhow::anyhow!("protocol.faucet_balance must be > 0"));
                }
            }
            FaucetMode::Disabled => {}
        }

        // Validate network configuration
        if self.network.max_peers < self.network.min_peers {
            return Err(anyhow::anyhow!("max_peers must be >= min_peers"));
        }
        
        if self.network.listen_addresses.is_empty() {
            return Err(anyhow::anyhow!("At least one listen address must be specified"));
        }
        
        // Validate consensus configuration
        if self.consensus.cycle_duration_seconds < 10 {
            return Err(anyhow::anyhow!("Cycle duration must be at least 10 seconds"));
        }
        
        if self.consensus.min_producer_count < 1 {
            return Err(anyhow::anyhow!("Minimum producer count must be at least 1"));
        }
        
        if self.consensus.max_producer_count < self.consensus.min_producer_count {
            return Err(anyhow::anyhow!("Max producer count must be >= min producer count"));
        }
        
        // Validate storage configuration
        if self.storage.capacity_gb == 0 && self.storage.enabled {
            return Err(anyhow::anyhow!("Storage capacity must be > 0 when storage is enabled"));
        }
        if self.storage.history_prune_enabled {
            if self.storage.history_prune_interval_seconds == 0 {
                return Err(anyhow::anyhow!(
                    "storage.history_prune_interval_seconds must be > 0 when pruning is enabled"
                ));
            }
            if self.storage.history_prune_batch_cycles == 0 {
                return Err(anyhow::anyhow!(
                    "storage.history_prune_batch_cycles must be > 0 when pruning is enabled"
                ));
            }
        }
        
        // Validate RPC configuration
        if self.rpc.enabled && self.rpc.port == 0 {
            return Err(anyhow::anyhow!("RPC port must be specified when RPC is enabled"));
        }
        
        // Validate service bus configuration
        if self.service_bus.enabled && self.service_bus.websocket_port == 0 {
            return Err(anyhow::anyhow!("WebSocket port must be specified when service bus is enabled"));
        }
        
        Ok(())
    }
    
    /// Create data directory if it doesn't exist
    pub fn ensure_data_dir(&self) -> Result<()> {
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

fn extract_node_index(path: &Path) -> Option<usize> {
    // Find first occurrence of "node" followed by digits.
    let s = path.to_string_lossy();
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i + 4 <= bytes.len() {
        if &bytes[i..i + 4] == b"node" {
            let mut j = i + 4;
            let mut n: usize = 0;
            let mut saw_digit = false;
            while j < bytes.len() {
                let b = bytes[j];
                if (b'0'..=b'9').contains(&b) {
                    saw_digit = true;
                    n = n.saturating_mul(10).saturating_add((b - b'0') as usize);
                    j += 1;
                } else {
                    break;
                }
            }
            if saw_digit && n > 0 {
                return Some(n);
            }
        }
        i += 1;
    }
    None
}