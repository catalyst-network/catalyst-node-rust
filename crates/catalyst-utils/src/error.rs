use thiserror::Error;

/// Core error types used across all Catalyst crates
#[derive(Error, Debug, Clone)]
pub enum CatalystError {
    /// Cryptographic operation failed
    #[error("Cryptographic error: {0}")]
    Crypto(String),
    
    /// Network-related errors
    #[error("Network error: {0}")]
    Network(String),
    
    /// Storage/Database errors
    #[error("Storage error: {0}")]
    Storage(String),
    
    /// Consensus-related errors
    #[error("Consensus error: {0}")]
    Consensus(String),
    
    /// Runtime execution errors
    #[error("Runtime error: {0}")]
    Runtime(String),
    
    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),
    
    /// Serialization/Deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    /// Transaction error
    #[error("Transaction error: {0}")]
    Transaction(String),
    
    /// Contract execution error
    #[error("Contract error: {0}")]
    Contract(String),
    
    /// DFS (Distributed File System) error
    #[error("DFS error: {0}")]
    Dfs(String),
    
    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),
    
    /// Invalid input or state
    #[error("Invalid: {0}")]
    Invalid(String),
    
    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),
    
    /// Operation timed out
    #[error("Timeout after {duration_ms}ms: {operation}")]
    Timeout { duration_ms: u64, operation: String },
    
    /// Internal system error
    #[error("Internal error: {0}")]
    Internal(String),
    
    /// Utility-specific errors
    #[error("Utility error: {0}")]
    Util(#[from] UtilError),
    
    /// Generic error with message
    #[error("Error: {0}")]
    Generic(String),
}

/// Standard Result type used across Catalyst
pub type CatalystResult<T> = Result<T, CatalystError>;

/// Common utility errors (moved from lib.rs to avoid circular dependency)
#[derive(Debug, Clone)]
pub enum UtilError {
    Io(String),
    Parse(String),
    Timeout,
    RateLimited,
    InvalidInput(String),
}

impl std::fmt::Display for UtilError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UtilError::Io(err) => write!(f, "IO error: {}", err),
            UtilError::Parse(msg) => write!(f, "Parse error: {}", msg),
            UtilError::Timeout => write!(f, "Operation timed out"),
            UtilError::RateLimited => write!(f, "Rate limited"),
            UtilError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
        }
    }
}

impl std::error::Error for UtilError {}

impl From<std::io::Error> for UtilError {
    fn from(err: std::io::Error) -> Self {
        UtilError::Io(err.to_string())
    }
}

impl From<hex::FromHexError> for UtilError {
    fn from(err: hex::FromHexError) -> Self {
        UtilError::Parse(format!("Hex decode error: {}", err))
    }
}

// Additional From implementations for CatalystError
impl From<std::io::Error> for CatalystError {
    fn from(err: std::io::Error) -> Self {
        CatalystError::Util(UtilError::from(err))
    }
}

impl From<hex::FromHexError> for CatalystError {
    fn from(err: hex::FromHexError) -> Self {
        CatalystError::Util(UtilError::from(err))
    }
}

impl From<serde_json::Error> for CatalystError {
    fn from(err: serde_json::Error) -> Self {
        CatalystError::Serialization(format!("JSON error: {}", err))
    }
}

/// Convenience macros for creating errors
#[macro_export]
macro_rules! crypto_error {
    ($msg:expr) => {
        $crate::error::CatalystError::Crypto($msg.to_string())
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::error::CatalystError::Crypto(format!($fmt, $($arg)*))
    };
}

#[macro_export]
macro_rules! network_error {
    ($msg:expr) => {
        $crate::error::CatalystError::Network($msg.to_string())
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::error::CatalystError::Network(format!($fmt, $($arg)*))
    };
}

#[macro_export]
macro_rules! consensus_error {
    ($msg:expr) => {
        $crate::error::CatalystError::Consensus($msg.to_string())
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::error::CatalystError::Consensus(format!($fmt, $($arg)*))
    };
}

#[macro_export]
macro_rules! storage_error {
    ($msg:expr) => {
        $crate::error::CatalystError::Storage($msg.to_string())
    };
    ($fmt:expr, $($arg:tt)*) => {
        $crate::error::CatalystError::Storage(format!($fmt, $($arg)*))
    };
}

#[macro_export]
macro_rules! timeout_error {
    ($duration_ms:expr, $operation:expr) => {
        $crate::error::CatalystError::Timeout {
            duration_ms: $duration_ms,
            operation: $operation.to_string(),
        }
    };
}