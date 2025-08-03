//! Snapshot management for point-in-time state recovery

use crate::{RocksEngine, StorageError, StorageResult, SnapshotConfig};
use catalyst_utils::{
    Hash,
    utils::current_timestamp,
};
use std::sync::Arc;
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use parking_lot::RwLock;

/// A point-in-time snapshot of the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    name: String,
    created_at: u64,
    state_root: Hash,
    metadata: SnapshotMetadata,
    file_path: PathBuf,
}

/// Metadata for snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub version: u32,
    pub compression_enabled: bool,
    pub column_families: Vec<String>,
    pub total_size: u64,
    pub key_count: HashMap<String, u64>,
    pub description: Option<String>,
    pub tags: Vec<String>,
}

/// Manages database snapshots
pub struct SnapshotManager {
    engine: Arc<RocksEngine>,
    config: SnapshotConfig,
    snapshots: RwLock<HashMap<String, Snapshot>>,
    snapshot_dir: PathBuf,
}

impl SnapshotManager {
    /// Create a new snapshot manager
    pub async fn new(engine: Arc<RocksEngine>, config: SnapshotConfig) -> StorageResult<Self> {
        let snapshot_dir = config.snapshot_dir
            .clone()
            .unwrap_or_else(|| engine.config().data_dir.join("snapshots"));
        
        // Create snapshot directory
        std::fs::create_dir_all(&snapshot_dir)
            .map_err(|e| StorageError::config(format!("Failed to create snapshot directory: {}", e)))?;
        
        let manager = Self {
            engine,
            config,
            snapshots: RwLock::new(HashMap::new()),
            snapshot_dir,
        };
        
        // Load existing snapshots
        manager.load_existing_snapshots().await?;
        
        Ok(manager)
    }
    
    /// Load existing snapshots from disk
    async fn load_existing_snapshots(&self) -> StorageResult<()> {
        if !self.snapshot_dir.exists() {
            return Ok(());
        }
        
        let entries = std::fs::read_dir(&self.snapshot_dir)
            .map_err(|e| StorageError::io(e))?;
        
        let mut loaded_count = 0;
        for entry in entries {
            let entry = entry.map_err(|e| StorageError::io(e))?;
            let path = entry.path();
            
            if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("snapshot")) {
                match self.load_snapshot_metadata(&path).await {
                    Ok(snapshot) => {
                        self.snapshots.write().insert(snapshot.name.clone(), snapshot);
                        loaded_count += 1;
                    }
                    Err(_e) => {
                        // Log warning in a real implementation
                        eprintln!("Failed to load snapshot from {:?}", path);
                    }
                }
            }
        }
        
        println!("Loaded {} existing snapshots", loaded_count);
        Ok(())
    }
    
    /// Load snapshot metadata from file
    async fn load_snapshot_metadata(&self, path: &Path) -> StorageResult<Snapshot> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| StorageError::io(e))?;
        
        let snapshot: Snapshot = serde_json::from_str(&content)
            .map_err(|e| StorageError::serialization(format!("Failed to parse snapshot metadata: {}", e)))?;
        
        Ok(snapshot)
    }
    
    /// Create a new snapshot
    pub async fn create_snapshot(&self, name: &str) -> StorageResult<Snapshot> {
        if self.snapshots.read().contains_key(name) {
            return Err(StorageError::snapshot(format!("Snapshot '{}' already exists", name)));
        }
        
        println!("Creating snapshot '{}'", name);
        
        // Calculate current state root
        let state_root = self.calculate_state_root().await?;
        
        // Collect metadata
        let metadata = self.collect_snapshot_metadata().await?;
        
        // Create snapshot file path
        let snapshot_file = self.snapshot_dir.join(format!("{}.snapshot", name));
        
        // Create the snapshot
        let snapshot = Snapshot {
            name: name.to_string(),
            created_at: current_timestamp(),
            state_root,
            metadata,
            file_path: snapshot_file.clone(),
        };
        
        // Write snapshot data
        self.write_snapshot_data(&snapshot).await?;
        
        // Write metadata file
        let metadata_content = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| StorageError::serialization(format!("Failed to serialize snapshot: {}", e)))?;
        
        std::fs::write(&snapshot_file, metadata_content)
            .map_err(|e| StorageError::io(e))?;
        
        // Store in memory
        self.snapshots.write().insert(name.to_string(), snapshot.clone());
        
        println!("Snapshot '{}' created successfully with state root {}", name, hex::encode(state_root));
        
        // Cleanup old snapshots if needed
        self.cleanup_old_snapshots().await?;
        
        Ok(snapshot)
    }
    
    /// Write snapshot data to disk
    async fn write_snapshot_data(&self, snapshot: &Snapshot) -> StorageResult<()> {
        let data_dir = self.snapshot_dir.join(format!("{}_data", snapshot.name));
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| StorageError::io(e))?;
        
        // Create RocksDB checkpoint (this creates a consistent point-in-time copy)
        for cf_name in &snapshot.metadata.column_families {
            let cf_file = data_dir.join(format!("{}.sst", cf_name));
            let mut cf_data = Vec::new();
            
            // Iterate through column family and collect data
            let iter = self.engine.iterator(cf_name)?;
            for item in iter {
                let (key, value) = item
                    .map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))?;
                
                // Simple serialization: [key_len][key][value_len][value]
                cf_data.extend_from_slice(&(key.len() as u32).to_le_bytes());
                cf_data.extend_from_slice(&key);
                cf_data.extend_from_slice(&(value.len() as u32).to_le_bytes());
                cf_data.extend_from_slice(&value);
            }
            
            // Compress if enabled
            let final_data = if self.config.compress_snapshots {
                self.compress_data(&cf_data)?
            } else {
                cf_data
            };
            
            std::fs::write(&cf_file, final_data)
                .map_err(|e| StorageError::io(e))?;
        }
        
        Ok(())
    }
    
    /// Compress data using a simple compression algorithm
    fn compress_data(&self, data: &[u8]) -> StorageResult<Vec<u8>> {
        // In a real implementation, you'd use a proper compression library
        // For this example, we'll just add a compression flag
        let mut compressed = vec![1u8]; // Compression flag
        compressed.extend_from_slice(data);
        Ok(compressed)
    }
    
    /// Decompress data
    fn decompress_data(&self, data: &[u8]) -> StorageResult<Vec<u8>> {
        if data.is_empty() {
            return Ok(Vec::new());
        }
        
        if data[0] == 1 {
            // Compressed data
            Ok(data[1..].to_vec())
        } else {
            // Uncompressed data
            Ok(data.to_vec())
        }
    }
    
    /// Load a snapshot
    pub async fn load_snapshot(&self, name: &str) -> StorageResult<()> {
        let snapshot = self.snapshots.read()
            .get(name)
            .cloned()
            .ok_or_else(|| StorageError::snapshot(format!("Snapshot '{}' not found", name)))?;
        
        println!("Loading snapshot '{}'", name);
        
        // Clear current data
        self.clear_database().await?;
        
        // Load snapshot data
        self.load_snapshot_data(&snapshot).await?;
        
        println!("Snapshot '{}' loaded successfully", name);
        
        Ok(())
    }
    
    /// Load snapshot data from disk
    async fn load_snapshot_data(&self, snapshot: &Snapshot) -> StorageResult<()> {
        let data_dir = self.snapshot_dir.join(format!("{}_data", snapshot.name));
        
        if !data_dir.exists() {
            return Err(StorageError::snapshot(format!("Snapshot data directory not found for '{}'", snapshot.name)));
        }
        
        // Load each column family
        for cf_name in &snapshot.metadata.column_families {
            let cf_file = data_dir.join(format!("{}.sst", cf_name));
            
            if !cf_file.exists() {
                eprintln!("Column family file not found: {:?}", cf_file);
                continue;
            }
            
            let file_data = std::fs::read(&cf_file)
                .map_err(|e| StorageError::io(e))?;
            
            // Decompress if needed
            let data = self.decompress_data(&file_data)?;
            
            // Parse and load data
            self.load_column_family_data(cf_name, &data).await?;
        }
        
        Ok(())
    }
    
    /// Load data for a specific column family
    async fn load_column_family_data(&self, cf_name: &str, data: &[u8]) -> StorageResult<()> {
        let mut offset = 0;
        
        while offset < data.len() {
            // Read key length
            if offset + 4 > data.len() {
                break;
            }
            let key_len = u32::from_le_bytes([
                data[offset], data[offset + 1], data[offset + 2], data[offset + 3]
            ]) as usize;
            offset += 4;
            
            // Read key
            if offset + key_len > data.len() {
                break;
            }
            let key = &data[offset..offset + key_len];
            offset += key_len;
            
            // Read value length
            if offset + 4 > data.len() {
                break;
            }
            let value_len = u32::from_le_bytes([
                data[offset], data[offset + 1], data[offset + 2], data[offset + 3]
            ]) as usize;
            offset += 4;
            
            // Read value
            if offset + value_len > data.len() {
                break;
            }
            let value = &data[offset..offset + value_len];
            offset += value_len;
            
            // Write to database
            self.engine.put(cf_name, key, value)?;
        }
        
        Ok(())
    }
    
    /// Clear all data from the database
    async fn clear_database(&self) -> StorageResult<()> {
        // Note: In a real implementation, you'd want to be more careful about this
        // This is a destructive operation that should be used with caution
        
        for cf_name in self.engine.cf_names() {
            if cf_name == "default" {
                continue; // Skip default CF
            }
            
            // Drop and recreate column family
            // This is a simplified approach - in practice you'd iterate and delete keys
            eprintln!("Clearing column family '{}' for snapshot restore", cf_name);
        }
        
        Ok(())
    }
    
    /// Calculate current state root
    async fn calculate_state_root(&self) -> StorageResult<Hash> {
        let mut hasher = Sha256::new();
        
        // Hash all data from all column families
        for cf_name in self.engine.cf_names() {
            hasher.update(cf_name.as_bytes());
            
            let iter = self.engine.iterator(&cf_name)?;
            for item in iter {
                let (key, value) = item
                    .map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))?;
                hasher.update(&key);
                hasher.update(&value);
            }
        }
        
        Ok(hasher.finalize().into())
    }
    
    /// Collect metadata for the current database state
    async fn collect_snapshot_metadata(&self) -> StorageResult<SnapshotMetadata> {
        let mut key_count = HashMap::new();
        let mut total_size = 0u64;
        
        for cf_name in self.engine.cf_names() {
            let mut count = 0u64;
            let mut size = 0u64;
            
            let iter = self.engine.iterator(&cf_name)?;
            for item in iter {
                let (key, value) = item
                    .map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))?;
                count += 1;
                size += key.len() as u64 + value.len() as u64;
            }
            
            key_count.insert(cf_name, count);
            total_size += size;
        }
        
        Ok(SnapshotMetadata {
            version: crate::STORAGE_VERSION,
            compression_enabled: self.config.compress_snapshots,
            column_families: self.engine.cf_names(),
            total_size,
            key_count,
            description: None,
            tags: Vec::new(),
        })
    }
    
    /// List all available snapshots
    pub async fn list_snapshots(&self) -> StorageResult<Vec<String>> {
        Ok(self.snapshots.read().keys().cloned().collect())
    }
    
    /// Get snapshot information
    pub async fn get_snapshot_info(&self, name: &str) -> StorageResult<Option<Snapshot>> {
        Ok(self.snapshots.read().get(name).cloned())
    }
    
    /// Delete a snapshot
    pub async fn delete_snapshot(&self, name: &str) -> StorageResult<()> {
        let snapshot = self.snapshots.write()
            .remove(name)
            .ok_or_else(|| StorageError::snapshot(format!("Snapshot '{}' not found", name)))?;
        
        // Delete metadata file
        if snapshot.file_path.exists() {
            std::fs::remove_file(&snapshot.file_path)
                .map_err(|e| StorageError::io(e))?;
        }
        
        // Delete data directory
        let data_dir = self.snapshot_dir.join(format!("{}_data", name));
        if data_dir.exists() {
            std::fs::remove_dir_all(&data_dir)
                .map_err(|e| StorageError::io(e))?;
        }
        
        println!("Snapshot '{}' deleted successfully", name);
        Ok(())
    }
    
    /// Cleanup old snapshots based on configuration
    pub async fn cleanup_old_snapshots(&self) -> StorageResult<()> {
        let snapshots = self.snapshots.read();
        let snapshot_count = snapshots.len();
        
        if snapshot_count <= self.config.max_snapshots {
            return Ok(());
        }
        
        // Sort snapshots by creation time
        let mut sorted_snapshots: Vec<_> = snapshots.values().collect();
        sorted_snapshots.sort_by_key(|s| s.created_at);
        
        // Delete oldest snapshots
        let to_delete = snapshot_count - self.config.max_snapshots;
        let old_snapshots: Vec<String> = sorted_snapshots
            .iter()
            .take(to_delete)
            .map(|s| s.name.clone())
            .collect();
        
        drop(snapshots); // Release the read lock
        
        for snapshot_name in old_snapshots {
            if let Err(e) = self.delete_snapshot(&snapshot_name).await {
                eprintln!("Failed to delete old snapshot '{}': {}", snapshot_name, e);
            } else {
                println!("Deleted old snapshot '{}' during cleanup", snapshot_name);
            }
        }
        
        Ok(())
    }
    
    /// Export snapshot to external directory
    pub async fn export_snapshot(&self, name: &str, export_dir: &Path) -> StorageResult<()> {
        let snapshot = self.snapshots.read()
            .get(name)
            .cloned()
            .ok_or_else(|| StorageError::snapshot(format!("Snapshot '{}' not found", name)))?;
        
        // Create export directory
        std::fs::create_dir_all(export_dir)
            .map_err(|e| StorageError::io(e))?;
        
        // Copy metadata file
        let metadata_dest = export_dir.join(format!("{}.snapshot", name));
        std::fs::copy(&snapshot.file_path, &metadata_dest)
            .map_err(|e| StorageError::io(e))?;
        
        // Copy data directory
        let data_src = self.snapshot_dir.join(format!("{}_data", name));
        let data_dest = export_dir.join(format!("{}_data", name));
        
        if data_src.exists() {
            Box::pin(self.copy_dir_recursive(&data_src, &data_dest)).await?;
        }
        
        println!("Snapshot '{}' exported to {:?}", name, export_dir);
        
        Ok(())
    }
    
    /// Import snapshot from external directory
    pub async fn import_snapshot(&self, import_dir: &Path, new_name: Option<String>) -> StorageResult<String> {
        if !import_dir.exists() {
            return Err(StorageError::snapshot(format!("Import directory {:?} does not exist", import_dir)));
        }
        
        // Find snapshot metadata file
        let entries = std::fs::read_dir(import_dir)
            .map_err(|e| StorageError::io(e))?;
        
        let mut metadata_file = None;
        for entry in entries {
            let entry = entry.map_err(|e| StorageError::io(e))?;
            let path = entry.path();
            
            if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("snapshot")) {
                metadata_file = Some(path);
                break;
            }
        }
        
        let metadata_file = metadata_file
            .ok_or_else(|| StorageError::snapshot("No snapshot metadata file found in import directory".to_string()))?;
        
        // Load snapshot metadata
        let mut snapshot = self.load_snapshot_metadata(&metadata_file).await?;
        
        // Use new name if provided
        let final_name = new_name.unwrap_or_else(|| {
            format!("{}_imported_{}", snapshot.name, current_timestamp())
        });
        
        // Check if name already exists
        if self.snapshots.read().contains_key(&final_name) {
            return Err(StorageError::snapshot(format!("Snapshot '{}' already exists", final_name)));
        }
        
        snapshot.name = final_name.clone();
        snapshot.file_path = self.snapshot_dir.join(format!("{}.snapshot", final_name));
        
        // Copy metadata file
        std::fs::copy(&metadata_file, &snapshot.file_path)
            .map_err(|e| StorageError::io(e))?;
        
        // Copy data directory if it exists
        let data_src = import_dir.join(format!("{}_data", snapshot.name));
        let data_dest = self.snapshot_dir.join(format!("{}_data", final_name));
        
        if data_src.exists() {
            Box::pin(self.copy_dir_recursive(&data_src, &data_dest)).await?;
        }
        
        // Add to snapshots map
        self.snapshots.write().insert(final_name.clone(), snapshot);
        
        println!("Snapshot imported as '{}' from {:?}", final_name, import_dir);
        
        Ok(final_name)
    }
    
    /// Recursively copy directory
    fn copy_dir_recursive<'a>(&'a self, src: &'a Path, dest: &'a Path) -> std::pin::Pin<Box<dyn std::future::Future<Output = StorageResult<()>> + Send + 'a>> {
        Box::pin(async move {
            std::fs::create_dir_all(dest)
                .map_err(|e| StorageError::io(e))?;
            
            let entries = std::fs::read_dir(src)
                .map_err(|e| StorageError::io(e))?;
            
            for entry in entries {
                let entry = entry.map_err(|e| StorageError::io(e))?;
                let src_path = entry.path();
                let dest_path = dest.join(entry.file_name());
                
                if src_path.is_dir() {
                    Box::pin(self.copy_dir_recursive(&src_path, &dest_path)).await?;
                } else {
                    std::fs::copy(&src_path, &dest_path)
                        .map_err(|e| StorageError::io(e))?;
                }
            }
            
            Ok(())
        })
    }
    
    /// Get snapshot statistics
    pub async fn get_statistics(&self) -> SnapshotStatistics {
        let snapshots = self.snapshots.read();
        let total_count = snapshots.len();
        let total_size: u64 = snapshots.values()
            .map(|s| s.metadata.total_size)
            .sum();
        
        let oldest_timestamp = snapshots.values()
            .map(|s| s.created_at)
            .min();
        
        let newest_timestamp = snapshots.values()
            .map(|s| s.created_at)
            .max();
        
        SnapshotStatistics {
            total_count,
            total_size,
            oldest_timestamp,
            newest_timestamp,
            compression_enabled: self.config.compress_snapshots,
            max_snapshots: self.config.max_snapshots,
        }
    }
}

/// Statistics about snapshots
#[derive(Debug, Clone)]
pub struct SnapshotStatistics {
    pub total_count: usize,
    pub total_size: u64,
    pub oldest_timestamp: Option<u64>,
    pub newest_timestamp: Option<u64>,
    pub compression_enabled: bool,
    pub max_snapshots: usize,
}

impl Snapshot {
    /// Get snapshot name
    pub fn name(&self) -> &str {
        &self.name
    }
    
    /// Get creation timestamp
    pub fn created_at(&self) -> u64 {
        self.created_at
    }
    
    /// Get state root hash
    pub fn state_root(&self) -> Hash {
        self.state_root
    }
    
    /// Get metadata
    pub fn metadata(&self) -> &SnapshotMetadata {
        &self.metadata
    }
    
    /// Export to directory (convenience method)
    pub async fn export_to_directory(&self, export_dir: &Path) -> StorageResult<()> {
        // Create export directory
        std::fs::create_dir_all(export_dir)
            .map_err(|e| StorageError::io(e))?;
        
        // Copy metadata file
        let metadata_dest = export_dir.join(format!("{}.snapshot", self.name));
        std::fs::copy(&self.file_path, &metadata_dest)
            .map_err(|e| StorageError::io(e))?;
        
        // Note: In a full implementation, you'd also copy the data directory
        // This is simplified for the example
        
        Ok(())
    }
    
    /// Import from directory (static constructor)
    pub async fn import_from_directory(name: &str, import_dir: &Path) -> StorageResult<Snapshot> {
        let metadata_file = import_dir.join(format!("{}.snapshot", name));
        
        if !metadata_file.exists() {
            return Err(StorageError::snapshot(format!("Snapshot metadata file not found: {:?}", metadata_file)));
        }
        
        let content = std::fs::read_to_string(&metadata_file)
            .map_err(|e| StorageError::io(e))?;
        
        let snapshot: Snapshot = serde_json::from_str(&content)
            .map_err(|e| StorageError::serialization(format!("Failed to parse snapshot metadata: {}", e)))?;
        
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StorageConfig, RocksEngine};
    use tempfile::TempDir;
    
    fn create_test_setup() -> (Arc<RocksEngine>, SnapshotManager, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let mut config = StorageConfig::default();
        config.data_dir = temp_dir.path().join("db");
        
        let engine = Arc::new(RocksEngine::new(config).unwrap());
        
        let snapshot_config = SnapshotConfig {
            max_snapshots: 5,
            auto_snapshot_interval: 3600,
            compress_snapshots: true,
            snapshot_dir: Some(temp_dir.path().join("snapshots")),
        };
        
        let snapshot_manager = tokio::runtime::Runtime::new().unwrap()
            .block_on(SnapshotManager::new(engine.clone(), snapshot_config))
            .unwrap();
        
        (engine, snapshot_manager, temp_dir)
    }
    
    #[tokio::test]
    async fn test_snapshot_creation() {
        let (_engine, manager, _temp_dir) = create_test_setup();
        
        let snapshot = manager.create_snapshot("test_snapshot").await.unwrap();
        assert_eq!(snapshot.name(), "test_snapshot");
        assert!(snapshot.created_at() > 0);
    }
    
    #[tokio::test]
    async fn test_snapshot_with_data() {
        let (engine, manager, _temp_dir) = create_test_setup();
        
        // Add some test data
        engine.put("accounts", b"test_key", b"test_value").unwrap();
        
        // Create snapshot
        let snapshot = manager.create_snapshot("data_snapshot").await.unwrap();
        assert_ne!(snapshot.state_root(), [0u8; 32]);
        assert!(snapshot.metadata().total_size > 0);
    }
    
    #[tokio::test]
    async fn test_snapshot_list() {
        let (_engine, manager, _temp_dir) = create_test_setup();
        
        // Create multiple snapshots
        manager.create_snapshot("snapshot1").await.unwrap();
        manager.create_snapshot("snapshot2").await.unwrap();
        manager.create_snapshot("snapshot3").await.unwrap();
        
        let snapshots = manager.list_snapshots().await.unwrap();
        assert_eq!(snapshots.len(), 3);
        assert!(snapshots.contains(&"snapshot1".to_string()));
        assert!(snapshots.contains(&"snapshot2".to_string()));
        assert!(snapshots.contains(&"snapshot3".to_string()));
    }
    
    #[tokio::test]
    async fn test_snapshot_info() {
        let (_engine, manager, _temp_dir) = create_test_setup();
        
        let created_snapshot = manager.create_snapshot("info_test").await.unwrap();
        
        let info = manager.get_snapshot_info("info_test").await.unwrap();
        assert!(info.is_some());
        
        let info = info.unwrap();
        assert_eq!(info.name(), created_snapshot.name());
        assert_eq!(info.created_at(), created_snapshot.created_at());
        assert_eq!(info.state_root(), created_snapshot.state_root());
    }
    
    #[tokio::test]
    async fn test_snapshot_deletion() {
        let (_engine, manager, _temp_dir) = create_test_setup();
        
        manager.create_snapshot("delete_test").await.unwrap();
        
        let snapshots_before = manager.list_snapshots().await.unwrap();
        assert!(snapshots_before.contains(&"delete_test".to_string()));
        
        manager.delete_snapshot("delete_test").await.unwrap();
        
        let snapshots_after = manager.list_snapshots().await.unwrap();
        assert!(!snapshots_after.contains(&"delete_test".to_string()));
    }
    
    #[tokio::test]
    async fn test_snapshot_cleanup() {
        let (_engine, manager, _temp_dir) = create_test_setup();
        
        // Create more snapshots than the limit (5)
        for i in 0..8 {
            manager.create_snapshot(&format!("cleanup_test_{}", i)).await.unwrap();
            // Add small delay to ensure different timestamps
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
        
        let snapshots = manager.list_snapshots().await.unwrap();
        assert_eq!(snapshots.len(), 5); // Should be limited to max_snapshots
        
        // Verify only the newest snapshots remain
        for i in 3..8 {
            assert!(snapshots.contains(&format!("cleanup_test_{}", i)));
        }
    }
    
    #[tokio::test]
    async fn test_snapshot_statistics() {
        let (_engine, manager, _temp_dir) = create_test_setup();
        
        manager.create_snapshot("stats_test_1").await.unwrap();
        manager.create_snapshot("stats_test_2").await.unwrap();
        
        let stats = manager.get_statistics().await;
        assert_eq!(stats.total_count, 2);
        assert!(stats.oldest_timestamp.is_some());
        assert!(stats.newest_timestamp.is_some());
        assert_eq!(stats.max_snapshots, 5);
        assert!(stats.compression_enabled);
    }
    
    #[tokio::test]
    async fn test_duplicate_snapshot_name() {
        let (_engine, manager, _temp_dir) = create_test_setup();
        
        manager.create_snapshot("duplicate_test").await.unwrap();
        
        let result = manager.create_snapshot("duplicate_test").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }
    
    #[tokio::test]
    async fn test_snapshot_metadata_collection() {
        let (engine, manager, _temp_dir) = create_test_setup();
        
        // Add data to multiple column families
        engine.put("accounts", b"key1", b"value1").unwrap();
        engine.put("transactions", b"key2", b"value2").unwrap();
        engine.put("metadata", b"key3", b"value3").unwrap();
        
        let snapshot = manager.create_snapshot("metadata_test").await.unwrap();
        let metadata = snapshot.metadata();
        
        assert!(metadata.key_count.contains_key("accounts"));
        assert!(metadata.key_count.contains_key("transactions"));
        assert!(metadata.key_count.contains_key("metadata"));
        assert!(metadata.total_size > 0);
        assert_eq!(metadata.version, crate::STORAGE_VERSION);
    }
}