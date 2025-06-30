//! Local DFS storage implementation
//! 
//! Provides local storage with IPFS-compatible content addressing

use crate::{
    ContentId, ContentMetadata, DfsConfig, DfsError, DfsStats, 
    DistributedFileSystem, GcResult, CategorizedStorage, ContentCategory
};
use catalyst_core::{Hash, LedgerStateUpdate};
use chrono::{DateTime, Utc};
use rocksdb::{DB, Options, WriteBatch};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::fs;
use uuid::Uuid;

/// Metadata storage key prefixes
const METADATA_PREFIX: &[u8] = b"meta:";
const CONTENT_PREFIX: &[u8] = b"content:";
const CATEGORY_PREFIX: &[u8] = b"category:";
const PIN_PREFIX: &[u8] = b"pin:";

/// Local DFS storage implementation
pub struct LocalDfsStorage {
    config: DfsConfig,
    db: Arc<DB>,
    stats: Arc<RwLock<DfsStats>>,
    content_dir: PathBuf,
}

impl LocalDfsStorage {
    /// Create a new local DFS storage instance
    pub async fn new(config: DfsConfig) -> Result<Self, DfsError> {
        // Ensure storage directory exists
        if !config.storage_dir.exists() {
            fs::create_dir_all(&config.storage_dir).await?;
        }

        let content_dir = config.storage_dir.join("content");
        if !content_dir.exists() {
            fs::create_dir_all(&content_dir).await?;
        }

        // Setup RocksDB for metadata
        let db_path = config.storage_dir.join("metadata");
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        
        let db = DB::open(&opts, db_path)
            .map_err(|e| DfsError::Storage(format!("Failed to open metadata DB: {}", e)))?;

        let stats = Arc::new(RwLock::new(DfsStats {
            total_objects: 0,
            total_bytes: 0,
            pinned_objects: 0,
            available_space: 0,
            hit_rate: 0.0,
            network_requests: 0,
        }));

        let storage = Self {
            config,
            db: Arc::new(db),
            stats,
            content_dir,
        };

        // Initialize stats
        storage.update_stats().await?;

        Ok(storage)
    }

    /// Get content file path for a given CID
    fn content_path(&self, cid: &ContentId) -> PathBuf {
        let cid_str = cid.to_string();
        // Use first 2 chars as subdirectory for better file system performance
        let subdir = &cid_str[0..2.min(cid_str.len())];
        self.content_dir.join(subdir).join(&cid_str)
    }

    /// Store metadata in the database
    fn store_metadata(&self, metadata: &ContentMetadata) -> Result<(), DfsError> {
        let key = Self::metadata_key(&metadata.cid);
        let value = serde_json::to_vec(metadata)
            .map_err(|e| DfsError::Serialization(format!("Failed to serialize metadata: {}", e)))?;
        
        self.db.put(&key, &value)
            .map_err(|e| DfsError::Storage(format!("Failed to store metadata: {}", e)))?;
        
        Ok(())
    }

    /// Retrieve metadata from the database
    fn get_metadata(&self, cid: &ContentId) -> Result<Option<ContentMetadata>, DfsError> {
        let key = Self::metadata_key(cid);
        
        match self.db.get(&key)
            .map_err(|e| DfsError::Storage(format!("Failed to get metadata: {}", e)))? 
        {
            Some(data) => {
                let metadata: ContentMetadata = serde_json::from_slice(&data)
                    .map_err(|e| DfsError::Serialization(format!("Failed to deserialize metadata: {}", e)))?;
                Ok(Some(metadata))
            }
            None => Ok(None),
        }
    }

    /// Generate metadata key
    fn metadata_key(cid: &ContentId) -> Vec<u8> {
        let mut key = METADATA_PREFIX.to_vec();
        key.extend_from_slice(cid.to_string().as_bytes());
        key
    }

    /// Update internal statistics
    async fn update_stats(&self) -> Result<(), DfsError> {
        let mut total_objects = 0u64;
        let mut total_bytes = 0u64;
        let mut pinned_objects = 0u64;

        // Iterate through all metadata entries
        let iter = self.db.prefix_iterator(METADATA_PREFIX);
        for item in iter {
            let (_, value) = item.map_err(|e| DfsError::Storage(e.to_string()))?;
            
            if let Ok(metadata) = serde_json::from_slice::<ContentMetadata>(&value) {
                total_objects += 1;
                total_bytes += metadata.size;
                if metadata.pinned {
                    pinned_objects += 1;
                }
            }
        }

        // Calculate available space
        let used_space = total_bytes;
        let available_space = if used_space < self.config.max_storage_size {
            self.config.max_storage_size - used_space
        } else {
            0
        };

        // Update stats
        if let Ok(mut stats) = self.stats.write() {
            stats.total_objects = total_objects;
            stats.total_bytes = total_bytes;
            stats.pinned_objects = pinned_objects;
            stats.available_space = available_space;
        }

        Ok(())
    }

    /// Check if we have enough space for new content
    fn check_space(&self, size: u64) -> Result<(), DfsError> {
        if let Ok(stats) = self.stats.read() {
            if stats.total_bytes + size > self.config.max_storage_size {
                return Err(DfsError::Storage(
                    format!("Insufficient storage space. Need: {}, Available: {}", 
                            size, stats.available_space)
                ));
            }
        }
        Ok(())
    }

    /// Store category mapping
    fn store_category(&self, cid: &ContentId, category: &ContentCategory) -> Result<(), DfsError> {
        let key = Self::category_key(cid);
        let value = serde_json::to_vec(category)
            .map_err(|e| DfsError::Serialization(format!("Failed to serialize category: {}", e)))?;
        
        self.db.put(&key, &value)
            .map_err(|e| DfsError::Storage(format!("Failed to store category: {}", e)))?;
        
        Ok(())
    }

    /// Generate category key
    fn category_key(cid: &ContentId) -> Vec<u8> {
        let mut key = CATEGORY_PREFIX.to_vec();
        key.extend_from_slice(cid.to_string().as_bytes());
        key
    }
}

#[async_trait::async_trait]
impl DistributedFileSystem for LocalDfsStorage {
    async fn put(&self, data: Vec<u8>) -> Result<ContentId, DfsError> {
        let size = data.len() as u64;
        
        // Check storage space
        self.check_space(size)?;
        
        // Generate content ID
        let cid = ContentId::from_data(&data)?;
        
        // Check if content already exists
        if self.has(&cid).await? {
            // Update access time and return existing CID
            if let Some(mut metadata) = self.get_metadata(&cid)? {
                metadata.accessed_at = Utc::now();
                metadata.access_count += 1;
                self.store_metadata(&metadata)?;
            }
            return Ok(cid);
        }

        // Store content to file
        let content_path = self.content_path(&cid);
        
        // Create subdirectory if needed
        if let Some(parent) = content_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        
        fs::write(&content_path, &data).await?;

        // Create and store metadata
        let now = Utc::now();
        let metadata = ContentMetadata {
            cid: cid.clone(),
            size,
            content_type: None, // Could be detected based on content
            created_at: now,
            accessed_at: now,
            access_count: 1,
            pinned: false,
        };

        self.store_metadata(&metadata)?;

        // Update stats
        self.update_stats().await?;

        Ok(cid)
    }

    async fn get(&self, cid: &ContentId) -> Result<Vec<u8>, DfsError> {
        // Check if content exists
        let metadata = self.get_metadata(cid)?
            .ok_or_else(|| DfsError::NotFound(format!("Content not found: {}", cid.to_string())))?;

        // Read content from file
        let content_path = self.content_path(cid);
        
        if !content_path.exists() {
            return Err(DfsError::NotFound(format!("Content file not found: {}", cid.to_string())));
        }

        let data = fs::read(&content_path).await?;

        // Update access metadata
        let mut updated_metadata = metadata;
        updated_metadata.accessed_at = Utc::now();
        updated_metadata.access_count += 1;
        self.store_metadata(&updated_metadata)?;

        Ok(data)
    }

    async fn has(&self, cid: &ContentId) -> Result<bool, DfsError> {
        let content_path = self.content_path(cid);
        Ok(content_path.exists() && self.get_metadata(cid)?.is_some())
    }

    async fn pin(&self, cid: &ContentId) -> Result<(), DfsError> {
        let mut metadata = self.get_metadata(cid)?
            .ok_or_else(|| DfsError::NotFound(format!("Content not found: {}", cid.to_string())))?;

        if !metadata.pinned {
            metadata.pinned = true;
            self.store_metadata(&metadata)?;
            
            // Update stats
            self.update_stats().await?;
        }

        Ok(())
    }

    async fn unpin(&self, cid: &ContentId) -> Result<(), DfsError> {
        let mut metadata = self.get_metadata(cid)?
            .ok_or_else(|| DfsError::NotFound(format!("Content not found: {}", cid.to_string())))?;

        if metadata.pinned {
            metadata.pinned = false;
            self.store_metadata(&metadata)?;
            
            // Update stats
            self.update_stats().await?;
        }

        Ok(())
    }

    async fn metadata(&self, cid: &ContentId) -> Result<ContentMetadata, DfsError> {
        self.get_metadata(cid)?
            .ok_or_else(|| DfsError::NotFound(format!("Content not found: {}", cid.to_string())))
    }

    async fn list(&self) -> Result<Vec<ContentMetadata>, DfsError> {
        let mut contents = Vec::new();
        
        let iter = self.db.prefix_iterator(METADATA_PREFIX);
        for item in iter {
            let (_, value) = item.map_err(|e| DfsError::Storage(e.to_string()))?;
            
            if let Ok(metadata) = serde_json::from_slice::<ContentMetadata>(&value) {
                contents.push(metadata);
            }
        }

        Ok(contents)
    }

    async fn gc(&self) -> Result<GcResult, DfsError> {
        let start_time = std::time::Instant::now();
        let mut objects_removed = 0u64;
        let mut bytes_freed = 0u64;

        if !self.config.enable_gc {
            return Ok(GcResult {
                objects_removed: 0,
                bytes_freed: 0,
                duration_ms: 0,
            });
        }

        // Find unpinned content older than threshold
        let threshold = Utc::now() - chrono::Duration::hours(24); // 24 hours
        let mut to_remove = Vec::new();

        let iter = self.db.prefix_iterator(METADATA_PREFIX);
        for item in iter {
            let (key, value) = item.map_err(|e| DfsError::Storage(e.to_string()))?;
            
            if let Ok(metadata) = serde_json::from_slice::<ContentMetadata>(&value) {
                if !metadata.pinned && metadata.accessed_at < threshold {
                    to_remove.push((key.to_vec(), metadata));
                }
            }
        }

        // Remove old content
        let mut batch = WriteBatch::default();
        
        for (key, metadata) in to_remove {
            // Remove content file
            let content_path = self.content_path(&metadata.cid);
            if content_path.exists() {
                if let Err(e) = fs::remove_file(&content_path).await {
                    log::warn!("Failed to remove content file {}: {}", content_path.display(), e);
                } else {
                    bytes_freed += metadata.size;
                    objects_removed += 1;
                }
            }

            // Remove metadata
            batch.delete(&key);
            
            // Remove category mapping if exists
            let category_key = Self::category_key(&metadata.cid);
            batch.delete(&category_key);
        }

        // Apply batch delete
        self.db.write(batch)
            .map_err(|e| DfsError::Storage(format!("Failed to apply GC batch: {}", e)))?;

        // Update stats
        self.update_stats().await?;

        let duration_ms = start_time.elapsed().as_millis() as u64;

        Ok(GcResult {
            objects_removed,
            bytes_freed,
            duration_ms,
        })
    }

    async fn stats(&self) -> Result<DfsStats, DfsError> {
        if let Ok(stats) = self.stats.read() {
            Ok(stats.clone())
        } else {
            Err(DfsError::Storage("Failed to read stats".to_string()))
        }
    }
}

#[async_trait::async_trait]
impl CategorizedStorage for LocalDfsStorage {
    async fn put_categorized(
        &self, 
        data: Vec<u8>, 
        category: ContentCategory
    ) -> Result<ContentId, DfsError> {
        let cid = self.put(data).await?;
        self.store_category(&cid, &category)?;
        Ok(cid)
    }

    async fn list_by_category(
        &self, 
        category: ContentCategory
    ) -> Result<Vec<ContentMetadata>, DfsError> {
        let mut results = Vec::new();
        
        let iter = self.db.prefix_iterator(CATEGORY_PREFIX);
        for item in iter {
            let (_, value) = item.map_err(|e| DfsError::Storage(e.to_string()))?;
            
            if let Ok(stored_category) = serde_json::from_slice::<ContentCategory>(&value) {
                if std::mem::discriminant(&stored_category) == std::mem::discriminant(&category) {
                    // Extract CID from key and get metadata
                    // This is a simplified approach - in practice you'd want to store the CID reference
                    let key_str = String::from_utf8_lossy(&item.0);
                    if let Some(cid_str) = key_str.strip_prefix("category:") {
                        if let Ok(cid) = ContentId::from_string(cid_str) {
                            if let Ok(Some(metadata)) = self.get_metadata(&cid) {
                                results.push(metadata);
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    async fn store_ledger_update(
        &self, 
        update: &LedgerStateUpdate
    ) -> Result<ContentId, DfsError> {
        use catalyst_core::serialization::CatalystSerialize;
        
        let data = update.serialize()
            .map_err(|e| DfsError::Serialization(format!("Failed to serialize ledger update: {}", e)))?;
        
        self.put_categorized(data, ContentCategory::LedgerUpdate).await
    }

    async fn get_ledger_update(
        &self, 
        cid: &ContentId
    ) -> Result<LedgerStateUpdate, DfsError> {
        use catalyst_core::serialization::CatalystDeserialize;
        
        let data = self.get(cid).await?;
        
        LedgerStateUpdate::deserialize(&data)
            .map_err(|e| DfsError::Serialization(format!("Failed to deserialize ledger update: {}", e)))
    }
}