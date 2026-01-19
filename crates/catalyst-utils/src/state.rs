// catalyst-utils/src/state.rs

use crate::error::{CatalystError, CatalystResult};
use crate::serialization::{CatalystDeserialize, CatalystSerialize};
use async_trait::async_trait;
use std::io::{Cursor, Read, Write};

// Define types locally since they're not in the main crate yet
pub type Hash = [u8; 32];

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
    /// 
    /// # Arguments
    /// * `keys` - Iterator of keys to retrieve
    /// 
    /// # Returns
    /// * `Ok(values)` where each value is `Some(data)` if found or `None` if not found
    /// * `Err(_)` on storage errors
    async fn get_many<'a>(&self, keys: impl Iterator<Item = &'a [u8]> + Send) -> CatalystResult<Vec<Option<Vec<u8>>>>;
    
    /// Set multiple key-value pairs efficiently
    /// 
    /// # Arguments
    /// * `pairs` - Iterator of (key, value) pairs to set
    /// 
    /// # Returns
    /// * `Ok(())` on success
    /// * `Err(_)` on storage errors
    async fn set_many<'a>(&self, pairs: impl Iterator<Item = (&'a [u8], Vec<u8>)> + Send) -> CatalystResult<()>;
    
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
}

/// Account types supported by the Catalyst ledger
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountType {
    /// Standard account with visible balance
    NonConfidential,
    /// Account with hidden balance using Pedersen commitments
    Confidential,
    /// Smart contract account
    Contract,
}

impl CatalystSerialize for AccountType {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        Ok(vec![match self {
            AccountType::NonConfidential => 0,
            AccountType::Confidential => 1,
            AccountType::Contract => 2,
        }])
    }

    fn serialize_to<W: Write>(&self, writer: &mut W) -> CatalystResult<()> {
        let b = match self {
            AccountType::NonConfidential => 0u8,
            AccountType::Confidential => 1u8,
            AccountType::Contract => 2u8,
        };
        writer
            .write_all(&[b])
            .map_err(|e| CatalystError::Serialization(format!("Failed to write AccountType: {}", e)))
    }

    fn serialized_size(&self) -> usize {
        1
    }
}

impl CatalystDeserialize for AccountType {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        let mut cursor = Cursor::new(data);
        Self::deserialize_from(&mut cursor)
    }

    fn deserialize_from<R: Read>(reader: &mut R) -> CatalystResult<Self> {
        let mut b = [0u8; 1];
        reader
            .read_exact(&mut b)
            .map_err(|e| CatalystError::Serialization(format!("Failed to read AccountType: {}", e)))?;
        match b[0] {
            0 => Ok(AccountType::NonConfidential),
            1 => Ok(AccountType::Confidential),
            2 => Ok(AccountType::Contract),
            other => Err(CatalystError::Serialization(format!(
                "Invalid AccountType tag: {}",
                other
            ))),
        }
    }
}

/// Account state structure
#[derive(Debug, Clone)]
pub struct AccountState {
    /// Account address (21 bytes: 1 byte prefix + 20 bytes hash)
    pub address: [u8; 21],
    /// Account type
    pub account_type: AccountType,
    /// Account balance (visible for non-confidential, commitment for confidential)
    pub balance: Vec<u8>,
    /// Optional contract data (max 64 bytes)
    pub data: Option<Vec<u8>>,
    /// Nonce for transaction ordering
    pub nonce: u64,
}

impl CatalystSerialize for AccountState {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        let mut out = Vec::with_capacity(self.serialized_size());
        self.serialize_to(&mut out)?;
        Ok(out)
    }

    fn serialize_to<W: Write>(&self, writer: &mut W) -> CatalystResult<()> {
        writer
            .write_all(&self.address)
            .map_err(|e| CatalystError::Serialization(format!("Failed to write address: {}", e)))?;

        self.account_type.serialize_to(writer)?;

        let balance_len: u32 = self
            .balance
            .len()
            .try_into()
            .map_err(|_| CatalystError::Serialization("Balance too large".to_string()))?;
        writer
            .write_all(&balance_len.to_le_bytes())
            .map_err(|e| CatalystError::Serialization(format!("Failed to write balance len: {}", e)))?;
        writer
            .write_all(&self.balance)
            .map_err(|e| CatalystError::Serialization(format!("Failed to write balance: {}", e)))?;

        match &self.data {
            Some(data) => {
                writer
                    .write_all(&[1u8])
                    .map_err(|e| CatalystError::Serialization(format!("Failed to write data flag: {}", e)))?;
                let data_len: u32 = data
                    .len()
                    .try_into()
                    .map_err(|_| CatalystError::Serialization("Account data too large".to_string()))?;
                writer
                    .write_all(&data_len.to_le_bytes())
                    .map_err(|e| CatalystError::Serialization(format!("Failed to write data len: {}", e)))?;
                writer
                    .write_all(data)
                    .map_err(|e| CatalystError::Serialization(format!("Failed to write data: {}", e)))?;
            }
            None => {
                writer
                    .write_all(&[0u8])
                    .map_err(|e| CatalystError::Serialization(format!("Failed to write data flag: {}", e)))?;
            }
        }

        writer
            .write_all(&self.nonce.to_le_bytes())
            .map_err(|e| CatalystError::Serialization(format!("Failed to write nonce: {}", e)))?;

        Ok(())
    }

    fn serialized_size(&self) -> usize {
        21 // address
            + 1 // type
            + 4 // balance len
            + self.balance.len()
            + 1 // data present
            + self.data.as_ref().map(|d| 4 + d.len()).unwrap_or(0)
            + 8 // nonce
    }
}

impl CatalystDeserialize for AccountState {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        let mut cursor = Cursor::new(data);
        Self::deserialize_from(&mut cursor)
    }

    fn deserialize_from<R: Read>(reader: &mut R) -> CatalystResult<Self> {
        let mut address = [0u8; 21];
        reader
            .read_exact(&mut address)
            .map_err(|e| CatalystError::Serialization(format!("Failed to read address: {}", e)))?;

        let account_type = AccountType::deserialize_from(reader)?;

        let mut len_bytes = [0u8; 4];
        reader
            .read_exact(&mut len_bytes)
            .map_err(|e| CatalystError::Serialization(format!("Failed to read balance len: {}", e)))?;
        let balance_len = u32::from_le_bytes(len_bytes) as usize;

        let mut balance = vec![0u8; balance_len];
        reader
            .read_exact(&mut balance)
            .map_err(|e| CatalystError::Serialization(format!("Failed to read balance: {}", e)))?;

        let mut flag = [0u8; 1];
        reader
            .read_exact(&mut flag)
            .map_err(|e| CatalystError::Serialization(format!("Failed to read data flag: {}", e)))?;

        let data = if flag[0] == 1 {
            reader
                .read_exact(&mut len_bytes)
                .map_err(|e| CatalystError::Serialization(format!("Failed to read data len: {}", e)))?;
            let data_len = u32::from_le_bytes(len_bytes) as usize;
            let mut data = vec![0u8; data_len];
            reader
                .read_exact(&mut data)
                .map_err(|e| CatalystError::Serialization(format!("Failed to read data: {}", e)))?;
            Some(data)
        } else {
            None
        };

        let mut nonce_bytes = [0u8; 8];
        reader
            .read_exact(&mut nonce_bytes)
            .map_err(|e| CatalystError::Serialization(format!("Failed to read nonce: {}", e)))?;
        let nonce = u64::from_le_bytes(nonce_bytes);

        Ok(AccountState {
            address,
            account_type,
            balance,
            data,
            nonce,
        })
    }
}

impl AccountState {
    /// Create a new non-confidential account
    pub fn new_non_confidential(address: [u8; 21], balance: u64) -> Self {
        Self {
            address,
            account_type: AccountType::NonConfidential,
            balance: balance.to_le_bytes().to_vec(),
            data: None,
            nonce: 0,
        }
    }
    
    /// Create a new confidential account with Pedersen commitment
    pub fn new_confidential(address: [u8; 21], commitment: [u8; 32]) -> Self {
        Self {
            address,
            account_type: AccountType::Confidential,
            balance: commitment.to_vec(),
            data: None,
            nonce: 0,
        }
    }
    
    /// Create a new contract account
    pub fn new_contract(address: [u8; 21], balance: u64, contract_data: Option<Vec<u8>>) -> Self {
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
    pub fn account_key(address: &[u8; 21]) -> Vec<u8> {
        let mut key = ACCOUNT_PREFIX.to_vec();
        key.extend_from_slice(address);
        key
    }
    
    /// Create a transaction key from hash
    pub fn transaction_key(tx_hash: &[u8; 32]) -> Vec<u8> {
        let mut key = TRANSACTION_PREFIX.to_vec();
        key.extend_from_slice(tx_hash);
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
        let address = [1u8; 21];
        
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
        let address = [1u8; 21];
        let tx_hash = [2u8; 32];
        
        let acc_key = state_keys::account_key(&address);
        assert!(acc_key.starts_with(state_keys::ACCOUNT_PREFIX));
        assert_eq!(acc_key.len(), state_keys::ACCOUNT_PREFIX.len() + 21);
        
        let tx_key = state_keys::transaction_key(&tx_hash);
        assert!(tx_key.starts_with(state_keys::TRANSACTION_PREFIX));
        assert_eq!(tx_key.len(), state_keys::TRANSACTION_PREFIX.len() + 32);
        
        let meta_key = state_keys::metadata_key("ledger_height");
        assert!(meta_key.starts_with(state_keys::METADATA_PREFIX));
    }
}