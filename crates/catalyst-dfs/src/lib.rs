//! Distributed File System for Catalyst Network
//! 
//! Implements IPFS-compatible distributed storage for:
//! - Historical ledger state updates
//! - Large files and media
//! - Smart contract code and data
//! - Application data storage

use async_trait::async_trait;
use catalyst_core::{Hash, LedgerStateUpdate};
use cid::Cid;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

pub mod ipfs;
pub mod provider;
pub mod storage;
pub mod swarm;

pub use ipfs::*;
pub use provider::*;
pub use storage::*;
pub use swarm::*;

#[derive(Error, Debug)]
pub enum DfsError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid CID: {0}")]
    InvalidCid(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// DFS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfsConfig {
    /// Local storage directory
    pub storage_dir: PathBuf,
    /// Maximum storage size in bytes
    pub max_storage_size: u64,
    /// Enable garbage collection
    pub enable_gc: bool,
    /// Garbage collection interval in seconds
    pub gc_interval: u64,
    /// Enable content discovery
    pub enable_discovery: bool,
    /// Provider announcement interval
    pub provider_interval: u64,
    /// Replication factor for important data
    pub replication_factor: usize,
}

impl Default for DfsConfig {
    fn default() -> Self {
        Self {
            storage_dir: PathBuf::from("./dfs"),
            max_storage_size: 100 * 1024 * 1024 * 1024, // 100GB
            enable_gc: true,
            gc_interval: 3600, // 1 hour
            enable_discovery: true,
            provider_interval: 900, // 15 minutes
            replication_factor: 3,
        }
    }
}

impl DfsConfig {
    pub fn validate(&self) -> Result<(), DfsError> {
        if !self.storage_dir.exists() {
            std::fs::create_dir_all(&self.storage_dir)?;
        }
        Ok(())
    }
}

/// Content addressing types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ContentId(pub Cid);

impl ContentId {
    /// Create content ID from data
    pub fn from_data(data: &[u8]) -> Result<Self, DfsError> {
        use multihash::{Code, MultihashDigest};
        
        let hash = Code::Sha2_256.digest(data);
        let cid = Cid::new_v1(0x55, hash); // 0x55 = raw codec
        Ok(ContentId(cid))
    }
    
    /// Get CID as string
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
    
    /// Parse CID from string
    pub fn from_string(s: &str) -> Result<Self, DfsError> {
        let cid = s.parse::<Cid>()
            .map_err(|e| DfsError::InvalidCid(e.to_string()))?;
        Ok(ContentId(cid))
    }
}

/// DFS content metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentMetadata {
    /// Content ID
    pub cid: ContentId,
    /// Content size in bytes
    pub size: u64,
    /// Content type/MIME type
    pub content_type: Option<String>,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last accessed timestamp
    pub accessed_at: chrono::DateTime<chrono::Utc>,
    /// Number of times accessed
    pub access_count: u64,
    /// Pin status (prevents garbage collection)
    pub pinned: bool,
}

/// Main DFS interface trait
#[async_trait]
pub trait DistributedFileSystem: Send + Sync {
    /// Store data and return content ID
    async fn put(&self, data: Vec<u8>) -> Result<ContentId, DfsError>;
    
    /// Retrieve data by content ID
    async fn get(&self, cid: &ContentId) -> Result<Vec<u8>, DfsError>;
    
    /// Check if content exists locally
    async fn has(&self, cid: &ContentId) -> Result<bool, DfsError>;
    
    /// Pin content to prevent garbage collection
    async fn pin(&self, cid: &ContentId) -> Result<(), DfsError>;
    
    /// Unpin content
    async fn unpin(&self, cid: &ContentId) -> Result<(), DfsError>;
    
    /// Get content metadata
    async fn metadata(&self, cid: &ContentId) -> Result<ContentMetadata, DfsError>;
    
    /// List all stored content
    async fn list(&self) -> Result<Vec<ContentMetadata>, DfsError>;
    
    /// Garbage collect unpinned content
    async fn gc(&self) -> Result<GcResult, DfsError>;
    
    /// Get storage statistics
    async fn stats(&self) -> Result<DfsStats, DfsError>;
}

/// Garbage collection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GcResult {
    /// Number of objects removed
    pub objects_removed: u64,
    /// Bytes freed
    pub bytes_freed: u64,
    /// Time taken for GC
    pub duration_ms: u64,
}

/// DFS statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfsStats {
    /// Total objects stored
    pub total_objects: u64,
    /// Total bytes stored
    pub total_bytes: u64,
    /// Pinned objects count
    pub pinned_objects: u64,
    /// Available storage space
    pub available_space: u64,
    /// Hit rate for local requests
    pub hit_rate: f64,
    /// Network requests made
    pub network_requests: u64,
}

/// Content provider for DFS network
#[async_trait]
pub trait ContentProvider: Send + Sync {
    /// Announce that we provide content
    async fn provide(&self, cid: &ContentId) -> Result<(), DfsError>;
    
    /// Stop providing content
    async fn unprovide(&self, cid: &ContentId) -> Result<(), DfsError>;
    
    /// Find providers for content
    async fn find_providers(&self, cid: &ContentId) -> Result<Vec<ProviderId>, DfsError>;
}

/// Provider identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ProviderId(pub String);

/// DFS content categories for organization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContentCategory {
    /// Ledger state updates
    LedgerUpdate,
    /// Smart contract bytecode
    Contract,
    /// Application data
    AppData,
    /// Media files
    Media,
    /// Generic files
    File,
}

/// Extended content storage with categorization
#[async_trait]
pub trait CategorizedStorage: Send + Sync {
    /// Store content with category
    async fn put_categorized(
        &self, 
        data: Vec<u8>, 
        category: ContentCategory
    ) -> Result<ContentId, DfsError>;
    
    /// List content by category
    async fn list_by_category(
        &self, 
        category: ContentCategory
    ) -> Result<Vec<ContentMetadata>, DfsError>;
    
    /// Store ledger update
    async fn store_ledger_update(
        &self, 
        update: &LedgerStateUpdate
    ) -> Result<ContentId, DfsError>;
    
    /// Retrieve ledger update
    async fn get_ledger_update(
        &self, 
        cid: &ContentId
    ) -> Result<LedgerStateUpdate, DfsError>;
}

/// DFS factory for creating instances
pub struct DfsFactory;

impl DfsFactory {
    /// Create a new DFS instance
    pub async fn create(config: &DfsConfig) -> Result<Box<dyn DistributedFileSystem>, DfsError> {
        config.validate()?;
        
        let storage = LocalDfsStorage::new(config.clone()).await?;
        Ok(Box::new(storage))
    }
    
    /// Create a networked DFS instance with IPFS compatibility
    pub async fn create_networked(
        config: &DfsConfig,
    ) -> Result<Box<dyn DistributedFileSystem>, DfsError> {
        config.validate()?;
        
        let networked_dfs = NetworkedDfs::new(config.clone()).await?;
        Ok(Box::new(networked_dfs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_content_id_creation() {
        let data = b"hello world";
        let cid = ContentId::from_data(data).unwrap();
        
        // Content ID should be deterministic
        let cid2 = ContentId::from_data(data).unwrap();
        assert_eq!(cid, cid2);
        
        // Different data should produce different CID
        let cid3 = ContentId::from_data(b"different data").unwrap();
        assert_ne!(cid, cid3);
    }

    #[test]
    fn test_content_id_string_conversion() {
        let data = b"test data";
        let cid = ContentId::from_data(data).unwrap();
        
        let cid_string = cid.to_string();
        let parsed_cid = ContentId::from_string(&cid_string).unwrap();
        
        assert_eq!(cid, parsed_cid);
    }

    #[tokio::test]
    async fn test_dfs_factory() {
        let temp_dir = TempDir::new().unwrap();
        let config = DfsConfig {
            storage_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let dfs = DfsFactory::create(&config).await.unwrap();
        
        // Test basic operations
        let data = b"test content".to_vec();
        let cid = dfs.put(data.clone()).await.unwrap();
        
        let retrieved = dfs.get(&cid).await.unwrap();
        assert_eq!(data, retrieved);
        
        assert!(dfs.has(&cid).await.unwrap());
    }
}