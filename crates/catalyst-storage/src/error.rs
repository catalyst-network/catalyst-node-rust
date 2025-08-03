//! Error types for storage operations

use thiserror::Error;

/// Storage operation result type
pub type StorageResult<T> = Result<T, StorageError>;

/// Storage error types
#[derive(Error, Debug)]
pub enum StorageError {
    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),
    
    /// Internal storage error
    #[error("Internal storage error: {0}")]
    Internal(String),
    
    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    /// Transaction error
    #[error("Transaction error: {0}")]
    Transaction(String),
    
    /// Migration error
    #[error("Migration error: {0}")]
    Migration(String),
    
    /// Snapshot error
    #[error("Snapshot error: {0}")]
    Snapshot(String),
    
    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    
    /// Column family not found
    #[error("Column family not found: {0}")]
    ColumnFamilyNotFound(String),
    
    /// Database corruption
    #[error("Database corruption detected: {0}")]
    Corruption(String),
    
    /// Storage limit exceeded
    #[error("Storage limit exceeded: {0}")]
    LimitExceeded(String),
    
    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    
    /// Resource not available
    #[error("Resource not available: {0}")]
    ResourceUnavailable(String),
}

impl StorageError {
    /// Create a configuration error
    pub fn config<S: AsRef<str>>(msg: S) -> Self {
        Self::Config(msg.as_ref().to_string())
    }
    
    /// Create an internal error
    pub fn internal<S: AsRef<str>>(msg: S) -> Self {
        Self::Internal(msg.as_ref().to_string())
    }
    
    /// Create a serialization error
    pub fn serialization<S: AsRef<str>>(msg: S) -> Self {
        Self::Serialization(msg.as_ref().to_string())
    }
    
    /// Create a transaction error
    pub fn transaction<S: AsRef<str>>(msg: S) -> Self {
        Self::Transaction(msg.as_ref().to_string())
    }
    
    /// Create a migration error
    pub fn migration<S: AsRef<str>>(msg: S) -> Self {
        Self::Migration(msg.as_ref().to_string())
    }
    
    /// Create a snapshot error
    pub fn snapshot<S: AsRef<str>>(msg: S) -> Self {
        Self::Snapshot(msg.as_ref().to_string())
    }
    
    /// Create an I/O error from std::io::Error
    pub fn io(err: std::io::Error) -> Self {
        Self::Io(err)
    }
    
    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::Config(_) | Self::PermissionDenied(_) => false,
            Self::Corruption(_) => false,
            Self::ResourceUnavailable(_) | Self::LimitExceeded(_) => true,
            _ => true,
        }
    }
    
    /// Get error category
    pub fn category(&self) -> &'static str {
        match self {
            Self::Config(_) => "configuration",
            Self::Internal(_) => "internal",
            Self::Serialization(_) => "serialization", 
            Self::Transaction(_) => "transaction",
            Self::Migration(_) => "migration",
            Self::Snapshot(_) => "snapshot",
            Self::Io(_) => "io",
            Self::ColumnFamilyNotFound(_) => "column_family",
            Self::Corruption(_) => "corruption",
            Self::LimitExceeded(_) => "limit",
            Self::PermissionDenied(_) => "permission",
            Self::ResourceUnavailable(_) => "resource",
        }
    }
}

/// Convert from RocksDB error
impl From<rocksdb::Error> for StorageError {
    fn from(err: rocksdb::Error) -> Self {
        // Check if it's a corruption error
        let err_str = err.to_string();
        if err_str.contains("Corruption") || err_str.contains("corruption") {
            Self::Corruption(err_str)
        } else {
            Self::Internal(err_str)
        }
    }
}

/// Convert from serde_json error
impl From<serde_json::Error> for StorageError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

/// Convert from bincode error
impl From<bincode::Error> for StorageError {
    fn from(err: bincode::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_error_creation() {
        let config_err = StorageError::config("invalid config");
        assert!(matches!(config_err, StorageError::Config(_)));
        
        let internal_err = StorageError::internal("internal failure");
        assert!(matches!(internal_err, StorageError::Internal(_)));
        
        let serialization_err = StorageError::serialization("failed to serialize");
        assert!(matches!(serialization_err, StorageError::Serialization(_)));
    }
    
    #[test]
    fn test_error_categories() {
        let config_err = StorageError::config("test");
        assert_eq!(config_err.category(), "configuration");
        
        let io_err = StorageError::io(std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"));
        assert_eq!(io_err.category(), "io");
    }
    
    #[test]
    fn test_recoverable() {
        let config_err = StorageError::config("test");
        assert!(!config_err.is_recoverable());
        
        let corruption_err = StorageError::Corruption("corrupted".to_string());
        assert!(!corruption_err.is_recoverable());
        
        let limit_err = StorageError::LimitExceeded("too big".to_string());
        assert!(limit_err.is_recoverable());
    }
    
    #[test]
    fn test_display() {
        let err = StorageError::config("test message");
        assert_eq!(err.to_string(), "Configuration error: test message");
    }
    
    #[test]
    fn test_from_conversions() {
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let storage_error = StorageError::from(io_error);
        assert!(matches!(storage_error, StorageError::Io(_)));
        
        let json_error = serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let storage_error = StorageError::from(json_error);
        assert!(matches!(storage_error, StorageError::Serialization(_)));
    }
}