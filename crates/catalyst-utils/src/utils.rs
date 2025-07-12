/// Convert bytes to hex string with 0x prefix
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

/// Convert hex string to bytes (handles 0x prefix)
pub fn hex_to_bytes(hex_str: &str) -> Result<Vec<u8>, hex::FromHexError> {
    let cleaned = if hex_str.starts_with("0x") {
        &hex_str[2..]
    } else {
        hex_str
    };
    hex::decode(cleaned)
}

// Add these missing error variants to CatalystError in lib.rs:

// Add to the CatalystError enum:
    /// Invalid input or state
    #[error("Invalid: {0}")]
    Invalid(String),
    
    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),
    
    /// Internal system error
    #[error("Internal error: {0}")]
    Internal(String),