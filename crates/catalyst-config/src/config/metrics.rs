use crate::error::{ConfigError, ConfigResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Metrics collection and monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,

    /// Metrics server configuration
    pub server: MetricsServerConfig,

    /// Collection configuration
    pub collection: MetricsCollectionConfig,

    /// Storage configuration
    pub storage: MetricsStorageConfig,

    /// Export configuration
    pub export: MetricsExportConfig,

    /// Alerting configuration
    pub alerting: AlertingConfig,

    /// Performance monitoring
    pub performance: MetricsPerformanceConfig,

    /// Custom metrics configuration
    pub custom_metrics: CustomMetricsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsServerConfig {
    /// Metrics server port (typically for Prometheus scraping)
    pub port: u16,

    /// Bind address
    pub bind_address: String,

    /// Metrics endpoint path
    pub endpoint_path: String,

    /// Enable authentication for metrics endpoint
    pub auth_enabled: bool,

    /// Authentication token
    pub auth_token: Option<String>,

    /// Request timeout in milliseconds
    pub request_timeout_ms: u64,

    /// Enable compression
    pub compression_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsCollectionConfig {
    /// Collection interval in seconds
    pub collection_interval_seconds: u64,

    /// Enable high-frequency metrics (sub-second collection)
    pub high_frequency_enabled: bool,

    /// High-frequency collection interval in milliseconds
    pub high_frequency_interval_ms: u64,

    /// Metrics categories to collect
    pub categories: Vec<MetricsCategory>,

    /// Node-specific metrics
    pub node_metrics: NodeMetricsConfig,

    /// System metrics
    pub system_metrics: SystemMetricsConfig,

    /// Application metrics
    pub application_metrics: ApplicationMetricsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricsCategory {
    /// Consensus-related metrics
    Consensus,
    /// Network and P2P metrics
    Network,
    /// Transaction processing metrics
    Transaction,
    /// Storage and database metrics
    Storage,
    /// Cryptographic operation metrics
    Crypto,
    /// EVM runtime metrics
    Runtime,
    /// Service bus metrics
    ServiceBus,
    /// Node management metrics
    NodeManagement,
    /// System resource metrics
    System,
    /// Custom application metrics
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMetricsConfig {
    /// Enable node identification in metrics
    pub include_node_id: bool,

    /// Enable network identification
    pub include_network_name: bool,

    /// Enable node type classification
    pub include_node_type: bool,

    /// Enable geographic information
    pub include_geographic_info: bool,

    /// Node uptime tracking
    pub track_uptime: bool,

    /// Node version information
    pub include_version_info: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetricsConfig {
    /// Enable CPU metrics
    pub cpu_enabled: bool,

    /// Enable memory metrics
    pub memory_enabled: bool,

    /// Enable disk I/O metrics
    pub disk_io_enabled: bool,

    /// Enable network I/O metrics
    pub network_io_enabled: bool,

    /// Enable file descriptor metrics
    pub file_descriptors_enabled: bool,

    /// Enable system load metrics
    pub system_load_enabled: bool,

    /// Process-specific metrics
    pub process_metrics: ProcessMetricsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetricsConfig {
    /// Enable process CPU usage
    pub cpu_usage: bool,

    /// Enable process memory usage
    pub memory_usage: bool,

    /// Enable thread count
    pub thread_count: bool,

    /// Enable file descriptor count
    pub fd_count: bool,

    /// Enable process start time
    pub start_time: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplicationMetricsConfig {
    /// Catalyst-specific metrics
    pub catalyst_metrics: CatalystMetricsConfig,

    /// Performance metrics
    pub performance_metrics: PerformanceMetricsConfig,

    /// Error and health metrics
    pub health_metrics: HealthMetricsConfig,

    /// Business logic metrics
    pub business_metrics: BusinessMetricsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalystMetricsConfig {
    /// Consensus cycle metrics
    pub consensus_cycles: bool,

    /// Producer selection metrics
    pub producer_selection: bool,

    /// Transaction throughput metrics
    pub transaction_throughput: bool,

    /// Network peer metrics
    pub network_peers: bool,

    /// DFS storage metrics
    pub dfs_storage: bool,

    /// Worker pool metrics
    pub worker_pool: bool,

    /// Ledger state metrics
    pub ledger_state: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetricsConfig {
    /// Request/response time metrics
    pub response_times: bool,

    /// Throughput metrics
    pub throughput: bool,

    /// Queue depth metrics
    pub queue_depths: bool,

    /// Cache hit/miss ratios
    pub cache_metrics: bool,

    /// Database query performance
    pub database_performance: bool,

    /// Garbage collection metrics
    pub gc_metrics: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetricsConfig {
    /// Error rate metrics
    pub error_rates: bool,

    /// Service availability metrics
    pub availability: bool,

    /// Health check results
    pub health_checks: bool,

    /// Alert metrics
    pub alerts: bool,

    /// SLA compliance metrics
    pub sla_compliance: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusinessMetricsConfig {
    /// User/client metrics
    pub user_metrics: bool,

    /// API usage metrics
    pub api_usage: bool,

    /// Feature usage metrics
    pub feature_usage: bool,

    /// Revenue/cost metrics
    pub revenue_metrics: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsStorageConfig {
    /// Storage backend type
    pub backend: StorageBackend,

    /// Local storage configuration
    pub local: LocalStorageConfig,

    /// Time series database configuration
    pub tsdb: TsdbConfig,

    /// Retention policy
    pub retention: RetentionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageBackend {
    /// In-memory storage (for development)
    Memory,
    /// Local file storage
    Local,
    /// Time series database
    TimeSeries(TsdbType),
    /// Distributed storage
    Distributed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TsdbType {
    /// Prometheus
    Prometheus,
    /// InfluxDB
    InfluxDb,
    /// TimescaleDB
    TimescaleDb,
    /// Custom TSDB
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalStorageConfig {
    /// Storage directory
    pub directory: PathBuf,

    /// Maximum storage size in bytes
    pub max_size_bytes: u64,

    /// File format
    pub file_format: FileFormat,

    /// Compression enabled
    pub compression_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FileFormat {
    Json,
    Csv,
    Parquet,
    Binary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsdbConfig {
    /// Database connection URL
    pub url: String,

    /// Database name
    pub database: String,

    /// Username for authentication
    pub username: Option<String>,

    /// Password for authentication
    pub password: Option<String>,

    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,

    /// Query timeout in milliseconds
    pub query_timeout_ms: u64,

    /// Maximum connections
    pub max_connections: usize,

    /// Batch size for writes
    pub batch_size: usize,

    /// Flush interval in seconds
    pub flush_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    /// Default retention period in seconds
    pub default_retention_seconds: u64,

    /// Category-specific retention policies
    pub category_policies: Vec<CategoryRetentionPolicy>,

    /// High-frequency data retention
    pub high_frequency_retention_seconds: u64,

    /// Aggregated data retention
    pub aggregated_retention_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryRetentionPolicy {
    /// Metrics category
    pub category: MetricsCategory,

    /// Retention period in seconds
    pub retention_seconds: u64,

    /// Aggregation rules
    pub aggregation: Option<AggregationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregationConfig {
    /// Aggregation interval in seconds
    pub interval_seconds: u64,

    /// Aggregation functions to apply
    pub functions: Vec<AggregationFunction>,

    /// Keep raw data alongside aggregated data
    pub keep_raw_data: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregationFunction {
    /// Average/mean
    Average,
    /// Sum
    Sum,
    /// Minimum
    Min,
    /// Maximum
    Max,
    /// Count
    Count,
    /// Percentiles
    Percentile(f64),
    /// Standard deviation
    StdDev,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsExportConfig {
    /// Enable metrics export
    pub enabled: bool,

    /// Export targets
    pub targets: Vec<ExportTarget>,

    /// Export format
    pub format: ExportFormat,

    /// Export interval in seconds
    pub interval_seconds: u64,

    /// Batch export configuration
    pub batch_config: BatchExportConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportTarget {
    /// Target name
    pub name: String,

    /// Target type
    pub target_type: ExportTargetType,

    /// Enable this target
    pub enabled: bool,

    /// Filter configuration
    pub filter: Option<ExportFilter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportTargetType {
    /// Prometheus push gateway
    Prometheus { gateway_url: String, job: String },
    /// InfluxDB
    InfluxDb {
        url: String,
        database: String,
        token: Option<String>,
    },
    /// StatsD
    StatsD {
        host: String,
        port: u16,
        prefix: Option<String>,
    },
    /// HTTP endpoint
    Http {
        url: String,
        headers: Vec<(String, String)>,
    },
    /// File export
    File { path: PathBuf, rotation: bool },
    /// Custom export
    Custom { config: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportFilter {
    /// Include only these metric names (patterns supported)
    pub include_metrics: Vec<String>,

    /// Exclude these metric names (patterns supported)
    pub exclude_metrics: Vec<String>,

    /// Include only these labels
    pub include_labels: Vec<String>,

    /// Exclude these labels
    pub exclude_labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportFormat {
    /// Prometheus format
    Prometheus,
    /// JSON format
    Json,
    /// CSV format
    Csv,
    /// InfluxDB line protocol
    InfluxLineProtocol,
    /// StatsD format
    StatsD,
    /// Custom format
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchExportConfig {
    /// Enable batch export
    pub enabled: bool,

    /// Maximum batch size
    pub max_batch_size: usize,

    /// Maximum wait time in milliseconds
    pub max_wait_time_ms: u64,

    /// Compression for batch exports
    pub compression_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertingConfig {
    /// Enable alerting
    pub enabled: bool,

    /// Alert rules
    pub rules: Vec<AlertRule>,

    /// Alert channels
    pub channels: Vec<AlertChannel>,

    /// Alert grouping configuration
    pub grouping: AlertGroupingConfig,

    /// Alert throttling configuration
    pub throttling: AlertThrottlingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    /// Rule name
    pub name: String,

    /// Rule description
    pub description: String,

    /// Metric to monitor
    pub metric: String,

    /// Alert condition
    pub condition: AlertCondition,

    /// Evaluation interval in seconds
    pub evaluation_interval_seconds: u64,

    /// Alert severity
    pub severity: AlertSeverity,

    /// Target channels for this alert
    pub channels: Vec<String>,

    /// Additional labels
    pub labels: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertCondition {
    /// Threshold condition
    Threshold {
        operator: ComparisonOperator,
        value: f64,
    },
    /// Rate of change condition
    RateOfChange {
        operator: ComparisonOperator,
        value: f64,
        duration_seconds: u64,
    },
    /// Absence condition (metric not reported)
    Absence { duration_seconds: u64 },
    /// Custom condition expression
    Custom { expression: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComparisonOperator {
    GreaterThan,
    GreaterThanOrEqual,
    LessThan,
    LessThanOrEqual,
    Equal,
    NotEqual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Critical,
    Warning,
    Info,
    Debug,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertChannel {
    /// Channel name
    pub name: String,

    /// Channel type
    pub channel_type: AlertChannelType,

    /// Enable this channel
    pub enabled: bool,

    /// Channel-specific configuration
    pub config: ChannelConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertChannelType {
    /// Email notifications
    Email,
    /// Slack notifications
    Slack,
    /// Discord notifications
    Discord,
    /// Webhook notifications
    Webhook,
    /// SMS notifications
    Sms,
    /// PagerDuty integration
    PagerDuty,
    /// Custom notification system
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelConfig {
    /// Channel-specific settings (JSON object)
    pub settings: serde_json::Value,

    /// Message template
    pub message_template: Option<String>,

    /// Retry configuration
    pub retry: RetryConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum retry attempts
    pub max_attempts: usize,

    /// Retry delay in milliseconds
    pub delay_ms: u64,

    /// Exponential backoff multiplier
    pub backoff_multiplier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertGroupingConfig {
    /// Enable alert grouping
    pub enabled: bool,

    /// Grouping keys
    pub group_by: Vec<String>,

    /// Group interval in seconds
    pub group_interval_seconds: u64,

    /// Maximum group size
    pub max_group_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThrottlingConfig {
    /// Enable alert throttling
    pub enabled: bool,

    /// Throttling rules
    pub rules: Vec<ThrottlingRule>,

    /// Default throttling interval in seconds
    pub default_interval_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThrottlingRule {
    /// Rule name
    pub name: String,

    /// Alert severity to throttle
    pub severity: AlertSeverity,

    /// Throttling interval in seconds
    pub interval_seconds: u64,

    /// Maximum alerts per interval
    pub max_alerts_per_interval: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsPerformanceConfig {
    /// Enable performance optimization
    pub optimization_enabled: bool,

    /// Metrics sampling configuration
    pub sampling: MetricsSamplingConfig,

    /// Metrics aggregation configuration
    pub aggregation: MetricsAggregationConfig,

    /// Memory management
    pub memory_management: MemoryManagementConfig,

    /// Concurrent processing
    pub concurrency: ConcurrencyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsSamplingConfig {
    /// Enable sampling
    pub enabled: bool,

    /// Sampling rules
    pub rules: Vec<SamplingRule>,

    /// Default sampling rate
    pub default_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplingRule {
    /// Metric name pattern
    pub metric_pattern: String,

    /// Sampling rate (0.0 to 1.0)
    pub rate: f64,

    /// Burst allowance
    pub burst_allowance: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsAggregationConfig {
    /// Enable real-time aggregation
    pub enabled: bool,

    /// Aggregation window in seconds
    pub window_seconds: u64,

    /// Aggregation functions to compute
    pub functions: Vec<AggregationFunction>,

    /// Keep individual data points
    pub keep_raw: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryManagementConfig {
    /// Maximum memory usage in bytes
    pub max_memory_bytes: u64,

    /// Memory cleanup interval in seconds
    pub cleanup_interval_seconds: u64,

    /// Memory pressure threshold (0.0 to 1.0)
    pub pressure_threshold: f64,

    /// Enable memory pooling
    pub pooling_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyConfig {
    /// Number of worker threads
    pub worker_threads: usize,

    /// Queue size for metrics processing
    pub queue_size: usize,

    /// Enable lock-free data structures
    pub lock_free_enabled: bool,

    /// Batch processing size
    pub batch_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomMetricsConfig {
    /// Enable custom metrics
    pub enabled: bool,

    /// Custom metric definitions
    pub definitions: Vec<CustomMetricDefinition>,

    /// Custom labels
    pub custom_labels: Vec<CustomLabel>,

    /// Dynamic metric creation
    pub dynamic_creation: DynamicCreationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomMetricDefinition {
    /// Metric name
    pub name: String,

    /// Metric description
    pub description: String,

    /// Metric type
    pub metric_type: CustomMetricType,

    /// Labels for this metric
    pub labels: Vec<String>,

    /// Collection source
    pub source: CustomMetricSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CustomMetricType {
    /// Counter (monotonically increasing)
    Counter,
    /// Gauge (can increase or decrease)
    Gauge,
    /// Histogram (distribution of values)
    Histogram { buckets: Vec<f64> },
    /// Summary (quantiles)
    Summary { quantiles: Vec<f64> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CustomMetricSource {
    /// Function call
    Function { function_name: String },
    /// File content
    File { file_path: PathBuf },
    /// HTTP endpoint
    Http { url: String },
    /// Database query
    Database { query: String },
    /// Command execution
    Command { command: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomLabel {
    /// Label name
    pub name: String,

    /// Label value source
    pub source: LabelSource,

    /// Whether label is required
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LabelSource {
    /// Static value
    Static(String),
    /// Environment variable
    Environment(String),
    /// Function result
    Function(String),
    /// Node property
    NodeProperty(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynamicCreationConfig {
    /// Enable dynamic metric creation
    pub enabled: bool,

    /// Maximum number of dynamic metrics
    pub max_dynamic_metrics: usize,

    /// Metric lifetime in seconds
    pub metric_lifetime_seconds: u64,

    /// Enable metric garbage collection
    pub gc_enabled: bool,
}

impl MetricsConfig {
    /// Create development metrics configuration
    pub fn devnet() -> Self {
        Self {
            enabled: true,
            server: MetricsServerConfig {
                port: 9091,
                bind_address: "127.0.0.1".to_string(),
                endpoint_path: "/metrics".to_string(),
                auth_enabled: false,
                auth_token: None,
                request_timeout_ms: 10000,
                compression_enabled: false,
            },
            collection: MetricsCollectionConfig {
                collection_interval_seconds: 15,
                high_frequency_enabled: false,
                high_frequency_interval_ms: 100,
                categories: vec![
                    MetricsCategory::Consensus,
                    MetricsCategory::Network,
                    MetricsCategory::Transaction,
                    MetricsCategory::System,
                ],
                node_metrics: NodeMetricsConfig {
                    include_node_id: true,
                    include_network_name: true,
                    include_node_type: true,
                    include_geographic_info: false,
                    track_uptime: true,
                    include_version_info: true,
                },
                system_metrics: SystemMetricsConfig {
                    cpu_enabled: true,
                    memory_enabled: true,
                    disk_io_enabled: true,
                    network_io_enabled: true,
                    file_descriptors_enabled: false,
                    system_load_enabled: true,
                    process_metrics: ProcessMetricsConfig {
                        cpu_usage: true,
                        memory_usage: true,
                        thread_count: true,
                        fd_count: false,
                        start_time: true,
                    },
                },
                application_metrics: ApplicationMetricsConfig {
                    catalyst_metrics: CatalystMetricsConfig {
                        consensus_cycles: true,
                        producer_selection: true,
                        transaction_throughput: true,
                        network_peers: true,
                        dfs_storage: false,
                        worker_pool: true,
                        ledger_state: true,
                    },
                    performance_metrics: PerformanceMetricsConfig {
                        response_times: true,
                        throughput: true,
                        queue_depths: false,
                        cache_metrics: false,
                        database_performance: false,
                        gc_metrics: false,
                    },
                    health_metrics: HealthMetricsConfig {
                        error_rates: true,
                        availability: true,
                        health_checks: false,
                        alerts: false,
                        sla_compliance: false,
                    },
                    business_metrics: BusinessMetricsConfig {
                        user_metrics: false,
                        api_usage: false,
                        feature_usage: false,
                        revenue_metrics: false,
                    },
                },
            },
            storage: MetricsStorageConfig {
                backend: StorageBackend::Memory,
                local: LocalStorageConfig {
                    directory: PathBuf::from("./data/devnet/metrics"),
                    max_size_bytes: 100 * 1024 * 1024, // 100MB
                    file_format: FileFormat::Json,
                    compression_enabled: false,
                },
                tsdb: TsdbConfig {
                    url: "http://localhost:9090".to_string(),
                    database: "catalyst_devnet".to_string(),
                    username: None,
                    password: None,
                    connection_timeout_ms: 5000,
                    query_timeout_ms: 30000,
                    max_connections: 10,
                    batch_size: 1000,
                    flush_interval_seconds: 10,
                },
                retention: RetentionConfig {
                    default_retention_seconds: 86400, // 1 day
                    category_policies: vec![],
                    high_frequency_retention_seconds: 3600, // 1 hour
                    aggregated_retention_seconds: 604800,   // 1 week
                },
            },
            export: MetricsExportConfig {
                enabled: false,
                targets: vec![],
                format: ExportFormat::Prometheus,
                interval_seconds: 60,
                batch_config: BatchExportConfig {
                    enabled: false,
                    max_batch_size: 1000,
                    max_wait_time_ms: 5000,
                    compression_enabled: false,
                },
            },
            alerting: AlertingConfig {
                enabled: false,
                rules: vec![],
                channels: vec![],
                grouping: AlertGroupingConfig {
                    enabled: false,
                    group_by: vec!["alertname".to_string()],
                    group_interval_seconds: 300,
                    max_group_size: 10,
                },
                throttling: AlertThrottlingConfig {
                    enabled: false,
                    rules: vec![],
                    default_interval_seconds: 300,
                },
            },
            performance: MetricsPerformanceConfig {
                optimization_enabled: false,
                sampling: MetricsSamplingConfig {
                    enabled: false,
                    rules: vec![],
                    default_rate: 1.0,
                },
                aggregation: MetricsAggregationConfig {
                    enabled: false,
                    window_seconds: 60,
                    functions: vec![AggregationFunction::Average],
                    keep_raw: true,
                },
                memory_management: MemoryManagementConfig {
                    max_memory_bytes: 50 * 1024 * 1024, // 50MB
                    cleanup_interval_seconds: 300,
                    pressure_threshold: 0.8,
                    pooling_enabled: false,
                },
                concurrency: ConcurrencyConfig {
                    worker_threads: 2,
                    queue_size: 10000,
                    lock_free_enabled: false,
                    batch_size: 100,
                },
            },
            custom_metrics: CustomMetricsConfig {
                enabled: false,
                definitions: vec![],
                custom_labels: vec![],
                dynamic_creation: DynamicCreationConfig {
                    enabled: false,
                    max_dynamic_metrics: 1000,
                    metric_lifetime_seconds: 3600,
                    gc_enabled: true,
                },
            },
        }
    }

    /// Create test network metrics configuration
    pub fn testnet() -> Self {
        let mut config = Self::devnet();

        // Override testnet-specific settings
        config.collection.collection_interval_seconds = 30;
        config.collection.high_frequency_enabled = true;
        config.collection.categories.extend(vec![
            MetricsCategory::Storage,
            MetricsCategory::Crypto,
            MetricsCategory::ServiceBus,
        ]);

        config.storage.backend = StorageBackend::Local;
        config.storage.local.max_size_bytes = 1024 * 1024 * 1024; // 1GB
        config.storage.local.compression_enabled = true;
        config.storage.retention.default_retention_seconds = 604800; // 1 week

        config.export.enabled = true;
        config.export.targets = vec![ExportTarget {
            name: "local_prometheus".to_string(),
            target_type: ExportTargetType::File {
                path: PathBuf::from("./metrics/testnet/prometheus.txt"),
                rotation: true,
            },
            enabled: true,
            filter: None,
        }];

        config.alerting.enabled = true;
        config.alerting.rules = vec![AlertRule {
            name: "high_error_rate".to_string(),
            description: "Error rate too high".to_string(),
            metric: "error_rate".to_string(),
            condition: AlertCondition::Threshold {
                operator: ComparisonOperator::GreaterThan,
                value: 0.05, // 5%
            },
            evaluation_interval_seconds: 60,
            severity: AlertSeverity::Warning,
            channels: vec!["console".to_string()],
            labels: vec![],
        }];

        config.performance.optimization_enabled = true;
        config.performance.sampling.enabled = true;

        config
    }

    /// Create main network metrics configuration (production)
    pub fn mainnet() -> Self {
        let mut config = Self::testnet();

        // Override mainnet-specific settings
        config.server.auth_enabled = true;
        config.server.auth_token = Some("CHANGE_THIS_IN_PRODUCTION".to_string());
        config.server.compression_enabled = true;

        config.collection.collection_interval_seconds = 60;
        config.collection.categories = vec![
            MetricsCategory::Consensus,
            MetricsCategory::Network,
            MetricsCategory::Transaction,
            MetricsCategory::Storage,
            MetricsCategory::Crypto,
            MetricsCategory::Runtime,
            MetricsCategory::ServiceBus,
            MetricsCategory::NodeManagement,
            MetricsCategory::System,
        ];

        config.storage.backend = StorageBackend::TimeSeries(TsdbType::Prometheus);
        config.storage.retention.default_retention_seconds = 2592000; // 30 days
        config.storage.retention.category_policies = vec![CategoryRetentionPolicy {
            category: MetricsCategory::System,
            retention_seconds: 604800, // 1 week for system metrics
            aggregation: Some(AggregationConfig {
                interval_seconds: 300, // 5 minutes
                functions: vec![AggregationFunction::Average, AggregationFunction::Max],
                keep_raw_data: false,
            }),
        }];

        config.export.targets = vec![
            ExportTarget {
                name: "prometheus_push".to_string(),
                target_type: ExportTargetType::Prometheus {
                    gateway_url: "http://prometheus-pushgateway:9091".to_string(),
                    job: "catalyst-mainnet".to_string(),
                },
                enabled: true,
                filter: None,
            },
            ExportTarget {
                name: "influxdb".to_string(),
                target_type: ExportTargetType::InfluxDb {
                    url: "http://influxdb:8086".to_string(),
                    database: "catalyst_metrics".to_string(),
                    token: Some("CHANGE_THIS_TOKEN".to_string()),
                },
                enabled: true,
                filter: Some(ExportFilter {
                    include_metrics: vec!["catalyst_*".to_string()],
                    exclude_metrics: vec!["catalyst_debug_*".to_string()],
                    include_labels: vec![],
                    exclude_labels: vec!["internal_*".to_string()],
                }),
            },
        ];

        config.alerting.rules.extend(vec![
            AlertRule {
                name: "consensus_failure".to_string(),
                description: "Consensus cycle failure".to_string(),
                metric: "consensus_cycles_failed_total".to_string(),
                condition: AlertCondition::RateOfChange {
                    operator: ComparisonOperator::GreaterThan,
                    value: 0.1,
                    duration_seconds: 300,
                },
                evaluation_interval_seconds: 60,
                severity: AlertSeverity::Critical,
                channels: vec!["pagerduty".to_string(), "slack".to_string()],
                labels: vec![("team".to_string(), "consensus".to_string())],
            },
            AlertRule {
                name: "node_down".to_string(),
                description: "Node appears to be down".to_string(),
                metric: "up".to_string(),
                condition: AlertCondition::Absence {
                    duration_seconds: 120,
                },
                evaluation_interval_seconds: 30,
                severity: AlertSeverity::Critical,
                channels: vec!["pagerduty".to_string()],
                labels: vec![("severity".to_string(), "critical".to_string())],
            },
        ]);

        config.alerting.channels = vec![
            AlertChannel {
                name: "pagerduty".to_string(),
                channel_type: AlertChannelType::PagerDuty,
                enabled: true,
                config: ChannelConfig {
                    settings: serde_json::json!({
                        "integration_key": "CHANGE_THIS_KEY",
                        "severity": "critical"
                    }),
                    message_template: Some("{{ .AlertName }}: {{ .Description }}".to_string()),
                    retry: RetryConfig {
                        max_attempts: 3,
                        delay_ms: 1000,
                        backoff_multiplier: 2.0,
                    },
                },
            },
            AlertChannel {
                name: "slack".to_string(),
                channel_type: AlertChannelType::Slack,
                enabled: true,
                config: ChannelConfig {
                    settings: serde_json::json!({
                        "webhook_url": "https://hooks.slack.com/services/CHANGE/THIS/URL",
                        "channel": "#catalyst-alerts"
                    }),
                    message_template: None,
                    retry: RetryConfig {
                        max_attempts: 2,
                        delay_ms: 500,
                        backoff_multiplier: 1.5,
                    },
                },
            },
        ];

        config.performance.aggregation.enabled = true;
        config.performance.memory_management.max_memory_bytes = 500 * 1024 * 1024; // 500MB
        config.performance.concurrency.worker_threads = 8;

        config.custom_metrics.enabled = true;
        config.custom_metrics.dynamic_creation.enabled = true;
        config.custom_metrics.dynamic_creation.max_dynamic_metrics = 10000;

        config
    }

    /// Validate metrics configuration
    pub fn validate(&self) -> ConfigResult<()> {
        if !self.enabled {
            return Ok(()); // Skip validation if disabled
        }

        // Validate server configuration
        if self.server.port == 0 {
            return Err(ConfigError::ValidationFailed(
                "Metrics server port cannot be 0".to_string(),
            ));
        }

        if self.server.bind_address.is_empty() {
            return Err(ConfigError::ValidationFailed(
                "Metrics server bind address cannot be empty".to_string(),
            ));
        }

        if self.server.endpoint_path.is_empty() {
            return Err(ConfigError::ValidationFailed(
                "Metrics endpoint path cannot be empty".to_string(),
            ));
        }

        // Validate collection intervals
        if self.collection.collection_interval_seconds == 0 {
            return Err(ConfigError::ValidationFailed(
                "Collection interval must be greater than 0".to_string(),
            ));
        }

        if self.collection.high_frequency_enabled && self.collection.high_frequency_interval_ms == 0
        {
            return Err(ConfigError::ValidationFailed(
                "High frequency interval must be greater than 0".to_string(),
            ));
        }

        // Validate storage configuration
        if let StorageBackend::Local = self.storage.backend {
            if self.storage.local.directory.as_os_str().is_empty() {
                return Err(ConfigError::ValidationFailed(
                    "Local storage directory cannot be empty".to_string(),
                ));
            }

            if self.storage.local.max_size_bytes == 0 {
                return Err(ConfigError::ValidationFailed(
                    "Local storage max size must be greater than 0".to_string(),
                ));
            }
        }

        // Validate retention policies
        if self.storage.retention.default_retention_seconds == 0 {
            return Err(ConfigError::ValidationFailed(
                "Default retention period must be greater than 0".to_string(),
            ));
        }

        // Validate export configuration
        if self.export.enabled {
            if self.export.interval_seconds == 0 {
                return Err(ConfigError::ValidationFailed(
                    "Export interval must be greater than 0".to_string(),
                ));
            }

            if self.export.targets.is_empty() {
                return Err(ConfigError::ValidationFailed(
                    "At least one export target must be configured when export is enabled"
                        .to_string(),
                ));
            }
        }

        // Validate alerting configuration
        if self.alerting.enabled {
            for rule in &self.alerting.rules {
                if rule.name.is_empty() {
                    return Err(ConfigError::ValidationFailed(
                        "Alert rule name cannot be empty".to_string(),
                    ));
                }

                if rule.metric.is_empty() {
                    return Err(ConfigError::ValidationFailed(
                        "Alert rule metric cannot be empty".to_string(),
                    ));
                }

                if rule.evaluation_interval_seconds == 0 {
                    return Err(ConfigError::ValidationFailed(
                        "Alert rule evaluation interval must be greater than 0".to_string(),
                    ));
                }
            }

            for channel in &self.alerting.channels {
                if channel.name.is_empty() {
                    return Err(ConfigError::ValidationFailed(
                        "Alert channel name cannot be empty".to_string(),
                    ));
                }
            }
        }

        // Validate performance configuration
        if self.performance.concurrency.worker_threads == 0 {
            return Err(ConfigError::ValidationFailed(
                "Worker threads must be greater than 0".to_string(),
            ));
        }

        if self.performance.concurrency.queue_size == 0 {
            return Err(ConfigError::ValidationFailed(
                "Queue size must be greater than 0".to_string(),
            ));
        }

        if self.performance.memory_management.max_memory_bytes == 0 {
            return Err(ConfigError::ValidationFailed(
                "Max memory bytes must be greater than 0".to_string(),
            ));
        }

        // Validate sampling rates
        if self.performance.sampling.enabled {
            if self.performance.sampling.default_rate < 0.0
                || self.performance.sampling.default_rate > 1.0
            {
                return Err(ConfigError::ValidationFailed(
                    "Default sampling rate must be between 0.0 and 1.0".to_string(),
                ));
            }

            for rule in &self.performance.sampling.rules {
                if rule.rate < 0.0 || rule.rate > 1.0 {
                    return Err(ConfigError::ValidationFailed(
                        "Sampling rate must be between 0.0 and 1.0".to_string(),
                    ));
                }
            }
        }

        // Validate custom metrics settings
        if self.custom_metrics.enabled {
            if self.custom_metrics.dynamic_creation.enabled {
                if self.custom_metrics.dynamic_creation.max_dynamic_metrics == 0 {
                    return Err(ConfigError::ValidationFailed(
                        "Max dynamic metrics must be greater than 0".to_string(),
                    ));
                }

                if self.custom_metrics.dynamic_creation.metric_lifetime_seconds == 0 {
                    return Err(ConfigError::ValidationFailed(
                        "Metric lifetime must be greater than 0".to_string(),
                    ));
                }
            }
        }

        // Validate TSDB configuration if using time series backend
        if let StorageBackend::TimeSeries(_) = self.storage.backend {
            if self.storage.tsdb.url.is_empty() {
                return Err(ConfigError::ValidationFailed(
                    "TSDB URL cannot be empty".to_string(),
                ));
            }

            if self.storage.tsdb.database.is_empty() {
                return Err(ConfigError::ValidationFailed(
                    "TSDB database name cannot be empty".to_string(),
                ));
            }

            if self.storage.tsdb.connection_timeout_ms == 0 {
                return Err(ConfigError::ValidationFailed(
                    "TSDB connection timeout must be greater than 0".to_string(),
                ));
            }

            if self.storage.tsdb.batch_size == 0 {
                return Err(ConfigError::ValidationFailed(
                    "TSDB batch size must be greater than 0".to_string(),
                ));
            }
        }

        // Validate export targets
        if self.export.enabled {
            for target in &self.export.targets {
                if target.name.is_empty() {
                    return Err(ConfigError::ValidationFailed(
                        "Export target name cannot be empty".to_string(),
                    ));
                }

                match &target.target_type {
                    ExportTargetType::Prometheus { gateway_url, job } => {
                        if gateway_url.is_empty() {
                            return Err(ConfigError::ValidationFailed(
                                "Prometheus gateway URL cannot be empty".to_string(),
                            ));
                        }
                        if job.is_empty() {
                            return Err(ConfigError::ValidationFailed(
                                "Prometheus job name cannot be empty".to_string(),
                            ));
                        }
                    }
                    ExportTargetType::InfluxDb { url, database, .. } => {
                        if url.is_empty() {
                            return Err(ConfigError::ValidationFailed(
                                "InfluxDB URL cannot be empty".to_string(),
                            ));
                        }
                        if database.is_empty() {
                            return Err(ConfigError::ValidationFailed(
                                "InfluxDB database name cannot be empty".to_string(),
                            ));
                        }
                    }
                    ExportTargetType::StatsD { host, port, .. } => {
                        if host.is_empty() {
                            return Err(ConfigError::ValidationFailed(
                                "StatsD host cannot be empty".to_string(),
                            ));
                        }
                        if *port == 0 {
                            return Err(ConfigError::ValidationFailed(
                                "StatsD port cannot be 0".to_string(),
                            ));
                        }
                    }
                    ExportTargetType::Http { url, .. } => {
                        if url.is_empty() {
                            return Err(ConfigError::ValidationFailed(
                                "HTTP export URL cannot be empty".to_string(),
                            ));
                        }
                    }
                    ExportTargetType::File { path, .. } => {
                        if path.as_os_str().is_empty() {
                            return Err(ConfigError::ValidationFailed(
                                "File export path cannot be empty".to_string(),
                            ));
                        }
                    }
                    ExportTargetType::Custom { config } => {
                        if config.is_empty() {
                            return Err(ConfigError::ValidationFailed(
                                "Custom export config cannot be empty".to_string(),
                            ));
                        }
                    }
                }
            }
        }

        // Validate aggregation configuration
        if self.performance.aggregation.enabled {
            if self.performance.aggregation.window_seconds == 0 {
                return Err(ConfigError::ValidationFailed(
                    "Aggregation window must be greater than 0".to_string(),
                ));
            }

            if self.performance.aggregation.functions.is_empty() {
                return Err(ConfigError::ValidationFailed(
                    "At least one aggregation function must be specified".to_string(),
                ));
            }
        }

        // Validate memory management thresholds
        if self.performance.memory_management.pressure_threshold < 0.0
            || self.performance.memory_management.pressure_threshold > 1.0
        {
            return Err(ConfigError::ValidationFailed(
                "Memory pressure threshold must be between 0.0 and 1.0".to_string(),
            ));
        }

        // Validate alert rule conditions
        for rule in &self.alerting.rules {
            match &rule.condition {
                AlertCondition::Threshold { value, .. } => {
                    if value.is_nan() || value.is_infinite() {
                        return Err(ConfigError::ValidationFailed(format!(
                            "Invalid threshold value for alert rule '{}'",
                            rule.name
                        )));
                    }
                }
                AlertCondition::RateOfChange {
                    value,
                    duration_seconds,
                    ..
                } => {
                    if value.is_nan() || value.is_infinite() {
                        return Err(ConfigError::ValidationFailed(format!(
                            "Invalid rate of change value for alert rule '{}'",
                            rule.name
                        )));
                    }
                    if *duration_seconds == 0 {
                        return Err(ConfigError::ValidationFailed(format!(
                            "Rate of change duration must be greater than 0 for alert rule '{}'",
                            rule.name
                        )));
                    }
                }
                AlertCondition::Absence { duration_seconds } => {
                    if *duration_seconds == 0 {
                        return Err(ConfigError::ValidationFailed(format!(
                            "Absence duration must be greater than 0 for alert rule '{}'",
                            rule.name
                        )));
                    }
                }
                AlertCondition::Custom { expression } => {
                    if expression.is_empty() {
                        return Err(ConfigError::ValidationFailed(format!(
                            "Custom alert expression cannot be empty for rule '{}'",
                            rule.name
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// Get the total estimated memory usage for metrics
    pub fn estimated_memory_usage(&self) -> u64 {
        let mut total = 0u64;

        // Storage memory
        total += self.storage.local.max_size_bytes;

        // Performance memory
        total += self.performance.memory_management.max_memory_bytes;

        // Buffer memory estimate (rough calculation)
        if self.export.enabled {
            total += (self.export.batch_config.max_batch_size * 1024) as u64; // ~1KB per metric
        }

        total
    }

    /// Get recommended collection interval based on network type
    pub fn recommended_collection_interval(network: &str) -> u64 {
        match network {
            "devnet" => 15,  // 15 seconds for development
            "testnet" => 30, // 30 seconds for testing
            "mainnet" => 60, // 60 seconds for production
            _ => 30,         // Default to moderate interval
        }
    }

    /// Get recommended retention period based on network type and metric category
    pub fn recommended_retention_period(network: &str, category: &MetricsCategory) -> u64 {
        let base_retention = match network {
            "devnet" => 86400,    // 1 day
            "testnet" => 604800,  // 1 week
            "mainnet" => 2592000, // 30 days
            _ => 604800,          // Default to 1 week
        };

        // Adjust based on category importance
        match category {
            MetricsCategory::System => base_retention / 2, // System metrics don't need long retention
            MetricsCategory::Consensus => base_retention * 2, // Consensus metrics are critical
            MetricsCategory::Transaction => base_retention, // Standard retention
            MetricsCategory::Network => base_retention,    // Standard retention
            MetricsCategory::Storage => base_retention,    // Standard retention
            MetricsCategory::Crypto => base_retention / 4, // Crypto metrics are frequent but less critical for long-term
            MetricsCategory::Runtime => base_retention,    // Standard retention
            MetricsCategory::ServiceBus => base_retention / 2, // Service bus metrics don't need long retention
            MetricsCategory::NodeManagement => base_retention, // Standard retention
            MetricsCategory::Custom(_) => base_retention / 2, // Custom metrics get shorter retention by default
        }
    }

    /// Check if the configuration is suitable for production use
    pub fn is_production_ready(&self) -> bool {
        if !self.enabled {
            return false;
        }

        // Production should have authentication enabled
        if !self.server.auth_enabled {
            return false;
        }

        // Production should have reasonable retention
        if self.storage.retention.default_retention_seconds < 604800 {
            // Less than 1 week
            return false;
        }

        // Production should have alerting enabled
        if !self.alerting.enabled {
            return false;
        }

        // Production should have at least some alert rules
        if self.alerting.rules.is_empty() {
            return false;
        }

        true
    }

    /// Get metrics configuration summary for reporting
    pub fn get_summary(&self) -> MetricsSummary {
        MetricsSummary {
            enabled: self.enabled,
            collection_interval: self.collection.collection_interval_seconds,
            categories_count: self.collection.categories.len(),
            storage_backend: format!("{:?}", self.storage.backend),
            export_enabled: self.export.enabled,
            export_targets_count: self.export.targets.len(),
            alerting_enabled: self.alerting.enabled,
            alert_rules_count: self.alerting.rules.len(),
            alert_channels_count: self.alerting.channels.len(),
            estimated_memory_mb: self.estimated_memory_usage() / (1024 * 1024),
            worker_threads: self.performance.concurrency.worker_threads,
            custom_metrics_enabled: self.custom_metrics.enabled,
        }
    }
}

/// Summary information about metrics configuration
#[derive(Debug, Clone)]
pub struct MetricsSummary {
    pub enabled: bool,
    pub collection_interval: u64,
    pub categories_count: usize,
    pub storage_backend: String,
    pub export_enabled: bool,
    pub export_targets_count: usize,
    pub alerting_enabled: bool,
    pub alert_rules_count: usize,
    pub alert_channels_count: usize,
    pub estimated_memory_mb: u64,
    pub worker_threads: usize,
    pub custom_metrics_enabled: bool,
}

impl std::fmt::Display for MetricsSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Metrics Summary:\n\
             - Enabled: {}\n\
             - Collection Interval: {}s\n\
             - Categories: {}\n\
             - Storage: {}\n\
             - Export: {} ({} targets)\n\
             - Alerting: {} ({} rules, {} channels)\n\
             - Memory: {}MB\n\
             - Worker Threads: {}\n\
             - Custom Metrics: {}",
            self.enabled,
            self.collection_interval,
            self.categories_count,
            self.storage_backend,
            self.export_enabled,
            self.export_targets_count,
            self.alerting_enabled,
            self.alert_rules_count,
            self.alert_channels_count,
            self.estimated_memory_mb,
            self.worker_threads,
            self.custom_metrics_enabled
        )
    }
}
