//! RocksDB engine implementation for Catalyst storage

use crate::{ColumnFamilyConfig, StorageConfig, StorageError, StorageResult};
use rocksdb::{
    BlockBasedOptions, BoundColumnFamily, Cache, ColumnFamilyDescriptor, DBIteratorWithThreadMode,
    DBWithThreadMode, MultiThreaded, Options, ReadOptions, WriteBatch, WriteOptions,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use parking_lot::RwLock;

type Db = DBWithThreadMode<MultiThreaded>;

/// RocksDB storage engine
pub struct RocksEngine {
    db: Db,
    column_families: RwLock<HashSet<String>>,
    config: StorageConfig,
    block_cache: Option<Cache>,
    row_cache: Option<Cache>,
}

impl RocksEngine {
    /// Create a new RocksDB engine
    pub fn new(config: StorageConfig) -> StorageResult<Self> {
        config.validate()
            .map_err(|e| StorageError::config(format!("Invalid configuration: {}", e)))?;
        
        // Create data directory if it doesn't exist
        std::fs::create_dir_all(&config.data_dir)
            .map_err(|e| StorageError::config(format!("Failed to create data directory: {}", e)))?;
        
        // Setup caches
        let block_cache = if config.block_cache_size > 0 {
            Some(Cache::new_lru_cache(config.block_cache_size))
        } else {
            None
        };
        
        let row_cache = if config.row_cache_size > 0 {
            Some(Cache::new_lru_cache(config.row_cache_size))
        } else {
            None
        };
        
        // Setup database options
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        db_opts.set_max_open_files(config.max_open_files);
        db_opts.set_max_total_wal_size(config.max_wal_size);
        db_opts.set_max_background_compactions(config.max_background_compactions);
        db_opts.set_max_background_flushes(config.max_background_flushes);
        db_opts.set_write_buffer_size(config.write_buffer_size);
        db_opts.set_max_write_buffer_number(config.max_write_buffer_number);
        db_opts.set_target_file_size_base(config.target_file_size_base);
        
        if config.enable_statistics {
            db_opts.enable_statistics();
        }
        
        if !config.auto_compaction {
            db_opts.set_disable_auto_compactions(true);
        }
        
        // Configure performance options
        if config.performance.enable_wal {
            db_opts.set_use_fsync(config.performance.sync_writes);
        } else {
            db_opts.set_wal_recovery_mode(rocksdb::DBRecoveryMode::SkipAnyCorruptedRecord);
        }
        
        if config.performance.use_direct_io {
            db_opts.set_use_direct_reads(true);
            db_opts.set_use_direct_io_for_flush_and_compaction(true);
        }
        
        if config.performance.compaction_rate_limit > 0 {
            // (rate_bytes_per_sec, refill_period_us, fairness)
            db_opts.set_ratelimiter(config.performance.compaction_rate_limit as i64, 100 * 1000, 10);
        }
        
        // Setup column family descriptors
        let mut cf_descriptors = Vec::new();
        
        // Add default column family
        cf_descriptors.push(ColumnFamilyDescriptor::new(
            "default",
            Self::create_cf_options(&ColumnFamilyConfig::default(), &block_cache)?
        ));
        
        // Add custom column families
        for (name, cf_config) in &config.column_families {
            cf_descriptors.push(ColumnFamilyDescriptor::new(
                name,
                Self::create_cf_options(cf_config, &block_cache)?
            ));
        }
        
        // Open database (MultiThreaded mode so create/drop CF work via &self)
        let db = Db::open_cf_descriptors(&db_opts, &config.data_dir, cf_descriptors)
            .map_err(|e| StorageError::config(format!("Failed to open database: {}", e)))?;

        // Track column family names we expect to exist
        let mut column_families = HashSet::new();
        column_families.insert("default".to_string());
        for name in config.column_families.keys() {
            column_families.insert(name.clone());
        }
        
        Ok(Self {
            db,
            column_families: RwLock::new(column_families),
            config,
            block_cache,
            row_cache,
        })
    }
    
    /// Create column family options from configuration
    fn create_cf_options(
        config: &ColumnFamilyConfig,
        block_cache: &Option<Cache>,
    ) -> StorageResult<Options> {
        let mut opts = Options::default();
        
        opts.set_write_buffer_size(config.write_buffer_size);
        opts.set_max_write_buffer_number(config.max_write_buffer_number);
        opts.set_target_file_size_base(config.target_file_size_base);
        opts.set_compression_type(config.compression_type.clone().into());
        
        // Setup block-based table options
        let mut block_opts = BlockBasedOptions::default();
        block_opts.set_block_size(config.block_size);
        block_opts.set_cache_index_and_filter_blocks(config.cache_index_and_filter_blocks);
        
        if config.bloom_filter_enabled {
            block_opts.set_bloom_filter(config.bloom_filter_bits_per_key as f64, false);
        }
        
        if let Some(cache) = block_cache {
            block_opts.set_block_cache(cache);
        }
        
        opts.set_block_based_table_factory(&block_opts);
        
        // TTL options are not supported in rocksdb crate version used by this workspace.
        
        Ok(opts)
    }
    
    /// Get a column family handle by name
    pub fn cf_handle(&self, name: &str) -> StorageResult<std::sync::Arc<BoundColumnFamily<'_>>> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| StorageError::ColumnFamilyNotFound(name.to_string()))
    }
    
    /// Get all column family names
    pub fn cf_names(&self) -> Vec<String> {
        let cf_set = self.column_families.read();
        cf_set.iter().cloned().collect()
    }
    
    /// Create a new column family
    pub fn create_cf(&self, name: &str, config: &ColumnFamilyConfig) -> StorageResult<()> {
        let opts = Self::create_cf_options(config, &self.block_cache)?;

        self.db
            .create_cf(name, &opts)
            .map_err(|e| StorageError::config(format!("Failed to create column family {}: {}", name, e)))?;

        // Track name after successful creation
        self.column_families.write().insert(name.to_string());
        
        Ok(())
    }
    
    /// Drop a column family
    pub fn drop_cf(&self, name: &str) -> StorageResult<()> {
        if name == "default" {
            return Err(StorageError::config("Cannot drop default column family".to_string()));
        }

        self.db
            .drop_cf(name)
            .map_err(|e| StorageError::config(format!("Failed to drop column family {}: {}", name, e)))?;

        self.column_families.write().remove(name);
        
        Ok(())
    }
    
    /// Get a value from the database
    pub fn get(&self, cf: &str, key: &[u8]) -> StorageResult<Option<Vec<u8>>> {
        let cf_handle = self.cf_handle(cf)?;
        
        self.db
            .get_cf(&cf_handle, key)
            .map_err(|e| StorageError::internal(format!("Failed to get key from {}: {}", cf, e)))
    }
    
    /// Get a value with custom read options
    pub fn get_with_options(
        &self,
        cf: &str,
        key: &[u8],
        read_opts: &ReadOptions,
    ) -> StorageResult<Option<Vec<u8>>> {
        let cf_handle = self.cf_handle(cf)?;
        
        self.db
            .get_cf_opt(&cf_handle, key, read_opts)
            .map_err(|e| StorageError::internal(format!("Failed to get key from {}: {}", cf, e)))
    }
    
    /// Put a value into the database
    pub fn put(&self, cf: &str, key: &[u8], value: &[u8]) -> StorageResult<()> {
        let cf_handle = self.cf_handle(cf)?;
        
        self.db
            .put_cf(&cf_handle, key, value)
            .map_err(|e| StorageError::internal(format!("Failed to put key to {}: {}", cf, e)))
    }
    
    /// Put a value with custom write options
    pub fn put_with_options(
        &self,
        cf: &str,
        key: &[u8],
        value: &[u8],
        write_opts: &WriteOptions,
    ) -> StorageResult<()> {
        let cf_handle = self.cf_handle(cf)?;
        
        self.db
            .put_cf_opt(&cf_handle, key, value, write_opts)
            .map_err(|e| StorageError::internal(format!("Failed to put key to {}: {}", cf, e)))
    }
    
    /// Delete a key from the database
    pub fn delete(&self, cf: &str, key: &[u8]) -> StorageResult<()> {
        let cf_handle = self.cf_handle(cf)?;
        
        self.db
            .delete_cf(&cf_handle, key)
            .map_err(|e| StorageError::internal(format!("Failed to delete key from {}: {}", cf, e)))
    }
    
    /// Delete a key with custom write options
    pub fn delete_with_options(
        &self,
        cf: &str,
        key: &[u8],
        write_opts: &WriteOptions,
    ) -> StorageResult<()> {
        let cf_handle = self.cf_handle(cf)?;
        
        self.db
            .delete_cf_opt(&cf_handle, key, write_opts)
            .map_err(|e| StorageError::internal(format!("Failed to delete key from {}: {}", cf, e)))
    }
    
    /// Execute a write batch atomically
    pub fn write_batch(&self, batch: WriteBatch) -> StorageResult<()> {
        self.db
            .write(batch)
            .map_err(|e| StorageError::transaction(format!("Failed to write batch: {}", e)))
    }
    
    /// Execute a write batch with custom options
    pub fn write_batch_with_options(
        &self,
        batch: WriteBatch,
        write_opts: &WriteOptions,
    ) -> StorageResult<()> {
        self.db
            .write_opt(batch, write_opts)
            .map_err(|e| StorageError::transaction(format!("Failed to write batch: {}", e)))
    }
    
    /// Create an iterator for a column family
    pub fn iterator(&self, cf: &str) -> StorageResult<DBIteratorWithThreadMode<'_, Db>> {
        let cf_handle = self.cf_handle(cf)?;
        Ok(self.db.iterator_cf(&cf_handle, rocksdb::IteratorMode::Start))
    }
    
    /// Create an iterator with custom read options
    pub fn iterator_with_options(
        &self,
        cf: &str,
        read_opts: ReadOptions,
        mode: rocksdb::IteratorMode,
    ) -> StorageResult<DBIteratorWithThreadMode<'_, Db>> {
        let cf_handle = self.cf_handle(cf)?;
        Ok(self.db.iterator_cf_opt(&cf_handle, read_opts, mode))
    }
    
    /// Get database property
    pub fn property(&self, property: &str) -> StorageResult<Option<String>> {
        Ok(self.db.property_value(property).unwrap_or(None))
    }
    
    /// Get column family property
    pub fn cf_property(&self, cf: &str, property: &str) -> StorageResult<Option<String>> {
        let cf_handle = self.cf_handle(cf)?;
        Ok(self.db.property_value_cf(&cf_handle, property).unwrap_or(None))
    }
    
    /// Flush column family
    pub fn flush_cf(&self, cf: &str) -> StorageResult<()> {
        let cf_handle = self.cf_handle(cf)?;
        self.db
            .flush_cf(&cf_handle)
            .map_err(|e| StorageError::internal(format!("Failed to flush {}: {}", cf, e)))
    }
    
    /// Compact range for column family
    pub fn compact_range_cf(&self, cf: &str, start: Option<&[u8]>, end: Option<&[u8]>) -> StorageResult<()> {
        let cf_handle = self.cf_handle(cf)?;
        self.db.compact_range_cf(&cf_handle, start, end);
        Ok(())
    }
    
    /// Get approximate memory usage
    pub fn memory_usage(&self) -> StorageResult<HashMap<String, u64>> {
        let mut usage = HashMap::new();
        
        // Get overall memory usage
        if let Some(mem_usage) = self.property("rocksdb.estimate-table-readers-mem")? {
            if let Ok(value) = mem_usage.parse::<u64>() {
                usage.insert("table_readers".to_string(), value);
            }
        }
        
        if let Some(mem_usage) = self.property("rocksdb.cur-size-all-mem-tables")? {
            if let Ok(value) = mem_usage.parse::<u64>() {
                usage.insert("memtables".to_string(), value);
            }
        }
        
        // Get per-column family usage
        for cf_name in self.cf_names() {
            if let Some(cf_usage) = self.cf_property(&cf_name, "rocksdb.estimate-table-readers-mem")? {
                if let Ok(value) = cf_usage.parse::<u64>() {
                    usage.insert(format!("cf_{}_table_readers", cf_name), value);
                }
            }
        }
        
        Ok(usage)
    }
    
    /// Get database statistics
    pub fn statistics(&self) -> StorageResult<Option<String>> {
        if self.config.enable_statistics {
            self.property("rocksdb.stats")
        } else {
            Ok(None)
        }
    }
    
    /// Perform consistency check
    pub fn check_consistency(&self) -> StorageResult<()> {
        // This would perform various consistency checks
        // For now, just verify we can access all column families
        for cf_name in self.cf_names() {
            self.cf_handle(&cf_name)?;
        }
        Ok(())
    }
    
    /// Get configuration
    pub fn config(&self) -> &StorageConfig {
        &self.config
    }
    
    /// Check if database is healthy
    pub fn is_healthy(&self) -> bool {
        // Basic health check - try to read a property
        self.property("rocksdb.num-files-at-level0").is_ok()
    }
    
    /// Close the database (automatically handled by Drop)
    pub fn close(self) -> StorageResult<()> {
        // RocksDB will be closed when self is dropped
        Ok(())
    }
}

impl Drop for RocksEngine {
    fn drop(&mut self) {
        // Perform cleanup operations
        if let Err(e) = self.flush_cf("default") {
            eprintln!("Warning: Failed to flush default CF during shutdown: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StorageConfig;
    use tempfile::TempDir;
    
    #[test]
    fn test_engine_creation() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = StorageConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();
        
        let engine = RocksEngine::new(config).unwrap();
        assert!(engine.is_healthy());
    }
    
    #[test]
    fn test_basic_operations() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = StorageConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();
        
        let engine = RocksEngine::new(config).unwrap();
        
        // Test put/get
        engine.put("default", b"key1", b"value1").unwrap();
        let value = engine.get("default", b"key1").unwrap();
        assert_eq!(value, Some(b"value1".to_vec()));
        
        // Test delete
        engine.delete("default", b"key1").unwrap();
        let value = engine.get("default", b"key1").unwrap();
        assert_eq!(value, None);
    }
    
    #[test]
    fn test_column_families() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = StorageConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();
        
        let engine = RocksEngine::new(config).unwrap();
        
        // Test operations on different column families
        engine.put("accounts", b"addr1", b"balance1").unwrap();
        engine.put("transactions", b"tx1", b"data1").unwrap();
        
        let balance = engine.get("accounts", b"addr1").unwrap();
        let tx_data = engine.get("transactions", b"tx1").unwrap();
        
        assert_eq!(balance, Some(b"balance1".to_vec()));
        assert_eq!(tx_data, Some(b"data1".to_vec()));
        
        // Verify isolation
        let no_balance = engine.get("transactions", b"addr1").unwrap();
        assert_eq!(no_balance, None);
    }
    
    #[test]
    fn test_write_batch() {
        let temp_dir = TempDir::new().unwrap();
        let mut config = StorageConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();
        
        let engine = RocksEngine::new(config).unwrap();
        let accounts_cf = engine.cf_handle("accounts").unwrap();
        
        let mut batch = WriteBatch::default();
        batch.put_cf(&accounts_cf, b"addr1", b"balance1");
        batch.put_cf(&accounts_cf, b"addr2", b"balance2");
        batch.delete_cf(&accounts_cf, b"addr3");
        
        engine.write_batch(batch).unwrap();
        
        assert_eq!(engine.get("accounts", b"addr1").unwrap(), Some(b"balance1".to_vec()));
        assert_eq!(engine.get("accounts", b"addr2").unwrap(), Some(b"balance2".to_vec()));
        assert_eq!(engine.get("accounts", b"addr3").unwrap(), None);
    }
}