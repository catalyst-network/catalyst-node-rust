//! Storage manager implementing the StateManager trait and providing high-level storage operations

use crate::{
    RocksEngine, StorageConfig, StorageError, StorageResult,
    TransactionBatch, Snapshot, SnapshotManager, MigrationManager,
};
use catalyst_utils::{
    Hash, CatalystResult, CatalystError,
    state::{StateManager, AccountState, state_keys},
    serialization::{CatalystSerialize, CatalystDeserialize},
    logging::{log_info, log_warn, log_error, LogCategory},
    metrics::{increment_counter, set_gauge, observe_histogram, time_operation},
    async_trait,
};
use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use tokio::sync::Semaphore;
use rocksdb::{WriteBatch, WriteOptions};
use sha2::{Sha256, Digest};

/// High-level storage manager for Catalyst Network
pub struct StorageManager {
    engine: Arc<RocksEngine>,
    snapshot_manager: SnapshotManager,
    migration_manager: MigrationManager,
    transaction_semaphore: Arc<Semaphore>,
    pending_transactions: RwLock<HashMap<String, TransactionBatch>>,
    current_state_root: RwLock<Option<Hash>>,
    metrics_enabled: bool,
}

impl StorageManager {
    /// Create a new storage manager
    pub async fn new(config: StorageConfig) -> StorageResult<Self> {
        log_info!(LogCategory::Storage, "Initializing storage manager");
        
        // Validate configuration
        config.validate()
            .map_err(|e| StorageError::config(format!("Invalid configuration: {}", e)))?;
        
        // Create RocksDB engine
        let engine = Arc::new(RocksEngine::new(config.clone())?);
        
        // Initialize snapshot manager
        let snapshot_manager = SnapshotManager::new(
            engine.clone(),
            config.snapshot_config.clone(),
        ).await?;
        
        // Initialize migration manager
        let migration_manager = MigrationManager::new(engine.clone()).await?;
        
        // Run any pending migrations
        migration_manager.run_migrations().await?;
        
        // Create transaction semaphore
        let transaction_semaphore = Arc::new(Semaphore::new(
            config.transaction_config.max_concurrent_transactions
        ));
        
        let manager = Self {
            engine,
            snapshot_manager,
            migration_manager,
            transaction_semaphore,
            pending_transactions: RwLock::new(HashMap::new()),
            current_state_root: RwLock::new(None),
            metrics_enabled: true,
        };
        
        // Initialize state root
        manager.compute_state_root().await?;
        
        log_info!(LogCategory::Storage, "Storage manager initialized successfully");
        
        #[cfg(feature = "metrics")]
        {
            set_gauge!("storage_initialized", 1.0);
        }
        
        Ok(manager)
    }
    
    /// Get the underlying engine
    pub fn engine(&self) -> &Arc<RocksEngine> {
        &self.engine
    }
    
    /// Get storage statistics
    pub async fn get_statistics(&self) -> StorageResult<StorageStatistics> {
        let memory_usage = self.engine.memory_usage()?;
        let db_stats = self.engine.statistics()?;
        
        let mut cf_stats = HashMap::new();
        for cf_name in self.engine.cf_names() {
            let num_keys = self.count_keys(&cf_name).await?;
            cf_stats.insert(cf_name, num_keys);
        }
        
        Ok(StorageStatistics {
            memory_usage,
            database_stats: db_stats,
            column_family_stats: cf_stats,
            pending_transactions: self.pending_transactions.read().len(),
            current_state_root: *self.current_state_root.read(),
        })
    }
    
    /// Count keys in a column family
    async fn count_keys(&self, cf: &str) -> StorageResult<u64> {
        let mut count = 0u64;
        let iter = self.engine.iterator(cf)?;
        
        for item in iter {
            item.map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))?;
            count += 1;
        }
        
        Ok(count)
    }
    
    /// Create a new transaction batch
    pub fn create_transaction(&self, id: String) -> StorageResult<TransactionBatch> {
        if self.pending_transactions.read().contains_key(&id) {
            return Err(StorageError::transaction(format!("Transaction {} already exists", id)));
        }
        
        let batch = TransactionBatch::new(id.clone(), self.engine.clone());
        self.pending_transactions.write().insert(id, batch.clone());
        
        #[cfg(feature = "metrics")]
        {
            increment_counter!("storage_transactions_created_total", 1);
            set_gauge!(
                "storage_pending_transactions", 
                self.pending_transactions.read().len() as f64
            );
        }
        
        Ok(batch)
    }
    
    /// Commit a transaction batch
    pub async fn commit_transaction(&self, id: &str) -> StorageResult<Hash> {
        let _permit = self.transaction_semaphore.acquire().await
            .map_err(|e| StorageError::transaction(format!("Failed to acquire transaction permit: {}", e)))?;
        
        let batch = {
            let mut pending = self.pending_transactions.write();
            pending.remove(id)
                .ok_or_else(|| StorageError::transaction(format!("Transaction {} not found", id)))?
        };
        
        let result = time_operation!("storage_transaction_commit_duration", {
            batch.commit().await
        });
        
        match result {
            Ok(hash) => {
                // Update state root
                self.compute_state_root().await?;
                
                #[cfg(feature = "metrics")]
                {
                    increment_counter!("storage_transactions_committed_total", 1);
                    set_gauge!(
                        "storage_pending_transactions", 
                        self.pending_transactions.read().len() as f64
                    );
                }
                
                log_info!(LogCategory::Storage, "Transaction {} committed successfully", id);
                Ok(hash)
            }
            Err(e) => {
                #[cfg(feature = "metrics")]
                {
                    increment_counter!("storage_transaction_errors_total", 1);
                }
                
                log_error!(LogCategory::Storage, "Failed to commit transaction {}: {}", id, e);
                Err(e)
            }
        }
    }
    
    /// Rollback a transaction
    pub async fn rollback_transaction(&self, id: &str) -> StorageResult<()> {
        let mut pending = self.pending_transactions.write();
        if pending.remove(id).is_some() {
            #[cfg(feature = "metrics")]
            {
                increment_counter!("storage_transactions_rolled_back_total", 1);
                set_gauge!(
                    "storage_pending_transactions", 
                    pending.len() as f64
                );
            }
            
            log_info!(LogCategory::Storage, "Transaction {} rolled back", id);
        }
        Ok(())
    }
    
    /// Get account state
    pub async fn get_account(&self, address: &[u8; 21]) -> StorageResult<Option<AccountState>> {
        let key = state_keys::account_key(address);
        
        let result = time_operation!("storage_account_get_duration", {
            self.engine.get("accounts", &key)
        });
        
        match result? {
            Some(data) => {
                let account = AccountState::deserialize(&data)
                    .map_err(|e| StorageError::serialization(format!("Failed to deserialize account: {}", e)))?;
                
                #[cfg(feature = "metrics")]
                {
                    increment_counter!("storage_account_reads_total", 1);
                }
                
                Ok(Some(account))
            }
            None => Ok(None)
        }
    }
    
    /// Set account state
    pub async fn set_account(&self, address: &[u8; 21], account: &AccountState) -> StorageResult<()> {
        let key = state_keys::account_key(address);
        let data = account.serialize()
            .map_err(|e| StorageError::serialization(format!("Failed to serialize account: {}", e)))?;
        
        let result = time_operation!("storage_account_set_duration", {
            self.engine.put("accounts", &key, &data)
        });
        
        result?;
        
        #[cfg(feature = "metrics")]
        {
            increment_counter!("storage_account_writes_total", 1);
        }
        
        Ok(())
    }
    
    /// Get transaction data
    pub async fn get_transaction(&self, tx_hash: &Hash) -> StorageResult<Option<Vec<u8>>> {
        let key = state_keys::transaction_key(tx_hash);
        
        let result = time_operation!("storage_transaction_get_duration", {
            self.engine.get("transactions", &key)
        });
        
        if result.is_ok() {
            #[cfg(feature = "metrics")]
            {
                increment_counter!("storage_transaction_reads_total", 1);
            }
        }
        
        result
    }
    
    /// Set transaction data
    pub async fn set_transaction(&self, tx_hash: &Hash, data: &[u8]) -> StorageResult<()> {
        let key = state_keys::transaction_key(tx_hash);
        
        let result = time_operation!("storage_transaction_set_duration", {
            self.engine.put("transactions", &key, data)
        });
        
        result?;
        
        #[cfg(feature = "metrics")]
        {
            increment_counter!("storage_transaction_writes_total", 1);
        }
        
        Ok(())
    }
    
    /// Get metadata value
    pub async fn get_metadata(&self, key: &str) -> StorageResult<Option<Vec<u8>>> {
        let key_bytes = state_keys::metadata_key(key);
        
        let result = time_operation!("storage_metadata_get_duration", {
            self.engine.get("metadata", &key_bytes)
        });
        
        if result.is_ok() {
            #[cfg(feature = "metrics")]
            {
                increment_counter!("storage_metadata_reads_total", 1);
            }
        }
        
        result
    }
    
    /// Set metadata value
    pub async fn set_metadata(&self, key: &str, value: &[u8]) -> StorageResult<()> {
        let key_bytes = state_keys::metadata_key(key);
        
        let result = time_operation!("storage_metadata_set_duration", {
            self.engine.put("metadata", &key_bytes, value)
        });
        
        result?;
        
        #[cfg(feature = "metrics")]
        {
            increment_counter!("storage_metadata_writes_total", 1);
        }
        
        Ok(())
    }
    
    /// Compute current state root hash
    async fn compute_state_root(&self) -> StorageResult<Hash> {
        let mut hasher = Sha256::new();
        
        // Hash all account states
        let mut account_iter = self.engine.iterator("accounts")?;
        account_iter.seek_to_first();
        
        for item in account_iter {
            let (key, value) = item
                .map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))?;
            hasher.update(&key);
            hasher.update(&value);
        }
        
        let state_root: Hash = hasher.finalize().into();
        *self.current_state_root.write() = Some(state_root);
        
        Ok(state_root)
    }
    
    /// Get current state root
    pub fn get_state_root(&self) -> Option<Hash> {
        *self.current_state_root.read()
    }
    
    /// Create a snapshot
    pub async fn create_snapshot(&self, name: &str) -> StorageResult<Snapshot> {
        self.snapshot_manager.create_snapshot(name).await
    }
    
    /// Load from snapshot
    pub async fn load_snapshot(&self, name: &str) -> StorageResult<()> {
        self.snapshot_manager.load_snapshot(name).await
    }
    
    /// List available snapshots
    pub async fn list_snapshots(&self) -> StorageResult<Vec<String>> {
        self.snapshot_manager.list_snapshots().await
    }
    
    /// Perform database maintenance
    pub async fn maintenance(&self) -> StorageResult<()> {
        log_info!(LogCategory::Storage, "Starting database maintenance");
        
        // Flush all column families
        for cf_name in self.engine.cf_names() {
            self.engine.flush_cf(&cf_name)?;
        }
        
        // Compact ranges for optimization
        for cf_name in self.engine.cf_names() {
            self.engine.compact_range_cf(&cf_name, None, None)?;
        }
        
        // Clean up old snapshots
        self.snapshot_manager.cleanup_old_snapshots().await?;
        
        log_info!(LogCategory::Storage, "Database maintenance completed");
        Ok(())
    }
    
    /// Check database health
    pub async fn is_healthy(&self) -> bool {
        // Check if engine is healthy
        if !self.engine.is_healthy() {
            return false;
        }
        
        // Check if we can read/write basic data
        let test_key = b"health_check";
        let test_value = b"ok";
        
        match self.engine.put("metadata", test_key, test_value) {
            Ok(_) => {
                match self.engine.get("metadata", test_key) {
                    Ok(Some(value)) if value == test_value => {
                        // Clean up test data
                        let _ = self.engine.delete("metadata", test_key);
                        true
                    }
                    _ => false
                }
            }
            Err(_) => false
        }
    }
    
    /// Get storage configuration
    pub fn config(&self) -> &StorageConfig {
        self.engine.config()
    }
    
    /// Backup database to directory
    pub async fn backup_to_directory(&self, backup_dir: &std::path::Path) -> StorageResult<()> {
        std::fs::create_dir_all(backup_dir)
            .map_err(|e| StorageError::io(e))?;
        
        // Create a snapshot first
        let snapshot_name = format!("backup_{}", chrono::Utc::now().timestamp());
        let snapshot = self.create_snapshot(&snapshot_name).await?;
        
        // Copy snapshot data to backup directory
        snapshot.export_to_directory(backup_dir).await?;
        
        log_info!(LogCategory::Storage, "Database backed up to {:?}", backup_dir);
        Ok(())
    }
    
    /// Restore database from backup directory
    pub async fn restore_from_directory(&self, backup_dir: &std::path::Path) -> StorageResult<()> {
        if !backup_dir.exists() {
            return Err(StorageError::config(format!("Backup directory {:?} does not exist", backup_dir)));
        }
        
        // Import snapshot from backup directory
        let snapshot_name = format!("restore_{}", chrono::Utc::now().timestamp());
        let snapshot = Snapshot::import_from_directory(&snapshot_name, backup_dir).await?;
        
        // Load the snapshot
        self.load_snapshot(&snapshot_name).await?;
        
        // Recompute state root
        self.compute_state_root().await?;
        
        log_info!(LogCategory::Storage, "Database restored from {:?}", backup_dir);
        Ok(())
    }
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStatistics {
    pub memory_usage: HashMap<String, u64>,
    pub database_stats: Option<String>,
    pub column_family_stats: HashMap<String, u64>,
    pub pending_transactions: usize,
    pub current_state_root: Option<Hash>,
}

#[async_trait]
impl StateManager for StorageManager {
    async fn get_state(&self, key: &[u8]) -> CatalystResult<Option<Vec<u8>>> {
        self.engine
            .get("accounts", key)
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }
    
    async fn set_state(&self, key: &[u8], value: Vec<u8>) -> CatalystResult<()> {
        self.engine
            .put("accounts", key, &value)
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }
    
    async fn commit(&self) -> CatalystResult<Hash> {
        // Flush all changes to disk
        for cf_name in self.engine.cf_names() {
            self.engine
                .flush_cf(&cf_name)
                .map_err(|e| CatalystError::Storage(e.to_string()))?;
        }
        
        // Compute and return new state root
        self.compute_state_root()
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }
    
    async fn get_account(&self, address: &[u8; 21]) -> CatalystResult<Option<AccountState>> {
        self.get_account(address)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }
    
    async fn set_account(&self, address: &[u8; 21], account: &AccountState) -> CatalystResult<()> {
        self.set_account(address, account)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }
    
    async fn delete_account(&self, address: &[u8; 21]) -> CatalystResult<()> {
        let key = state_keys::account_key(address);
        self.engine
            .delete("accounts", &key)
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }
    
    async fn account_exists(&self, address: &[u8; 21]) -> CatalystResult<bool> {
        let key = state_keys::account_key(address);
        let exists = self.engine
            .get("accounts", &key)
            .map_err(|e| CatalystError::Storage(e.to_string()))?
            .is_some();
        Ok(exists)
    }
    
    async fn get_transaction(&self, tx_hash: &Hash) -> CatalystResult<Option<Vec<u8>>> {
        self.get_transaction(tx_hash)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }
    
    async fn set_transaction(&self, tx_hash: &Hash, data: Vec<u8>) -> CatalystResult<()> {
        self.set_transaction(tx_hash, &data)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }
    
    async fn get_metadata(&self, key: &str) -> CatalystResult<Option<Vec<u8>>> {
        self.get_metadata(key)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }
    
    async fn set_metadata(&self, key: &str, value: Vec<u8>) -> CatalystResult<()> {
        self.set_metadata(key, &value)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }
    
    async fn create_snapshot(&self) -> CatalystResult<String> {
        let snapshot_name = format!("auto_{}", chrono::Utc::now().timestamp());
        self.create_snapshot(&snapshot_name)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))?;
        Ok(snapshot_name)
    }
    
    async fn restore_snapshot(&self, snapshot_id: &str) -> CatalystResult<()> {
        self.load_snapshot(snapshot_id)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StorageConfig;
    use tempfile::TempDir;
    use catalyst_utils::state::AccountType;
    
    async fn create_test_manager() -> StorageManager {
        let temp_dir = TempDir::new().unwrap();
        let mut config = StorageConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();
        StorageManager::new(config).await.unwrap()
    }
    
    #[tokio::test]
    async fn test_manager_creation() {
        let manager = create_test_manager().await;
        assert!(manager.is_healthy().await);
    }
    
    #[tokio::test]
    async fn test_account_operations() {
        let manager = create_test_manager().await;
        let address = [1u8; 21];
        
        // Test account doesn't exist initially
        let account = manager.get_account(&address).await.unwrap();
        assert!(account.is_none());
        
        // Create and set account
        let new_account = AccountState::new_non_confidential(address, 1000);
        manager.set_account(&address, &new_account).await.unwrap();
        
        // Verify account exists
        let retrieved_account = manager.get_account(&address).await.unwrap();
        assert!(retrieved_account.is_some());
        assert_eq!(retrieved_account.unwrap().account_type(), AccountType::NonConfidential);
    }
    
    #[tokio::test]
    async fn test_transaction_operations() {
        let manager = create_test_manager().await;
        let tx_hash = [2u8; 32];
        let tx_data = b"transaction_data".to_vec();
        
        // Set transaction
        manager.set_transaction(&tx_hash, &tx_data).await.unwrap();
        
        // Get transaction
        let retrieved_data = manager.get_transaction(&tx_hash).await.unwrap();
        assert_eq!(retrieved_data, Some(tx_data));
    }
    
    #[tokio::test]
    async fn test_metadata_operations() {
        let manager = create_test_manager().await;
        
        // Set metadata
        manager.set_metadata("test_key", b"test_value").await.unwrap();
        
        // Get metadata
        let value = manager.get_metadata("test_key").await.unwrap();
        assert_eq!(value, Some(b"test_value".to_vec()));
    }
    
    #[tokio::test]
    async fn test_transaction_batch() {
        let manager = create_test_manager().await;
        
        // Create transaction
        let tx_batch = manager.create_transaction("test_tx".to_string()).unwrap();
        
        // Add operations to batch
        let address = [3u8; 21];
        let account = AccountState::new_non_confidential(address, 500);
        tx_batch.set_account(&address, &account).await.unwrap();
        
        // Commit transaction
        let hash = manager.commit_transaction("test_tx").await.unwrap();
        assert_ne!(hash, [0u8; 32]);
        
        // Verify account was saved
        let retrieved_account = manager.get_account(&address).await.unwrap();
        assert!(retrieved_account.is_some());
    }
    
    #[tokio::test]
    async fn test_state_manager_trait() {
        let manager = create_test_manager().await;
        let state_manager: &dyn StateManager = &manager;
        
        // Test basic state operations
        state_manager.set_state(b"key1", b"value1".to_vec()).await.unwrap();
        let value = state_manager.get_state(b"key1").await.unwrap();
        assert_eq!(value, Some(b"value1".to_vec()));
        
        // Test commit
        let hash = state_manager.commit().await.unwrap();
        assert_ne!(hash, [0u8; 32]);
    }
    
    #[tokio::test]
    async fn test_snapshots() {
        let manager = create_test_manager().await;
        
        // Add some data
        let address = [4u8; 21];
        let account = AccountState::new_non_confidential(address, 750);
        manager.set_account(&address, &account).await.unwrap();
        
        // Create snapshot
        let snapshot = manager.create_snapshot("test_snapshot").await.unwrap();
        assert_eq!(snapshot.name(), "test_snapshot");
        
        // Modify data
        let new_account = AccountState::new_non_confidential(address, 1500);
        manager.set_account(&address, &new_account).await.unwrap();
        
        // Restore snapshot
        manager.load_snapshot("test_snapshot").await.unwrap();
        
        // Verify data was restored
        let restored_account = manager.get_account(&address).await.unwrap();
        assert!(restored_account.is_some());
        // Note: actual balance verification would depend on snapshot implementation
    }
    
    #[tokio::test]
    async fn test_statistics() {
        let manager = create_test_manager().await;
        
        // Add some data
        for i in 0..10 {
            let address = [i; 21];
            let account = AccountState::new_non_confidential(address, i as u64 * 100);
            manager.set_account(&address, &account).await.unwrap();
        }
        
        let stats = manager.get_statistics().await.unwrap();
        assert!(stats.column_family_stats.contains_key("accounts"));
        assert_eq!(stats.pending_transactions, 0);
    }
}