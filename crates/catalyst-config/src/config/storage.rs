use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::error::{ConfigError, ConfigResult};

/// Storage configuration for local state management and distributed file system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Database configuration
    pub database: DatabaseConfig,
    
    /// State management configuration
    pub state_management: StateManagementConfig,
    
    /// Distributed file system configuration
    pub dfs: DfsConfig,
    
    /// Cache configuration
    pub cache: CacheConfig,
    
    /// Backup and recovery configuration
    pub backup: BackupConfig,
    
    /// Storage security configuration
    pub security: StorageSecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Database type
    pub db_type: DatabaseType,
    
    /// Data directory path
    pub data_directory: PathBuf,
    
    /// Maximum database size in bytes
    pub max_db_size_bytes: Option<u64>,
    
    /// Write buffer size in bytes
    pub write_buffer_size: usize,
    
    /// Block cache size in bytes
    pub block_cache_size: usize,
    
    /// Number of background threads
    pub background_threads: usize,
    
    /// Enable compression
    pub compression_enabled: bool,
    
    /// Compression algorithm
    pub compression_type: CompressionType,
    
    /// RocksDB specific settings
    pub rocksdb: RocksDbConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DatabaseType {
    /// RocksDB (recommended for production)
    RocksDb,
    /// In-memory database (for testing)
    Memory,
    /// SQLite (for development)
    Sqlite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionType {
    None,
    Snappy,
    Lz4,
    Zstd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocksDbConfig {
    /// Enable WAL (Write-Ahead Logging)
    pub wal_enabled: bool,
    
    /// WAL size limit in bytes
    pub wal_size_limit: u64,
    
    /// Maximum number of write buffers
    pub max_write_buffer_number: usize,
    
    /// Target file size for compaction
    pub target_file_size_base: u64,
    
    /// Enable bloom filter
    pub bloom_filter_enabled: bool,
    
    /// Bloom filter bits per key
    pub bloom_filter_bits_per_key: i32,
    
    /// Level compaction dynamic level bytes
    pub level_compaction_dynamic_level_bytes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateManagementConfig {
    /// Three-level ledger structure configuration
    pub ledger_structure: LedgerStructureConfig,
    
    /// State partitioning configuration
    pub partitioning: PartitioningConfig,
    
    /// State synchronization settings
    pub synchronization: StateSyncConfig,
    
    /// Merkle tree configuration for state verification
    pub merkle_tree: MerkleTreeConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerStructureConfig {
    /// Current Ledger State (CLS) retention in memory
    pub cls_memory_size: usize,
    
    /// Number of recent updates to keep in fast storage
    pub recent_updates_count: usize,
    
    /// Threshold for moving updates to DFS (age in cycles)
    pub historical_threshold_cycles: u64,
    
    /// State snapshot interval in cycles
    pub snapshot_interval_cycles: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitioningConfig {
    /// Enable account type partitioning
    pub account_type_partitioning: bool,
    
    /// Non-confidential account partition size
    pub non_confidential_partition_size: usize,
    
    /// Confidential account partition size
    pub confidential_partition_size: usize,
    
    /// Smart contract partition size
    pub contract_partition_size: usize,
    
    /// Enable geographic partitioning
    pub geographic_partitioning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSyncConfig {
    /// State synchronization batch size
    pub sync_batch_size: usize,
    
    /// Maximum concurrent sync operations
    pub max_concurrent_syncs: usize,
    
    /// Sync timeout in milliseconds
    pub sync_timeout_ms: u64,
    
    /// Enable incremental state sync
    pub incremental_sync: bool,
    
    /// State diff compression
    pub diff_compression: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleTreeConfig {
    /// Merkle tree branching factor
    pub branching_factor: usize,
    
    /// Tree cache size
    pub cache_size: usize,
    
    /// Enable tree pruning
    pub pruning_enabled: bool,
    
    /// Pruning threshold in cycles
    pub pruning_threshold_cycles: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfsConfig {
    /// Enable distributed file system
    pub enabled: bool,
    
    /// DFS implementation type
    pub dfs_type: DfsType,
    
    /// Local DFS cache directory
    pub cache_directory: PathBuf,
    
    /// Maximum local cache size in bytes
    pub max_cache_size_bytes: u64,
    
    /// File replication factor
    pub replication_factor: usize,
    
    /// IPFS specific configuration
    pub ipfs: IpfsConfig,
    
    /// Content addressing configuration
    pub content_addressing: ContentAddressingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DfsType {
    /// IPFS integration
    Ipfs,
    /// Custom Catalyst DFS
    CatalystDfs,
    /// Local filesystem (for development)
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpfsConfig {
    /// IPFS API endpoint
    pub api_endpoint: String,
    
    /// IPFS gateway endpoint
    pub gateway_endpoint: String,
    
    /// Enable IPFS clustering
    pub clustering_enabled: bool,
    
    /// IPFS swarm key for private networks
    pub swarm_key: Option<String>,
    
    /// Pin important content locally
    pub pin_important_content: bool,
    
    /// Garbage collection settings
    pub gc_settings: IpfsGcConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpfsGcConfig {
    /// Enable automatic garbage collection
    pub auto_gc_enabled: bool,
    
    /// GC interval in seconds
    pub gc_interval_seconds: u64,
    
    /// Storage threshold for triggering GC (percentage)
    pub storage_threshold_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentAddressingConfig {
    /// Hash function for content addressing
    pub hash_function: String,
    
    /// Chunk size for large files
    pub chunk_size: usize,
    
    /// Enable content deduplication
    pub deduplication_enabled: bool,
    
    /// Content encryption for sensitive data
    pub encryption_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Overall cache configuration
    pub enabled: bool,
    
    /// Cache type
    pub cache_type: CacheType,
    
    /// Maximum cache size in bytes
    pub max_cache_size_bytes: u64,
    
    /// Cache eviction policy
    pub eviction_policy: EvictionPolicy,
    
    /// Cache TTL in seconds
    pub ttl_seconds: u64,
    
    /// Cache layers configuration
    pub layers: CacheLayersConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CacheType {
    /// In-memory cache
    Memory,
    /// Redis cache
    Redis,
    /// Hybrid memory + disk cache
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionPolicy {
    /// Least Recently Used
    Lru,
    /// Least Frequently Used
    Lfu,
    /// Time-based expiration
    Ttl,
    /// First In, First Out
    Fifo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheLayersConfig {
    /// L1 cache size (hot data)
    pub l1_size_bytes: u64,
    
    /// L2 cache size (warm data)
    pub l2_size_bytes: u64,
    
    /// L3 cache size (cold data)
    pub l3_size_bytes: u64,
    
    /// Cache promotion thresholds
    pub promotion_threshold: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    /// Enable automatic backups
    pub enabled: bool,
    
    /// Backup interval in seconds
    pub backup_interval_seconds: u64,
    
    /// Backup directory
    pub backup_directory: PathBuf,
    
    /// Number of backups to retain
    pub retention_count: usize,
    
    /// Enable backup compression
    pub compression_enabled: bool,
    
    /// Enable backup encryption
    pub encryption_enabled: bool,
    
    /// Remote backup settings
    pub remote_backup: Option<RemoteBackupConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteBackupConfig {
    /// Remote backup provider
    pub provider: RemoteBackupProvider,
    
    /// Backup endpoint
    pub endpoint: String,
    
    /// Authentication credentials
    pub credentials: String,
    
    /// Backup frequency
    pub frequency_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RemoteBackupProvider {
    S3,
    GoogleCloud,
    Azure,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageSecurityConfig {
    /// Enable storage encryption at rest
    pub encryption_at_rest: bool,
    
    /// Encryption algorithm
    pub encryption_algorithm: String,
    
    /// Key rotation interval in seconds
    pub key_rotation_interval_seconds: Option<u64>,
    
    /// Enable data integrity checks
    pub integrity_checks: bool,
    
    /// Secure deletion of sensitive data
    pub secure_deletion: bool,
    
    /// Access control settings
    pub access_control: AccessControlConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccessControlConfig {
    /// Enable access control
    pub enabled: bool,
    
    /// File permissions mode
    pub file_permissions: u32,
    
    /// Directory permissions mode
    pub directory_permissions: u32,
    
    /// User and group settings
    pub user_group: Option<UserGroupConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserGroupConfig {
    /// Run as specific user
    pub user: Option<String>,
    
    /// Run as specific group
    pub group: Option<String>,
}

impl StorageConfig {
    /// Create development storage configuration
    pub fn devnet() -> Self {
        Self {
            database: DatabaseConfig {
                db_type: DatabaseType::RocksDb,
                data_directory: PathBuf::from("./data/devnet"),
                max_db_size_bytes: Some(1024 * 1024 * 1024), // 1GB
                write_buffer_size: 64 * 1024 * 1024, // 64MB
                block_cache_size: 128 * 1024 * 1024, // 128MB
                background_threads: 2,
                compression_enabled: false,
                compression_type: CompressionType::None,
                rocksdb: RocksDbConfig {
                    wal_enabled: true,
                    wal_size_limit: 64 * 1024 * 1024, // 64MB
                    max_write_buffer_number: 3,
                    target_file_size_base: 64 * 1024 * 1024, // 64MB
                    bloom_filter_enabled: false,
                    bloom_filter_bits_per_key: 10,
                    level_compaction_dynamic_level_bytes: false,
                },
            },
            state_management: StateManagementConfig {
                ledger_structure: LedgerStructureConfig {
                    cls_memory_size: 10 * 1024 * 1024, // 10MB
                    recent_updates_count: 100,
                    historical_threshold_cycles: 1000,
                    snapshot_interval_cycles: 100,
                },
                partitioning: PartitioningConfig {
                    account_type_partitioning: true,
                    non_confidential_partition_size: 1000,
                    confidential_partition_size: 1000,
                    contract_partition_size: 1000,
                    geographic_partitioning: false,
                },
                synchronization: StateSyncConfig {
                    sync_batch_size: 100,
                    max_concurrent_syncs: 5,
                    sync_timeout_ms: 5000,
                    incremental_sync: true,
                    diff_compression: false,
                },
                merkle_tree: MerkleTreeConfig {
                    branching_factor: 16,
                    cache_size: 1000,
                    pruning_enabled: false,
                    pruning_threshold_cycles: 1000,
                },
            },
            dfs: DfsConfig {
                enabled: false, // Disabled for development simplicity
                dfs_type: DfsType::Local,
                cache_directory: PathBuf::from("./data/devnet/dfs"),
                max_cache_size_bytes: 100 * 1024 * 1024, // 100MB
                replication_factor: 1,
                ipfs: IpfsConfig {
                    api_endpoint: "http://127.0.0.1:5001".to_string(),
                    gateway_endpoint: "http://127.0.0.1:8080".to_string(),
                    clustering_enabled: false,
                    swarm_key: None,
                    pin_important_content: true,
                    gc_settings: IpfsGcConfig {
                        auto_gc_enabled: false,
                        gc_interval_seconds: 3600,
                        storage_threshold_percent: 90.0,
                    },
                },
                content_addressing: ContentAddressingConfig {
                    hash_function: "blake2b-256".to_string(),
                    chunk_size: 256 * 1024, // 256KB
                    deduplication_enabled: true,
                    encryption_enabled: false,
                },
            },
            cache: CacheConfig {
                enabled: true,
                cache_type: CacheType::Memory,
                max_cache_size_bytes: 50 * 1024 * 1024, // 50MB
                eviction_policy: EvictionPolicy::Lru,
                ttl_seconds: 3600,
                layers: CacheLayersConfig {
                    l1_size_bytes: 10 * 1024 * 1024, // 10MB
                    l2_size_bytes: 20 * 1024 * 1024, // 20MB
                    l3_size_bytes: 20 * 1024 * 1024, // 20MB
                    promotion_threshold: 0.8,
                },
            },
            backup: BackupConfig {
                enabled: false,
                backup_interval_seconds: 3600,
                backup_directory: PathBuf::from("./backups/devnet"),
                retention_count: 5,
                compression_enabled: false,
                encryption_enabled: false,
                remote_backup: None,
            },
            security: StorageSecurityConfig {
                encryption_at_rest: false,
                encryption_algorithm: "AES-256-GCM".to_string(),
                key_rotation_interval_seconds: None,
                integrity_checks: false,
                secure_deletion: false,
                access_control: AccessControlConfig {
                    enabled: false,
                    file_permissions: 0o644,
                    directory_permissions: 0o755,
                    user_group: None,
                },
            },
        }
    }
    
    /// Create test network storage configuration
    pub fn testnet() -> Self {
        let mut config = Self::devnet();
        
        // Override testnet-specific settings
        config.database.data_directory = PathBuf::from("./data/testnet");
        config.database.max_db_size_bytes = Some(10 * 1024 * 1024 * 1024); // 10GB
        config.database.write_buffer_size = 128 * 1024 * 1024; // 128MB
        config.database.block_cache_size = 256 * 1024 * 1024; // 256MB
        config.database.background_threads = 4;
        config.database.compression_enabled = true;
        config.database.compression_type = CompressionType::Snappy;
        
        config.dfs.enabled = true;
        config.dfs.dfs_type = DfsType::Ipfs;
        config.dfs.cache_directory = PathBuf::from("./data/testnet/dfs");
        config.dfs.max_cache_size_bytes = 1024 * 1024 * 1024; // 1GB
        config.dfs.replication_factor = 3;
        
        config.cache.max_cache_size_bytes = 200 * 1024 * 1024; // 200MB
        
        config.backup.enabled = true;
        config.backup.backup_directory = PathBuf::from("./backups/testnet");
        
        config.security.encryption_at_rest = true;
        config.security.integrity_checks = true;
        
        config
    }
    
    /// Create main network storage configuration (production)
    pub fn mainnet() -> Self {
        let mut config = Self::testnet();
        
        // Override mainnet-specific settings
        config.database.data_directory = PathBuf::from("./data/mainnet");
        config.database.max_db_size_bytes = Some(100 * 1024 * 1024 * 1024); // 100GB
        config.database.write_buffer_size = 256 * 1024 * 1024; // 256MB
        config.database.block_cache_size = 512 * 1024 * 1024; // 512MB
        config.database.background_threads = 8;
        config.database.compression_type = CompressionType::Zstd;
        
        config.database.rocksdb.bloom_filter_enabled = true;
        config.database.rocksdb.level_compaction_dynamic_level_bytes = true;
        
        config.dfs.cache_directory = PathBuf::from("./data/mainnet/dfs");
        config.dfs.max_cache_size_bytes = 10 * 1024 * 1024 * 1024; // 10GB
        config.dfs.replication_factor = 5;
        
        config.cache.cache_type = CacheType::Hybrid;
        config.cache.max_cache_size_bytes = 1024 * 1024 * 1024; // 1GB
        
        config.backup.backup_directory = PathBuf::from("./backups/mainnet");
        config.backup.compression_enabled = true;
        config.backup.encryption_enabled = true;
        
        config.security.key_rotation_interval_seconds = Some(86400); // 24 hours
        config.security.secure_deletion = true;
        config.security.access_control.enabled = true;
        
        config
    }
    
    /// Validate storage configuration
    pub fn validate(&self) -> ConfigResult<()> {
        // Validate data directory
        if self.database.data_directory.as_os_str().is_empty() {
            return Err(ConfigError::ValidationFailed("Data directory cannot be empty".to_string()));
        }
        
        // Validate cache sizes
        if self.database.write_buffer_size == 0 {
            return Err(ConfigError::ValidationFailed("Write buffer size must be greater than 0".to_string()));
        }
        
        if self.database.block_cache_size == 0 {
            return Err(ConfigError::ValidationFailed("Block cache size must be greater than 0".to_string()));
        }
        
        // Validate DFS settings
        if self.dfs.enabled {
            if self.dfs.replication_factor == 0 {
                return Err(ConfigError::ValidationFailed("DFS replication factor must be greater than 0".to_string()));
            }
            
            if self.dfs.max_cache_size_bytes == 0 {
                return Err(ConfigError::ValidationFailed("DFS cache size must be greater than 0".to_string()));
            }
        }
        
        // Validate cache configuration
        if self.cache.enabled {
            if self.cache.max_cache_size_bytes == 0 {
                return Err(ConfigError::ValidationFailed("Cache size must be greater than 0".to_string()));
            }
            
            let total_layer_size = self.cache.layers.l1_size_bytes + 
                                  self.cache.layers.l2_size_bytes + 
                                  self.cache.layers.l3_size_bytes;
            
            if total_layer_size > self.cache.max_cache_size_bytes {
                return Err(ConfigError::ValidationFailed("Sum of cache layer sizes exceeds max cache size".to_string()));
            }
        }
        
        // Validate backup settings
        if self.backup.enabled {
            if self.backup.retention_count == 0 {
                return Err(ConfigError::ValidationFailed("Backup retention count must be greater than 0".to_string()));
            }
        }
        
        Ok(())
    }
}