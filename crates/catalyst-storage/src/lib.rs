//! Storage layer for Catalyst Network
//! 
//! Provides persistent storage for:
//! - Current Ledger State (CLS)
//! - Recent ledger state updates
//! - Historical data via Distributed File System (DFS)
//! - Account balances and transaction history

use async_trait::async_trait;
use catalyst_core::{Block, Transaction, LedgerStateUpdate, Hash};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

pub mod backends;
pub mod dfs;
pub mod ledger;
pub mod state;

pub use backends::*;
pub use dfs::*;
pub use ledger::*;
pub use state::*;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] bincode::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid data: {0}")]
    InvalidData(String),
    #[error("DFS error: {0}")]
    Dfs(String),
}

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Data directory path
    pub data_dir: PathBuf,
    /// Database backend type
    pub backend: StorageBackend,
    /// Enable DFS functionality
    pub enable_dfs: bool,
    /// DFS configuration
    pub dfs_config: DfsConfig,
    /// Cache size in MB
    pub cache_size: usize,
    /// Enable compression
    pub enable_compression: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data"),
            backend: StorageBackend::RocksDb,
            enable_dfs: true,
            dfs_config: DfsConfig::default(),
            cache_size: 256, // 256MB default cache
            enable_compression: true,
        }
    }
}

impl StorageConfig {
    pub fn validate(&self) -> Result<(), StorageError> {
        if !self.data_dir.exists() {
            std::fs::create_dir_all(&self.data_dir)?;
        }
        Ok(())
    }
}

/// Storage backend types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageBackend {
    RocksDb,
    Sled,
    Memory, // For testing
}

/// Main storage interface trait
#[async_trait]
pub trait Storage: Send + Sync {
    /// Store a block
    async fn store_block(&self, block: &Block) -> Result<(), StorageError>;
    
    /// Retrieve a block by hash
    async fn get_block(&self, hash: &Hash) -> Result<Option<Block>, StorageError>;
    
    /// Store ledger state update
    async fn store_ledger_update(&self, update: &LedgerStateUpdate) -> Result<(), StorageError>;
    
    /// Get latest ledger state update
    async fn get_latest_ledger_update(&self) -> Result<Option<LedgerStateUpdate>, StorageError>;
    
    /// Store transaction
    async fn store_transaction(&self, tx: &Transaction) -> Result<(), StorageError>;
    
    /// Get transaction by hash
    async fn get_transaction(&self, hash: &Hash) -> Result<Option<Transaction>, StorageError>;
    
    /// Store arbitrary key-value data
    async fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StorageError>;
    
    /// Get value by key
    async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StorageError>;
    
    /// Delete key
    async fn delete(&self, key: &[u8]) -> Result<(), StorageError>;
    
    /// Check if key exists
    async fn exists(&self, key: &[u8]) -> Result<bool, StorageError>;
    
    /// Get database statistics
    async fn stats(&self) -> Result<StorageStats, StorageError>;
}

/// Storage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageStats {
    pub total_size: u64,
    pub block_count: u64,
    pub transaction_count: u64,
    pub ledger_update_count: u64,
    pub cache_hit_rate: f64,
}

/// Storage factory for creating storage instances
pub struct StorageFactory;

impl StorageFactory {
    /// Create a new storage instance based on configuration
    pub async fn create(config: &StorageConfig) -> Result<Box<dyn Storage>, StorageError> {
        config.validate()?;
        
        match config.backend {
            StorageBackend::RocksDb => {
                let storage = RocksDbStorage::new(&config.data_dir, config)?;
                Ok(Box::new(storage))
            },
            StorageBackend::Sled => {
                let storage = SledStorage::new(&config.data_dir, config)?;
                Ok(Box::new(storage))
            },
            StorageBackend::Memory => {
                let storage = MemoryStorage::new();
                Ok(Box::new(storage))
            },
        }
    }
}

/// Database key prefixes
pub mod keys {
    pub const BLOCK_PREFIX: &[u8] = b"block:";
    pub const TRANSACTION_PREFIX: &[u8] = b"tx:";
    pub const LEDGER_UPDATE_PREFIX: &[u8] = b"ledger:";
    pub const ACCOUNT_PREFIX: &[u8] = b"account:";
    pub const STATE_PREFIX: &[u8] = b"state:";
    pub const META_PREFIX: &[u8] = b"meta:";
    
    pub fn block_key(hash: &catalyst_core::Hash) -> Vec<u8> {
        let mut key = BLOCK_PREFIX.to_vec();
        key.extend_from_slice(hash.as_bytes());
        key
    }
    
    pub fn transaction_key(hash: &catalyst_core::Hash) -> Vec<u8> {
        let mut key = TRANSACTION_PREFIX.to_vec();
        key.extend_from_slice(hash.as_bytes());
        key
    }
    
    pub fn ledger_update_key(cycle: u64) -> Vec<u8> {
        let mut key = LEDGER_UPDATE_PREFIX.to_vec();
        key.extend_from_slice(&cycle.to_be_bytes());
        key
    }
    
    pub fn account_key(address: &catalyst_core::Hash) -> Vec<u8> {
        let mut key = ACCOUNT_PREFIX.to_vec();
        key.extend_from_slice(address.as_bytes());
        key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_storage_factory() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            data_dir: temp_dir.path().to_path_buf(),
            backend: StorageBackend::Memory,
            ..Default::default()
        };
        
        let storage = StorageFactory::create(&config).await.unwrap();
        
        // Test basic operations
        let key = b"test_key";
        let value = b"test_value";
        
        storage.put(key, value).await.unwrap();
        let retrieved = storage.get(key).await.unwrap();
        assert_eq!(retrieved, Some(value.to_vec()));
        
        assert!(storage.exists(key).await.unwrap());
        
        storage.delete(key).await.unwrap();
        assert!(!storage.exists(key).await.unwrap());
    }
}