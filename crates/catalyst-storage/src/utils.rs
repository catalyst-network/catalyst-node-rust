//! Utility functions for storage operations

use crate::{StorageError, StorageResult};
use std::path::Path;
use std::fs;

/// Calculate directory size recursively
pub fn calculate_directory_size(path: &Path) -> StorageResult<u64> {
    if !path.exists() {
        return Ok(0);
    }
    
    if path.is_file() {
        return Ok(path.metadata()
            .map_err(|e| StorageError::io(e))?
            .len());
    }
    
    let mut size = 0u64;
    let entries = fs::read_dir(path)
        .map_err(|e| StorageError::io(e))?;
    
    for entry in entries {
        let entry = entry.map_err(|e| StorageError::io(e))?;
        let entry_path = entry.path();
        
        if entry_path.is_dir() {
            size += calculate_directory_size(&entry_path)?;
        } else {
            size += entry.metadata()
                .map_err(|e| StorageError::io(e))?
                .len();
        }
    }
    
    Ok(size)
}

/// Check available disk space
pub fn check_disk_space(path: &Path) -> StorageResult<u64> {
    // Note: This is a simplified implementation
    // In a real implementation, you'd use platform-specific APIs
    // to get actual disk space information
    
    if !path.exists() {
        return Err(StorageError::config(format!("Path does not exist: {:?}", path)));
    }
    
    // For now, return a large number
    // In practice, you'd use statvfs on Unix or GetDiskFreeSpaceEx on Windows
    Ok(u64::MAX)
}

/// Ensure directory exists and is writable
pub fn ensure_directory_writable(path: &Path) -> StorageResult<()> {
    // Create directory if it doesn't exist
    if !path.exists() {
        fs::create_dir_all(path)
            .map_err(|e| StorageError::config(format!("Failed to create directory {:?}: {}", path, e)))?;
    }
    
    // Check if it's actually a directory
    if !path.is_dir() {
        return Err(StorageError::config(format!("Path is not a directory: {:?}", path)));
    }
    
    // Check if it's writable by trying to create a test file
    let test_file = path.join(".write_test");
    match fs::write(&test_file, b"test") {
        Ok(_) => {
            // Clean up test file
            let _ = fs::remove_file(&test_file);
            Ok(())
        }
        Err(e) => Err(StorageError::config(format!("Directory not writable {:?}: {}", path, e)))
    }
}

/// Format bytes into human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB", "PB"];
    
    if bytes == 0 {
        return "0 B".to_string();
    }
    
    let mut size = bytes as f64;
    let mut unit_index = 0;
    
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }
    
    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

/// Generate a random database path for testing
#[cfg(feature = "testing")]
pub fn random_test_path() -> std::path::PathBuf {
    use std::env;
    use uuid::Uuid;
    
    let temp_dir = env::temp_dir();
    temp_dir.join(format!("catalyst_test_{}", Uuid::new_v4()))
}

/// Clean up test database
#[cfg(feature = "testing")]
pub fn cleanup_test_db(path: &Path) -> StorageResult<()> {
    if path.exists() {
        fs::remove_dir_all(path)
            .map_err(|e| StorageError::io(e))?;
    }
    Ok(())
}

/// Key-value pair for database operations
#[derive(Debug, Clone, PartialEq)]
pub struct KeyValue {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

impl KeyValue {
    pub fn new(key: Vec<u8>, value: Vec<u8>) -> Self {
        Self { key, value }
    }
    
    pub fn size(&self) -> usize {
        self.key.len() + self.value.len()
    }
}

/// Batch of key-value operations
#[derive(Debug, Clone)]
pub struct KeyValueBatch {
    pub operations: Vec<KeyValueOperation>,
}

#[derive(Debug, Clone)]
pub enum KeyValueOperation {
    Put { key: Vec<u8>, value: Vec<u8> },
    Delete { key: Vec<u8> },
}

impl KeyValueBatch {
    pub fn new() -> Self {
        Self {
            operations: Vec::new(),
        }
    }
    
    pub fn put(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.operations.push(KeyValueOperation::Put { key, value });
    }
    
    pub fn delete(&mut self, key: Vec<u8>) {
        self.operations.push(KeyValueOperation::Delete { key });
    }
    
    pub fn len(&self) -> usize {
        self.operations.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
    
    pub fn estimated_size(&self) -> usize {
        self.operations.iter().map(|op| match op {
            KeyValueOperation::Put { key, value } => key.len() + value.len(),
            KeyValueOperation::Delete { key } => key.len(),
        }).sum()
    }
}

impl Default for KeyValueBatch {
    fn default() -> Self {
        Self::new()
    }
}

/// Database iterator wrapper for easier use
pub struct DatabaseIterator<'a> {
    inner: rocksdb::DBIterator<'a>,
}

impl<'a> DatabaseIterator<'a> {
    pub fn new(inner: rocksdb::DBIterator<'a>) -> Self {
        Self { inner }
    }
    
    pub fn seek_to_first(&mut self) {
        self.inner.seek_to_first();
    }
    
    pub fn seek_to_last(&mut self) {
        self.inner.seek_to_last();
    }
    
    pub fn seek(&mut self, key: &[u8]) {
        self.inner.seek(key);
    }
    
    pub fn collect_all(&mut self) -> StorageResult<Vec<KeyValue>> {
        let mut results = Vec::new();
        
        for item in &mut self.inner {
            let (key, value) = item
                .map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))?;
            results.push(KeyValue::new(key.to_vec(), value.to_vec()));
        }
        
        Ok(results)
    }
    
    pub fn collect_keys(&mut self) -> StorageResult<Vec<Vec<u8>>> {
        let mut results = Vec::new();
        
        for item in &mut self.inner {
            let (key, _) = item
                .map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))?;
            results.push(key.to_vec());
        }
        
        Ok(results)
    }
    
    pub fn count(&mut self) -> StorageResult<u64> {
        let mut count = 0u64;
        
        for item in &mut self.inner {
            item.map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))?;
            count += 1;
        }
        
        Ok(count)
    }
}

impl<'a> Iterator for DatabaseIterator<'a> {
    type Item = StorageResult<KeyValue>;
    
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|result| {
            result
                .map(|(key, value)| KeyValue::new(key.to_vec(), value.to_vec()))
                .map_err(|e| StorageError::internal(format!("Iterator error: {}", e)))
        })
    }
}

/// Database range for iteration
#[derive(Debug, Clone)]
pub struct Range {
    pub start: Option<Vec<u8>>,
    pub end: Option<Vec<u8>>,
}

impl Range {
    pub fn new(start: Option<Vec<u8>>, end: Option<Vec<u8>>) -> Self {
        Self { start, end }
    }
    
    pub fn from_start(start: Vec<u8>) -> Self {
        Self {
            start: Some(start),
            end: None,
        }
    }
    
    pub fn to_end(end: Vec<u8>) -> Self {
        Self {
            start: None,
            end: Some(end),
        }
    }
    
    pub fn between(start: Vec<u8>, end: Vec<u8>) -> Self {
        Self {
            start: Some(start),
            end: Some(end),
        }
    }
    
    pub fn all() -> Self {
        Self {
            start: None,
            end: None,
        }
    }
}

/// Compaction options
#[derive(Debug, Clone)]
pub struct CompactionOptions {
    pub target_level: Option<i32>,
    pub bottommost_level_compaction: bool,
    pub change_level: bool,
    pub exclusive_manual_compaction: bool,
}

impl Default for CompactionOptions {
    fn default() -> Self {
        Self {
            target_level: None,
            bottommost_level_compaction: false,
            change_level: false,
            exclusive_manual_compaction: true,
        }
    }
}

/// Database repair utilities
pub struct RepairUtilities;

impl RepairUtilities {
    /// Check database integrity
    pub fn check_integrity(db_path: &Path) -> StorageResult<IntegrityReport> {
        // Note: This is a simplified integrity check
        // In a real implementation, you'd use RocksDB's built-in repair tools
        
        if !db_path.exists() {
            return Err(StorageError::config(format!("Database path does not exist: {:?}", db_path)));
        }
        
        // Check for basic structural integrity
        let mut issues = Vec::new();
        
        // Check for lock file
        let lock_file = db_path.join("LOCK");
        if !lock_file.exists() {
            issues.push("Missing LOCK file".to_string());
        }
        
        // Check for current file
        let current_file = db_path.join("CURRENT");
        if !current_file.exists() {
            issues.push("Missing CURRENT file".to_string());
        }
        
        Ok(IntegrityReport {
            is_healthy: issues.is_empty(),
            issues,
            checked_at: catalyst_utils::utils::current_timestamp(),
        })
    }
    
    /// Attempt to repair database
    pub fn repair_database(db_path: &Path) -> StorageResult<()> {
        use rocksdb::DB;
        
        // Use RocksDB's built-in repair function
        DB::repair(&rocksdb::Options::default(), db_path)
            .map_err(|e| StorageError::internal(format!("Database repair failed: {}", e)))?;
        
        Ok(())
    }
}

/// Database integrity report
#[derive(Debug, Clone)]
pub struct IntegrityReport {
    pub is_healthy: bool,
    pub issues: Vec<String>,
    pub checked_at: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(100), "100 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(1048576), "1.00 MB");
        assert_eq!(format_bytes(1073741824), "1.00 GB");
    }
    
    #[test]
    fn test_calculate_directory_size() {
        let temp_dir = TempDir::new().unwrap();
        let test_dir = temp_dir.path();
        
        // Empty directory
        let size = calculate_directory_size(test_dir).unwrap();
        assert_eq!(size, 0);
        
        // Create some files
        std::fs::write(test_dir.join("file1.txt"), b"hello").unwrap();
        std::fs::write(test_dir.join("file2.txt"), b"world!").unwrap();
        
        let size = calculate_directory_size(test_dir).unwrap();
        assert_eq!(size, 11); // 5 + 6 bytes
    }
    
    #[test]
    fn test_ensure_directory_writable() {
        let temp_dir = TempDir::new().unwrap();
        let test_dir = temp_dir.path().join("new_dir");
        
        // Directory doesn't exist yet
        assert!(!test_dir.exists());
        
        // Should create and verify writability
        ensure_directory_writable(&test_dir).unwrap();
        assert!(test_dir.exists());
        assert!(test_dir.is_dir());
    }
    
    #[test]
    fn test_key_value_batch() {
        let mut batch = KeyValueBatch::new();
        assert!(batch.is_empty());
        
        batch.put(b"key1".to_vec(), b"value1".to_vec());
        batch.put(b"key2".to_vec(), b"value2".to_vec());
        batch.delete(b"key3".to_vec());
        
        assert_eq!(batch.len(), 3);
        assert!(!batch.is_empty());
        
        let estimated_size = batch.estimated_size();
        assert!(estimated_size > 0);
        assert_eq!(estimated_size, 4 + 6 + 4 + 6 + 4); // keys + values
    }
    
    #[test]
    fn test_range() {
        let range1 = Range::all();
        assert!(range1.start.is_none());
        assert!(range1.end.is_none());
        
        let range2 = Range::from_start(b"start".to_vec());
        assert_eq!(range2.start, Some(b"start".to_vec()));
        assert!(range2.end.is_none());
        
        let range3 = Range::between(b"start".to_vec(), b"end".to_vec());
        assert_eq!(range3.start, Some(b"start".to_vec()));
        assert_eq!(range3.end, Some(b"end".to_vec()));
    }
    
    #[test]
    fn test_key_value() {
        let kv = KeyValue::new(b"key".to_vec(), b"value".to_vec());
        assert_eq!(kv.key, b"key");
        assert_eq!(kv.value, b"value");
        assert_eq!(kv.size(), 8); // 3 + 5 bytes
    }
    
    #[cfg(feature = "testing")]
    #[test]
    fn test_random_test_path() {
        let path1 = random_test_path();
        let path2 = random_test_path();
        
        // Should be different paths
        assert_ne!(path1, path2);
        
        // Should be in temp directory
        assert!(path1.starts_with(std::env::temp_dir()));
        assert!(path2.starts_with(std::env::temp_dir()));
    }
    
    #[test]
    fn test_integrity_check() {
        let temp_dir = TempDir::new().unwrap();
        
        // Non-existent database
        let result = RepairUtilities::check_integrity(&temp_dir.path().join("nonexistent"));
        assert!(result.is_err());
        
        // Empty directory (not a valid database)
        let report = RepairUtilities::check_integrity(temp_dir.path()).unwrap();
        assert!(!report.is_healthy);
        assert!(!report.issues.is_empty());
    }
}