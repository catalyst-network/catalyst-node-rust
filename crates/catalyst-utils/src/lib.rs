use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// Module declarations - comment out missing modules for Phase 1
// pub mod encoding;     // TODO: Create for Phase 2
// pub mod metrics;      // TODO: Create for Phase 2  
// pub mod time;         // TODO: Create for Phase 2
// pub mod async_utils;  // TODO: Create for Phase 2
// pub mod math;         // TODO: Create for Phase 2

// Re-exports - comment out for now
// pub use encoding::*;
// pub use metrics::*;
// pub use time::*;
// pub use async_utils::*;
// pub use math::*;

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

/// Logging utilities
pub mod logging {
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    /// Initialize logging with default configuration
    pub fn init_logging() {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    /// Initialize logging with custom level
    pub fn init_logging_with_level(level: &str) {
        tracing_subscriber::registry()
            .with(tracing_subscriber::EnvFilter::new(level))
            .with(tracing_subscriber::fmt::layer())
            .init();
    }
}

/// Error handling utilities
pub mod errors {
    use std::fmt;

    /// Generic result type for utilities
    pub type UtilResult<T> = Result<T, UtilError>;

    /// Common utility errors
    #[derive(Debug)]
    pub enum UtilError {
        Io(std::io::Error),
        Parse(String),
        Timeout,
        RateLimited,
        InvalidInput(String),
    }

    impl fmt::Display for UtilError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
            UtilError::Io(err)
        }
    }
}

/// Network utilities
pub mod network {
    use std::net::{IpAddr, SocketAddr};
    use std::str::FromStr;

    /// Parse a network address string
    pub fn parse_address(addr: &str) -> Result<SocketAddr, crate::errors::UtilError> {
        SocketAddr::from_str(addr)
            .map_err(|e| crate::errors::UtilError::Parse(e.to_string()))
    }

    /// Check if an IP address is local
    pub fn is_local_ip(ip: &IpAddr) -> bool {
        match ip {
            IpAddr::V4(ipv4) => {
                ipv4.is_loopback() || ipv4.is_private() || ipv4.is_link_local()
            }
            IpAddr::V6(ipv6) => {
                ipv6.is_loopback() || ipv6.is_unspecified()
            }
        }
    }

    /// Get local IP addresses
    pub fn get_local_interfaces() -> Vec<IpAddr> {
        // Simplified implementation - in production you might use a crate like `local-ip-address`
        vec![
            IpAddr::from_str("127.0.0.1").unwrap(),
            IpAddr::from_str("::1").unwrap(),
        ]
    }
}

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

    #[test]
    fn test_network_utils() {
        let addr = network::parse_address("127.0.0.1:8000").unwrap();
        assert_eq!(addr.port(), 8000);
        
        let local_ip = std::net::IpAddr::from_str("127.0.0.1").unwrap();
        assert!(network::is_local_ip(&local_ip));
    }
}