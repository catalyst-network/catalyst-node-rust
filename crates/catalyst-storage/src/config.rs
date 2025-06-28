//! Configuration for the storage layer

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::collections::HashMap;

/// Configuration for the storage manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Directory to store database files
    pub data_dir: PathBuf,
    
    /// Maximum number of open files
    pub max_open_files: i32,
    
    /// Maximum size of WAL files in bytes
    pub max_wal_size: u64,
    
    /// Maximum size of manifest files in bytes
    pub max_manifest_size: u64,
    
    /// Maximum number of background compaction threads
    pub max_background_compactions: i32,
    
    /// Maximum number of background flush threads
    pub max_background_flushes: i32,
    
    /// Size of write buffer in bytes
    pub write_buffer_size: usize,
    
    /// Maximum number of write buffers
    pub max_write_buffer_number: i32,
    
    /// Target file size for compaction
    pub target_file_size_base: u64,
    
    /// Size of block cache in bytes
    pub block_cache_size: usize,
    
    /// Size of row cache in bytes
    pub row_cache_size: usize,
    
    /// Enable compression
    pub compression_enabled: bool,
    
    /// Compression type
    pub compression_type: CompressionType,
    
    /// Enable statistics collection
    pub enable_statistics: bool,
    
    /// Enable automatic compaction
    pub auto_compaction: bool,
    
    /// Column family configurations
    pub column_families: HashMap<String, ColumnFamilyConfig>,
    
    /// Snapshot configuration
    pub snapshot_config: SnapshotConfig,
    
    /// Transaction configuration
    pub transaction_config: TransactionConfig,
    
    /// Performance tuning options
    pub performance: PerformanceConfig,
}

/// Configuration for individual column families
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnFamilyConfig {
    /// Write buffer size for this column family
    pub write_buffer_size: usize,
    
    /// Maximum number of write buffers
    pub max_write_buffer_number: i32,
    
    /// Target file size for compaction
    pub target_file_size_base: u64,
    
    /// Compression type
    pub compression_type: CompressionType,
    
    /// Enable bloom filter
    pub bloom_filter_enabled: bool,
    
    /// Bloom filter bits per key
    pub bloom_filter_bits_per_key: i32,
    
    /// Block size for this column family
    pub block_size: usize,
    
    /// Cache index and filter blocks
    pub cache_index_and_filter_blocks: bool,
    
    /// TTL for data in seconds (0 = no TTL)
    pub ttl_seconds: u64,
}

/// Snapshot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotConfig {
    /// Maximum number of snapshots to keep
    pub max_snapshots: usize,
    
    /// Automatic snapshot interval in seconds
    pub auto_snapshot_interval: u64,
    
    /// Compress snapshots
    pub compress_snapshots: bool,
    
    /// Directory to store snapshots
    pub snapshot_dir: Option<PathBuf>,
}

/// Transaction configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionConfig {
    /// Maximum transaction size in bytes
    pub max_transaction_size: usize,
    
    /// Transaction timeout in seconds
    pub transaction_timeout: u64,
    
    /// Maximum number of concurrent transactions
    pub max_concurrent_transactions: usize,
    
    /// Enable transaction logging
    pub enable_transaction_log: bool,
}

/// Performance tuning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable parallel compaction
    pub parallel_compaction: bool,
    
    /// Enable write ahead log
    pub enable_wal: bool,
    
    /// Sync writes to disk
    pub sync_writes: bool,
    
    /// Use direct I/O
    pub use_direct_io: bool,
    
    /// Prefetch compaction input files
    pub prefetch_compaction_input: bool,
    
    /// Rate limiter for compaction (bytes per second, 0 = unlimited)
    pub compaction_rate_limit: u64,
    
    /// Rate limiter for flushes (bytes per second, 0 = unlimited)
    pub flush_rate_limit: u64,
}

/// Compression types supported by RocksDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionType {
    None,
    Snappy,
    Zlib,
    Bz2,
    Lz4,
    Lz4hc,
    Zstd,
}

impl Default for StorageConfig {
    fn default() -> Self {
        let mut column_families = HashMap::new();
        
        // Default configurations for Catalyst column families
        column_families.insert("accounts".to_string(), ColumnFamilyConfig::for_accounts());
        column_families.insert("transactions".to_string(), ColumnFamilyConfig::for_transactions());
        column_families.insert("consensus".to_string(), ColumnFamilyConfig::for_consensus());
        column_families.insert("metadata".to_string(), ColumnFamilyConfig::for_metadata());
        column_families.insert("contracts".to_string(), ColumnFamilyConfig::for_contracts());
        column_families.insert("dfs_refs".to_string(), ColumnFamilyConfig::for_dfs_refs());
        
        Self {
            data_dir: PathBuf::from("./catalyst_data"),
            max_open_files: 1000,
            max_wal_size: 128 * 1024 * 1024, // 128 MB
            max_manifest_size: 64 * 1024 * 1024, // 64 MB
            max_background_compactions: 4,
            max_background_flushes: 2,
            write_buffer_size: 64 * 1024 * 1024, // 64 MB
            max_write_buffer_number: 3,
            target_file_size_base: 64 * 1024 * 1024, // 64 MB
            block_cache_size: 256 * 1024 * 1024, // 256 MB
            row_cache_size: 64 * 1024 * 1024, // 64 MB
            compression_enabled: true,
            compression_type: CompressionType::Lz4,
            enable_statistics: true,
            auto_compaction: true,
            column_families,
            snapshot_config: SnapshotConfig::default(),
            transaction_config: TransactionConfig::default(),
            performance: PerformanceConfig::default(),
        }
    }
}

impl Default for ColumnFamilyConfig {
    fn default() -> Self {
        Self {
            write_buffer_size: 32 * 1024 * 1024, // 32 MB
            max_write_buffer_number: 3,
            target_file_size_base: 32 * 1024 * 1024, // 32 MB
            compression_type: CompressionType::Lz4,
            bloom_filter_enabled: true,
            bloom_filter_bits_per_key: 10,
            block_size: 4 * 1024, // 4 KB
            cache_index_and_filter_blocks: true,
            ttl_seconds: 0,
        }
    }
}

impl ColumnFamilyConfig {
    /// Configuration optimized for account data
    pub fn for_accounts() -> Self {
        Self {
            write_buffer_size: 64 * 1024 * 1024, // 64 MB - accounts have frequent updates
            bloom_filter_bits_per_key: 15, // Higher precision for account lookups
            ..Default::default()
        }
    }
    
    /// Configuration optimized for transaction data
    pub fn for_transactions() -> Self {
        Self {
            write_buffer_size: 128 * 1024 * 1024, // 128 MB - high transaction volume
            compression_type: CompressionType::Zstd, // Better compression for transaction data
            ..Default::default()
        }
    }
    
    /// Configuration optimized for consensus data
    pub fn for_consensus() -> Self {
        Self {
            write_buffer_size: 32 * 1024 * 1024, // 32 MB - moderate volume
            bloom_filter_bits_per_key: 12,
            ..Default::default()
        }
    }
    
    /// Configuration optimized for metadata
    pub fn for_metadata() -> Self {
        Self {
            write_buffer_size: 16 * 1024 * 1024, // 16 MB - low volume
            compression_type: CompressionType::Zstd,
            ..Default::default()
        }
    }
    
    /// Configuration optimized for smart contracts
    pub fn for_contracts() -> Self {
        Self {
            write_buffer_size: 64 * 1024 * 1024, // 64 MB
            compression_type: CompressionType::Zstd, // Compress contract bytecode
            block_size: 8 * 1024, // 8 KB - larger blocks for contract data
            ..Default::default()
        }
    }
    
    /// Configuration optimized for DFS references
    pub fn for_dfs_refs() -> Self {
        Self {
            write_buffer_size: 32 * 1024 * 1024, // 32 MB
            compression_type: CompressionType::Lz4, // Fast compression for references
            ..Default::default()
        }
    }
}

impl Default for SnapshotConfig {
    fn default() -> Self {
        Self {
            max_snapshots: 10,
            auto_snapshot_interval: 3600, // 1 hour
            compress_snapshots: true,
            snapshot_dir: None, // Use data_dir/snapshots by default
        }
    }
}

impl Default for TransactionConfig {
    fn default() -> Self {
        Self {
            max_transaction_size: 16 * 1024 * 1024, // 16 MB
            transaction_timeout: 30, // 30 seconds
            max_concurrent_transactions: 100,
            enable_transaction_log: true,
        }
    }
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            parallel_compaction: true,
            enable_wal: true,
            sync_writes: false, // Async writes for better performance
            use_direct_io: false, // Let OS handle I/O optimization
            prefetch_compaction_input: true,
            compaction_rate_limit: 0, // Unlimited
            flush_rate_limit: 0, // Unlimited
        }
    }
}

impl From<CompressionType> for rocksdb::DBCompressionType {
    fn from(compression: CompressionType) -> Self {
        match compression {
            CompressionType::None => rocksdb::DBCompressionType::None,
            CompressionType::Snappy => rocksdb::DBCompressionType::Snappy,
            CompressionType::Zlib => rocksdb::DBCompressionType::Zlib,
            CompressionType::Bz2 => rocksdb::DBCompressionType::Bz2,
            CompressionType::Lz4 => rocksdb::DBCompressionType::Lz4,
            CompressionType::Lz4hc => rocksdb::DBCompressionType::Lz4hc,
            CompressionType::Zstd => rocksdb::DBCompressionType::Zstd,
        }
    }
}

impl StorageConfig {
    /// Create a configuration for testing with temporary directory
    #[cfg(feature = "testing")]
    pub fn for_testing() -> Self {
        Self {
            data_dir: std::env::temp_dir().join("catalyst_test_storage"),
            max_open_files: 100,
            max_wal_size: 1024 * 1024, // 1 MB
            write_buffer_size: 1024 * 1024, // 1 MB
            block_cache_size: 8 * 1024 * 1024, // 8 MB
            enable_statistics: false,
            ..Default::default()
        }
    }
    
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        if self.max_open_files < 10 {
            return Err("max_open_files must be at least 10".to_string());
        }
        
        if self.write_buffer_size < 1024 * 1024 {
            return Err("write_buffer_size must be at least 1MB".to_string());
        }
        
        if self.max_write_buffer_number < 1 {
            return Err("max_write_buffer_number must be at least 1".to_string());
        }
        
        if self.block_cache_size < 1024 * 1024 {
            return Err("block_cache_size must be at least 1MB".to_string());
        }
        
        // Validate column family configs
        for (name, config) in &self.column_families {
            if config.write_buffer_size < 1024 * 1024 {
                return Err(format!("Column family {} write_buffer_size must be at least 1MB", name));
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_config_validation() {
        let config = StorageConfig::default();
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_config_validation_errors() {
        let mut config = StorageConfig::default();
        config.max_open_files = 5;
        assert!(config.validate().is_err());
        
        config = StorageConfig::default();
        config.write_buffer_size = 512 * 1024; // 512 KB
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_column_family_configs() {
        let accounts_config = ColumnFamilyConfig::for_accounts();
        assert_eq!(accounts_config.bloom_filter_bits_per_key, 15);
        
        let tx_config = ColumnFamilyConfig::for_transactions();
        assert_eq!(tx_config.write_buffer_size, 128 * 1024 * 1024);
        
        let contract_config = ColumnFamilyConfig::for_contracts();
        assert_eq!(contract_config.block_size, 8 * 1024);
    }
    
    #[cfg(feature = "testing")]
    #[test]
    fn test_testing_config() {
        let config = StorageConfig::for_testing();
        assert!(config.data_dir.to_string_lossy().contains("catalyst_test_storage"));
        assert_eq!(config.max_open_files, 100);
    }
}