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
    #[error("Consensus error: {phase}: {message}")]
    Consensus { phase: String, message: String },
    
    /// Runtime execution errors
    #[error("Runtime error: {0}")]
    Runtime(String),
    
    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),
    
    /// Serialization/Deserialization errors
    #[error("Serialization error: {0}")]
    Serialization(String),
    
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
    
    /// Utility-specific errors (integrate your existing UtilError)
    #[error("Utility error: {0}")]
    Util(#[from] crate::errors::UtilError),
}

/// Standard Result type used across Catalyst
pub type CatalystResult<T> = Result<T, CatalystError>;

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
    ($phase:expr, $msg:expr) => {
        $crate::error::CatalystError::Consensus {
            phase: $phase.to_string(),
            message: $msg.to_string(),
        }
    };
    ($phase:expr, $fmt:expr, $($arg:tt)*) => {
        $crate::error::CatalystError::Consensus {
            phase: $phase.to_string(),
            message: format!($fmt, $($arg)*),
        }
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