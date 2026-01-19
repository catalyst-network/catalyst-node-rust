// examples/basic_usage.rs

use catalyst_utils::*;
use catalyst_utils::utils;

fn main() -> CatalystResult<()> {
    println!("=== Catalyst Utils Basic Usage Example ===\n");

    // 1. System Information
    println!("1. System Information:");
    let system_info = SystemInfo::new();
    println!("   Version: {}", system_info.version);
    println!("   Build time: {}", system_info.build_timestamp);
    println!("   Target: {}", system_info.target_triple);
    println!();

    // 2. Utility Functions
    println!("2. Utility Functions:");
    
    // Timestamps
    let timestamp_sec = utils::current_timestamp();
    let timestamp_ms = utils::current_timestamp_ms();
    println!("   Current timestamp (sec): {}", timestamp_sec);
    println!("   Current timestamp (ms): {}", timestamp_ms);
    
    // Random bytes
    let random_data = utils::random_bytes(32);
    println!("   Random 32 bytes: {}", utils::bytes_to_hex(&random_data));
    
    // Byte formatting
    let sizes = vec![0, 512, 1024, 1048576, 1073741824];
    for size in sizes {
        println!("   {} bytes = {}", size, utils::format_bytes(size));
    }
    println!();

    // 3. Error Handling
    println!("3. Error Handling:");
    
    // Create different types of errors
    let crypto_err = crypto_error!("Invalid key length: {}", 16);
    println!("   Crypto error: {}", crypto_err);
    
    let consensus_err = consensus_error!("voting", "Failed to reach supermajority");
    println!("   Consensus error: {}", consensus_err);
    
    let timeout_err = timeout_error!(5000, "block synchronization");
    println!("   Timeout error: {}", timeout_err);
    
    // Error conversion
    let util_err = error::UtilError::Parse("invalid transaction format".to_string());
    let catalyst_err: CatalystError = util_err.into();
    println!("   Converted error: {}", catalyst_err);
    println!();

    // 4. Rate Limiting
    println!("4. Rate Limiting:");
    let mut rate_limiter = utils::RateLimiter::new(10, 2); // 10 tokens, 2 per second
    
    println!("   Initial state - can acquire 5 tokens: {}", rate_limiter.try_acquire(5));
    println!("   After using 5 - can acquire 5 more: {}", rate_limiter.try_acquire(5));
    println!("   After using all - can acquire 1 more: {}", rate_limiter.try_acquire(1));
    println!();

    // 5. Type Aliases
    println!("5. Type Aliases:");
    let hash: Hash = [0u8; 32];
    let address: Address = [1u8; 21];
    let public_key: PublicKey = [2u8; 32];
    let signature: Signature = [3u8; 64];
    
    println!("   Hash length: {} bytes", hash.len());
    println!("   Address length: {} bytes", address.len());
    println!("   Public key length: {} bytes", public_key.len());
    println!("   Signature length: {} bytes", signature.len());
    println!();

    // 6. Hex Conversion
    println!("6. Hex Conversion:");
    let test_data = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let hex_string = utils::bytes_to_hex(&test_data);
    println!("   Bytes {:?} -> hex: {}", test_data, hex_string);
    
    let decoded = utils::hex_to_bytes(&hex_string)?;
    println!("   Hex {} -> bytes: {:?}", hex_string, decoded);
    println!();

    println!("=== Example completed successfully! ===");
    Ok(())
}

// Helper function to demonstrate error propagation
#[allow(dead_code)]
fn demonstrate_error_propagation() -> CatalystResult<String> {
    // Simulate some operation that might fail
    let data = utils::hex_to_bytes("invalid_hex")
        .map_err(|_| CatalystError::Invalid("Invalid hex data".to_string()))?;
    
    Ok(format!("Processed {} bytes", data.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_usage_examples() {
        // Test that our examples work correctly
        assert!(main().is_ok());
        
        // Test error propagation
        assert!(demonstrate_error_propagation().is_err());
    }
    
    #[test]
    fn test_demonstrate_error_propagation() {
        // This test uses the function, removing the dead code warning
        let result = demonstrate_error_propagation();
        assert!(result.is_err());
    }
}