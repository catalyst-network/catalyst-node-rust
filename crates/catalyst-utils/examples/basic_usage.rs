// catalyst-utils/examples/basic_usage.rs

use catalyst_utils::{
    consensus_error,
    crypto::hash_data,
    logging::{CatalystLogger, LogCategory, LogConfig},
    time::current_timestamp,
    utils::RateLimiter,
    Address, CatalystResult, Hash, SystemInfo,
};

fn main() -> CatalystResult<()> {
    println!("Catalyst Utils Basic Usage Example");

    // System Information
    let system_info = SystemInfo::new();
    println!("\n=== System Information ===");
    println!("   Node ID: {}", system_info.node_id);
    println!("   Version: {}", system_info.version);
    println!("   Start time: {}", system_info.start_time);
    println!("   Uptime: {} seconds", system_info.uptime);

    // Hash and Address usage
    println!("\n=== Cryptographic Types ===");
    let hash: Hash = Hash::new([0u8; 32]);
    let address: Address = Address::new([1u8; 20]);

    println!("   Zero hash: {}", hash);
    println!("   Test address: {}", address);
    println!("   Hash is zero: {}", hash.is_zero());
    println!("   Address is zero: {}", address.is_zero());

    // Logging example
    println!("\n=== Logging Example ===");
    let config = LogConfig::default();
    let logger = CatalystLogger::new(config);

    logger.info(LogCategory::System, "Application started successfully")?;
    logger.warn(LogCategory::Network, "This is a warning message")?;

    // Error handling
    println!("\n=== Error Handling ===");
    let consensus_err = consensus_error!("Failed to reach supermajority");
    println!("   Consensus error: {}", consensus_err);

    // Rate limiting
    println!("\n=== Rate Limiting ===");
    let mut rate_limiter = RateLimiter::new(5, 1);

    println!(
        "   Initial state - can acquire token: {}",
        rate_limiter.try_acquire()
    );
    println!(
        "   After using 1 - can acquire another: {}",
        rate_limiter.try_acquire()
    );
    println!(
        "   After using 2 - can acquire another: {}",
        rate_limiter.try_acquire()
    );

    // Cryptographic operations
    println!("\n=== Cryptographic Operations ===");
    let data = b"Hello, Catalyst Network!";
    let hash = hash_data(data);
    println!("   Data: {:?}", String::from_utf8_lossy(data));
    println!("   SHA-256 hash: {}", hash);

    // Time utilities
    println!("\n=== Time Utilities ===");
    let timestamp = current_timestamp();
    println!("   Current timestamp: {}", timestamp);

    Ok(())
}
