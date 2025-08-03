// catalyst-utils/src/state.rs

use crate::{CatalystResult, Hash, Address};
use async_trait::async_trait;
use serde::{Serialize, Deserialize};

/// Core trait for managing ledger state across the Catalyst network
/// 
/// The StateManager provides a unified interface for state operations that can be 
/// implemented by different storage backends (RocksDB, in-memory, etc.)
#[async_trait]
pub trait StateManager: Send + Sync {
    /// Retrieve a value from state by key
    /// 
    /// # Arguments
    /// * `key` - The key to look up in state
    /// 
    /// # Returns
    /// * `Ok(Some(value))` if the key exists
    /// * `Ok(None)` if the key doesn't exist
    /// * `Err(_)` on storage errors
    async fn get_state(&self, key: &[u8]) -> CatalystResult<Option<Vec<u8>>>;
    
    /// Set a value in state
    /// 
    /// # Arguments
    /// * `key` - The key to store under
    /// * `value` - The value to store
    /// 
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err(_)` on storage errors
    async fn set_state(&self, key: &[u8], value: Vec<u8>) -> CatalystResult<()>;
    
    /// Delete a key from state
    /// 
    /// # Arguments
    /// * `key` - The key to delete
    /// 
    /// # Returns
    /// * `Ok(true)` if the key was deleted
    /// * `Ok(false)` if the key didn't exist
    /// * `Err(_)` on storage errors
    async fn delete_state(&self, key: &[u8]) -> CatalystResult<bool>;
    
    /// Commit all pending changes and return the state root hash
    /// 
    /// This should atomically apply all changes made since the last commit
    /// and return a cryptographic hash representing the new state.
    /// 
    /// # Returns
    /// * `Ok(hash)` containing the new state root hash
    /// * `Err(_)` on commit failures
    async fn commit(&self) -> CatalystResult<Hash>;
    
    /// Get the current state root hash without committing changes
    /// 
    /// # Returns
    /// * `Ok(hash)` containing the current state root hash
    /// * `Err(_)` on errors
    async fn get_state_root(&self) -> CatalystResult<Hash>;
    
    /// Begin a new transaction/batch for atomic operations
    /// 
    /// This allows grouping multiple state operations together
    /// so they can be committed or rolled back atomically.
    /// 
    /// # Returns
    /// * `Ok(transaction_id)` that can be used to identify this transaction
    /// * `Err(_)` on errors
    async fn begin_transaction(&self) -> CatalystResult<u64>;
    
    /// Commit a specific transaction
    /// 
    /// # Arguments
    /// * `transaction_id` - The transaction to commit
    /// 
    /// # Returns
    /// * `Ok(())` on successful commit
    /// * `Err(_)` on commit failures
    async fn commit_transaction(&self, transaction_id: u64) -> CatalystResult<()>;
    
    /// Rollback a specific transaction
    /// 
    /// # Arguments
    /// * `transaction_id` - The transaction to rollback
    /// 
    /// # Returns
    /// * `Ok(())` on successful rollback
    /// * `Err(_)` on rollback failures
    async fn rollback_transaction(&self, transaction_id: u64) -> CatalystResult<()>;
    
    /// Check if a key exists in state
    /// 
    /// # Arguments
    /// * `key` - The key to check
    /// 
    /// # Returns
    /// * `Ok(true)` if the key exists
    /// * `Ok(false)` if the key doesn't exist
    /// * `Err(_)` on storage errors
    async fn contains_key(&self, key: &[u8]) -> CatalystResult<bool>;
    
    /// Get multiple values by their keys efficiently
    /// This method signature uses a concrete slice type to avoid lifetime issues
    async fn get_many(&self, keys: &[Vec<u8>]) -> CatalystResult<Vec<Option<Vec<u8>>>>;
    
    /// Set multiple key-value pairs efficiently
    /// This method signature uses a concrete slice type to avoid lifetime issues
    async fn set_many(&self, pairs: &[(Vec<u8>, Vec<u8>)]) -> CatalystResult<()>;
    
    /// Create a snapshot of the current state
    /// 
    /// This creates a point-in-time view of the state that can be restored later.
    /// Useful for rollbacks and state migrations.
    /// 
    /// # Returns
    /// * `Ok(snapshot_id)` that can be used to restore this state later
    /// * `Err(_)` on snapshot creation failures
    async fn create_snapshot(&self) -> CatalystResult<Hash>;
    
    /// Restore state from a snapshot
    /// 
    /// # Arguments
    /// * `snapshot_id` - The snapshot to restore from
    /// 
    /// # Returns
    /// * `Ok(())` on successful restore
    /// * `Err(_)` on restore failures
    async fn restore_snapshot(&self, snapshot_id: &Hash) -> CatalystResult<()>;
    
    /// High-level account methods
    async fn get_account(&self, address: &Address) -> CatalystResult<Option<AccountState>>;
    async fn update_account(&self, account: &AccountState) -> CatalystResult<()>;
    async fn account_exists(&self, address: &Address) -> CatalystResult<bool>;
}

/// Account types supported by the Catalyst ledger
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountType {
    /// Standard account with visible balance
    NonConfidential,
    /// Account with hidden balance using Pedersen commitments
    Confidential,
    /// Smart contract account
    Contract,
}

/// Account state structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    /// Account address
    pub address: Address,
    /// Account type
    pub account_type: AccountType,
    /// Account balance (visible for non-confidential, commitment for confidential)
    pub balance: Vec<u8>,
    /// Optional contract data (max 64 bytes)
    pub data: Option<Vec<u8>>,
    /// Nonce for transaction ordering
    pub nonce: u64,
}

impl AccountState {
    /// Create a new non-confidential account
    pub fn new_non_confidential(address: Address, balance: u64) -> Self {
        Self {
            address,
            account_type: AccountType::NonConfidential,
            balance: balance.to_le_bytes().to_vec(),
            data: None,
            nonce: 0,
        }
    }
    
    /// Create a new confidential account with Pedersen commitment
    pub fn new_confidential(address: Address, commitment: [u8; 32]) -> Self {
        Self {
            address,
            account_type: AccountType::Confidential,
            balance: commitment.to_vec(),
            data: None,
            nonce: 0,
        }
    }
    
    /// Create a new contract account
    pub fn new_contract(address: Address, balance: u64, contract_data: Option<Vec<u8>>) -> Self {
        // Ensure contract data doesn't exceed 64 bytes
        let data = contract_data.and_then(|d| {
            if d.len() <= 64 {
                Some(d)
            } else {
                None
            }
        });
        
        Self {
            address,
            account_type: AccountType::Contract,
            balance: balance.to_le_bytes().to_vec(),
            data,
            nonce: 0,
        }
    }
    
    /// Get balance as u64 for non-confidential accounts
    pub fn get_balance_as_u64(&self) -> Option<u64> {
        match self.account_type {
            AccountType::NonConfidential | AccountType::Contract => {
                if self.balance.len() == 8 {
                    let mut bytes = [0u8; 8];
                    bytes.copy_from_slice(&self.balance);
                    Some(u64::from_le_bytes(bytes))
                } else {
                    None
                }
            }
            AccountType::Confidential => None,
        }
    }
    
    /// Get commitment for confidential accounts
    pub fn get_commitment(&self) -> Option<[u8; 32]> {
        match self.account_type {
            AccountType::Confidential => {
                if self.balance.len() == 32 {
                    let mut commitment = [0u8; 32];
                    commitment.copy_from_slice(&self.balance);
                    Some(commitment)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// State key prefixes for different data types
pub mod state_keys {
    use super::{Hash, Address};
    
    /// Prefix for account state entries
    pub const ACCOUNT_PREFIX: &[u8] = b"acc:";
    /// Prefix for transaction entries
    pub const TRANSACTION_PREFIX: &[u8] = b"tx:";
    /// Prefix for ledger metadata
    pub const METADATA_PREFIX: &[u8] = b"meta:";
    /// Prefix for producer information
    pub const PRODUCER_PREFIX: &[u8] = b"prod:";
    /// Prefix for worker pool data
    pub const WORKER_PREFIX: &[u8] = b"work:";
    
    /// Create an account state key from address
    pub fn account_key(address: &Address) -> Vec<u8> {
        let mut key = ACCOUNT_PREFIX.to_vec();
        key.extend_from_slice(address.as_slice());
        key
    }
    
    /// Create a transaction key from hash
    pub fn transaction_key(tx_hash: &Hash) -> Vec<u8> {
        let mut key = TRANSACTION_PREFIX.to_vec();
        key.extend_from_slice(tx_hash.as_slice());
        key
    }
    
    /// Create a metadata key
    pub fn metadata_key(name: &str) -> Vec<u8> {
        let mut key = METADATA_PREFIX.to_vec();
        key.extend_from_slice(name.as_bytes());
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_account_state_creation() {
        let address = Address::new([1u8; 20]);
        
        // Test non-confidential account
        let non_conf = AccountState::new_non_confidential(address, 1000);
        assert_eq!(non_conf.account_type, AccountType::NonConfidential);
        assert_eq!(non_conf.get_balance_as_u64(), Some(1000));
        assert!(non_conf.get_commitment().is_none());
        
        // Test confidential account
        let commitment = [2u8; 32];
        let conf = AccountState::new_confidential(address, commitment);
        assert_eq!(conf.account_type, AccountType::Confidential);
        assert!(conf.get_balance_as_u64().is_none());
        assert_eq!(conf.get_commitment(), Some(commitment));
        
        // Test contract account
        let contract_data = vec![1, 2, 3, 4];
        let contract = AccountState::new_contract(address, 500, Some(contract_data.clone()));
        assert_eq!(contract.account_type, AccountType::Contract);
        assert_eq!(contract.get_balance_as_u64(), Some(500));
        assert_eq!(contract.data, Some(contract_data));
    }
    
    #[test]
    fn test_state_keys() {
        let address = Address::new([1u8; 20]);
        let tx_hash = Hash::new([2u8; 32]);
        
        let acc_key = state_keys::account_key(&address);
        assert!(acc_key.starts_with(state_keys::ACCOUNT_PREFIX));
        assert_eq!(acc_key.len(), state_keys::ACCOUNT_PREFIX.len() + 20);
        
        let tx_key = state_keys::transaction_key(&tx_hash);
        assert!(tx_key.starts_with(state_keys::TRANSACTION_PREFIX));
        assert_eq!(tx_key.len(), state_keys::TRANSACTION_PREFIX.len() + 32);
        
        let meta_key = state_keys::metadata_key("ledger_height");
        assert!(meta_key.starts_with(state_keys::METADATA_PREFIX));
    }
    
    #[test]
    fn test_serialization() {
        let address = Address::new([1u8; 20]);
        let account = AccountState::new_non_confidential(address, 1000);
        
        // Test that AccountState can be serialized and deserialized
        let serialized = serde_json::to_vec(&account).unwrap();
        let deserialized: AccountState = serde_json::from_slice(&serialized).unwrap();
        
        assert_eq!(account.address, deserialized.address);
        assert_eq!(account.account_type, deserialized.account_type);
        assert_eq!(account.get_balance_as_u64(), deserialized.get_balance_as_u64());
    }
}