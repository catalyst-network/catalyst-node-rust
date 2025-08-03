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
            write_buffer_size: 64 * 1024 * 1024, // 64MB
            max_write_buffer_number: 3,
            target_file_size_base: 64 * 1024 * 1024,
            compression_type: CompressionType::Lz4,
            block_size: 16 * 1024,
            cache_index_and_filter_blocks: true,
            bloom_filter_enabled: true,
            bloom_filter_bits_per_key: 15, // Fixed: Use 15 instead of 10 for better performance
            ttl_seconds: 0,
        }
    }
    
    /// Configuration optimized for transaction data
    pub fn for_transactions() -> Self {
        Self {
            write_buffer_size: 128 * 1024 * 1024, // 128MB
            max_write_buffer_number: 3,
            target_file_size_base: 64 * 1024 * 1024,
            compression_type: CompressionType::Snappy,
            block_size: 16 * 1024,
            cache_index_and_filter_blocks: true,
            bloom_filter_enabled: true,
            bloom_filter_bits_per_key: 15,
            ttl_seconds: 0,
        }
    }
    
    /// Configuration optimized for consensus data
    pub fn for_consensus() -> Self {
        Self {
            write_buffer_size: 32 * 1024 * 1024, // 32 MB - moderate volume
            max_write_buffer_number: 3,
            target_file_size_base: 32 * 1024 * 1024,
            compression_type: CompressionType::Lz4,
            bloom_filter_enabled: true,
            bloom_filter_bits_per_key: 12,
            block_size: 4 * 1024,
            cache_index_and_filter_blocks: true,
            ttl_seconds: 0,
        }
    }
    
    /// Configuration optimized for metadata
    pub fn for_metadata() -> Self {
        Self {
            write_buffer_size: 16 * 1024 * 1024, // 16 MB - low volume
            max_write_buffer_number: 3,
            target_file_size_base: 32 * 1024 * 1024,
            compression_type: CompressionType::Zstd,
            bloom_filter_enabled: true,
            bloom_filter_bits_per_key: 10,
            block_size: 4 * 1024,
            cache_index_and_filter_blocks: true,
            ttl_seconds: 0,
        }
    }
    
    /// Configuration optimized for smart contracts
    pub fn for_contracts() -> Self {
        Self {
            write_buffer_size: 64 * 1024 * 1024, // 64 MB
            max_write_buffer_number: 3,
            target_file_size_base: 32 * 1024 * 1024,
            compression_type: CompressionType::Zstd, // Compress contract bytecode
            bloom_filter_enabled: true,
            bloom_filter_bits_per_key: 10,
            block_size: 8 * 1024, // 8 KB - larger blocks for contract data
            cache_index_and_filter_blocks: true,
            ttl_seconds: 0,
        }
    }
    
    /// Configuration optimized for DFS references
    pub fn for_dfs_refs() -> Self {
        Self {
            write_buffer_size: 32 * 1024 * 1024, // 32 MB
            max_write_buffer_number: 3,
            target_file_size_base: 32 * 1024 * 1024,
            compression_type: CompressionType::Lz4, // Fast compression for references
            bloom_filter_enabled: true,
            bloom_filter_bits_per_key: 10,
            block_size: 4 * 1024,
            cache_index_and_filter_blocks: true,
            ttl_seconds: 0,
        }
    }
    
    /// Create a custom configuration with specific parameters
    pub fn custom(
        write_buffer_size: usize,
        compression_type: CompressionType,
        bloom_filter_bits: i32,
    ) -> Self {
        Self {
            write_buffer_size,
            compression_type,
            bloom_filter_bits_per_key: bloom_filter_bits,
            ..Default::default()
        }
    }
    
    /// Create a configuration optimized for high-throughput writes
    pub fn for_high_throughput() -> Self {
        Self {
            write_buffer_size: 256 * 1024 * 1024, // 256 MB
            max_write_buffer_number: 6,
            target_file_size_base: 256 * 1024 * 1024, // 256 MB
            compression_type: CompressionType::Lz4, // Fast compression
            bloom_filter_enabled: true,
            bloom_filter_bits_per_key: 15,
            block_size: 32 * 1024, // 32 KB blocks
            cache_index_and_filter_blocks: true,
            ttl_seconds: 0,
        }
    }
    
    /// Create a configuration optimized for read-heavy workloads
    pub fn for_read_heavy() -> Self {
        Self {
            write_buffer_size: 32 * 1024 * 1024, // 32 MB
            max_write_buffer_number: 2,
            target_file_size_base: 128 * 1024 * 1024, // 128 MB
            compression_type: CompressionType::Zstd, // Better compression
            bloom_filter_enabled: true,
            bloom_filter_bits_per_key: 20, // More bits for better read performance
            block_size: 8 * 1024, // 8 KB blocks
            cache_index_and_filter_blocks: true,
            ttl_seconds: 0,
        }
    }
    
    /// Create a configuration for temporary/cache data with TTL
    pub fn for_cache(ttl_seconds: u64) -> Self {
        Self {
            write_buffer_size: 16 * 1024 * 1024, // 16 MB
            max_write_buffer_number: 2,
            target_file_size_base: 32 * 1024 * 1024, // 32 MB
            compression_type: CompressionType::Lz4,
            bloom_filter_enabled: true,
            bloom_filter_bits_per_key: 10,
            block_size: 4 * 1024, // 4 KB blocks
            cache_index_and_filter_blocks: false, // Don't cache temporary data
            ttl_seconds,
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
    /// Create a new storage config with custom data directory
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            ..Default::default()
        }
    }
    
    /// Create a high-performance configuration for production use
    pub fn for_production() -> Self {
        Self {
            data_dir: PathBuf::from("/var/lib/catalyst/storage"),
            max_open_files: 10000,
            max_wal_size: 512 * 1024 * 1024, // 512 MB
            max_manifest_size: 128 * 1024 * 1024, // 128 MB
            max_background_compactions: 8,
            max_background_flushes: 4,
            write_buffer_size: 128 * 1024 * 1024, // 128 MB
            max_write_buffer_number: 4,
            target_file_size_base: 128 * 1024 * 1024, // 128 MB
            block_cache_size: 1024 * 1024 * 1024, // 1 GB
            row_cache_size: 256 * 1024 * 1024, // 256 MB
            compression_enabled: true,
            compression_type: CompressionType::Lz4,
            enable_statistics: true,
            auto_compaction: true,
            performance: PerformanceConfig {
                parallel_compaction: true,
                enable_wal: true,
                sync_writes: true, // Sync for production safety
                use_direct_io: true, // Use direct I/O for production
                prefetch_compaction_input: true,
                compaction_rate_limit: 100 * 1024 * 1024, // 100 MB/s
                flush_rate_limit: 50 * 1024 * 1024, // 50 MB/s
            },
            ..Default::default()
        }
    }
    
    /// Create a lightweight configuration for development
    pub fn for_development() -> Self {
        Self {
            data_dir: PathBuf::from("./dev_catalyst_data"),
            max_open_files: 100,
            max_wal_size: 16 * 1024 * 1024, // 16 MB
            max_manifest_size: 8 * 1024 * 1024, // 8 MB
            max_background_compactions: 2,
            max_background_flushes: 1,
            write_buffer_size: 16 * 1024 * 1024, // 16 MB
            block_cache_size: 64 * 1024 * 1024, // 64 MB
            row_cache_size: 16 * 1024 * 1024, // 16 MB
            enable_statistics: false,
            performance: PerformanceConfig {
                sync_writes: false,
                use_direct_io: false,
                ..Default::default()
            },
            ..Default::default()
        }
    }
    
    /// Create a configuration for testing with temporary directory
    #[cfg(feature = "testing")]
    pub fn for_testing() -> Self {
        use std::env;
        use uuid::Uuid;
        
        Self {
            data_dir: env::temp_dir().join(format!("catalyst_test_{}", Uuid::new_v4())),
            max_open_files: 100,
            max_wal_size: 1024 * 1024, // 1 MB
            max_manifest_size: 512 * 1024, // 512 KB
            max_background_compactions: 1,
            max_background_flushes: 1,
            write_buffer_size: 1024 * 1024, // 1 MB
            max_write_buffer_number: 2,
            target_file_size_base: 1024 * 1024, // 1 MB
            block_cache_size: 8 * 1024 * 1024, // 8 MB
            row_cache_size: 2 * 1024 * 1024, // 2 MB
            compression_enabled: false, // Disable compression for faster tests
            enable_statistics: false,
            auto_compaction: false, // Manual compaction for deterministic tests
            performance: PerformanceConfig {
                parallel_compaction: false,
                enable_wal: false, // Disable WAL for faster tests
                sync_writes: false,
                use_direct_io: false,
                prefetch_compaction_input: false,
                compaction_rate_limit: 0,
                flush_rate_limit: 0,
            },
            snapshot_config: SnapshotConfig {
                max_snapshots: 3,
                auto_snapshot_interval: 0, // Disable auto snapshots in tests
                compress_snapshots: false,
                snapshot_dir: None,
            },
            transaction_config: TransactionConfig {
                max_transaction_size: 1024 * 1024, // 1 MB
                transaction_timeout: 5, // 5 seconds
                max_concurrent_transactions: 10,
                enable_transaction_log: false,
            },
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
        
        if self.max_background_compactions < 1 {
            return Err("max_background_compactions must be at least 1".to_string());
        }
        
        if self.max_background_flushes < 1 {
            return Err("max_background_flushes must be at least 1".to_string());
        }
        
        if self.target_file_size_base < 1024 * 1024 {
            return Err("target_file_size_base must be at least 1MB".to_string());
        }
        
        // Validate column family configs
        for (name, config) in &self.column_families {
            if config.write_buffer_size < 1024 * 1024 {
                return Err(format!("Column family {} write_buffer_size must be at least 1MB", name));
            }
            
            if config.max_write_buffer_number < 1 {
                return Err(format!("Column family {} max_write_buffer_number must be at least 1", name));
            }
            
            if config.target_file_size_base < 1024 * 1024 {
                return Err(format!("Column family {} target_file_size_base must be at least 1MB", name));
            }
            
            if config.bloom_filter_bits_per_key < 1 || config.bloom_filter_bits_per_key > 30 {
                return Err(format!("Column family {} bloom_filter_bits_per_key must be between 1 and 30", name));
            }
            
            if config.block_size < 1024 {
                return Err(format!("Column family {} block_size must be at least 1KB", name));
            }
        }
        
        // Validate snapshot config
        if self.snapshot_config.max_snapshots == 0 {
            return Err("snapshot_config.max_snapshots must be at least 1".to_string());
        }
        
        // Validate transaction config
        if self.transaction_config.max_transaction_size < 1024 {
            return Err("transaction_config.max_transaction_size must be at least 1KB".to_string());
        }
        
        if self.transaction_config.transaction_timeout == 0 {
            return Err("transaction_config.transaction_timeout must be greater than 0".to_string());
        }
        
        if self.transaction_config.max_concurrent_transactions == 0 {
            return Err("transaction_config.max_concurrent_transactions must be at least 1".to_string());
        }
        
        Ok(())
    }
    
    /// Get memory usage estimate in bytes
    pub fn estimated_memory_usage(&self) -> usize {
        let mut total = 0;
        
        // Block cache
        total += self.block_cache_size;
        
        // Row cache
        total += self.row_cache_size;
        
        // Write buffers for each column family
        for config in self.column_families.values() {
            total += config.write_buffer_size * config.max_write_buffer_number as usize;
        }
        
        // Add overhead estimate (approximately 20% of configured memory)
        total += total / 5;
        
        total
    }
    
    /// Update configuration for a specific column family
    pub fn set_column_family_config(&mut self, name: String, config: ColumnFamilyConfig) {
        self.column_families.insert(name, config);
    }
    
    /// Remove a column family configuration
    pub fn remove_column_family_config(&mut self, name: &str) -> Option<ColumnFamilyConfig> {
        self.column_families.remove(name)
    }
    
    /// Get a column family configuration by name
    pub fn get_column_family_config(&self, name: &str) -> Option<&ColumnFamilyConfig> {
        self.column_families.get(name)
    }
    
    /// List all configured column family names
    pub fn column_family_names(&self) -> Vec<String> {
        self.column_families.keys().cloned().collect()
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
        
        config = StorageConfig::default();
        config.block_cache_size = 512 * 1024; // 512 KB
        assert!(config.validate().is_err());
        
        config = StorageConfig::default();
        config.max_background_compactions = 0;
        assert!(config.validate().is_err());
    }
    
    #[test]
    fn test_column_family_configs() {
        let accounts_config = ColumnFamilyConfig::for_accounts();
        assert_eq!(accounts_config.bloom_filter_bits_per_key, 15);
        assert_eq!(accounts_config.write_buffer_size, 64 * 1024 * 1024);
        
        let tx_config = ColumnFamilyConfig::for_transactions();
        assert_eq!(tx_config.write_buffer_size, 128 * 1024 * 1024);
        assert!(matches!(tx_config.compression_type, CompressionType::Snappy));
        
        let contract_config = ColumnFamilyConfig::for_contracts();
        assert_eq!(contract_config.block_size, 8 * 1024);
        assert!(matches!(contract_config.compression_type, CompressionType::Zstd));
    }
    
    #[test]
    fn test_production_config() {
        let config = StorageConfig::for_production();
        assert_eq!(config.max_open_files, 10000);
        assert_eq!(config.block_cache_size, 1024 * 1024 * 1024);
        assert!(config.performance.sync_writes);
        assert!(config.performance.use_direct_io);
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_development_config() {
        let config = StorageConfig::for_development();
        assert_eq!(config.max_open_files, 100);
        assert!(!config.performance.sync_writes);
        assert!(!config.performance.use_direct_io);
        assert!(config.validate().is_ok());
    }
    
    #[cfg(feature = "testing")]
    #[test]
    fn test_testing_config() {
        let config = StorageConfig::for_testing();
        assert!(config.data_dir.to_string_lossy().contains("catalyst_test_"));
        assert_eq!(config.max_open_files, 100);
        assert!(!config.compression_enabled);
        assert!(!config.enable_statistics);
        assert!(!config.auto_compaction);
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_compression_type_conversion() {
        assert!(matches!(
            rocksdb::DBCompressionType::from(CompressionType::None),
            rocksdb::DBCompressionType::None
        ));
        assert!(matches!(
            rocksdb::DBCompressionType::from(CompressionType::Lz4),
            rocksdb::DBCompressionType::Lz4
        ));
        assert!(matches!(
            rocksdb::DBCompressionType::from(CompressionType::Zstd),
            rocksdb::DBCompressionType::Zstd
        ));
    }
    
    #[test]
    fn test_memory_usage_estimation() {
        let config = StorageConfig::default();
        let estimated = config.estimated_memory_usage();
        
        // Should include block cache + row cache + write buffers + overhead
        let expected_minimum = config.block_cache_size + config.row_cache_size;
        assert!(estimated > expected_minimum);
    }
    
    #[test]
    fn test_column_family_management() {
        let mut config = StorageConfig::default();
        
        // Test adding custom column family
        let custom_config = ColumnFamilyConfig::for_high_throughput();
        config.set_column_family_config("custom".to_string(), custom_config.clone());
        
        assert!(config.get_column_family_config("custom").is_some());
        assert_eq!(
            config.get_column_family_config("custom").unwrap().write_buffer_size,
            custom_config.write_buffer_size
        );
        
        // Test removing column family
        let removed = config.remove_column_family_config("custom");
        assert!(removed.is_some());
        assert!(config.get_column_family_config("custom").is_none());
        
        // Test listing column family names
        let names = config.column_family_names();
        assert!(names.contains(&"accounts".to_string()));
        assert!(names.contains(&"transactions".to_string()));
        assert!(!names.contains(&"custom".to_string()));
    }
    
    #[test]
    fn test_column_family_validation() {
        let mut config = StorageConfig::default();
        
        // Create invalid column family config
        let mut invalid_config = ColumnFamilyConfig::default();
        invalid_config.write_buffer_size = 512 * 1024; // Too small
        config.set_column_family_config("invalid".to_string(), invalid_config);
        
        assert!(config.validate().is_err());
        
        // Fix the config
        let valid_config = ColumnFamilyConfig::default();
        config.set_column_family_config("invalid".to_string(), valid_config);
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_custom_column_family_configs() {
        let high_throughput = ColumnFamilyConfig::for_high_throughput();
        assert_eq!(high_throughput.write_buffer_size, 256 * 1024 * 1024);
        assert_eq!(high_throughput.max_write_buffer_number, 6);
        assert_eq!(high_throughput.bloom_filter_bits_per_key, 15);
        
        let read_heavy = ColumnFamilyConfig::for_read_heavy();
        assert_eq!(read_heavy.bloom_filter_bits_per_key, 20);
        assert!(matches!(read_heavy.compression_type, CompressionType::Zstd));
        
        let cache_config = ColumnFamilyConfig::for_cache(3600);
        assert_eq!(cache_config.ttl_seconds, 3600);
        assert!(!cache_config.cache_index_and_filter_blocks);
        
        let custom = ColumnFamilyConfig::custom(
            128 * 1024 * 1024,
            CompressionType::Snappy,
            12
        );
        assert_eq!(custom.write_buffer_size, 128 * 1024 * 1024);
        assert!(matches!(custom.compression_type, CompressionType::Snappy));
        assert_eq!(custom.bloom_filter_bits_per_key, 12);
    }
    
    #[test]
    fn test_snapshot_config_validation() {
        let mut config = StorageConfig::default();
        config.snapshot_config.max_snapshots = 0;
        assert!(config.validate().is_err());
        
        config.snapshot_config.max_snapshots = 5;
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_transaction_config_validation() {
        let mut config = StorageConfig::default();
        
        // Test invalid transaction size
        config.transaction_config.max_transaction_size = 512; // Too small
        assert!(config.validate().is_err());
        
        // Test invalid timeout
        config.transaction_config.max_transaction_size = 1024 * 1024;
        config.transaction_config.transaction_timeout = 0;
        assert!(config.validate().is_err());
        
        // Test invalid concurrent transactions
        config.transaction_config.transaction_timeout = 30;
        config.transaction_config.max_concurrent_transactions = 0;
        assert!(config.validate().is_err());
        
        // Fix all issues
        config.transaction_config.max_concurrent_transactions = 10;
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_config_builder_pattern() {
        let config = StorageConfig::new(PathBuf::from("/custom/path"));
        assert_eq!(config.data_dir, PathBuf::from("/custom/path"));
        
        // Test that other fields are still default
        assert_eq!(config.max_open_files, 1000);
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_performance_config_defaults() {
        let perf_config = PerformanceConfig::default();
        assert!(perf_config.parallel_compaction);
        assert!(perf_config.enable_wal);
        assert!(!perf_config.sync_writes);
        assert!(!perf_config.use_direct_io);
        assert!(perf_config.prefetch_compaction_input);
        assert_eq!(perf_config.compaction_rate_limit, 0);
        assert_eq!(perf_config.flush_rate_limit, 0);
    }
    
    #[test]
    fn test_all_column_family_factories() {
        // Test that all factory methods produce valid configs
        let configs = vec![
            ColumnFamilyConfig::for_accounts(),
            ColumnFamilyConfig::for_transactions(),
            ColumnFamilyConfig::for_consensus(),
            ColumnFamilyConfig::for_metadata(),
            ColumnFamilyConfig::for_contracts(),
            ColumnFamilyConfig::for_dfs_refs(),
            ColumnFamilyConfig::for_high_throughput(),
            ColumnFamilyConfig::for_read_heavy(),
            ColumnFamilyConfig::for_cache(3600),
        ];
        
        for config in configs {
            // All configs should have reasonable values
            assert!(config.write_buffer_size >= 1024 * 1024);
            assert!(config.max_write_buffer_number >= 1);
            assert!(config.target_file_size_base >= 1024 * 1024);
            assert!(config.bloom_filter_bits_per_key >= 1);
            assert!(config.bloom_filter_bits_per_key <= 30);
            assert!(config.block_size >= 1024);
        }
    }
    
    #[test]
    fn test_serde_serialization() {
        let config = StorageConfig::default();
        
        // Test JSON serialization
        let json = serde_json::to_string(&config).expect("Failed to serialize to JSON");
        let deserialized: StorageConfig = serde_json::from_str(&json)
            .expect("Failed to deserialize from JSON");
        
        // Verify key fields are preserved
        assert_eq!(config.data_dir, deserialized.data_dir);
        assert_eq!(config.max_open_files, deserialized.max_open_files);
        assert_eq!(config.block_cache_size, deserialized.block_cache_size);
        assert_eq!(config.column_families.len(), deserialized.column_families.len());
    }
    
    #[test]
    fn test_config_edge_cases() {
        // Test minimum valid values
        let mut config = StorageConfig::default();
        config.max_open_files = 10;
        config.write_buffer_size = 1024 * 1024;
        config.max_write_buffer_number = 1;
        config.block_cache_size = 1024 * 1024;
        config.max_background_compactions = 1;
        config.max_background_flushes = 1;
        config.target_file_size_base = 1024 * 1024;
        
        assert!(config.validate().is_ok());
        
        // Test just below minimum valid values
        config.max_open_files = 9;
        assert!(config.validate().is_err());
    }
}