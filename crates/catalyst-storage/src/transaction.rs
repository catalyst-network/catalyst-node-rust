//! Transaction batching for atomic storage operations

use crate::{RocksEngine, StorageError, StorageResult};
use catalyst_utils::{
    log_debug, 
    Hash, 
    state::{AccountState, state_keys},
    serialization::CatalystSerialize,
    utils::current_timestamp,
    logging::LogCategory,
};
use rocksdb::{WriteBatch, WriteOptions};
use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use sha2::{Sha256, Digest};
use uuid::Uuid;

/// A transaction batch for atomic storage operations
#[derive(Clone)]
pub struct TransactionBatch {
    id: String,
    engine: Arc<RocksEngine>,
    operations: Arc<RwLock<Vec<BatchOperation>>>,
    metadata: Arc<RwLock<TransactionMetadata>>,
    committed: Arc<RwLock<bool>>,
}

/// Metadata for a transaction batch
#[derive(Debug, Clone)]
struct TransactionMetadata {
    created_at: u64,
    operation_count: usize,
    estimated_size: usize,
    checkpoints: Vec<String>,
}

/// Individual operation in a batch
#[derive(Debug, Clone)]
enum BatchOperation {
    PutAccount {
        address: [u8; 21],
        account: AccountState,
    },
    DeleteAccount {
        address: [u8; 21],
    },
    PutTransaction {
        hash: Hash,
        data: Vec<u8>,
    },
    DeleteTransaction {
        hash: Hash,
    },
    PutMetadata {
        key: String,
        value: Vec<u8>,
    },
    DeleteMetadata {
        key: String,
    },
    PutConsensus {
        key: Vec<u8>,
        value: Vec<u8>,
    },
    DeleteConsensus {
        key: Vec<u8>,
    },
    PutContract {
        address: [u8; 21],
        code: Vec<u8>,
        storage: HashMap<Vec<u8>, Vec<u8>>,
    },
    DeleteContract {
        address: [u8; 21],
    },
    PutDfsRef {
        hash: Hash,
        reference: Vec<u8>,
    },
    DeleteDfsRef {
        hash: Hash,
    },
}

impl TransactionBatch {
    /// Create a new transaction batch
    pub fn new(id: String, engine: Arc<RocksEngine>) -> Self {
        let metadata = TransactionMetadata {
            created_at: current_timestamp(),
            operation_count: 0,
            estimated_size: 0,
            checkpoints: Vec::new(),
        };
        
        Self {
            id,
            engine,
            operations: Arc::new(RwLock::new(Vec::new())),
            metadata: Arc::new(RwLock::new(metadata)),
            committed: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Get transaction ID
    pub fn id(&self) -> &str {
        &self.id
    }
    
    /// Check if transaction has been committed
    pub fn is_committed(&self) -> bool {
        *self.committed.read()
    }
    
    /// Get transaction metadata
    pub fn metadata(&self) -> TransactionMetadata {
        self.metadata.read().clone()
    }
    
    /// Add an account operation
    pub async fn set_account(&self, address: &[u8; 21], account: &AccountState) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let operation = BatchOperation::PutAccount {
            address: *address,
            account: account.clone(),
        };
        
        self.add_operation(operation).await
    }
    
    /// Delete an account
    pub async fn delete_account(&self, address: &[u8; 21]) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let operation = BatchOperation::DeleteAccount {
            address: *address,
        };
        
        self.add_operation(operation).await
    }
    
    /// Add a transaction operation
    pub async fn set_transaction(&self, hash: &Hash, data: &[u8]) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let operation = BatchOperation::PutTransaction {
            hash: *hash,
            data: data.to_vec(),
        };
        
        self.add_operation(operation).await
    }
    
    /// Delete a transaction
    pub async fn delete_transaction(&self, hash: &Hash) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let operation = BatchOperation::DeleteTransaction {
            hash: *hash,
        };
        
        self.add_operation(operation).await
    }
    
    /// Add a metadata operation
    pub async fn set_metadata(&self, key: &str, value: &[u8]) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let operation = BatchOperation::PutMetadata {
            key: key.to_string(),
            value: value.to_vec(),
        };
        
        self.add_operation(operation).await
    }
    
    /// Delete metadata
    pub async fn delete_metadata(&self, key: &str) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let operation = BatchOperation::DeleteMetadata {
            key: key.to_string(),
        };
        
        self.add_operation(operation).await
    }
    
    /// Add a consensus data operation
    pub async fn set_consensus(&self, key: &[u8], value: &[u8]) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let operation = BatchOperation::PutConsensus {
            key: key.to_vec(),
            value: value.to_vec(),
        };
        
        self.add_operation(operation).await
    }
    
    /// Delete consensus data
    pub async fn delete_consensus(&self, key: &[u8]) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let operation = BatchOperation::DeleteConsensus {
            key: key.to_vec(),
        };
        
        self.add_operation(operation).await
    }
    
    /// Add a smart contract operation
    pub async fn set_contract(
        &self,
        address: &[u8; 21],
        code: &[u8],
        storage: HashMap<Vec<u8>, Vec<u8>>,
    ) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let operation = BatchOperation::PutContract {
            address: *address,
            code: code.to_vec(),
            storage,
        };
        
        self.add_operation(operation).await
    }
    
    /// Delete a smart contract
    pub async fn delete_contract(&self, address: &[u8; 21]) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let operation = BatchOperation::DeleteContract {
            address: *address,
        };
        
        self.add_operation(operation).await
    }
    
    /// Add a DFS reference operation
    pub async fn set_dfs_ref(&self, hash: &Hash, reference: &[u8]) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let operation = BatchOperation::PutDfsRef {
            hash: *hash,
            reference: reference.to_vec(),
        };
        
        self.add_operation(operation).await
    }
    
    /// Delete a DFS reference
    pub async fn delete_dfs_ref(&self, hash: &Hash) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let operation = BatchOperation::DeleteDfsRef {
            hash: *hash,
        };
        
        self.add_operation(operation).await
    }
    
    /// Create a checkpoint
    pub async fn checkpoint(&self, name: &str) -> StorageResult<()> {
        self.check_not_committed()?;
        
        let mut metadata = self.metadata.write();
        metadata.checkpoints.push(name.to_string());
        
        log_debug!(
            LogCategory::Storage,
            "Created checkpoint '{}' for transaction {}",
            name,
            self.id
        );
        
        Ok(())
    }
    
    /// Add an operation to the batch
    async fn add_operation(&self, operation: BatchOperation) -> StorageResult<()> {
        let mut operations = self.operations.write();
        let mut metadata = self.metadata.write();
        
        // Estimate size of operation
        let op_size = self.estimate_operation_size(&operation);
        
        operations.push(operation);
        metadata.operation_count += 1;
        metadata.estimated_size += op_size;
        
        log_debug!(
            LogCategory::Storage,
            "Added operation to transaction {} (total: {} ops, ~{} bytes)",
            self.id,
            metadata.operation_count,
            metadata.estimated_size
        );
        
        Ok(())
    }
    
    /// Estimate the size of an operation in bytes
    fn estimate_operation_size(&self, operation: &BatchOperation) -> usize {
        match operation {
            BatchOperation::PutAccount { account, .. } => {
                // Approximate size for AccountState (it doesn't implement CatalystSerialize)
                // address(21) + type(1) + nonce(8) + balance(len) + optional data(len)
                21
                    + 1
                    + 8
                    + account.balance.len()
                    + account.data.as_ref().map(|d| d.len()).unwrap_or(0)
            }
            BatchOperation::DeleteAccount { .. } => 21,
            BatchOperation::PutTransaction { data, .. } => {
                32 + data.len() // hash + data
            }
            BatchOperation::DeleteTransaction { .. } => 32,
            BatchOperation::PutMetadata { key, value } => {
                key.len() + value.len()
            }
            BatchOperation::DeleteMetadata { key } => key.len(),
            BatchOperation::PutConsensus { key, value } => {
                key.len() + value.len()
            }
            BatchOperation::DeleteConsensus { key } => key.len(),
            BatchOperation::PutContract { code, storage, .. } => {
                21 + code.len() + storage.iter()
                    .map(|(k, v)| k.len() + v.len())
                    .sum::<usize>()
            }
            BatchOperation::DeleteContract { .. } => 21,
            BatchOperation::PutDfsRef { reference, .. } => {
                32 + reference.len() // hash + reference
            }
            BatchOperation::DeleteDfsRef { .. } => 32,
        }
    }
    
    /// Check if transaction is not committed
    fn check_not_committed(&self) -> StorageResult<()> {
        if *self.committed.read() {
            return Err(StorageError::transaction(
                format!("Transaction {} has already been committed", self.id)
            ));
        }
        Ok(())
    }
    
    /// Commit the transaction batch atomically
    pub async fn commit(&self) -> StorageResult<Hash> {
        self.check_not_committed()?;
        
        let operations = self.operations.read().clone();
        if operations.is_empty() {
            return Err(StorageError::transaction("Cannot commit empty transaction".to_string()));
        }
        
        log_debug!(
            LogCategory::Storage,
            "Committing transaction {} with {} operations",
            self.id,
            operations.len()
        );
        
        // Create RocksDB write batch
        let mut batch = WriteBatch::default();
        let mut hasher = Sha256::new();
        
        // Add transaction ID to hash
        hasher.update(self.id.as_bytes());
        
        // Process each operation
        for operation in &operations {
            match operation {
                BatchOperation::PutAccount { address, account } => {
                    let key = state_keys::account_key(address);
                    let value = account.serialize()
                        .map_err(|e| StorageError::serialization(e.to_string()))?;
                    
                    let cf_handle = self.engine.cf_handle("accounts")?;
                    batch.put_cf(&cf_handle, &key, &value);
                    
                    hasher.update(&key);
                    hasher.update(&value);
                }
                BatchOperation::DeleteAccount { address } => {
                    let key = state_keys::account_key(address);
                    let cf_handle = self.engine.cf_handle("accounts")?;
                    batch.delete_cf(&cf_handle, &key);
                    
                    hasher.update(&key);
                    hasher.update(b"DELETE");
                }
                BatchOperation::PutTransaction { hash, data } => {
                    let key = state_keys::transaction_key(hash);
                    let cf_handle = self.engine.cf_handle("transactions")?;
                    batch.put_cf(&cf_handle, &key, data);
                    
                    hasher.update(&key);
                    hasher.update(data);
                }
                BatchOperation::DeleteTransaction { hash } => {
                    let key = state_keys::transaction_key(hash);
                    let cf_handle = self.engine.cf_handle("transactions")?;
                    batch.delete_cf(&cf_handle, &key);
                    
                    hasher.update(&key);
                    hasher.update(b"DELETE");
                }
                BatchOperation::PutMetadata { key, value } => {
                    let key_bytes = state_keys::metadata_key(key);
                    let cf_handle = self.engine.cf_handle("metadata")?;
                    batch.put_cf(&cf_handle, &key_bytes, value);
                    
                    hasher.update(&key_bytes);
                    hasher.update(value);
                }
                BatchOperation::DeleteMetadata { key } => {
                    let key_bytes = state_keys::metadata_key(key);
                    let cf_handle = self.engine.cf_handle("metadata")?;
                    batch.delete_cf(&cf_handle, &key_bytes);
                    
                    hasher.update(&key_bytes);
                    hasher.update(b"DELETE");
                }
                BatchOperation::PutConsensus { key, value } => {
                    let cf_handle = self.engine.cf_handle("consensus")?;
                    batch.put_cf(&cf_handle, key, value);
                    
                    hasher.update(key);
                    hasher.update(value);
                }
                BatchOperation::DeleteConsensus { key } => {
                    let cf_handle = self.engine.cf_handle("consensus")?;
                    batch.delete_cf(&cf_handle, key);
                    
                    hasher.update(key);
                    hasher.update(b"DELETE");
                }
                BatchOperation::PutContract { address, code, storage } => {
                    let cf_handle = self.engine.cf_handle("contracts")?;
                    
                    // Store contract code
                    let code_key = [b"code:", address.as_slice()].concat();
                    batch.put_cf(&cf_handle, &code_key, code);
                    hasher.update(&code_key);
                    hasher.update(code);
                    
                    // Store contract storage
                    for (storage_key, storage_value) in storage {
                        let full_key = [b"storage:", address.as_slice(), b":", storage_key.as_slice()].concat();
                        batch.put_cf(&cf_handle, &full_key, storage_value);
                        hasher.update(&full_key);
                        hasher.update(storage_value);
                    }
                }
                BatchOperation::DeleteContract { address } => {
                    // Note: This is a simplified deletion. In practice, you'd need to
                    // iterate and delete all storage keys for this contract
                    let cf_handle = self.engine.cf_handle("contracts")?;
                    let code_key = [b"code:", address.as_slice()].concat();
                    batch.delete_cf(&cf_handle, &code_key);
                    
                    hasher.update(&code_key);
                    hasher.update(b"DELETE");
                }
                BatchOperation::PutDfsRef { hash, reference } => {
                    // Use transaction_key as a placeholder for DFS ref key since the function doesn't exist
                    let key = state_keys::transaction_key(hash);
                    let cf_handle = self.engine.cf_handle("dfs_refs")?;
                    batch.put_cf(&cf_handle, &key, reference);
                    
                    hasher.update(&key);
                    hasher.update(reference);
                }
                BatchOperation::DeleteDfsRef { hash } => {
                    // Use transaction_key as a placeholder for DFS ref key since the function doesn't exist
                    let key = state_keys::transaction_key(hash);
                    let cf_handle = self.engine.cf_handle("dfs_refs")?;
                    batch.delete_cf(&cf_handle, &key);
                    
                    hasher.update(&key);
                    hasher.update(b"DELETE");
                }
            }
        }
        
        // Configure write options for atomic commit
        let mut write_opts = WriteOptions::default();
        write_opts.set_sync(true); // Ensure durability
        
        // Execute the batch atomically
        self.engine.write_batch_with_options(batch, &write_opts)?;
        
        // Mark as committed
        *self.committed.write() = true;
        
        // Calculate transaction hash
        let tx_hash: Hash = hasher.finalize().into();
        
        log_debug!(
            LogCategory::Storage,
            "Transaction {} committed successfully with hash {:?}",
            self.id,
            hex::encode(tx_hash)
        );
        
        Ok(tx_hash)
    }
    
    /// Get operation count
    pub fn operation_count(&self) -> usize {
        self.operations.read().len()
    }
    
    /// Get estimated size in bytes
    pub fn estimated_size(&self) -> usize {
        self.metadata.read().estimated_size
    }
    
    /// Clear all operations (only if not committed)
    pub fn clear(&self) -> StorageResult<()> {
        self.check_not_committed()?;
        
        self.operations.write().clear();
        let mut metadata = self.metadata.write();
        metadata.operation_count = 0;
        metadata.estimated_size = 0;
        metadata.checkpoints.clear();
        
        Ok(())
    }
    
    /// Get a preview of operations without committing
    pub fn preview_operations(&self) -> Vec<String> {
        self.operations
            .read()
            .iter()
            .map(|op| self.operation_description(op))
            .collect()
    }
    
    /// Get a human-readable description of an operation
    fn operation_description(&self, operation: &BatchOperation) -> String {
        match operation {
            BatchOperation::PutAccount { address, .. } => {
                format!("PUT_ACCOUNT: {}", hex::encode(address))
            }
            BatchOperation::DeleteAccount { address } => {
                format!("DELETE_ACCOUNT: {}", hex::encode(address))
            }
            BatchOperation::PutTransaction { hash, data } => {
                format!("PUT_TRANSACTION: {} ({} bytes)", hex::encode(hash), data.len())
            }
            BatchOperation::DeleteTransaction { hash } => {
                format!("DELETE_TRANSACTION: {}", hex::encode(hash))
            }
            BatchOperation::PutMetadata { key, value } => {
                format!("PUT_METADATA: {} ({} bytes)", key, value.len())
            }
            BatchOperation::DeleteMetadata { key } => {
                format!("DELETE_METADATA: {}", key)
            }
            BatchOperation::PutConsensus { key, value } => {
                format!("PUT_CONSENSUS: {} ({} bytes)", hex::encode(key), value.len())
            }
            BatchOperation::DeleteConsensus { key } => {
                format!("DELETE_CONSENSUS: {}", hex::encode(key))
            }
            BatchOperation::PutContract { address, code, storage } => {
                format!(
                    "PUT_CONTRACT: {} (code: {} bytes, storage: {} entries)",
                    hex::encode(address),
                    code.len(),
                    storage.len()
                )
            }
            BatchOperation::DeleteContract { address } => {
                format!("DELETE_CONTRACT: {}", hex::encode(address))
            }
            BatchOperation::PutDfsRef { hash, reference } => {
                format!("PUT_DFS_REF: {} ({} bytes)", hex::encode(hash), reference.len())
            }
            BatchOperation::DeleteDfsRef { hash } => {
                format!("DELETE_DFS_REF: {}", hex::encode(hash))
            }
        }
    }
}

/// Builder for creating transaction batches with fluent interface
pub struct TransactionBatchBuilder {
    id: String,
    engine: Arc<RocksEngine>,
}

impl TransactionBatchBuilder {
    /// Create a new builder
    pub fn new(engine: Arc<RocksEngine>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            engine,
        }
    }
    
    /// Set custom transaction ID
    pub fn with_id(mut self, id: String) -> Self {
        self.id = id;
        self
    }
    
    /// Build the transaction batch
    pub fn build(self) -> TransactionBatch {
        TransactionBatch::new(self.id, self.engine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StorageConfig, RocksEngine};
    use tempfile::TempDir;
    use catalyst_utils::state::{AccountState, AccountType};
    
    fn create_test_engine() -> Arc<RocksEngine> {
        let temp_dir = TempDir::new().unwrap();
        let mut config = StorageConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();
        Arc::new(RocksEngine::new(config).unwrap())
    }
    
    #[tokio::test]
    async fn test_transaction_batch_creation() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("test_tx".to_string(), engine);
        
        assert_eq!(batch.id(), "test_tx");
        assert!(!batch.is_committed());
        assert_eq!(batch.operation_count(), 0);
    }
    
    #[tokio::test]
    async fn test_account_operations() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("test_tx".to_string(), engine);
        
        let address = [1u8; 21];
        let account = AccountState::new_non_confidential(address, 1000);
        
        // Add account operation
        batch.set_account(&address, &account).await.unwrap();
        assert_eq!(batch.operation_count(), 1);
        
        // Add delete operation
        batch.delete_account(&address).await.unwrap();
        assert_eq!(batch.operation_count(), 2);
    }
    
    #[tokio::test]
    async fn test_transaction_operations() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("test_tx".to_string(), engine);
        
        let tx_hash = [2u8; 32];
        let tx_data = b"transaction_data";
        
        batch.set_transaction(&tx_hash, tx_data).await.unwrap();
        assert_eq!(batch.operation_count(), 1);
        
        batch.delete_transaction(&tx_hash).await.unwrap();
        assert_eq!(batch.operation_count(), 2);
    }
    
    #[tokio::test]
    async fn test_metadata_operations() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("test_tx".to_string(), engine);
        
        batch.set_metadata("key1", b"value1").await.unwrap();
        batch.set_metadata("key2", b"value2").await.unwrap();
        batch.delete_metadata("key3").await.unwrap();
        
        assert_eq!(batch.operation_count(), 3);
    }
    
    #[tokio::test]
    async fn test_contract_operations() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("test_tx".to_string(), engine);
        
        let address = [3u8; 21];
        let code = b"contract_bytecode";
        let mut storage = HashMap::new();
        storage.insert(b"storage_key".to_vec(), b"storage_value".to_vec());
        
        batch.set_contract(&address, code, storage).await.unwrap();
        assert_eq!(batch.operation_count(), 1);
        
        batch.delete_contract(&address).await.unwrap();
        assert_eq!(batch.operation_count(), 2);
    }
    
    #[tokio::test]
    async fn test_dfs_operations() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("test_tx".to_string(), engine);
        
        let hash = [4u8; 32];
        let reference = b"ipfs_hash_reference";
        
        batch.set_dfs_ref(&hash, reference).await.unwrap();
        assert_eq!(batch.operation_count(), 1);
        
        batch.delete_dfs_ref(&hash).await.unwrap();
        assert_eq!(batch.operation_count(), 2);
    }
    
    #[tokio::test]
    async fn test_checkpoints() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("test_tx".to_string(), engine);
        
        // Add some operations
        let address = [5u8; 21];
        let account = AccountState::new_non_confidential(address, 500);
        batch.set_account(&address, &account).await.unwrap();
        
        // Create checkpoint
        batch.checkpoint("after_account").await.unwrap();
        
        // Add more operations
        batch.set_metadata("test_key", b"test_value").await.unwrap();
        batch.checkpoint("after_metadata").await.unwrap();
        
        let metadata = batch.metadata();
        assert_eq!(metadata.checkpoints.len(), 2);
        assert_eq!(metadata.checkpoints[0], "after_account");
        assert_eq!(metadata.checkpoints[1], "after_metadata");
    }
    
    #[tokio::test]
    async fn test_commit() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("test_tx".to_string(), engine.clone());
        
        // Add operations
        let address = [6u8; 21];
        let account = AccountState::new_non_confidential(address, 750);
        batch.set_account(&address, &account).await.unwrap();
        
        let tx_hash = [7u8; 32];
        batch.set_transaction(&tx_hash, b"tx_data").await.unwrap();
        
        // Commit
        let commit_hash = batch.commit().await.unwrap();
        assert_ne!(commit_hash, [0u8; 32]);
        assert!(batch.is_committed());
        
        // Verify data was written
        let key = state_keys::account_key(&address);
        let stored_data = engine.get("accounts", &key).unwrap();
        assert!(stored_data.is_some());
    }
    
    #[tokio::test]
    async fn test_commit_empty_batch() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("empty_tx".to_string(), engine);
        
        let result = batch.commit().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty transaction"));
    }
    
    #[tokio::test]
    async fn test_operations_after_commit() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("test_tx".to_string(), engine);
        
        // Add operation and commit
        let address = [8u8; 21];
        let account = AccountState::new_non_confidential(address, 100);
        batch.set_account(&address, &account).await.unwrap();
        let _commit_hash = batch.commit().await.unwrap();
        
        // Try to add more operations after commit
        let result = batch.set_metadata("key", b"value").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already been committed"));
    }
    
    #[tokio::test]
    async fn test_builder_pattern() {
        let engine = create_test_engine();
        
        let batch = TransactionBatchBuilder::new(engine)
            .with_id("custom_id".to_string())
            .build();
        
        assert_eq!(batch.id(), "custom_id");
    }
    
    #[tokio::test]
    async fn test_operation_preview() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("test_tx".to_string(), engine);
        
        let address = [9u8; 21];
        let account = AccountState::new_non_confidential(address, 200);
        batch.set_account(&address, &account).await.unwrap();
        batch.set_metadata("test_key", b"test_value").await.unwrap();
        
        let preview = batch.preview_operations();
        assert_eq!(preview.len(), 2);
        assert!(preview[0].starts_with("PUT_ACCOUNT:"));
        assert!(preview[1].starts_with("PUT_METADATA:"));
    }
    
    #[tokio::test]
    async fn test_size_estimation() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("test_tx".to_string(), engine);
        
        let initial_size = batch.estimated_size();
        assert_eq!(initial_size, 0);
        
        let address = [10u8; 21];
        let account = AccountState::new_non_confidential(address, 300);
        batch.set_account(&address, &account).await.unwrap();
        
        let size_after_account = batch.estimated_size();
        assert!(size_after_account > initial_size);
        
        batch.set_metadata("large_key", &vec![0u8; 1000]).await.unwrap();
        
        let final_size = batch.estimated_size();
        assert!(final_size > size_after_account + 1000); // key + value
    }
    
    #[tokio::test]
    async fn test_clear_operations() {
        let engine = create_test_engine();
        let batch = TransactionBatch::new("test_tx".to_string(), engine);
        
        // Add operations
        let address = [11u8; 21];
        let account = AccountState::new_non_confidential(address, 400);
        batch.set_account(&address, &account).await.unwrap();
        batch.set_metadata("key", b"value").await.unwrap();
        
        assert_eq!(batch.operation_count(), 2);
        assert!(batch.estimated_size() > 0);
        
        // Clear operations
        batch.clear().unwrap();
        
        assert_eq!(batch.operation_count(), 0);
        assert_eq!(batch.estimated_size(), 0);
    }
}