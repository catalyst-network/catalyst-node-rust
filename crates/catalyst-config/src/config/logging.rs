use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::error::{ConfigError, ConfigResult};

/// Logging configuration for the Catalyst Network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Global logging level
    pub level: LogLevel,
    
    /// Logging format
    pub format: LogFormat,
    
    /// Output destinations
    pub outputs: Vec<LogOutput>,
    
    /// Category-specific logging levels
    pub category_levels: Vec<CategoryLevel>,
    
    /// Structured logging configuration
    pub structured: StructuredLoggingConfig,
    
    /// Log rotation configuration
    pub rotation: LogRotationConfig,
    
    /// Performance settings
    pub performance: LogPerformanceConfig,
    
    /// Filtering configuration
    pub filtering: LogFilteringConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Critical,
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    /// Human-readable text format
    Text,
    /// JSON structured format
    Json,
    /// Compact format for production
    Compact,
    /// Custom format string
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogOutput {
    /// Output type
    pub output_type: OutputType,
    
    /// Minimum level for this output
    pub min_level: LogLevel,
    
    /// Format override for this output
    pub format_override: Option<LogFormat>,
    
    /// Buffer size for this output
    pub buffer_size: Option<usize>,
    
    /// Flush interval in milliseconds
    pub flush_interval_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutputType {
    /// Console output (stdout/stderr)
    Console { use_stderr: bool },
    /// File output
    File { path: PathBuf, append: bool },
    /// Syslog output
    Syslog { facility: String, hostname: Option<String> },
    /// Network logging (TCP/UDP)
    Network { endpoint: String, protocol: NetworkProtocol },
    /// Distributed logging system
    Distributed { system: DistributedLoggingSystem },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkProtocol {
    Tcp,
    Udp,
    Http,
    Https,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DistributedLoggingSystem {
    /// Elasticsearch/ELK stack
    Elasticsearch { endpoint: String, index: String },
    /// Fluentd
    Fluentd { endpoint: String },
    /// Custom system
    Custom { endpoint: String, config: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryLevel {
    /// Log category name
    pub category: String,
    
    /// Specific level for this category
    pub level: LogLevel,
    
    /// Enable/disable this category
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredLoggingConfig {
    /// Enable structured logging
    pub enabled: bool,
    
    /// Default fields to include in all log entries
    pub default_fields: Vec<DefaultField>,
    
    /// Node identification in logs
    pub node_identification: NodeIdentificationConfig,
    
    /// Context tracking
    pub context_tracking: ContextTrackingConfig,
    
    /// Sensitive data handling
    pub sensitive_data: SensitiveDataConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultField {
    /// Field name
    pub name: String,
    
    /// Field value source
    pub source: FieldSource,
    
    /// Whether field is required
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldSource {
    /// Static value
    Static(String),
    /// Environment variable
    Environment(String),
    /// System property
    System(SystemProperty),
    /// Node property
    Node(NodeProperty),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemProperty {
    Timestamp,
    Hostname,
    ProcessId,
    ThreadId,
    MemoryUsage,
    CpuUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeProperty {
    NodeId,
    NetworkName,
    NodeType,
    Version,
    Uptime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIdentificationConfig {
    /// Include node ID in logs
    pub include_node_id: bool,
    
    /// Include network name
    pub include_network_name: bool,
    
    /// Include node type
    pub include_node_type: bool,
    
    /// Include version information
    pub include_version: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextTrackingConfig {
    /// Enable context tracking
    pub enabled: bool,
    
    /// Track consensus cycles
    pub track_consensus_cycles: bool,
    
    /// Track transaction flows
    pub track_transaction_flows: bool,
    
    /// Track network events
    pub track_network_events: bool,
    
    /// Maximum context depth
    pub max_context_depth: usize,
    
    /// Context retention time in seconds
    pub context_retention_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitiveDataConfig {
    /// Enable sensitive data filtering
    pub filtering_enabled: bool,
    
    /// Fields to redact
    pub redacted_fields: Vec<String>,
    
    /// Patterns to redact (regex)
    pub redacted_patterns: Vec<String>,
    
    /// Redaction replacement text
    pub redaction_text: String,
    
    /// Enable IP address anonymization
    pub anonymize_ip_addresses: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogRotationConfig {
    /// Enable log rotation
    pub enabled: bool,
    
    /// Rotation strategy
    pub strategy: RotationStrategy,
    
    /// Maximum file size before rotation (bytes)
    pub max_file_size_bytes: u64,
    
    /// Maximum number of archived files
    pub max_archived_files: usize,
    
    /// Compression for archived files
    pub compress_archived: bool,
    
    /// Rotation schedule
    pub schedule: Option<RotationSchedule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationStrategy {
    /// Rotate by file size
    Size,
    /// Rotate by time
    Time,
    /// Rotate by both size and time
    SizeAndTime,
    /// Never rotate
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationSchedule {
    /// Rotation frequency
    pub frequency: RotationFrequency,
    
    /// Specific time for daily rotation (hour)
    pub daily_hour: Option<u8>,
    
    /// Specific day for weekly rotation (0-6, Sunday=0)
    pub weekly_day: Option<u8>,
    
    /// Specific day for monthly rotation (1-31)
    pub monthly_day: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RotationFrequency {
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Custom(String), // Cron expression
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogPerformanceConfig {
    /// Enable asynchronous logging
    pub async_logging: bool,
    
    /// Worker thread count for async logging
    pub async_worker_threads: usize,
    
    /// Queue size for async logging
    pub async_queue_size: usize,
    
    /// Batching configuration
    pub batching: BatchingConfig,
    
    /// Buffer configuration
    pub buffering: BufferingConfig,
    
    /// Sampling configuration for high-volume logs
    pub sampling: SamplingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchingConfig {
    /// Enable log batching
    pub enabled: bool,
    
    /// Maximum batch size
    pub max_batch_size: usize,
    
    /// Maximum batch wait time in milliseconds
    pub max_wait_time_ms: u64,
    
    /// Batch compression
    pub compression_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferingConfig {
    /// Enable output buffering
    pub enabled: bool,
    
    /// Buffer size in bytes
    pub buffer_size_bytes: usize,
    
    /// Flush interval in milliseconds
    pub flush_interval_ms: u64,
    
    /// Force flush on critical messages
    pub flush_on_critical: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingConfig {
    /// Enable log sampling
    pub enabled: bool,
    
    /// Sampling rules by category
    pub rules: Vec<SamplingRule>,
    
    /// Default sampling rate (0.0 to 1.0)
    pub default_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingRule {
    /// Category pattern (supports wildcards)
    pub category_pattern: String,
    
    /// Log level
    pub level: LogLevel,
    
    /// Sampling rate (0.0 to 1.0)
    pub rate: f64,
    
    /// Burst allowance
    pub burst_allowance: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogFilteringConfig {
    /// Enable log filtering
    pub enabled: bool,
    
    /// Filtering rules
    pub rules: Vec<FilteringRule>,
    
    /// Default action for unmatched logs
    pub default_action: FilterAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilteringRule {
    /// Rule name
    pub name: String,
    
    /// Category filter (supports wildcards)
    pub category_filter: Option<String>,
    
    /// Level filter
    pub level_filter: Option<LogLevel>,
    
    /// Message content filter (regex)
    pub message_filter: Option<String>,
    
    /// Field filters
    pub field_filters: Vec<FieldFilter>,
    
    /// Action to take when rule matches
    pub action: FilterAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldFilter {
    /// Field name
    pub field_name: String,
    
    /// Field value pattern (regex)
    pub value_pattern: String,
    
    /// Whether pattern should match or not match
    pub should_match: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterAction {
    /// Allow the log entry
    Allow,
    /// Deny the log entry
    Deny,
    /// Modify the log entry
    Modify(Vec<FieldModification>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldModification {
    /// Field name to modify
    pub field_name: String,
    
    /// Modification type
    pub modification_type: ModificationType,
    
    /// New value (for Replace and Add)
    pub new_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModificationType {
    /// Replace field value
    Replace,
    /// Add new field
    Add,
    /// Remove field
    Remove,
    /// Redact field value
    Redact,
}

impl LoggingConfig {
    /// Create development logging configuration
    pub fn devnet() -> Self {
        Self {
            level: LogLevel::Debug,
            format: LogFormat::Text,
            outputs: vec![
                LogOutput {
                    output_type: OutputType::Console { use_stderr: false },
                    min_level: LogLevel::Debug,
                    format_override: None,
                    buffer_size: None,
                    flush_interval_ms: None,
                },
                LogOutput {
                    output_type: OutputType::File {
                        path: PathBuf::from("./logs/devnet/catalyst.log"),
                        append: true,
                    },
                    min_level: LogLevel::Info,
                    format_override: Some(LogFormat::Json),
                    buffer_size: Some(8192),
                    flush_interval_ms: Some(1000),
                },
            ],
            category_levels: vec![
                CategoryLevel {
                    category: "catalyst_consensus".to_string(),
                    level: LogLevel::Debug,
                    enabled: true,
                },
                CategoryLevel {
                    category: "catalyst_network".to_string(),
                    level: LogLevel::Info,
                    enabled: true,
                },
                CategoryLevel {
                    category: "catalyst_crypto".to_string(),
                    level: LogLevel::Warn,
                    enabled: true,
                },
            ],
            structured: StructuredLoggingConfig {
                enabled: true,
                default_fields: vec![
                    DefaultField {
                        name: "timestamp".to_string(),
                        source: FieldSource::System(SystemProperty::Timestamp),
                        required: true,
                    },
                    DefaultField {
                        name: "node_id".to_string(),
                        source: FieldSource::Node(NodeProperty::NodeId),
                        required: true,
                    },
                    DefaultField {
                        name: "network".to_string(),
                        source: FieldSource::Node(NodeProperty::NetworkName),
                        required: true,
                    },
                ],
                node_identification: NodeIdentificationConfig {
                    include_node_id: true,
                    include_network_name: true,
                    include_node_type: true,
                    include_version: true,
                },
                context_tracking: ContextTrackingConfig {
                    enabled: true,
                    track_consensus_cycles: true,
                    track_transaction_flows: true,
                    track_network_events: true,
                    max_context_depth: 10,
                    context_retention_seconds: 3600,
                },
                sensitive_data: SensitiveDataConfig {
                    filtering_enabled: false, // Relaxed for development
                    redacted_fields: vec![],
                    redacted_patterns: vec![],
                    redaction_text: "[REDACTED]".to_string(),
                    anonymize_ip_addresses: false,
                },
            },
            rotation: LogRotationConfig {
                enabled: true,
                strategy: RotationStrategy::Size,
                max_file_size_bytes: 100 * 1024 * 1024, // 100MB
                max_archived_files: 5,
                compress_archived: false,
                schedule: None,
            },
            performance: LogPerformanceConfig {
                async_logging: true,
                async_worker_threads: 2,
                async_queue_size: 10000,
                batching: BatchingConfig {
                    enabled: false, // Disabled for development simplicity
                    max_batch_size: 100,
                    max_wait_time_ms: 100,
                    compression_enabled: false,
                },
                buffering: BufferingConfig {
                    enabled: true,
                    buffer_size_bytes: 8192,
                    flush_interval_ms: 1000,
                    flush_on_critical: true,
                },
                sampling: SamplingConfig {
                    enabled: false, // Disabled for development
                    rules: vec![],
                    default_rate: 1.0,
                },
            },
            filtering: LogFilteringConfig {
                enabled: false, // Disabled for development
                rules: vec![],
                default_action: FilterAction::Allow,
            },
        }
    }
    
    /// Create test network logging configuration
    pub fn testnet() -> Self {
        let mut config = Self::devnet();
        
        // Override testnet-specific settings
        config.level = LogLevel::Info;
        config.outputs = vec![
            LogOutput {
                output_type: OutputType::Console { use_stderr: false },
                min_level: LogLevel::Info,
                format_override: Some(LogFormat::Compact),
                buffer_size: None,
                flush_interval_ms: None,
            },
            LogOutput {
                output_type: OutputType::File {
                    path: PathBuf::from("./logs/testnet/catalyst.log"),
                    append: true,
                },
                min_level: LogLevel::Info,
                format_override: Some(LogFormat::Json),
                buffer_size: Some(16384),
                flush_interval_ms: Some(5000),
            },
        ];
        
        config.structured.sensitive_data.filtering_enabled = true;
        config.structured.sensitive_data.redacted_fields = vec![
            "private_key".to_string(),
            "secret".to_string(),
            "password".to_string(),
        ];
        
        config.rotation.max_file_size_bytes = 500 * 1024 * 1024; // 500MB
        config.rotation.max_archived_files = 10;
        config.rotation.compress_archived = true;
        
        config.performance.batching.enabled = true;
        config.performance.sampling.enabled = true;
        config.performance.sampling.rules = vec![
            SamplingRule {
                category_pattern: "catalyst_network::gossip".to_string(),
                level: LogLevel::Debug,
                rate: 0.1, // Sample 10% of debug network gossip logs
                burst_allowance: 10,
            },
        ];
        
        config
    }
    
    /// Create main network logging configuration (production)
    pub fn mainnet() -> Self {
        let mut config = Self::testnet();
        
        // Override mainnet-specific settings
        config.level = LogLevel::Info;
        config.outputs = vec![
            LogOutput {
                output_type: OutputType::Console { use_stderr: true },
                min_level: LogLevel::Warn,
                format_override: Some(LogFormat::Compact),
                buffer_size: None,
                flush_interval_ms: None,
            },
            LogOutput {
                output_type: OutputType::File {
                    path: PathBuf::from("/var/log/catalyst/catalyst.log"),
                    append: true,
                },
                min_level: LogLevel::Info,
                format_override: Some(LogFormat::Json),
                buffer_size: Some(32768),
                flush_interval_ms: Some(10000),
            },
            LogOutput {
                output_type: OutputType::Syslog {
                    facility: "daemon".to_string(),
                    hostname: None,
                },
                min_level: LogLevel::Error,
                format_override: Some(LogFormat::Compact),
                buffer_size: None,
                flush_interval_ms: None,
            },
        ];
        
        config.structured.sensitive_data.redacted_patterns = vec![
            r"sk[a-fA-F0-9]{64}".to_string(), // Private keys
            r"\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}\b".to_string(), // IP addresses
        ];
        config.structured.sensitive_data.anonymize_ip_addresses = true;
        
        config.rotation.strategy = RotationStrategy::SizeAndTime;
        config.rotation.max_file_size_bytes = 1024 * 1024 * 1024; // 1GB
        config.rotation.max_archived_files = 30;
        config.rotation.schedule = Some(RotationSchedule {
            frequency: RotationFrequency::Daily,
            daily_hour: Some(2), // 2 AM
            weekly_day: None,
            monthly_day: None,
        });
        
        config.performance.async_worker_threads = 4;
        config.performance.async_queue_size = 50000;
        config.performance.batching.enabled = true;
        config.performance.batching.max_batch_size = 1000;
        config.performance.batching.compression_enabled = true;
        
        config.performance.sampling.rules = vec![
            SamplingRule {
                category_pattern: "catalyst_network::gossip".to_string(),
                level: LogLevel::Debug,
                rate: 0.01, // Sample 1% of debug network gossip logs
                burst_allowance: 5,
            },
            SamplingRule {
                category_pattern: "catalyst_consensus::*".to_string(),
                level: LogLevel::Trace,
                rate: 0.05, // Sample 5% of trace consensus logs
                burst_allowance: 10,
            },
        ];
        
        config.filtering.enabled = true;
        config.filtering.rules = vec![
            FilteringRule {
                name: "security_filter".to_string(),
                category_filter: None,
                level_filter: None,
                message_filter: Some(r"(private_key|secret|password)".to_string()),
                field_filters: vec![],
                action: FilterAction::Modify(vec![
                    FieldModification {
                        field_name: "message".to_string(),
                        modification_type: ModificationType::Redact,
                        new_value: None,
                    },
                ]),
            },
        ];
        
        config
    }
    
    /// Validate logging configuration
    pub fn validate(&self) -> ConfigResult<()> {
        // Validate that at least one output is configured
        if self.outputs.is_empty() {
            return Err(ConfigError::ValidationFailed("At least one log output must be configured".to_string()));
        }
        
        // Validate file output paths
        for output in &self.outputs {
            if let OutputType::File { path, .. } = &output.output_type {
                if path.as_os_str().is_empty() {
                    return Err(ConfigError::ValidationFailed("Log file path cannot be empty".to_string()));
                }
            }
        }
        
        // Validate rotation settings
        if self.rotation.enabled {
            if self.rotation.max_file_size_bytes == 0 {
                return Err(ConfigError::ValidationFailed("Max file size for rotation must be greater than 0".to_string()));
            }
            
            if self.rotation.max_archived_files == 0 {
                return Err(ConfigError::ValidationFailed("Max archived files must be greater than 0".to_string()));
            }
        }
        
        // Validate performance settings
        if self.performance.async_logging {
            if self.performance.async_worker_threads == 0 {
                return Err(ConfigError::ValidationFailed("Async worker threads must be greater than 0".to_string()));
            }
            
            if self.performance.async_queue_size == 0 {
                return Err(ConfigError::ValidationFailed("Async queue size must be greater than 0".to_string()));
            }
        }
        
        // Validate sampling rates
        if self.performance.sampling.enabled {
            if self.performance.sampling.default_rate < 0.0 || self.performance.sampling.default_rate > 1.0 {
                return Err(ConfigError::ValidationFailed("Default sampling rate must be between 0.0 and 1.0".to_string()));
            }
            
            for rule in &self.performance.sampling.rules {
                if rule.rate < 0.0 || rule.rate > 1.0 {
                    return Err(ConfigError::ValidationFailed("Sampling rate must be between 0.0 and 1.0".to_string()));
                }
            }
        }
        
        // Validate context tracking settings
        if self.structured.context_tracking.enabled {
            if self.structured.context_tracking.max_context_depth == 0 {
                return Err(ConfigError::ValidationFailed("Max context depth must be greater than 0".to_string()));
            }
        }
        
        Ok(())
    }
}