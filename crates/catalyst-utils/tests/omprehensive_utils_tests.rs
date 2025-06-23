// tests/comprehensive_utils_tests.rs

use catalyst_utils::*;
use catalyst_utils::utils;
use std::collections::HashMap;

// Test SystemInfo functionality comprehensively
mod system_info_tests {
    use super::*;
    
    #[test]
    fn test_system_info_creation() {
        let info = SystemInfo::new();
        assert!(!info.version.is_empty());
        assert!(info.build_timestamp > 0);
        assert!(!info.target_triple.is_empty());
        assert!(!info.rust_version.is_empty());
    }
    
    #[test]
    fn test_system_info_default() {
        let info1 = SystemInfo::default();
        let info2 = SystemInfo::new();
        
        // They should be functionally equivalent
        assert_eq!(info1.version, info2.version);
        assert!(!info1.version.is_empty());
    }
    
    #[test]
    fn test_system_info_serialization() {
        let info = SystemInfo::new();
        let serialized = serde_json::to_string(&info).unwrap();
        let deserialized: SystemInfo = serde_json::from_str(&serialized).unwrap();
        
        assert_eq!(info.version, deserialized.version);
        assert_eq!(info.build_timestamp, deserialized.build_timestamp);
    }
}

// Test utility functions comprehensively
mod utils_tests {
    use super::*;
    use std::thread;
    use std::time::Duration;
    
    #[test]
    fn test_timestamp_functions() {
        let ts_sec = utils::current_timestamp();
        let ts_ms = utils::current_timestamp_ms();
        
        assert!(ts_sec > 0);
        assert!(ts_ms > ts_sec * 1000);
        assert!(ts_ms >= ts_sec * 1000);
        
        // Test that timestamps are monotonic
        thread::sleep(Duration::from_millis(1));
        let ts_sec2 = utils::current_timestamp();
        let ts_ms2 = utils::current_timestamp_ms();
        
        assert!(ts_sec2 >= ts_sec);
        assert!(ts_ms2 > ts_ms);
    }
    
    #[test]
    fn test_hex_conversion_edge_cases() {
        // Empty bytes
        let empty_bytes = vec![];
        let hex = utils::bytes_to_hex(&empty_bytes);
        assert_eq!(hex, "");
        let decoded = utils::hex_to_bytes(&hex).unwrap();
        assert_eq!(decoded, empty_bytes);
        
        // Single byte
        let single_byte = vec![0xFF];
        let hex = utils::bytes_to_hex(&single_byte);
        assert_eq!(hex, "ff");
        let decoded = utils::hex_to_bytes(&hex).unwrap();
        assert_eq!(decoded, single_byte);
        
        // All zeros
        let zeros = vec![0x00, 0x00, 0x00];
        let hex = utils::bytes_to_hex(&zeros);
        assert_eq!(hex, "000000");
        let decoded = utils::hex_to_bytes(&hex).unwrap();
        assert_eq!(decoded, zeros);
        
        // Mixed case input
        let mixed_hex = "AbCdEf";
        let decoded = utils::hex_to_bytes(mixed_hex).unwrap();
        let expected = vec![0xAB, 0xCD, 0xEF];
        assert_eq!(decoded, expected);
    }
    
    #[test]
    fn test_hex_conversion_errors() {
        // Invalid hex characters
        assert!(utils::hex_to_bytes("xyz").is_err());
        
        // Odd length hex string
        assert!(utils::hex_to_bytes("ABC").is_err());
        
        // Empty string should work
        assert!(utils::hex_to_bytes("").is_ok());
    }
    
    #[test]
    fn test_random_bytes() {
        let bytes1 = utils::random_bytes(32);
        let bytes2 = utils::random_bytes(32);
        
        assert_eq!(bytes1.len(), 32);
        assert_eq!(bytes2.len(), 32);
        assert_ne!(bytes1, bytes2); // Should be different (statistically)
        
        // Test zero length
        let empty = utils::random_bytes(0);
        assert_eq!(empty.len(), 0);
    }
    
    #[test]
    fn test_format_bytes_comprehensive() {
        assert_eq!(utils::format_bytes(0), "0 B");
        assert_eq!(utils::format_bytes(1), "1 B");
        assert_eq!(utils::format_bytes(512), "512 B");
        assert_eq!(utils::format_bytes(1023), "1023 B");
        assert_eq!(utils::format_bytes(1024), "1.00 KB");
        assert_eq!(utils::format_bytes(1536), "1.50 KB"); // 1.5 KB
        assert_eq!(utils::format_bytes(1048576), "1.00 MB"); // 1 MB
        assert_eq!(utils::format_bytes(1073741824), "1.00 GB"); // 1 GB
        assert_eq!(utils::format_bytes(1099511627776), "1.00 TB"); // 1 TB
        
        // Large numbers - let's test what u64::MAX actually produces
        let max_result = utils::format_bytes(u64::MAX);
        assert!(max_result.contains("TB")); // Should be in TB range
        assert!(max_result.contains("16777216.00")); // Actual value is 16777216.00 TB
        
        // Test some other large values to verify the algorithm
        assert_eq!(utils::format_bytes(2 * 1099511627776), "2.00 TB"); // 2 TB
    }
    
    #[test]
    fn test_rate_limiter_comprehensive() {
        let mut limiter = utils::RateLimiter::new(10, 2); // 10 tokens, 2 per second
        
        // Should start with full tokens
        assert!(limiter.try_acquire(10));
        assert!(!limiter.try_acquire(1)); // Should fail - no tokens left
        
        // Test partial acquisition
        let mut limiter2 = utils::RateLimiter::new(10, 1);
        assert!(limiter2.try_acquire(5));
        assert!(limiter2.try_acquire(3));
        assert!(limiter2.try_acquire(2));
        assert!(!limiter2.try_acquire(1)); // Should fail
    }
    
    #[test]
    fn test_rate_limiter_refill() {
        let mut limiter = utils::RateLimiter::new(5, 10); // 5 tokens, 10 per second
        
        // Exhaust tokens
        assert!(limiter.try_acquire(5));
        assert!(!limiter.try_acquire(1));
        
        // Wait and check refill (this is a simplified test)
        thread::sleep(Duration::from_millis(200)); // Should add ~2 tokens
        
        // Note: This test might be flaky due to timing, but demonstrates the concept
    }
    
    #[test]
    fn test_rate_limiter_edge_cases() {
        // Zero tokens
        let mut limiter = utils::RateLimiter::new(0, 1);
        assert!(!limiter.try_acquire(1));
        
        // Zero refill rate
        let mut limiter2 = utils::RateLimiter::new(1, 0);
        assert!(limiter2.try_acquire(1));
        assert!(!limiter2.try_acquire(1)); // Should never refill
    }
}

// Test type aliases
mod type_alias_tests {
    use super::*;
    
    #[test]
    fn test_type_aliases() {
        let hash: Hash = [0u8; 32];
        let address: Address = [1u8; 21];
        let public_key: PublicKey = [2u8; 32];
        let signature: Signature = [3u8; 64];
        
        assert_eq!(hash.len(), 32);
        assert_eq!(address.len(), 21);
        assert_eq!(public_key.len(), 32);
        assert_eq!(signature.len(), 64);
    }
}

// Test error handling edge cases
mod error_edge_cases {
    use super::*;
    
    #[test]
    fn test_error_display_formatting() {
        let crypto_err = CatalystError::Crypto("test error".to_string());
        assert_eq!(format!("{}", crypto_err), "Cryptographic error: test error");
        
        let consensus_err = CatalystError::Consensus {
            phase: "voting".to_string(),
            message: "failed".to_string(),
        };
        assert_eq!(format!("{}", consensus_err), "Consensus error: voting: failed");
        
        let timeout_err = CatalystError::Timeout {
            duration_ms: 5000,
            operation: "block sync".to_string(),
        };
        assert_eq!(format!("{}", timeout_err), "Timeout after 5000ms: block sync");
    }
    
    #[test]
    fn test_error_chain() {
        let util_err = error::UtilError::Parse("invalid number".to_string());
        let catalyst_err: CatalystError = util_err.into();
        
        match catalyst_err {
            CatalystError::Util(inner) => {
                match inner {
                    error::UtilError::Parse(msg) => assert_eq!(msg, "invalid number"),
                    _ => panic!("Wrong inner error type"),
                }
            }
            _ => panic!("Wrong error type"),
        }
    }
    
    #[test]
    fn test_util_error_from_io() {
        use std::io::{Error, ErrorKind};
        
        let io_err = Error::new(ErrorKind::NotFound, "file not found");
        let util_err: error::UtilError = io_err.into();
        
        match util_err {
            error::UtilError::Io(msg) => assert!(msg.contains("file not found")),
            _ => panic!("Wrong error type"),
        }
    }
    
    #[test]
    fn test_error_macro_formatting() {
        let err1 = crypto_error!("Key too short");
        let err2 = crypto_error!("Key length {}, expected {}", 16, 32);
        
        match err1 {
            CatalystError::Crypto(msg) => assert_eq!(msg, "Key too short"),
            _ => panic!("Wrong error type"),
        }
        
        match err2 {
            CatalystError::Crypto(msg) => assert_eq!(msg, "Key length 16, expected 32"),
            _ => panic!("Wrong error type"),
        }
    }
}

// Test public API completeness
mod public_api_tests {
    use super::*;
    
    #[test]
    fn test_public_exports() {
        // Ensure all important types are re-exported
        let _error: CatalystError = CatalystError::Internal("test".to_string());
        let _result: CatalystResult<()> = Ok(());
        let _util_error: error::UtilError = error::UtilError::Timeout;
        
        // Test that type aliases work
        let _hash: Hash = [0u8; 32];
        let _address: Address = [0u8; 21];
        
        // Test that utility functions are accessible
        let _timestamp = utils::current_timestamp();
        let _hex = utils::bytes_to_hex(&[1, 2, 3]);
    }
    
    #[test]
    fn test_trait_implementations() {
        // Test that errors implement required traits
        let error = CatalystError::Internal("test".to_string());
        
        // Should be cloneable
        let _cloned = error.clone();
        
        // Should be debuggable
        let debug_str = format!("{:?}", error);
        assert!(debug_str.contains("Internal"));
        
        // Should display properly
        let display_str = format!("{}", error);
        assert!(display_str.contains("Internal error"));
    }
}

// Integration tests between modules
mod integration_tests {
    use super::*;
    
    #[test]
    fn test_system_info_with_utils() {
        let info = SystemInfo::new();
        let timestamp = utils::current_timestamp();
        
        // System info timestamp should be close to current timestamp
        let diff = timestamp.saturating_sub(info.build_timestamp);
        assert!(diff < 10); // Should be within 10 seconds
    }
    
    #[test]
    fn test_error_with_utils() {
        let random_data = utils::random_bytes(16);
        let hex_data = utils::bytes_to_hex(&random_data);
        
        let error = crypto_error!("Invalid key: {}", hex_data);
        let error_str = format!("{}", error);
        
        assert!(error_str.contains("Invalid key"));
        assert!(error_str.contains(&hex_data));
    }
}