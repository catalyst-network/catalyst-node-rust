use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// Add ALL module declarations - THIS IS CRITICAL
pub mod error;
pub mod logging;
pub mod metrics;
pub mod serialization;
pub mod state;
pub mod network;

// Re-export core types that other crates will use
pub use error::{CatalystError, CatalystResult, UtilError};

// Re-export important traits that other crates need
pub use serialization::{CatalystSerialize, CatalystDeserialize, CatalystCodec, SerializationContext, CatalystSerializeWithContext};
pub use state::StateManager;
pub use network::{NetworkMessage, MessageType, MessageEnvelope};

// Common type aliases used across the Catalyst ecosystem
pub type Hash = [u8; 32];
pub type Address = [u8; 21];
pub type PublicKey = [u8; 32];
pub type Signature = [u8; 64];

// Re-export async_trait since many of our traits use it
pub use async_trait::async_trait;

/// System information utilities
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub version: String,
    pub build_timestamp: u64,
    pub target_triple: String,
    pub rust_version: String,
    pub git_commit: Option<String>,
}

impl SystemInfo {
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            build_timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            target_triple: std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string()),
            rust_version: std::env::var("RUSTC_VERSION").unwrap_or_else(|_| "unknown".to_string()),
            git_commit: option_env!("GIT_COMMIT").map(|s| s.to_string()),
        }
    }
}

impl Default for SystemInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Utility functions for common operations
pub mod utils {
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Get current timestamp in seconds
    pub fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// Get current timestamp in milliseconds
    pub fn current_timestamp_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Convert bytes to hex string
    pub fn bytes_to_hex(bytes: &[u8]) -> String {
        hex::encode(bytes)
    }

    /// Convert hex string to bytes
    pub fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, hex::FromHexError> {
        hex::decode(hex)
    }

    /// Generate random bytes
    pub fn random_bytes(len: usize) -> Vec<u8> {
        use rand::RngCore;
        let mut bytes = vec![0u8; len];
        rand::thread_rng().fill_bytes(&mut bytes);
        bytes
    }

    /// Format bytes as human readable size
    pub fn format_bytes(bytes: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        
        if bytes == 0 {
            return "0 B".to_string();
        }

        let unit_index = (63 - bytes.leading_zeros()) as usize / 10;
        let unit_index = unit_index.min(UNITS.len() - 1);
        
        let value = bytes as f64 / (1024_f64.powi(unit_index as i32));
        
        if unit_index == 0 {
            format!("{} {}", bytes, UNITS[unit_index])
        } else {
            format!("{:.2} {}", value, UNITS[unit_index])
        }
    }

    /// Simple rate limiter
    pub struct RateLimiter {
        last_reset: SystemTime,
        tokens: u32,
        max_tokens: u32,
        refill_rate: u32, // tokens per second
    }

    impl RateLimiter {
        pub fn new(max_tokens: u32, refill_rate: u32) -> Self {
            Self {
                last_reset: SystemTime::now(),
                tokens: max_tokens,
                max_tokens,
                refill_rate,
            }
        }

        pub fn try_acquire(&mut self, tokens: u32) -> bool {
            self.refill();
            
            if self.tokens >= tokens {
                self.tokens -= tokens;
                true
            } else {
                false
            }
        }

        fn refill(&mut self) {
            let now = SystemTime::now();
            let elapsed = now.duration_since(self.last_reset).unwrap_or_default();
            let tokens_to_add = (elapsed.as_secs() as u32 * self.refill_rate)
                .min(self.max_tokens - self.tokens);
            
            self.tokens += tokens_to_add;
            self.last_reset = now;
        }
    }
}

// Keep all your existing tests...
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_info() {
        let info = SystemInfo::new();
        assert!(!info.version.is_empty());
        assert!(info.build_timestamp > 0);
    }

    #[test]
    fn test_timestamp_utils() {
        let ts1 = utils::current_timestamp();
        let ts2 = utils::current_timestamp_ms();
        
        assert!(ts1 > 0);
        assert!(ts2 > ts1 * 1000);
    }

    #[test]
    fn test_hex_conversion() {
        let bytes = vec![0x01, 0x23, 0x45, 0x67];
        let hex = utils::bytes_to_hex(&bytes);
        assert_eq!(hex, "01234567");
        
        let decoded = utils::hex_to_bytes(&hex).unwrap();
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(utils::format_bytes(0), "0 B");
        assert_eq!(utils::format_bytes(1024), "1.00 KB");
        assert_eq!(utils::format_bytes(1048576), "1.00 MB");
    }

    #[test]
    fn test_rate_limiter() {
        let mut limiter = utils::RateLimiter::new(10, 1);
        
        // Should be able to acquire tokens initially
        assert!(limiter.try_acquire(5));
        assert!(limiter.try_acquire(5));
        
        // Should fail when out of tokens
        assert!(!limiter.try_acquire(1));
    }
}

#[cfg(test)]
mod error_tests {
    use super::*;
    
    #[test]
    fn test_error_creation_macros() {
        let crypto_err = crypto_error!("Invalid key length");
        match crypto_err {
            CatalystError::Crypto(msg) => assert_eq!(msg, "Invalid key length"),
            _ => panic!("Wrong error type"),
        }
        
        let consensus_err = consensus_error!("voting", "Failed to reach threshold");
        match consensus_err {
            CatalystError::Consensus { phase, message } => {
                assert_eq!(phase, "voting");
                assert_eq!(message, "Failed to reach threshold");
            },
            _ => panic!("Wrong error type"),
        }
        
        let timeout_err = timeout_error!(5000, "block proposal");
        match timeout_err {
            CatalystError::Timeout { duration_ms, operation } => {
                assert_eq!(duration_ms, 5000);
                assert_eq!(operation, "block proposal");
            },
            _ => panic!("Wrong error type"),
        }
    }
    
    #[test]
    fn test_error_conversion() {
        let util_err = UtilError::Timeout;
        let catalyst_err: CatalystError = util_err.into();
        
        match catalyst_err {
            CatalystError::Util(_) => (),
            _ => panic!("Conversion failed"),
        }
    }
}