use serde::{Deserialize, Serialize};
use crate::error::{ConfigError, ConfigResult};

/// Service Bus configuration for Web2 integration and event streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceBusConfig {
    /// Enable service bus functionality
    pub enabled: bool,
    
    /// WebSocket server configuration
    pub websocket: WebSocketConfig,
    
    /// REST API configuration
    pub rest_api: RestApiConfig,
    
    /// Event processing configuration
    pub event_processing: EventProcessingConfig,
    
    /// Authentication and authorization
    pub auth: AuthConfig,
    
    /// Rate limiting configuration
    pub rate_limiting: RateLimitingConfig,
    
    /// Client SDK configuration
    pub client_sdk: ClientSdkConfig,
    
    /// Performance and monitoring
    pub performance: PerformanceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// WebSocket server port
    pub port: u16,
    
    /// Bind address
    pub bind_address: String,
    
    /// Maximum number of concurrent connections
    pub max_connections: usize,
    
    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,
    
    /// Heartbeat interval in milliseconds
    pub heartbeat_interval_ms: u64,
    
    /// Maximum message size in bytes
    pub max_message_size: usize,
    
    /// Message compression settings
    pub compression: CompressionConfig,
    
    /// SSL/TLS configuration
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestApiConfig {
    /// REST API server port
    pub port: u16,
    
    /// Bind address
    pub bind_address: String,
    
    /// Maximum request size in bytes
    pub max_request_size: usize,
    
    /// Request timeout in milliseconds
    pub request_timeout_ms: u64,
    
    /// Enable CORS
    pub cors_enabled: bool,
    
    /// CORS allowed origins
    pub cors_origins: Vec<String>,
    
    /// API versioning
    pub versioning: ApiVersioningConfig,
    
    /// SSL/TLS configuration
    pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    /// Enable message compression
    pub enabled: bool,
    
    /// Compression algorithm
    pub algorithm: CompressionAlgorithm,
    
    /// Compression level (1-9)
    pub level: u8,
    
    /// Minimum message size to compress
    pub min_size_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    Gzip,
    Deflate,
    Brotli,
    Lz4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Enable TLS
    pub enabled: bool,
    
    /// Certificate file path
    pub cert_file: String,
    
    /// Private key file path
    pub key_file: String,
    
    /// Certificate authority file path
    pub ca_file: Option<String>,
    
    /// Minimum TLS version
    pub min_version: TlsVersion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TlsVersion {
    TlsV1_2,
    TlsV1_3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiVersioningConfig {
    /// Default API version
    pub default_version: String,
    
    /// Supported API versions
    pub supported_versions: Vec<String>,
    
    /// Version header name
    pub version_header: String,
    
    /// Enable version in URL path
    pub version_in_path: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventProcessingConfig {
    /// Event buffer configuration
    pub buffer: EventBufferConfig,
    
    /// Event filtering configuration
    pub filtering: EventFilteringConfig,
    
    /// Event transformation configuration
    pub transformation: EventTransformationConfig,
    
    /// Event replay configuration
    pub replay: EventReplayConfig,
    
    /// Event persistence configuration
    pub persistence: EventPersistenceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBufferConfig {
    /// Buffer size (number of events)
    pub size: usize,
    
    /// Buffer type
    pub buffer_type: BufferType,
    
    /// Overflow behavior
    pub overflow_behavior: OverflowBehavior,
    
    /// Buffer flush interval in milliseconds
    pub flush_interval_ms: u64,
    
    /// Maximum batch size for processing
    pub max_batch_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BufferType {
    /// In-memory ring buffer
    RingBuffer,
    /// Persistent queue
    PersistentQueue,
    /// Redis-backed buffer
    Redis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OverflowBehavior {
    /// Drop oldest events
    DropOldest,
    /// Drop newest events
    DropNewest,
    /// Block until space available
    Block,
    /// Expand buffer dynamically
    Expand,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilteringConfig {
    /// Enable event filtering
    pub enabled: bool,
    
    /// Default filter rules
    pub default_filters: Vec<FilterRule>,
    
    /// Maximum filters per client
    pub max_filters_per_client: usize,
    
    /// Filter complexity limits
    pub complexity_limits: FilterComplexityLimits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterRule {
    /// Filter name/identifier
    pub name: String,
    
    /// Event types to include
    pub include_types: Vec<String>,
    
    /// Event types to exclude
    pub exclude_types: Vec<String>,
    
    /// Address filters
    pub address_filters: Vec<String>,
    
    /// Custom filter expressions
    pub custom_expressions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterComplexityLimits {
    /// Maximum number of conditions per filter
    pub max_conditions: usize,
    
    /// Maximum nesting depth
    pub max_nesting_depth: usize,
    
    /// Maximum filter execution time in microseconds
    pub max_execution_time_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTransformationConfig {
    /// Enable event transformation
    pub enabled: bool,
    
    /// Available transformation types
    pub available_transformations: Vec<TransformationType>,
    
    /// Maximum transformations per event
    pub max_transformations_per_event: usize,
    
    /// Transformation timeout in milliseconds
    pub transformation_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransformationType {
    /// JSON transformation
    JsonTransform,
    /// Field mapping
    FieldMapping,
    /// Data enrichment
    DataEnrichment,
    /// Format conversion
    FormatConversion,
    /// Custom transformation
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventReplayConfig {
    /// Enable event replay functionality
    pub enabled: bool,
    
    /// Maximum replay duration in hours
    pub max_replay_duration_hours: u64,
    
    /// Maximum events per replay
    pub max_events_per_replay: usize,
    
    /// Replay rate limiting (events per second)
    pub replay_rate_limit: u64,
    
    /// Enable replay caching
    pub caching_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPersistenceConfig {
    /// Enable event persistence
    pub enabled: bool,
    
    /// Persistence duration in hours
    pub retention_hours: u64,
    
    /// Storage backend
    pub storage_backend: PersistenceBackend,
    
    /// Compression for stored events
    pub compression_enabled: bool,
    
    /// Batch size for persistence operations
    pub persistence_batch_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PersistenceBackend {
    /// Local file storage
    FileSystem,
    /// Database storage
    Database,
    /// Distributed storage
    Distributed,
    /// Cloud storage
    Cloud(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Enable authentication
    pub enabled: bool,
    
    /// Authentication methods
    pub methods: Vec<AuthMethod>,
    
    /// JWT configuration
    pub jwt: JwtConfig,
    
    /// API key configuration
    pub api_keys: ApiKeyConfig,
    
    /// OAuth configuration
    pub oauth: Option<OAuthConfig>,
    
    /// Session configuration
    pub sessions: SessionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    /// JWT tokens
    Jwt,
    /// API keys
    ApiKey,
    /// OAuth 2.0
    OAuth,
    /// Basic authentication
    Basic,
    /// Custom authentication
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtConfig {
    /// JWT secret key
    pub secret_key: String,
    
    /// Token expiration time in seconds
    pub expiration_seconds: u64,
    
    /// Issuer identifier
    pub issuer: String,
    
    /// Audience identifier
    pub audience: String,
    
    /// Algorithm for signing
    pub algorithm: JwtAlgorithm,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JwtAlgorithm {
    HS256,
    HS384,
    HS512,
    RS256,
    RS384,
    RS512,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyConfig {
    /// API key length
    pub key_length: usize,
    
    /// Key expiration time in seconds
    pub expiration_seconds: Option<u64>,
    
    /// Enable key rotation
    pub rotation_enabled: bool,
    
    /// Key storage backend
    pub storage_backend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// OAuth provider
    pub provider: String,
    
    /// Client ID
    pub client_id: String,
    
    /// Client secret
    pub client_secret: String,
    
    /// Redirect URI
    pub redirect_uri: String,
    
    /// OAuth scopes
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Session timeout in seconds
    pub timeout_seconds: u64,
    
    /// Session storage
    pub storage: SessionStorage,
    
    /// Enable secure cookies
    pub secure_cookies: bool,
    
    /// Cookie same-site policy
    pub same_site_policy: SameSitePolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionStorage {
    Memory,
    Redis,
    Database,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SameSitePolicy {
    Strict,
    Lax,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitingConfig {
    /// Enable rate limiting
    pub enabled: bool,
    
    /// Global rate limits
    pub global_limits: GlobalRateLimits,
    
    /// Per-client rate limits
    pub per_client_limits: PerClientRateLimits,
    
    /// Rate limiting algorithm
    pub algorithm: RateLimitingAlgorithm,
    
    /// Rate limiting storage
    pub storage: RateLimitingStorage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRateLimits {
    /// Requests per second
    pub requests_per_second: u64,
    
    /// Burst allowance
    pub burst_allowance: u64,
    
    /// WebSocket connections per second
    pub connections_per_second: u64,
    
    /// Events per second
    pub events_per_second: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerClientRateLimits {
    /// Requests per minute per client
    pub requests_per_minute: u64,
    
    /// Events per minute per client
    pub events_per_minute: u64,
    
    /// Maximum concurrent connections per client
    pub max_concurrent_connections: usize,
    
    /// Data transfer limits in bytes per minute
    pub data_transfer_per_minute: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RateLimitingAlgorithm {
    /// Token bucket algorithm
    TokenBucket,
    /// Leaky bucket algorithm
    LeakyBucket,
    /// Fixed window counter
    FixedWindow,
    /// Sliding window counter
    SlidingWindow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RateLimitingStorage {
    Memory,
    Redis,
    Database,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientSdkConfig {
    /// Enable client SDK generation
    pub enabled: bool,
    
    /// Supported SDK languages
    pub supported_languages: Vec<SdkLanguage>,
    
    /// SDK versioning
    pub versioning: SdkVersioningConfig,
    
    /// SDK documentation
    pub documentation: SdkDocumentationConfig,
    
    /// SDK examples and samples
    pub examples: SdkExamplesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SdkLanguage {
    JavaScript,
    Python,
    Go,
    Rust,
    Java,
    CSharp,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkVersioningConfig {
    /// SDK version format
    pub version_format: String,
    
    /// Auto-generate SDK versions
    pub auto_generate: bool,
    
    /// Compatibility matrix
    pub compatibility_matrix: Vec<CompatibilityEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityEntry {
    /// SDK version
    pub sdk_version: String,
    
    /// Compatible API versions
    pub api_versions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkDocumentationConfig {
    /// Auto-generate documentation
    pub auto_generate: bool,
    
    /// Documentation format
    pub format: DocumentationFormat,
    
    /// Include code examples
    pub include_examples: bool,
    
    /// Documentation output directory
    pub output_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentationFormat {
    Markdown,
    Html,
    Json,
    OpenApi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdkExamplesConfig {
    /// Generate example code
    pub generate_examples: bool,
    
    /// Example scenarios
    pub scenarios: Vec<ExampleScenario>,
    
    /// Example output directory
    pub output_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleScenario {
    /// Scenario name
    pub name: String,
    
    /// Scenario description
    pub description: String,
    
    /// Required features
    pub features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Performance monitoring
    pub monitoring: PerformanceMonitoringConfig,
    
    /// Connection pooling
    pub connection_pooling: ConnectionPoolingConfig,
    
    /// Caching configuration
    pub caching: ServiceBusCachingConfig,
    
    /// Load balancing
    pub load_balancing: LoadBalancingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMonitoringConfig {
    /// Enable performance monitoring
    pub enabled: bool,
    
    /// Metrics collection interval in seconds
    pub collection_interval_seconds: u64,
    
    /// Performance metrics to collect
    pub metrics: Vec<PerformanceMetric>,
    
    /// Alert thresholds
    pub alert_thresholds: AlertThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PerformanceMetric {
    ResponseTime,
    Throughput,
    ErrorRate,
    ConnectionCount,
    MemoryUsage,
    CpuUsage,
    EventProcessingRate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// Maximum response time in milliseconds
    pub max_response_time_ms: u64,
    
    /// Maximum error rate percentage
    pub max_error_rate_percent: f64,
    
    /// Maximum memory usage percentage
    pub max_memory_usage_percent: f64,
    
    /// Maximum CPU usage percentage
    pub max_cpu_usage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoolingConfig {
    /// Enable connection pooling
    pub enabled: bool,
    
    /// Initial pool size
    pub initial_size: usize,
    
    /// Maximum pool size
    pub max_size: usize,
    
    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,
    
    /// Idle timeout in milliseconds
    pub idle_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceBusCachingConfig {
    /// Enable caching
    pub enabled: bool,
    
    /// Cache size in bytes
    pub cache_size_bytes: u64,
    
    /// Cache TTL in seconds
    pub ttl_seconds: u64,
    
    /// Cache eviction policy
    pub eviction_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadBalancingConfig {
    /// Enable load balancing
    pub enabled: bool,
    
    /// Load balancing algorithm
    pub algorithm: LoadBalancingAlgorithm,
    
    /// Health check configuration
    pub health_check: HealthCheckConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoadBalancingAlgorithm {
    RoundRobin,
    LeastConnections,
    WeightedRoundRobin,
    IpHash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckConfig {
    /// Health check interval in seconds
    pub interval_seconds: u64,
    
    /// Health check timeout in milliseconds
    pub timeout_ms: u64,
    
    /// Health check endpoint
    pub endpoint: String,
}

impl ServiceBusConfig {
    /// Create development service bus configuration
    pub fn devnet() -> Self {
        Self {
            enabled: true,
            websocket: WebSocketConfig {
                port: 9090,
                bind_address: "127.0.0.1".to_string(),
                max_connections: 100,
                connection_timeout_ms: 30000,
                heartbeat_interval_ms: 30000,
                max_message_size: 1024 * 1024, // 1MB
                compression: CompressionConfig {
                    enabled: false,
                    algorithm: CompressionAlgorithm::Gzip,
                    level: 6,
                    min_size_bytes: 1024,
                },
                tls: None,
            },
            rest_api: RestApiConfig {
                port: 8081,
                bind_address: "127.0.0.1".to_string(),
                max_request_size: 1024 * 1024, // 1MB
                request_timeout_ms: 10000,
                cors_enabled: true,
                cors_origins: vec!["*".to_string()],
                versioning: ApiVersioningConfig {
                    default_version: "v1".to_string(),
                    supported_versions: vec!["v1".to_string()],
                    version_header: "X-API-Version".to_string(),
                    version_in_path: true,
                },
                tls: None,
            },
            event_processing: EventProcessingConfig {
                buffer: EventBufferConfig {
                    size: 1000,
                    buffer_type: BufferType::RingBuffer,
                    overflow_behavior: OverflowBehavior::DropOldest,
                    flush_interval_ms: 1000,
                    max_batch_size: 100,
                },
                filtering: EventFilteringConfig {
                    enabled: true,
                    default_filters: vec![],
                    max_filters_per_client: 10,
                    complexity_limits: FilterComplexityLimits {
                        max_conditions: 10,
                        max_nesting_depth: 3,
                        max_execution_time_us: 1000,
                    },
                },
                transformation: EventTransformationConfig {
                    enabled: false,
                    available_transformations: vec![TransformationType::JsonTransform],
                    max_transformations_per_event: 3,
                    transformation_timeout_ms: 1000,
                },
                replay: EventReplayConfig {
                    enabled: false,
                    max_replay_duration_hours: 1,
                    max_events_per_replay: 1000,
                    replay_rate_limit: 100,
                    caching_enabled: false,
                },
                persistence: EventPersistenceConfig {
                    enabled: false,
                    retention_hours: 24,
                    storage_backend: PersistenceBackend::FileSystem,
                    compression_enabled: false,
                    persistence_batch_size: 100,
                },
            },
            auth: AuthConfig {
                enabled: false,
                methods: vec![AuthMethod::ApiKey],
                jwt: JwtConfig {
                    secret_key: "dev_secret_key_change_in_production".to_string(),
                    expiration_seconds: 3600,
                    issuer: "catalyst-devnet".to_string(),
                    audience: "catalyst-clients".to_string(),
                    algorithm: JwtAlgorithm::HS256,
                },
                api_keys: ApiKeyConfig {
                    key_length: 32,
                    expiration_seconds: None,
                    rotation_enabled: false,
                    storage_backend: "memory".to_string(),
                },
                oauth: None,
                sessions: SessionConfig {
                    timeout_seconds: 3600,
                    storage: SessionStorage::Memory,
                    secure_cookies: false,
                    same_site_policy: SameSitePolicy::Lax,
                },
            },
            rate_limiting: RateLimitingConfig {
                enabled: false,
                global_limits: GlobalRateLimits {
                    requests_per_second: 1000,
                    burst_allowance: 100,
                    connections_per_second: 10,
                    events_per_second: 1000,
                },
                per_client_limits: PerClientRateLimits {
                    requests_per_minute: 1000,
                    events_per_minute: 1000,
                    max_concurrent_connections: 10,
                    data_transfer_per_minute: 10 * 1024 * 1024, // 10MB
                },
                algorithm: RateLimitingAlgorithm::TokenBucket,
                storage: RateLimitingStorage::Memory,
            },
            client_sdk: ClientSdkConfig {
                enabled: false,
                supported_languages: vec![SdkLanguage::JavaScript, SdkLanguage::Python],
                versioning: SdkVersioningConfig {
                    version_format: "semver".to_string(),
                    auto_generate: false,
                    compatibility_matrix: vec![],
                },
                documentation: SdkDocumentationConfig {
                    auto_generate: false,
                    format: DocumentationFormat::Markdown,
                    include_examples: true,
                    output_directory: "./docs/sdk".to_string(),
                },
                examples: SdkExamplesConfig {
                    generate_examples: false,
                    scenarios: vec![],
                    output_directory: "./examples".to_string(),
                },
            },
            performance: PerformanceConfig {
                monitoring: PerformanceMonitoringConfig {
                    enabled: false,
                    collection_interval_seconds: 60,
                    metrics: vec![
                        PerformanceMetric::ResponseTime,
                        PerformanceMetric::Throughput,
                        PerformanceMetric::ConnectionCount,
                    ],
                    alert_thresholds: AlertThresholds {
                        max_response_time_ms: 1000,
                        max_error_rate_percent: 5.0,
                        max_memory_usage_percent: 80.0,
                        max_cpu_usage_percent: 80.0,
                    },
                },
                connection_pooling: ConnectionPoolingConfig {
                    enabled: false,
                    initial_size: 5,
                    max_size: 20,
                    connection_timeout_ms: 5000,
                    idle_timeout_ms: 300000,
                },
                caching: ServiceBusCachingConfig {
                    enabled: false,
                    cache_size_bytes: 10 * 1024 * 1024, // 10MB
                    ttl_seconds: 300,
                    eviction_policy: "lru".to_string(),
                },
                load_balancing: LoadBalancingConfig {
                    enabled: false,
                    algorithm: LoadBalancingAlgorithm::RoundRobin,
                    health_check: HealthCheckConfig {
                        interval_seconds: 30,
                        timeout_ms: 5000,
                        endpoint: "/health".to_string(),
                    },
                },
            },
        }
    }
    
    /// Create test network service bus configuration
    pub fn testnet() -> Self {
        let mut config = Self::devnet();
        
        // Override testnet-specific settings
        config.websocket.max_connections = 500;
        config.websocket.compression.enabled = true;
        
        config.event_processing.buffer.size = 5000;
        config.event_processing.filtering.max_filters_per_client = 20;
        config.event_processing.persistence.enabled = true;
        config.event_processing.persistence.retention_hours = 72;
        
        config.auth.enabled = true;
        config.rate_limiting.enabled = true;
        
        config.performance.monitoring.enabled = true;
        config.performance.caching.enabled = true;
        
        config
    }
    
    /// Create main network service bus configuration (production)
    pub fn mainnet() -> Self {
        let mut config = Self::testnet();
        
        // Override mainnet-specific settings
        config.websocket.max_connections = 2000;
        config.websocket.tls = Some(TlsConfig {
            enabled: true,
            cert_file: "/etc/catalyst/tls/cert.pem".to_string(),
            key_file: "/etc/catalyst/tls/key.pem".to_string(),
            ca_file: Some("/etc/catalyst/tls/ca.pem".to_string()),
            min_version: TlsVersion::TlsV1_3,
        });
        
        config.rest_api.tls = config.websocket.tls.clone();
        config.rest_api.cors_origins = vec![
            "https://app.catalyst.network".to_string(),
            "https://dashboard.catalyst.network".to_string(),
        ];
        
        config.event_processing.buffer.size = 20000;
        config.event_processing.persistence.retention_hours = 168; // 7 days
        config.event_processing.persistence.compression_enabled = true;
        
        config.auth.jwt.secret_key = "CHANGE_THIS_IN_PRODUCTION".to_string();
        config.auth.sessions.secure_cookies = true;
        
        config.rate_limiting.global_limits.requests_per_second = 10000;
        config.rate_limiting.per_client_limits.requests_per_minute = 10000;
        
        config.client_sdk.enabled = true;
        config.client_sdk.supported_languages = vec![
            SdkLanguage::JavaScript,
            SdkLanguage::Python,
            SdkLanguage::Go,
            SdkLanguage::Rust,
        ];
        
        config.performance.connection_pooling.enabled = true;
        config.performance.load_balancing.enabled = true;
        
        config
    }
    
    /// Validate service bus configuration
    pub fn validate(&self) -> ConfigResult<()> {
        if !self.enabled {
            return Ok(()); // Skip validation if disabled
        }
        
        // Validate ports
        if self.websocket.port == 0 {
            return Err(ConfigError::ValidationFailed("WebSocket port cannot be 0".to_string()));
        }
        
        if self.rest_api.port == 0 {
            return Err(ConfigError::ValidationFailed("REST API port cannot be 0".to_string()));
        }
        
        if self.websocket.port == self.rest_api.port {
            return Err(ConfigError::ValidationFailed("WebSocket and REST API ports cannot be the same".to_string()));
        }
        
        // Validate connection limits
        if self.websocket.max_connections == 0 {
            return Err(ConfigError::ValidationFailed("Max WebSocket connections must be greater than 0".to_string()));
        }
        
        // Validate buffer configuration
        if self.event_processing.buffer.size == 0 {
            return Err(ConfigError::ValidationFailed("Event buffer size must be greater than 0".to_string()));
        }
        
        if self.event_processing.buffer.max_batch_size > self.event_processing.buffer.size {
            return Err(ConfigError::ValidationFailed("Max batch size cannot exceed buffer size".to_string()));
        }
        
        // Validate compression settings
        if self.websocket.compression.enabled {
            if self.websocket.compression.level == 0 || self.websocket.compression.level > 9 {
                return Err(ConfigError::ValidationFailed("Compression level must be between 1 and 9".to_string()));
            }
        }
        
        // Validate TLS configuration
        if let Some(ref tls) = self.websocket.tls {
            if tls.enabled {
                if tls.cert_file.is_empty() {
                    return Err(ConfigError::ValidationFailed("TLS certificate file path cannot be empty".to_string()));
                }
                if tls.key_file.is_empty() {
                    return Err(ConfigError::ValidationFailed("TLS key file path cannot be empty".to_string()));
                }
            }
        }
        
        // Validate authentication settings
        if self.auth.enabled {
            if self.auth.methods.is_empty() {
                return Err(ConfigError::ValidationFailed("At least one authentication method must be specified".to_string()));
            }
            
            if self.auth.jwt.secret_key.is_empty() {
                return Err(ConfigError::ValidationFailed("JWT secret key cannot be empty".to_string()));
            }
            
            if self.auth.jwt.expiration_seconds == 0 {
                return Err(ConfigError::ValidationFailed("JWT expiration time must be greater than 0".to_string()));
            }
        }
        
        // Validate rate limiting
        if self.rate_limiting.enabled {
            if self.rate_limiting.global_limits.requests_per_second == 0 {
                return Err(ConfigError::ValidationFailed("Global rate limit must be greater than 0".to_string()));
            }
            
            if self.rate_limiting.per_client_limits.requests_per_minute == 0 {
                return Err(ConfigError::ValidationFailed("Per-client rate limit must be greater than 0".to_string()));
            }
        }
        
        Ok(())
    }
}