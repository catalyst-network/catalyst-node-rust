// catalyst-utils/examples/logging_example.rs

use catalyst_utils::{
    CatalystResult,
    logging::{
        LogConfig, CatalystLogger, LogCategory, LogLevel, LogValue,
        init_logger, get_logger,
    },
    patterns,
};
use std::collections::HashMap;

fn main() -> CatalystResult<()> {
    println!("Catalyst Logging System Example");
    
    // Initialize the global logger
    let config = LogConfig {
        min_level: LogLevel::Debug,
        include_source: true,
        json_format: false,
        include_timestamp: true,
        max_file_size: 100 * 1024 * 1024, // 100MB
        log_dir: None,
        console_output: true,
        filtered_categories: vec![], // Log all categories
    };
    
    init_logger(config)?;
    
    // Basic logging examples
    println!("\n=== Basic Logging ===");
    let logger = CatalystLogger::new(LogConfig::default());
    
    logger.trace(LogCategory::System, "This is a trace message")?;
    logger.debug(LogCategory::System, "This is a debug message")?;
    logger.info(LogCategory::System, "Application started successfully")?;
    logger.warn(LogCategory::Network, "Connection timeout detected")?;
    logger.error(LogCategory::Consensus, "Failed to reach consensus")?;
    logger.critical(LogCategory::System, "Critical system error")?;
    
    // Structured logging with fields
    println!("\n=== Structured Logging ===");
    let fields = vec![
        ("node_id", LogValue::String("node_12345".to_string())),
        ("block_height", LogValue::Integer(100)),
        ("processing_time_ms", LogValue::Float(45.7)),
        ("success", LogValue::Boolean(true)),
    ];
    
    logger.log_with_fields(
        LogLevel::Info,
        LogCategory::Transaction,
        "Transaction processed successfully",
        &fields,
    )?;
    
    // Logging different categories
    println!("\n=== Category-Specific Logging ===");
    logger.info(LogCategory::Network, "New peer connected: 192.168.1.100")?;
    logger.info(LogCategory::Consensus, "Started new consensus round")?;
    logger.info(LogCategory::Transaction, "Added transaction to mempool")?;
    logger.info(LogCategory::Storage, "Syncing state to disk")?;
    logger.info(LogCategory::Crypto, "Generated new keypair")?;
    logger.info(LogCategory::Runtime, "Contract executed successfully")?;
    logger.info(LogCategory::ServiceBus, "Event published to subscribers")?;
    logger.info(LogCategory::NodeManagement, "Worker selected for task")?;
    logger.info(LogCategory::Config, "Configuration reloaded")?;
    logger.info(LogCategory::Metrics, "Performance metrics collected")?;
    
    // Using logging patterns
    println!("\n=== Logging Patterns ===");
    patterns::log_consensus_phase(
        &logger,
        "preparation",
        42,
        "node_12345",
        Some({
            let mut fields = HashMap::new();
            fields.insert("validators".to_string(), LogValue::Integer(5));
            fields.insert("round_time_ms".to_string(), LogValue::Float(1250.0));
            fields
        }),
    )?;
    
    patterns::log_transaction(
        &logger,
        "tx_abc123",
        "confirmed",
        Some(1000),
    )?;
    
    patterns::log_network_event(
        &logger,
        "peer_connected",
        Some("peer_xyz789"),
        Some({
            let mut details = HashMap::new();
            details.insert("ip_address".to_string(), LogValue::String("192.168.1.100".to_string()));
            details.insert("port".to_string(), LogValue::Integer(8080));
            details
        }),
    )?;
    
    // JSON format logging
    println!("\n=== JSON Format Logging ===");
    let json_config = LogConfig {
        json_format: true,
        ..LogConfig::default()
    };
    let json_logger = CatalystLogger::new(json_config);
    
    json_logger.info(LogCategory::System, "This message will be in JSON format")?;
    
    // Advanced logging with custom fields
    println!("\n=== Advanced Structured Logging ===");
    let advanced_fields = vec![
        ("transaction_id", LogValue::String("tx_def456".to_string())),
        ("from_address", LogValue::String("0x1234...5678".to_string())),
        ("to_address", LogValue::String("0xabcd...efgh".to_string())),
        ("amount", LogValue::Integer(500)),
        ("gas_used", LogValue::Integer(21000)),
        ("gas_price", LogValue::Float(20.5)),
        ("block_number", LogValue::Integer(12345)),
        ("confirmations", LogValue::Integer(6)),
        ("valid", LogValue::Boolean(true)),
        ("signature", LogValue::Bytes(vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xab])),
    ];
    
    logger.log_with_fields(
        LogLevel::Info,
        LogCategory::Transaction,
        "Transaction details",
        &advanced_fields,
    )?;
    
    // Filtered logging
    println!("\n=== Filtered Logging ===");
    let filtered_config = LogConfig {
        min_level: LogLevel::Warn,
        filtered_categories: vec![LogCategory::Network, LogCategory::Consensus],
        ..LogConfig::default()
    };
    let filtered_logger = CatalystLogger::new(filtered_config);
    
    // These won't be logged due to level filter
    filtered_logger.info(LogCategory::Network, "This info message won't appear")?;
    filtered_logger.debug(LogCategory::Consensus, "This debug message won't appear")?;
    
    // This will be logged (correct level and category)
    filtered_logger.error(LogCategory::Network, "This error message will appear")?;
    
    // This won't be logged (wrong category)
    filtered_logger.error(LogCategory::System, "This error won't appear (wrong category)")?;
    
    // Performance logging
    println!("\n=== Performance Logging ===");
    let start_time = std::time::Instant::now();
    
    // Simulate some work
    std::thread::sleep(std::time::Duration::from_millis(10));
    
    let elapsed = start_time.elapsed();
    let perf_fields = vec![
        ("operation", LogValue::String("block_validation".to_string())),
        ("duration_ms", LogValue::Float(elapsed.as_millis() as f64)),
        ("items_processed", LogValue::Integer(150)),
        ("throughput_per_sec", LogValue::Float(150.0 / elapsed.as_secs_f64())),
    ];
    
    logger.log_with_fields(
        LogLevel::Info,
        LogCategory::Metrics,
        "Performance measurement",
        &perf_fields,
    )?;
    
    // Error logging with context
    println!("\n=== Error Logging with Context ===");
    let error_fields = vec![
        ("error_code", LogValue::String("CONSENSUS_001".to_string())),
        ("error_message", LogValue::String("Timeout waiting for votes".to_string())),
        ("timeout_ms", LogValue::Integer(5000)),
        ("votes_received", LogValue::Integer(3)),
        ("votes_required", LogValue::Integer(5)),
        ("retry_attempt", LogValue::Integer(2)),
        ("max_retries", LogValue::Integer(3)),
    ];
    
    logger.log_with_fields(
        LogLevel::Error,
        LogCategory::Consensus,
        "Consensus timeout occurred",
        &error_fields,
    )?;
    
    // Global logger usage
    println!("\n=== Global Logger Usage ===");
    if let Some(global_logger) = get_logger() {
        global_logger.info(LogCategory::System, "Using global logger instance")?;
    }
    
    println!("\n=== All Logging Examples Completed ===");
    Ok(())
}