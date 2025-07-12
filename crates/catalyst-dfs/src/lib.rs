//! Catalyst DFS - Minimal local-only implementation

use async_trait::async_trait;
use catalyst_utils::LedgerStateUpdate;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

/// DFS errors
#[derive(Error, Debug)]
pub enum DfsError {
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Content not found: {0}")]
    NotFound(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Generic error: {0}")]
    Generic(String),
}

/// Content identifier
pub type ContentId = String;

/// Content metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentMetadata {
    pub cid: ContentId,
    pub size: u64,
    pub created_at: u64,
    pub category: Option<String>,
}

/// DFS statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfsStats {
    pub total_stored_bytes: u64,
    pub total_items: u64,
    pub cache_hit_ratio: f64,
}

/// Garbage collection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcResult {
    pub freed_bytes: u64,
    pub removed_items: u64,
}

/// Main DFS trait
#[async_trait]
pub trait DistributedFileSystem: Send + Sync {
    /// Store data and return content ID
    async fn put(&self, data: Vec<u8>) -> Result<ContentId, DfsError>;
    
    /// Retrieve data by content ID
    async fn get(&self, cid: &ContentId) -> Result<Vec<u8>, DfsError>;
    
    /// Check if content exists
    async fn has(&self, cid: &ContentId) -> Result<bool, DfsError>;
    
    /// Pin content to prevent garbage collection
    async fn pin(&self, cid: &ContentId) -> Result<(), DfsError>;
    
    /// Unpin content
    async fn unpin(&self, cid: &ContentId) -> Result<(), DfsError>;
    
    /// Get content metadata
    async fn metadata(&self, cid: &ContentId) -> Result<ContentMetadata, DfsError>;
    
    /// List all content
    async fn list(&self) -> Result<Vec<ContentMetadata>, DfsError>;
    
    /// Garbage collection
    async fn gc(&self) -> Result<GcResult, DfsError>;
    
    /// Get statistics
    async fn stats(&self) -> Result<DfsStats, DfsError>;
}

/// Categorized storage trait
#[async_trait]
pub trait CategorizedStorage: Send + Sync {
    /// Store data with category
    async fn put_categorized(
        &self, 
        data: Vec<u8>, 
        category: &str
    ) -> Result<ContentId, DfsError>;
    
    /// List content by category
    async fn list_by_category(
        &self, 
        category: &str
    ) -> Result<Vec<ContentMetadata>, DfsError>;
}

/// Ledger update storage trait
#[async_trait]
pub trait LedgerUpdateStorage: Send + Sync {
    /// Store a ledger state update
    async fn store_ledger_update(
        &self,
        update: &LedgerStateUpdate
    ) -> Result<ContentId, DfsError>;
    
    /// Retrieve a ledger state update
    async fn get_ledger_update(
        &self,
        cid: &ContentId
    ) -> Result<LedgerStateUpdate, DfsError>;
}

/// Simple local-only DFS implementation
pub struct LocalDfs {
    storage: Arc<RwLock<HashMap<ContentId, Vec<u8>>>>,
    metadata: Arc<RwLock<HashMap<ContentId, ContentMetadata>>>,
}

impl LocalDfs {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    fn generate_cid(&self, data: &[u8]) -> String {
        // Simple hash-based content ID
        use catalyst_utils::crypto::hash_data;
        let hash = hash_data(data);
        hex::encode(hash.as_bytes())
    }
    
    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

impl Default for LocalDfs {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DistributedFileSystem for LocalDfs {
    async fn put(&self, data: Vec<u8>) -> Result<ContentId, DfsError> {
        let cid = self.generate_cid(&data);
        let size = data.len() as u64;
        
        // Store data
        {
            let mut storage = self.storage.write().unwrap();
            storage.insert(cid.clone(), data);
        }
        
        // Store metadata
        {
            let mut metadata = self.metadata.write().unwrap();
            metadata.insert(cid.clone(), ContentMetadata {
                cid: cid.clone(),
                size,
                created_at: Self::current_timestamp(),
                category: None,
            });
        }
        
        Ok(cid)
    }
    
    async fn get(&self, cid: &ContentId) -> Result<Vec<u8>, DfsError> {
        let storage = self.storage.read().unwrap();
        storage.get(cid)
            .cloned()
            .ok_or_else(|| DfsError::NotFound(format!("Content not found: {}", cid)))
    }
    
    async fn has(&self, cid: &ContentId) -> Result<bool, DfsError> {
        let storage = self.storage.read().unwrap();
        Ok(storage.contains_key(cid))
    }
    
    async fn pin(&self, _cid: &ContentId) -> Result<(), DfsError> {
        // No-op for local storage
        Ok(())
    }
    
    async fn unpin(&self, _cid: &ContentId) -> Result<(), DfsError> {
        // No-op for local storage
        Ok(())
    }
    
    async fn metadata(&self, cid: &ContentId) -> Result<ContentMetadata, DfsError> {
        let metadata = self.metadata.read().unwrap();
        metadata.get(cid)
            .cloned()
            .ok_or_else(|| DfsError::NotFound(format!("Metadata not found: {}", cid)))
    }
    
    async fn list(&self) -> Result<Vec<ContentMetadata>, DfsError> {
        let metadata = self.metadata.read().unwrap();
        Ok(metadata.values().cloned().collect())
    }
    
    async fn gc(&self) -> Result<GcResult, DfsError> {
        // No-op for local storage - everything stays
        Ok(GcResult {
            freed_bytes: 0,
            removed_items: 0,
        })
    }
    
    async fn stats(&self) -> Result<DfsStats, DfsError> {
        let storage = self.storage.read().unwrap();
        
        let total_stored_bytes = storage.values()
            .map(|data| data.len() as u64)
            .sum();
        let total_items = storage.len() as u64;
        
        Ok(DfsStats {
            total_stored_bytes,
            total_items,
            cache_hit_ratio: 1.0, // Everything is cached locally
        })
    }
}

#[async_trait]
impl CategorizedStorage for LocalDfs {
    async fn put_categorized(
        &self, 
        data: Vec<u8>, 
        category: &str
    ) -> Result<ContentId, DfsError> {
        let cid = self.generate_cid(&data);
        let size = data.len() as u64;
        
        // Store data
        {
            let mut storage = self.storage.write().unwrap();
            storage.insert(cid.clone(), data);
        }
        
        // Store metadata with category
        {
            let mut metadata = self.metadata.write().unwrap();
            metadata.insert(cid.clone(), ContentMetadata {
                cid: cid.clone(),
                size,
                created_at: Self::current_timestamp(),
                category: Some(category.to_string()),
            });
        }
        
        Ok(cid)
    }
    
    async fn list_by_category(
        &self, 
        category: &str
    ) -> Result<Vec<ContentMetadata>, DfsError> {
        let metadata = self.metadata.read().unwrap();
        Ok(metadata.values()
            .filter(|meta| meta.category.as_deref() == Some(category))
            .cloned()
            .collect())
    }
}

#[async_trait]
impl LedgerUpdateStorage for LocalDfs {
    async fn store_ledger_update(
        &self,
        update: &LedgerStateUpdate
    ) -> Result<ContentId, DfsError> {
        let data = update.serialize()
            .map_err(|e| DfsError::Serialization(e.to_string()))?;
        self.put_categorized(data, "ledger_update").await
    }
    
    async fn get_ledger_update(
        &self,
        cid: &ContentId
    ) -> Result<LedgerStateUpdate, DfsError> {
        let data = self.get(cid).await?;
        LedgerStateUpdate::deserialize(&data)
            .map_err(|e| DfsError::Serialization(e.to_string()))
    }
}

// Re-export main types
pub use LocalDfs as Dfs;