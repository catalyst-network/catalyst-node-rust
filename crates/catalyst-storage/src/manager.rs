//! Storage manager implementing the StateManager trait and providing high-level storage operations

use crate::{
    MigrationManager, RocksEngine, Snapshot, SnapshotManager, StorageConfig, StorageError,
    StorageResult, TransactionBatch,
};
use catalyst_utils::{
    async_trait,
    state::{AccountState, StateManager},
    Address, CatalystError, CatalystResult, Hash,
};
use parking_lot::RwLock;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

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
        // Validate configuration
        config
            .validate()
            .map_err(|e| StorageError::config(format!("Invalid configuration: {}", e)))?;

        // Create RocksDB engine
        let engine = Arc::new(RocksEngine::new(config.clone())?);

        // Initialize snapshot manager
        let snapshot_manager =
            SnapshotManager::new(engine.clone(), config.snapshot_config.clone()).await?;

        // Initialize migration manager
        let migration_manager = MigrationManager::new(engine.clone()).await?;

        // Run any pending migrations
        migration_manager.run_migrations().await?;

        // Create transaction semaphore
        let transaction_semaphore = Arc::new(Semaphore::new(
            config.transaction_config.max_concurrent_transactions,
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
            return Err(StorageError::transaction(format!(
                "Transaction {} already exists",
                id
            )));
        }

        let batch = TransactionBatch::new(id.clone(), self.engine.clone());
        self.pending_transactions.write().insert(id, batch.clone());

        Ok(batch)
    }

    /// Commit a transaction batch
    pub async fn commit_transaction(&self, id: &str) -> StorageResult<Hash> {
        let _permit = self.transaction_semaphore.acquire().await.map_err(|e| {
            StorageError::transaction(format!("Failed to acquire transaction permit: {}", e))
        })?;

        let batch = {
            let mut pending = self.pending_transactions.write();
            pending
                .remove(id)
                .ok_or_else(|| StorageError::transaction(format!("Transaction {} not found", id)))?
        };

        let result = batch.commit().await;

        match result {
            Ok(hash) => {
                // Update state root
                self.compute_state_root().await?;
                Ok(hash)
            }
            Err(e) => {
                // Treat committing an empty transaction as a no-op
                let msg = e.to_string().to_lowercase();
                if msg.contains("empty transaction") {
                    let root = self.compute_state_root().await?;
                    Ok(root)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Rollback a transaction
    pub async fn rollback_transaction(&self, id: &str) -> StorageResult<()> {
        let mut pending = self.pending_transactions.write();
        pending.remove(id);
        Ok(())
    }

    /// Get transaction data
    pub async fn get_transaction(&self, tx_hash: &Hash) -> StorageResult<Option<Vec<u8>>> {
        let key = [b"tx:", tx_hash.as_slice()].concat();
        self.engine.get("transactions", &key)
    }

    /// Set transaction data
    pub async fn set_transaction(&self, tx_hash: &Hash, data: &[u8]) -> StorageResult<()> {
        let key = [b"tx:", tx_hash.as_slice()].concat();
        self.engine.put("transactions", &key, data)
    }

    /// Get metadata value
    pub async fn get_metadata(&self, key: &str) -> StorageResult<Option<Vec<u8>>> {
        let key_bytes = [b"meta:", key.as_bytes()].concat();
        self.engine.get("metadata", &key_bytes)
    }

    /// Set metadata value
    pub async fn set_metadata(&self, key: &str, value: &[u8]) -> StorageResult<()> {
        let key_bytes = [b"meta:", key.as_bytes()].concat();
        self.engine.put("metadata", &key_bytes, value)
    }

    /// Compute current state root hash
    async fn compute_state_root(&self) -> StorageResult<Hash> {
        let mut hasher = Sha256::new();

        // Hash all account states
        let iter = self.engine.iterator("accounts")?;

        for item in iter {
            let (key, value) =
                item.map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))?;
            hasher.update(&key);
            hasher.update(&value);
        }

        let state_root: Hash = hasher.finalize().into();
        *self.current_state_root.write() = Some(state_root);

        Ok(state_root)
    }

    /// Get current state root
    pub fn get_state_root_sync(&self) -> Option<Hash> {
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

        Ok(())
    }

    /// Check database health
    pub async fn is_healthy(&self) -> bool {
        println!("🔍 Starting storage health check...");

        // Check engine health
        if !self.engine.is_healthy() {
            println!("❌ Engine health check failed");
            return false;
        }
        println!("✅ Engine is healthy");

        // Check column families
        let cf_names = self.engine.cf_names();
        println!("📁 Available column families: {:?}", cf_names);

        // Try a simple operation
        match self.engine.put("metadata", b"health_test", b"ok") {
            Ok(_) => {
                println!("✅ Write test successful");
                match self.engine.get("metadata", b"health_test") {
                    Ok(Some(_)) => {
                        println!("✅ Read test successful");
                        let _ = self.engine.delete("metadata", b"health_test");
                        true
                    }
                    Ok(None) => {
                        println!("❌ Read test failed - value not found");
                        false
                    }
                    Err(e) => {
                        println!("❌ Read test failed: {}", e);
                        false
                    }
                }
            }
            Err(e) => {
                println!("❌ Write test failed: {}", e);
                false
            }
        }
    }

    /// Get storage configuration
    pub fn config(&self) -> &StorageConfig {
        self.engine.config()
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
        match self.engine.get("accounts", key) {
            Ok(Some(_)) => {
                self.engine
                    .delete("accounts", key)
                    .map_err(|e| CatalystError::Storage(e.to_string()))?;
                Ok(true)
            }
            Ok(None) => Ok(false),
            Err(e) => Err(CatalystError::Storage(e.to_string())),
        }
    }

    async fn get_state_root(&self) -> CatalystResult<Hash> {
        self.compute_state_root()
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }

    async fn begin_transaction(&self) -> CatalystResult<u64> {
        let id = uuid::Uuid::new_v4().as_u128() as u64;
        let tx_id = id.to_string();
        self.create_transaction(tx_id)
            .map_err(|e| CatalystError::Storage(e.to_string()))?;
        Ok(id)
    }

    async fn commit_transaction(&self, tx_id: u64) -> CatalystResult<()> {
        let tx_id_str = tx_id.to_string();
        self.commit_transaction(&tx_id_str)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn rollback_transaction(&self, tx_id: u64) -> CatalystResult<()> {
        let tx_id_str = tx_id.to_string();
        self.rollback_transaction(&tx_id_str)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }

    async fn contains_key(&self, key: &[u8]) -> CatalystResult<bool> {
        match self.engine.get("accounts", key) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(CatalystError::Storage(e.to_string())),
        }
    }

    async fn get_many(&self, keys: &[Vec<u8>]) -> CatalystResult<Vec<Option<Vec<u8>>>> {
        let mut results = Vec::with_capacity(keys.len());

        for key in keys {
            let result = self.get_state(key).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Simple batched set that reuses set_state
    async fn set_many(&self, pairs: &[(Vec<u8>, Vec<u8>)]) -> CatalystResult<()> {
        for (key, value) in pairs {
            self.set_state(key, value.clone()).await?;
        }
        Ok(())
    }

    async fn commit(&self) -> CatalystResult<Hash> {
        self.get_state_root_sync()
            .ok_or_else(|| CatalystError::Storage("State root not computed".to_string()))
    }

    async fn create_snapshot(&self) -> CatalystResult<Hash> {
        // Return the current state root without creating a persisted snapshot here
        self.compute_state_root()
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }

    async fn restore_snapshot(&self, snapshot_id: &Hash) -> CatalystResult<()> {
        // Convert hash to snapshot name - simplified mapping
        let snapshot_name = format!("snapshot_{}", hex::encode(snapshot_id));
        self.load_snapshot(&snapshot_name)
            .await
            .map_err(|e| CatalystError::Storage(e.to_string()))
    }

    async fn get_account(&self, address: &Address) -> Result<Option<AccountState>, CatalystError> {
        let key = address.as_bytes();
        match self.get_state(key).await? {
            Some(data) => {
                let account: AccountState = serde_json::from_slice(&data)
                    .map_err(|e| CatalystError::Serialization(e.to_string()))?;
                Ok(Some(account))
            }
            None => Ok(None),
        }
    }

    async fn update_account(&self, account: &AccountState) -> Result<(), CatalystError> {
        let key = account.address.as_bytes();
        let data =
            serde_json::to_vec(account).map_err(|e| CatalystError::Serialization(e.to_string()))?;
        self.set_state(key, data).await
    }

    async fn account_exists(&self, address: &Address) -> Result<bool, CatalystError> {
        let key = address.as_bytes();
        self.contains_key(key).await
    }
}

// Keep these if your trait bounds require Send/Sync on implementors.
unsafe impl Send for StorageManager {}
unsafe impl Sync for StorageManager {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
    async fn test_transaction_operations() {
        let manager = create_test_manager().await;
        let tx_hash: Hash = [2u8; 32].into();
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
        manager
            .set_metadata("test_key", b"test_value")
            .await
            .unwrap();

        // Get metadata
        let value = manager.get_metadata("test_key").await.unwrap();
        assert_eq!(value, Some(b"test_value".to_vec()));
    }

    #[tokio::test]
    async fn test_state_manager_trait() {
        let manager = create_test_manager().await;
        let state_manager: &dyn StateManager = &manager;

        // Test basic state operations
        state_manager
            .set_state(b"key1", b"value1".to_vec())
            .await
            .unwrap();
        let value = state_manager.get_state(b"key1").await.unwrap();
        assert_eq!(value, Some(b"value1".to_vec()));

        // Test state root
        let hash = state_manager.get_state_root().await.unwrap();
        assert_ne!(hash, Hash::from([0u8; 32]));
    }

    #[tokio::test]
    async fn test_transaction_lifecycle() {
        let manager = create_test_manager().await;
        let state_manager: &dyn StateManager = &manager;

        // Begin transaction
        let tx_id = state_manager.begin_transaction().await.unwrap();

        // Commit transaction (no-ops are tolerated)
        state_manager.commit_transaction(tx_id).await.unwrap();
    }

    #[tokio::test]
    async fn test_key_operations() {
        let manager = create_test_manager().await;
        let state_manager: &dyn StateManager = &manager;

        // Key doesn't exist initially
        assert!(!state_manager.contains_key(b"nonexistent").await.unwrap());

        // Set key
        state_manager
            .set_state(b"testkey", b"testvalue".to_vec())
            .await
            .unwrap();

        // Key exists now
        assert!(state_manager.contains_key(b"testkey").await.unwrap());

        // Delete key
        let deleted = state_manager.delete_state(b"testkey").await.unwrap();
        assert!(deleted);

        // Key doesn't exist anymore
        assert!(!state_manager.contains_key(b"testkey").await.unwrap());
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let manager = create_test_manager().await;
        let state_manager: &dyn StateManager = &manager;

        let pairs: Vec<(Vec<u8>, Vec<u8>)> = vec![
            (b"key1".to_vec(), b"value1".to_vec()),
            (b"key2".to_vec(), b"value2".to_vec()),
        ];

        state_manager.set_many(&pairs).await.unwrap();

        let keys: Vec<Vec<u8>> = pairs.iter().map(|(k, _)| k.clone()).collect();
        let values = state_manager.get_many(&keys).await.unwrap();

        assert_eq!(values.len(), 2);
        assert_eq!(values[0], Some(b"value1".to_vec()));
        assert_eq!(values[1], Some(b"value2".to_vec()));
    }
}
