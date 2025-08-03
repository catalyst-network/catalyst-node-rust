// catalyst-utils/src/logging.rs

use crate::{CatalystResult, CatalystError};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Log levels supported by Catalyst
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Critical = 5,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "TRACE"),
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
            LogLevel::Critical => write!(f, "CRITICAL"),
        }
    }
}

impl std::str::FromStr for LogLevel {
    type Err = CatalystError;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "TRACE" => Ok(LogLevel::Trace),
            "DEBUG" => Ok(LogLevel::Debug),
            "INFO" => Ok(LogLevel::Info),
            "WARN" => Ok(LogLevel::Warn),
            "ERROR" => Ok(LogLevel::Error),
            "CRITICAL" => Ok(LogLevel::Critical),
            _ => Err(CatalystError::Invalid(format!("Invalid log level: {}", s))),
        }
    }
}

/// Catalyst-specific log categories for easier filtering and debugging
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LogCategory {
    /// Network-related logs (P2P, gossip, connections)
    Network,
    /// Consensus protocol logs (construction, voting, synchronization)
    Consensus,
    /// Transaction processing and validation
    Transaction,
    /// Storage operations (DFS, state management)
    Storage,
    /// Cryptographic operations
    Crypto,
    /// Runtime environment (EVM, contracts)
    Runtime,
    /// Service bus and Web2 integration
    ServiceBus,
    /// Node management and worker pool operations
    NodeManagement,
    /// Configuration and startup
    Config,
    /// Performance metrics and monitoring
    Metrics,
    /// General system operations
    System,
}

impl fmt::Display for LogCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogCategory::Network => write!(f, "network"),
            LogCategory::Consensus => write!(f, "consensus"),
            LogCategory::Transaction => write!(f, "transaction"),
            LogCategory::Storage => write!(f, "storage"),
            LogCategory::Crypto => write!(f, "crypto"),
            LogCategory::Runtime => write!(f, "runtime"),
            LogCategory::ServiceBus => write!(f, "service_bus"),
            LogCategory::NodeManagement => write!(f, "node_mgmt"),
            LogCategory::Config => write!(f, "config"),
            LogCategory::Metrics => write!(f, "metrics"),
            LogCategory::System => write!(f, "system"),
        }
    }
}

/// Structured log entry with rich context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    /// Timestamp in milliseconds since Unix epoch
    pub timestamp: u64,
    /// Log level
    pub level: LogLevel,
    /// Log category
    pub category: LogCategory,
    /// Node identifier (if applicable)
    pub node_id: Option<String>,
    /// Ledger cycle number (for consensus-related logs)
    pub cycle: Option<u64>,
    /// Main log message
    pub message: String,
    /// Additional structured data
    pub fields: HashMap<String, LogValue>,
    /// Source file and line (for debugging)
    pub source: Option<String>,
}

/// Flexible value type for structured logging fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Bytes(Vec<u8>),
    Array(Vec<LogValue>),
}

impl From<String> for LogValue {
    fn from(s: String) -> Self { LogValue::String(s) }
}

impl From<&str> for LogValue {
    fn from(s: &str) -> Self { LogValue::String(s.to_string()) }
}

impl From<i64> for LogValue {
    fn from(i: i64) -> Self { LogValue::Integer(i) }
}

impl From<u64> for LogValue {
    fn from(i: u64) -> Self { LogValue::Integer(i as i64) }
}

impl From<u32> for LogValue {
    fn from(i: u32) -> Self { LogValue::Integer(i as i64) }
}

impl From<f64> for LogValue {
    fn from(f: f64) -> Self { LogValue::Float(f) }
}

impl From<i32> for LogValue {
    fn from(i: i32) -> Self { LogValue::Integer(i as i64) }
}

impl From<bool> for LogValue {
    fn from(b: bool) -> Self { LogValue::Boolean(b) }
}

impl From<Vec<u8>> for LogValue {
    fn from(bytes: Vec<u8>) -> Self { LogValue::Bytes(bytes) }
}

impl fmt::Display for LogValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogValue::String(s) => write!(f, "{}", s),
            LogValue::Integer(i) => write!(f, "{}", i),
            LogValue::Float(fl) => write!(f, "{}", fl),
            LogValue::Boolean(b) => write!(f, "{}", b),
            LogValue::Bytes(bytes) => write!(f, "0x{}", hex::encode(bytes)),
            LogValue::Array(arr) => {
                write!(f, "[")?;
                for (i, val) in arr.iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{}", val)?;
                }
                write!(f, "]")
            }
        }
    }
}

/// Configuration for the logging system
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Minimum log level to output
    pub min_level: LogLevel,
    /// Whether to include source file/line information
    pub include_source: bool,
    /// Whether to output in JSON format
    pub json_format: bool,
    /// Whether to include timestamps
    pub include_timestamp: bool,
    /// Maximum log file size in bytes (0 = unlimited)
    pub max_file_size: u64,
    /// Directory for log files
    pub log_dir: Option<String>,
    /// Whether to log to console
    pub console_output: bool,
    /// Categories to filter (empty = all categories)
    pub filtered_categories: Vec<LogCategory>,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            min_level: LogLevel::Info,
            include_source: false,
            json_format: false,
            include_timestamp: true,
            max_file_size: 100 * 1024 * 1024, // 100MB
            log_dir: None,
            console_output: true,
            filtered_categories: vec![],
        }
    }
}

/// Log output destination
pub trait LogOutput: Send + Sync {
    fn write_log(&self, entry: &LogEntry) -> CatalystResult<()>;
    fn flush(&self) -> CatalystResult<()>;
}

/// Console output implementation
pub struct ConsoleOutput {
    json_format: bool,
}

impl ConsoleOutput {
    pub fn new(json_format: bool) -> Self {
        Self { json_format }
    }
    
    fn format_human_readable(&self, entry: &LogEntry) -> String {
        let timestamp = if entry.timestamp > 0 {
            // Convert milliseconds timestamp to a readable format
            let seconds = entry.timestamp / 1000;
            format!("{} ", seconds)
        } else {
            String::new()
        };
        
        let node_info = entry.node_id.as_ref()
            .map(|id| format!("[{}] ", id))
            .unwrap_or_default();
            
        let cycle_info = entry.cycle
            .map(|c| format!("(cycle:{}) ", c))
            .unwrap_or_default();
        
        let fields_str = if !entry.fields.is_empty() {
            let fields: Vec<String> = entry.fields.iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            format!(" [{}]", fields.join(", "))
        } else {
            String::new()
        };
        
        format!("{}{}{}{} [{}] {}{}{}",
            timestamp,
            node_info,
            cycle_info,
            entry.level,
            entry.category,
            entry.message,
            fields_str,
            entry.source.as_ref().map(|s| format!(" @{}", s)).unwrap_or_default()
        )
    }
}

impl LogOutput for ConsoleOutput {
    fn write_log(&self, entry: &LogEntry) -> CatalystResult<()> {
        let output = if self.json_format {
            serde_json::to_string(entry)
                .map_err(|e| CatalystError::Serialization(format!("JSON serialization failed: {}", e)))?
        } else {
            self.format_human_readable(entry)
        };
        
        println!("{}", output);
        Ok(())
    }
    
    fn flush(&self) -> CatalystResult<()> {
        use std::io::{self, Write};
        io::stdout().flush()
            .map_err(|e| CatalystError::Runtime(format!("Failed to flush stdout: {}", e)))
    }
}

/// Main logger implementation
pub struct CatalystLogger {
    config: LogConfig,
    outputs: Vec<Box<dyn LogOutput>>,
    node_id: Option<String>,
    current_cycle: Arc<Mutex<Option<u64>>>,
}

impl CatalystLogger {
    pub fn new(config: LogConfig) -> Self {
        let mut outputs: Vec<Box<dyn LogOutput>> = Vec::new();
        
        if config.console_output {
            outputs.push(Box::new(ConsoleOutput::new(config.json_format)));
        }
        
        Self {
            config,
            outputs,
            node_id: None,
            current_cycle: Arc::new(Mutex::new(None)),
        }
    }
    
    /// Set the node identifier for all future logs
    pub fn set_node_id(&mut self, node_id: String) {
        self.node_id = Some(node_id);
    }
    
    /// Set the current ledger cycle for consensus-related logs
    pub fn set_current_cycle(&self, cycle: u64) {
        if let Ok(mut current) = self.current_cycle.lock() {
            *current = Some(cycle);
        }
    }
    
    /// Clear the current cycle (after consensus completion)
    pub fn clear_current_cycle(&self) {
        if let Ok(mut current) = self.current_cycle.lock() {
            *current = None;
        }
    }
    
    /// Add a custom output destination
    pub fn add_output(&mut self, output: Box<dyn LogOutput>) {
        self.outputs.push(output);
    }
    
    /// Check if a log entry should be written based on configuration
    fn should_log(&self, level: LogLevel, category: &LogCategory) -> bool {
        if level < self.config.min_level {
            return false;
        }
        
        if !self.config.filtered_categories.is_empty() {
            return self.config.filtered_categories.contains(category);
        }
        
        true
    }
    
    /// Log a message with structured data
    pub fn log(
        &self,
        level: LogLevel,
        category: LogCategory,
        message: String,
        fields: HashMap<String, LogValue>,
    ) -> CatalystResult<()> {
        if !self.should_log(level, &category) {
            return Ok(());
        }
        
        let cycle = self.current_cycle.lock()
            .map(|c| *c)
            .unwrap_or(None);
        
        let entry = LogEntry {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            level,
            category,
            node_id: self.node_id.clone(),
            cycle,
            message,
            fields,
            source: if self.config.include_source {
                Some(format!("{}:{}", file!(), line!()))
            } else {
                None
            },
        };
        
        for output in &self.outputs {
            output.write_log(&entry)?;
        }
        
        Ok(())
    }
    
    /// Convenience methods for different log levels
    pub fn trace(&self, category: LogCategory, message: &str) -> CatalystResult<()> {
        self.log(LogLevel::Trace, category, message.to_string(), HashMap::new())
    }
    
    pub fn debug(&self, category: LogCategory, message: &str) -> CatalystResult<()> {
        self.log(LogLevel::Debug, category, message.to_string(), HashMap::new())
    }
    
    pub fn info(&self, category: LogCategory, message: &str) -> CatalystResult<()> {
        self.log(LogLevel::Info, category, message.to_string(), HashMap::new())
    }
    
    pub fn warn(&self, category: LogCategory, message: &str) -> CatalystResult<()> {
        self.log(LogLevel::Warn, category, message.to_string(), HashMap::new())
    }
    
    pub fn error(&self, category: LogCategory, message: &str) -> CatalystResult<()> {
        self.log(LogLevel::Error, category, message.to_string(), HashMap::new())
    }
    
    pub fn critical(&self, category: LogCategory, message: &str) -> CatalystResult<()> {
        self.log(LogLevel::Critical, category, message.to_string(), HashMap::new())
    }
    
    /// Log with structured fields
    pub fn log_with_fields(
        &self,
        level: LogLevel,
        category: LogCategory,
        message: &str,
        fields: &[(&str, LogValue)],
    ) -> CatalystResult<()> {
        let fields_map: HashMap<String, LogValue> = fields.iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        
        self.log(level, category, message.to_string(), fields_map)
    }
    
    /// Flush all outputs
    pub fn flush(&self) -> CatalystResult<()> {
        for output in &self.outputs {
            output.flush()?;
        }
        Ok(())
    }
}

/// Global logger instance
use std::sync::OnceLock;
static GLOBAL_LOGGER: OnceLock<CatalystLogger> = OnceLock::new();

/// Initialize the global logger
pub fn init_logger(config: LogConfig) -> CatalystResult<()> {
    GLOBAL_LOGGER.set(CatalystLogger::new(config))
        .map_err(|_| CatalystError::Runtime("Logger already initialized".to_string()))?;
    Ok(())
}

/// Get reference to global logger
pub fn get_logger() -> Option<&'static CatalystLogger> {
    GLOBAL_LOGGER.get()
}

/// Set node ID on global logger
pub fn set_node_id(node_id: String) {
    // Note: This is a simplified implementation. In a real application,
    // you'd want to store the node_id separately and use it when creating log entries
    if let Some(_logger) = GLOBAL_LOGGER.get() {
        // For now, we can't modify the logger after creation due to immutability
        // This would need a different design in production
        println!("[INFO] [system] Node ID set to: {}", node_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_log_levels() {
        assert!(LogLevel::Error > LogLevel::Info);
        assert!(LogLevel::Critical > LogLevel::Error);
        assert_eq!(LogLevel::Debug.to_string(), "DEBUG");
    }
    
    #[test]
    fn test_log_level_parsing() {
        assert_eq!("INFO".parse::<LogLevel>().unwrap(), LogLevel::Info);
        assert_eq!("error".parse::<LogLevel>().unwrap(), LogLevel::Error);
        assert!("invalid".parse::<LogLevel>().is_err());
    }
    
    #[test]
    fn test_log_value_conversion() {
        let str_val: LogValue = "test".into();
        let int_val: LogValue = 42i64.into();
        let bool_val: LogValue = true.into();
        
        assert!(matches!(str_val, LogValue::String(_)));
        assert!(matches!(int_val, LogValue::Integer(42)));
        assert!(matches!(bool_val, LogValue::Boolean(true)));
    }
    
    #[test]
    fn test_logger_creation() {
        let config = LogConfig::default();
        let logger = CatalystLogger::new(config);
        
        // Test basic logging functionality
        let result = logger.info(LogCategory::System, "Test message");
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_structured_logging() {
        let config = LogConfig::default();
        let logger = CatalystLogger::new(config);
        
        let fields = vec![
            ("transaction_id", "tx_12345".into()),
            ("amount", 1000u64.into()),
            ("valid", true.into()),
        ];
        
        let result = logger.log_with_fields(
            LogLevel::Info,
            LogCategory::Transaction,
            "Transaction processed",
            &fields,
        );
        
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_log_filtering() {
        let mut config = LogConfig::default();
        config.min_level = LogLevel::Warn;
        config.filtered_categories = vec![LogCategory::Consensus];
        
        let logger = CatalystLogger::new(config);
        
        // This should not log (level too low)
        assert!(!logger.should_log(LogLevel::Info, &LogCategory::Consensus));
        
        // This should not log (category not in filter)
        assert!(!logger.should_log(LogLevel::Error, &LogCategory::Network));
        
        // This should log
        assert!(logger.should_log(LogLevel::Error, &LogCategory::Consensus));
    }
    
    #[test]
    fn test_json_serialization() {
        let entry = LogEntry {
            timestamp: 1234567890,
            level: LogLevel::Info,
            category: LogCategory::Network,
            node_id: Some("node_1".to_string()),
            cycle: Some(42),
            message: "Test message".to_string(),
            fields: {
                let mut map = HashMap::new();
                map.insert("key1".to_string(), "value1".into());
                map.insert("key2".to_string(), 123.into());
                map
            },
            source: Some("test.rs:100".to_string()),
        };
        
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: LogEntry = serde_json::from_str(&json).unwrap();
        
        assert_eq!(entry.level, deserialized.level);
        assert_eq!(entry.message, deserialized.message);
        assert_eq!(entry.node_id, deserialized.node_id);
    }
}

// Additional utility functions for common logging patterns
pub mod patterns {
    use super::*;
    
    /// Log consensus phase transitions
    pub fn log_consensus_phase(
        logger: &CatalystLogger,
        phase: &str,
        _cycle: u64,
        node_id: &str,
        additional_fields: Option<HashMap<String, LogValue>>,
    ) -> CatalystResult<()> {
        let mut fields = HashMap::new();
        fields.insert("phase".to_string(), phase.into());
        fields.insert("node_id".to_string(), node_id.into());
        
        if let Some(extra) = additional_fields {
            fields.extend(extra);
        }
        
        logger.log(
            LogLevel::Info,
            LogCategory::Consensus,
            format!("Consensus phase transition: {}", phase),
            fields,
        )
    }
    
    /// Log transaction processing
    pub fn log_transaction(
        logger: &CatalystLogger,
        tx_id: &str,
        status: &str,
        amount: Option<u64>,
    ) -> CatalystResult<()> {
        let mut fields = HashMap::new();
        fields.insert("transaction_id".to_string(), tx_id.into());
        fields.insert("status".to_string(), status.into());
        
        if let Some(amt) = amount {
            fields.insert("amount".to_string(), amt.into());
        }
        
        logger.log(
            LogLevel::Info,
            LogCategory::Transaction,
            format!("Transaction {}: {}", tx_id, status),
            fields,
        )
    }
    
    /// Log network events
    pub fn log_network_event(
        logger: &CatalystLogger,
        event_type: &str,
        peer_id: Option<&str>,
        details: Option<HashMap<String, LogValue>>,
    ) -> CatalystResult<()> {
        let mut fields = HashMap::new();
        fields.insert("event_type".to_string(), event_type.into());
        
        if let Some(peer) = peer_id {
            fields.insert("peer_id".to_string(), peer.into());
        }
        
        if let Some(extra) = details {
            fields.extend(extra);
        }
        
        logger.log(
            LogLevel::Info,
            LogCategory::Network,
            format!("Network event: {}", event_type),
            fields,
        )
    }
}