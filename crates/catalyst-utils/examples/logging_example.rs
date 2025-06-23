// examples/logging_example.rs

use catalyst_utils::*;
use catalyst_utils::logging::*;
use std::collections::HashMap;

fn main() -> CatalystResult<()> {
    println!("=== Catalyst Logging System Example ===\n");

    // 1. Initialize the logging system
    println!("1. Initializing logging system...");
    let config = LogConfig {
        min_level: LogLevel::Debug,
        include_source: true,
        json_format: false, // Human readable for this example
        include_timestamp: true,
        console_output: true,
        filtered_categories: vec![], // Log all categories
        ..Default::default()
    };
    
    init_logger(config)?;
    set_node_id("example_node_1".to_string());
    println!("   ✓ Logging initialized\n");

    // 2. Basic logging with different levels
    println!("2. Basic logging examples:");
    
    let logger = create_example_logger();
    
    logger.trace(LogCategory::System, "This is a trace message")?;
    logger.debug(LogCategory::System, "This is a debug message")?;
    logger.info(LogCategory::System, "This is an info message")?;
    logger.warn(LogCategory::System, "This is a warning message")?;
    logger.error(LogCategory::System, "This is an error message")?;
    logger.critical(LogCategory::System, "This is a critical message")?;
    println!();

    // 3. Structured logging with fields
    println!("3. Structured logging with fields:");
    
    let fields = vec![
        ("transaction_id", LogValue::String("tx_12345".to_string())),
        ("amount", LogValue::Integer(1000)),
        ("fees", LogValue::Float(0.001)),
        ("valid", LogValue::Boolean(true)),
        ("signature", LogValue::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF])),
    ];
    
    logger.log_with_fields(
        LogLevel::Info,
        LogCategory::Transaction,
        "Transaction processed successfully",
        &fields,
    )?;
    println!();

    // 4. Category-specific logging
    println!("4. Category-specific logging:");
    
    // Network events
    logger.info(LogCategory::Network, "Peer connected: peer_abc123")?;
    logger.warn(LogCategory::Network, "Connection timeout to peer_xyz789")?;
    
    // Consensus events
    logger.info(LogCategory::Consensus, "Starting construction phase for cycle 42")?;
    logger.info(LogCategory::Consensus, "Voting phase completed successfully")?;
    
    // Storage events
    logger.debug(LogCategory::Storage, "Writing state update to disk")?;
    logger.info(LogCategory::Storage, "State snapshot created")?;
    
    // Crypto events
    logger.debug(LogCategory::Crypto, "Verifying aggregated signature")?;
    logger.warn(LogCategory::Crypto, "Signature verification took longer than expected")?;
    println!();

    // 5. Advanced patterns for Catalyst Network
    println!("5. Catalyst-specific logging patterns:");
    
    // Simulate consensus cycle tracking
    logger.set_current_cycle(42);
    
    // Log consensus phases
    patterns::log_consensus_phase(
        &logger,
        "Construction",
        42,
        "producer_node_1",
        Some({
            let mut fields = HashMap::new();
            fields.insert("transactions_count".to_string(), LogValue::Integer(150));
            fields.insert("mempool_size".to_string(), LogValue::Integer(300));
            fields
        }),
    )?;
    
    // Log transaction processing
    patterns::log_transaction(
        &logger,
        "tx_67890",
        "validated",
        Some(2500),
    )?;
    
    // Log network events
    patterns::log_network_event(
        &logger,
        "peer_connected",
        Some("peer_def456"),
        Some({
            let mut details = HashMap::new();
            details.insert("peer_type".to_string(), LogValue::String("producer".to_string()));
            details.insert("latency_ms".to_string(), LogValue::Float(15.5));
            details
        }),
    )?;
    
    // Clear cycle when consensus completes
    logger.clear_current_cycle();
    println!();

    // 6. Error logging integration
    println!("6. Error logging integration:");
    
    let error = crypto_error!("Invalid signature for transaction tx_99999");
    logger.log_with_fields(
        LogLevel::Error,
        LogCategory::Crypto,
        &format!("Cryptographic error occurred: {}", error),
        &[
            ("error_type", LogValue::String("signature_verification".to_string())),
            ("transaction_id", LogValue::String("tx_99999".to_string())),
            ("error_code", LogValue::Integer(4001)),
        ],
    )?;
    println!();

    // 7. Performance-sensitive logging
    println!("7. Performance-sensitive logging:");
    
    // Use macros for zero-cost logging when level is filtered out
    log_trace!(LogCategory::System, "Detailed trace information: {}", "some_data");
    log_debug!(LogCategory::Network, "Message routing: {} -> {}", "node1", "node2");
    log_info!(LogCategory::Consensus, "Phase transition: {} -> {}", "voting", "sync");
    
    // Structured logging with macros
    log_with_fields!(
        LogLevel::Info,
        LogCategory::Transaction,
        "High-frequency transaction processed",
        "tx_id" => LogValue::String("tx_rapid_001".to_string()),
        "processing_time_ms" => LogValue::Float(0.125),
        "gas_used" => LogValue::Integer(21000)
    );
    println!();

    // 8. JSON format example
    println!("8. JSON format example:");
    println!("   (Reinitializing with JSON format...)");
    
    let json_config = LogConfig {
        min_level: LogLevel::Info,
        json_format: true,
        console_output: true,
        ..Default::default()
    };
    
    // Note: In a real application, you'd typically not reinitialize the logger
    // This is just for demonstration
    let mut json_logger = CatalystLogger::new(json_config);
    json_logger.set_node_id("json_example_node".to_string());
    
    json_logger.log_with_fields(
        LogLevel::Info,
        LogCategory::System,
        "JSON formatted log entry",
        &[
            ("example", LogValue::Boolean(true)),
            ("format", LogValue::String("json".to_string())),
            ("structured", LogValue::Boolean(true)),
        ],
    )?;
    println!();

    println!("=== Logging example completed successfully! ===");
    Ok(())
}

// Helper function to create a logger for examples
fn create_example_logger() -> CatalystLogger {
    let config = LogConfig {
        min_level: LogLevel::Trace,
        include_source: false, // Keep output clean for examples
        json_format: false,
        console_output: true,
        ..Default::default()
    };
    
    let mut logger = CatalystLogger::new(config);
    logger.set_node_id("example_node".to_string());
    logger
}

// Example of custom log output implementation
struct FileLogOutput {
    file_path: String,
}

impl FileLogOutput {
    pub fn new(file_path: String) -> Self {
        Self { file_path }
    }
}

impl LogOutput for FileLogOutput {
    fn write_log(&self, entry: &LogEntry) -> CatalystResult<()> {
        // In a real implementation, you'd write to a file
        println!("[FILE LOG] {}: {}", entry.level, entry.message);
        Ok(())
    }
    
    fn flush(&self) -> CatalystResult<()> {
        // In a real implementation, you'd flush the file buffer
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_examples() {
        // Test that our logging examples work
        assert!(main().is_ok());
    }
    
    #[test]
    fn test_log_levels() {
        assert!(LogLevel::Error > LogLevel::Info);
        assert!(LogLevel::Critical > LogLevel::Error);
    }
    
    #[test]
    fn test_log_value_conversions() {
        let string_val: LogValue = "test".into();
        let int_val: LogValue = 42i64.into();
        let bool_val: LogValue = true.into();
        
        assert!(matches!(string_val, LogValue::String(_)));
        assert!(matches!(int_val, LogValue::Integer(42)));
        assert!(matches!(bool_val, LogValue::Boolean(true)));
    }
}