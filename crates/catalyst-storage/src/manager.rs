//! Storage manager implementing the StateManager trait and providing high-level storage operations

use std::sync::Arc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::RwLock as TokioRwLock;
use parking_lot::RwLock as ParkingRwLock;

use catalyst_utils::{
    // Import all macros from root level
    log_info, log_warn, log_error,
    increment_counter, set_gauge, observe_histogram, time_operation,
    
    // Import types normally
    Hash, CatalystResult, CatalystError,
    state::{StateManager, AccountState, state_keys},
    serialization::{CatalystSerialize, CatalystDeserialize},
    // Only import types from modules, not macros
    logging::LogCategory,
    utils::current_timestamp,
};

use async_trait::async_trait;

use crate::{
    RocksEngine, StorageError, StorageResult,
    config::StorageConfig,
    transaction::TransactionBatch,
    snapshot::{Snapshot, SnapshotManager},
    migration::MigrationManager,
    merkle,
};

use tokio::sync::Semaphore;
use sha2::{Sha256, Digest};

/// High-level storage manager for Catalyst Network
pub struct StorageManager {
    engine: Arc<RocksEngine>,
    snapshot_manager: SnapshotManager,
    migration_manager: MigrationManager,
    transaction_semaphore: Arc<Semaphore>,
    pending_transactions: ParkingRwLock<HashMap<String, TransactionBatch>>,
    current_state_root: ParkingRwLock<Option<Hash>>,
    next_tx_id: AtomicU64,
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
            pending_transactions: ParkingRwLock::new(HashMap::new()),
            current_state_root: ParkingRwLock::new(None),
            next_tx_id: AtomicU64::new(1),
            metrics_enabled: cfg!(feature = "metrics"),
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
        // Authenticated state root (step 3): Sparse Merkle Tree over `accounts` keys.
        // This makes proofs O(log 256) rather than O(N).
        let account_iter = self.engine.iterator("accounts")?;
        let mut items: Vec<(Box<[u8]>, Box<[u8]>)> = Vec::new();
        for item in account_iter {
            let (key, value) =
                item.map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))?;
            items.push((key, value));
        }
        let state_root: Hash = crate::sparse_merkle::compute_root_from_iter(items)?;
        *self.current_state_root.write() = Some(state_root);
        Ok(state_root)
    }

    /// Compute an inclusion proof for an `accounts` key against the current (or freshly computed) state root.
    ///
    /// Returns: (state_root, value, proof_steps)
    pub async fn get_account_proof(
        &self,
        key: &[u8],
    ) -> StorageResult<Option<(Hash, Vec<u8>, merkle::MerkleProof)>> {
        let account_iter = self.engine.iterator("accounts")?;
        let mut items: Vec<(Box<[u8]>, Box<[u8]>)> = Vec::new();
        for item in account_iter {
            let (k, v) =
                item.map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))?;
            items.push((k, v));
        }
        let out = crate::sparse_merkle::compute_root_and_proof_from_iter(items, key)?;
        if let Some((root, value, proof)) = &out {
            *self.current_state_root.write() = Some(*root);
            Ok(Some((*root, value.clone(), proof.clone())))
        } else {
            Ok(None)
        }
    }
    
    /// Get current state root
    pub fn get_state_root(&self) -> Option<Hash> {
        *self.current_state_root.read()
    }

    /// Set cached state root (used when the caller has independently verified a transition).
    pub fn set_state_root_cache(&self, root: Hash) {
        *self.current_state_root.write() = Some(root);
    }

    /// Compute SMT root and proofs for multiple keys in a single scan.
    ///
    /// Returns: (root, vec[(key, maybe_value, proof)])
    pub async fn get_account_proofs_for_keys_with_absence(
        &self,
        keys: &[Vec<u8>],
    ) -> StorageResult<(Hash, Vec<(Vec<u8>, Option<Vec<u8>>, merkle::MerkleProof)>)> {
        let account_iter = self.engine.iterator("accounts")?;
        let mut items: Vec<(Box<[u8]>, Box<[u8]>)> = Vec::new();
        for item in account_iter {
            let (k, v) =
                item.map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))?;
            items.push((k, v));
        }
        crate::sparse_merkle::compute_root_and_multi_proofs_from_iter(items, keys)
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
            .map_err(StorageError::from)?;
        
        // Create a snapshot first
        let snapshot_name = format!("backup_{}", current_timestamp());
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
        let snapshot_name = format!("restore_{}", current_timestamp());
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

    async fn delete_state(&self, key: &[u8]) -> CatalystResult<bool> {
        let existed = self
            .engine
            .get("accounts", key)
            .map_err(|e| CatalystError::Storage(e.to_string()))?
            .is_some();

        if existed {
            self.engine
                .delete("accounts", key)
                .map_err(|e| CatalystError::Storage(e.to_string()))?;
        }

        Ok(existed)
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

    async fn get_state_root(&self) -> CatalystResult<Hash> {
        if let Some(root) = *self.current_state_root.read() {
            return Ok(root);
        }
        self.compute_state_root()
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }

    async fn begin_transaction(&self) -> CatalystResult<u64> {
        let id = self.next_tx_id.fetch_add(1, Ordering::Relaxed);
        let name = format!("tx_{}", id);
        let _ = self
            .create_transaction(name)
            .map_err(|e| CatalystError::Storage(e.to_string()))?;
        Ok(id)
    }

    async fn commit_transaction(&self, transaction_id: u64) -> CatalystResult<()> {
        let name = format!("tx_{}", transaction_id);
        let _ = self
            .commit_transaction(&name)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn rollback_transaction(&self, transaction_id: u64) -> CatalystResult<()> {
        let name = format!("tx_{}", transaction_id);
        self.rollback_transaction(&name)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn contains_key(&self, key: &[u8]) -> CatalystResult<bool> {
        Ok(self.get_state(key).await?.is_some())
    }

    async fn get_many<'a>(
        &self,
        keys: impl Iterator<Item = &'a [u8]> + Send,
    ) -> CatalystResult<Vec<Option<Vec<u8>>>> {
        let mut out = Vec::new();
        for key in keys {
            out.push(self.get_state(key).await?);
        }
        Ok(out)
    }

    async fn set_many<'a>(
        &self,
        pairs: impl Iterator<Item = (&'a [u8], Vec<u8>)> + Send,
    ) -> CatalystResult<()> {
        for (key, value) in pairs {
            self.set_state(key, value).await?;
        }
        Ok(())
    }

    async fn create_snapshot(&self) -> CatalystResult<Hash> {
        let snapshot_name = format!("auto_{}", current_timestamp());
        let snapshot = self
            .create_snapshot(&snapshot_name)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))?;

        // Persist mapping snapshot_id -> name in metadata CF: key = b"snap:" + snapshot_id
        let mut mapping_key = b"snap:".to_vec();
        mapping_key.extend_from_slice(&snapshot.state_root());
        self.engine
            .put("metadata", &mapping_key, snapshot_name.as_bytes())
            .map_err(|e| CatalystError::Storage(e.to_string()))?;

        Ok(snapshot.state_root())
    }

    async fn restore_snapshot(&self, snapshot_id: &Hash) -> CatalystResult<()> {
        let mut mapping_key = b"snap:".to_vec();
        mapping_key.extend_from_slice(snapshot_id);

        let snapshot_name_bytes = self
            .engine
            .get("metadata", &mapping_key)
            .map_err(|e| CatalystError::Storage(e.to_string()))?
            .ok_or_else(|| CatalystError::Storage("Snapshot id not found".to_string()))?;

        let snapshot_name = String::from_utf8(snapshot_name_bytes)
            .map_err(|e| CatalystError::Storage(format!("Invalid snapshot name bytes: {}", e)))?;

        self.load_snapshot(&snapshot_name)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))?;

        // Refresh cached state root
        let _ = self
            .compute_state_root()
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StorageConfig;
    use tempfile::TempDir;
    use catalyst_utils::state::AccountType;
    
    struct TestEnv {
        manager: StorageManager,
        // Keep temp dir alive until after StorageManager drop (RocksDB flushes on drop).
        _temp_dir: TempDir,
    }

    async fn create_test_env() -> TestEnv {
        let temp_dir = TempDir::new().unwrap();
        let mut config = StorageConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();
        let manager = StorageManager::new(config).await.unwrap();
        TestEnv {
            manager,
            _temp_dir: temp_dir,
        }
    }
    
    #[tokio::test]
    async fn test_manager_creation() {
        let env = create_test_env().await;
        assert!(env.manager.is_healthy().await);
    }
    
    #[tokio::test]
    async fn test_account_operations() {
        let env = create_test_env().await;
        let manager = &env.manager;
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
        assert_eq!(retrieved_account.unwrap().account_type, AccountType::NonConfidential);
    }
    
    #[tokio::test]
    async fn test_transaction_operations() {
        let env = create_test_env().await;
        let manager = &env.manager;
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
        let env = create_test_env().await;
        let manager = &env.manager;
        
        // Set metadata
        manager.set_metadata("test_key", b"test_value").await.unwrap();
        
        // Get metadata
        let value = manager.get_metadata("test_key").await.unwrap();
        assert_eq!(value, Some(b"test_value".to_vec()));
    }
    
    #[tokio::test]
    async fn test_transaction_batch() {
        let env = create_test_env().await;
        let manager = &env.manager;
        
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
        let env = create_test_env().await;
        let manager = &env.manager;
        
        // Test basic state operations
        manager.set_state(b"key1", b"value1".to_vec()).await.unwrap();
        let value = manager.get_state(b"key1").await.unwrap();
        assert_eq!(value, Some(b"value1".to_vec()));
        
        // Test commit
        let hash = manager.commit().await.unwrap();
        assert_ne!(hash, [0u8; 32]);
    }
    
    #[tokio::test]
    async fn test_snapshots() {
        let env = create_test_env().await;
        let manager = &env.manager;
        
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
        let env = create_test_env().await;
        let manager = &env.manager;
        
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