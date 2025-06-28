//! Error types for the storage layer

use thiserror::Error;

/// Storage-specific error types
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("RocksDB error: {0}")]
    RocksDb(#[from] rocksdb::Error),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Transaction error: {0}")]
    Transaction(String),
    
    #[error("Migration error: {0}")]
    Migration(String),
    
    #[error("Snapshot error: {0}")]
    Snapshot(String),
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Column family not found: {0}")]
    ColumnFamilyNotFound(String),
    
    #[error("Database corruption detected: {0}")]
    Corruption(String),
    
    #[error("Insufficient disk space: required {required}, available {available}")]
    InsufficientSpace { required: u64, available: u64 },
    
    #[error("Database locked by another process")]
    DatabaseLocked,
    
    #[error("Invalid key: {0}")]
    InvalidKey(String),
    
    #[error("Invalid value: {0}")]
    InvalidValue(String),
    
    #[error("Timeout during operation: {operation}")]
    Timeout { operation: String },
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Convenience type alias for storage results
pub type StorageResult<T> = Result<T, StorageError>;

impl StorageError {
    /// Create a serialization error
    pub fn serialization<S: AsRef<str>>(msg: S) -> Self {
        StorageError::Serialization(msg.as_ref().to_string())
    }
    
    /// Create a transaction error
    pub fn transaction<S: AsRef<str>>(msg: S) -> Self {
        StorageError::Transaction(msg.as_ref().to_string())
    }
    
    /// Create a migration error
    pub fn migration<S: AsRef<str>>(msg: S) -> Self {
        StorageError::Migration(msg.as_ref().to_string())
    }
    
    /// Create a snapshot error
    pub fn snapshot<S: AsRef<str>>(msg: S) -> Self {
        StorageError::Snapshot(msg.as_ref().to_string())
    }
    
    /// Create a configuration error
    pub fn config<S: AsRef<str>>(msg: S) -> Self {
        StorageError::Config(msg.as_ref().to_string())
    }
    
    /// Create an internal error
    pub fn internal<S: AsRef<str>>(msg: S) -> Self {
        StorageError::Internal(msg.as_ref().to_string())
    }
    
    /// Check if this error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            StorageError::RocksDb(_) => false,
            StorageError::Corruption(_) => false,
            StorageError::DatabaseLocked => true,
            StorageError::InsufficientSpace { .. } => false,
            StorageError::Timeout { .. } => true,
            StorageError::Io(_) => false,
            _ => true,
        }
    }
    
    /// Check if this error indicates data corruption
    pub fn is_corruption(&self) -> bool {
        matches!(self, StorageError::Corruption(_))
    }
    
    /// Get error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            StorageError::RocksDb(_) => "rocksdb",
            StorageError::Serialization(_) => "serialization",
            StorageError::Transaction(_) => "transaction",
            StorageError::Migration(_) => "migration",
            StorageError::Snapshot(_) => "snapshot",
            StorageError::Config(_) => "config",
            StorageError::ColumnFamilyNotFound(_) => "column_family",
            StorageError::Corruption(_) => "corruption",
            StorageError::InsufficientSpace { .. } => "disk_space",
            StorageError::DatabaseLocked => "lock",
            StorageError::InvalidKey(_) => "invalid_key",
            StorageError::InvalidValue(_) => "invalid_value",
            StorageError::Timeout { .. } => "timeout",
            StorageError::Io(_) => "io",
            StorageError::Internal(_) => "internal",
        }
    }
}

/// Convert StorageError to CatalystError
impl From<StorageError> for catalyst_utils::CatalystError {
    fn from(err: StorageError) -> Self {
        catalyst_utils::CatalystError::Storage(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_categories() {
        assert_eq!(StorageError::config("test").category(), "config");
        assert_eq!(StorageError::transaction("test").category(), "transaction");
        assert_eq!(StorageError::DatabaseLocked.category(), "lock");
    }
    
    #[test]
    fn test_error_recoverability() {
        assert!(StorageError::DatabaseLocked.is_recoverable());
        assert!(!StorageError::Corruption("test".to_string()).is_recoverable());
        assert!(StorageError::Timeout { operation: "test".to_string() }.is_recoverable());
    }
    
    #[test]
    fn test_corruption_detection() {
        assert!(StorageError::Corruption("test".to_string()).is_corruption());
        assert!(!StorageError::DatabaseLocked.is_corruption());
    }
}