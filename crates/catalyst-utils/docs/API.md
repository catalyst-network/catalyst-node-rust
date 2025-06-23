# Catalyst Utils API Documentation

This document provides comprehensive API documentation for the `catalyst-utils` crate.

## Table of Contents

1. [Error Handling](#error-handling)
2. [Logging System](#logging-system)
3. [Metrics Collection](#metrics-collection)
4. [Serialization](#serialization)
5. [State Management](#state-management)
6. [Network Primitives](#network-primitives)
7. [Utility Functions](#utility-functions)

## Error Handling

### Core Types

#### `CatalystError`

The primary error type used throughout the Catalyst Network.

```rust
pub enum CatalystError {
    Crypto(String),
    Network(String),
    Storage(String),
    Consensus { phase: String, message: String },
    Runtime(String),
    Config(String),
    Serialization(String),
    Invalid(String),
    NotFound(String),
    Timeout { duration_ms: u64, operation: String },
    Internal(String),
    Util(UtilError),
}
```

#### `CatalystResult<T>`

Standard result type: `Result<T, CatalystError>`

#### `UtilError`

Utility-specific errors:

```rust
pub enum UtilError {
    Io(String),
    Parse(String),
    Timeout,
    RateLimited,
    InvalidInput(String),
}
```

### Error Creation Macros

#### `crypto_error!`

Creates cryptographic errors:

```rust
let error = crypto_error!("Invalid key length: {}", key.len());
let error = crypto_error!("Signature verification failed");
```

#### `network_error!`

Creates network-related errors:

```rust
let error = network_error!("Connection timeout to peer {}", peer_id);
```

#### `consensus_error!`

Creates consensus-specific errors:

```rust
let error = consensus_error!("voting", "Failed to reach supermajority");
let error = consensus_error!("construction", "Invalid transaction: {}", tx_id);
```

#### `storage_error!`

Creates storage-related errors:

```rust
let error = storage_error!("Failed to write state: {}", reason);
```

#### `timeout_error!`

Creates timeout errors:

```rust
let error = timeout_error!(5000, "block synchronization");
```

## Logging System

### Core Types

#### `LogLevel`

```rust
pub enum LogLevel {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Critical = 5,
}
```

#### `LogCategory`

Catalyst-specific logging categories:

```rust
pub enum LogCategory {
    Network,        // P2P communications
    Consensus,      // 4-phase consensus process
    Transaction,    // Transaction processing
    Storage,        // DFS and state operations
    Crypto,         // Cryptographic operations
    Runtime,        // EVM execution
    ServiceBus,     // Web2 integration
    NodeManagement, // Worker pools
    Config,         // Configuration
    Metrics,        // Performance monitoring
    System,         // General system operations
}
```

#### `LogEntry`

Structured log entry:

```rust
pub struct LogEntry {
    pub timestamp: u64,
    pub level: LogLevel,
    pub category: LogCategory,
    pub node_id: Option<String>,
    pub cycle: Option<u64>,
    pub message: String,
    pub fields: HashMap<String, LogValue>,
    pub source: Option<String>,
}
```

#### `LogValue`

Flexible value type for structured fields:

```rust
pub enum LogValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Bytes(Vec<u8>),
    Array(Vec<LogValue>),
}
```

### Configuration

#### `LogConfig`

```rust
pub struct LogConfig {
    pub min_level: LogLevel,
    pub include_source: bool,
    pub json_format: bool,
    pub include_timestamp: bool,
    pub max_file_size: u64,
    pub log_dir: Option<String>,
    pub console_output: bool,
    pub filtered_categories: Vec<LogCategory>,
}
```

### Core Functions

#### `init_logger(config: LogConfig) -> CatalystResult<()>`

Initialize the global logging system.

#### `set_node_id(node_id: String)`

Set the node identifier for all logs.

### Logger Methods

#### `CatalystLogger` Core Methods

```rust
impl CatalystLogger {
    pub fn new(config: LogConfig) -> Self;
    pub fn set_node_id(&mut self, node_id: String);
    pub fn set_current_cycle(&self, cycle: u64);
    pub fn clear_current_cycle(&self);
    
    // Basic logging
    pub fn trace(&self, category: LogCategory, message: &str) -> CatalystResult<()>;
    pub fn debug(&self, category: LogCategory, message: &str) -> CatalystResult<()>;
    pub fn info(&self, category: LogCategory, message: &str) -> CatalystResult<()>;
    pub fn warn(&self, category: LogCategory, message: &str) -> CatalystResult<()>;
    pub fn error(&self, category: LogCategory, message: &str) -> CatalystResult<()>;
    pub fn critical(&self, category: LogCategory, message: &str) -> CatalystResult<()>;
    
    // Structured logging
    pub fn log_with_fields(
        &self,
        level: LogLevel,
        category: LogCategory,
        message: &str,
        fields: &[(&str, LogValue)],
    ) -> CatalystResult<()>;
    
    pub fn flush(&self) -> CatalystResult<()>;
}
```

### Logging Macros

#### Basic Logging Macros

```rust
log_trace!(category, format, args...);
log_debug!(category, format, args...);
log_info!(category, format, args...);
log_warn!(category, format, args...);
log_error!(category, format, args...);
log_critical!(category, format, args...);
```

**Example:**
```rust
log_info!(LogCategory::Network, "Peer connected: {}", peer_id);
```

#### Structured Logging Macro

```rust
log_with_fields!(level, category, message, key => value, ...);
```

**Example:**
```rust
log_with_fields!(
    LogLevel::Info,
    LogCategory::Transaction,
    "Transaction processed",
    "tx_id" => "0x123...",
    "amount" => 1000u64,
    "valid" => true
);
```

### Specialized Logging Patterns

#### `patterns::log_consensus_phase`

```rust
pub fn log_consensus_phase(
    logger: &CatalystLogger,
    phase: &str,
    cycle: u64,
    node_id: &str,
    additional_fields: Option<HashMap<String, LogValue>>,
) -> CatalystResult<()>;
```

#### `patterns::log_transaction`

```rust
pub fn log_transaction(
    logger: &CatalystLogger,
    tx_id: &str,
    status: &str,
    amount: Option<u64>,
) -> CatalystResult<()>;
```

#### `patterns::log_network_event`

```rust
pub fn log_network_event(
    logger: &CatalystLogger,
    event_type: &str,
    peer_id: Option<&str>,
    details: Option<HashMap<String, LogValue>>,
) -> CatalystResult<()>;
```

## Metrics Collection

### Core Types

#### `MetricType`

```rust
pub enum MetricType {
    Counter,    // Monotonically increasing values
    Gauge,      // Current values that can increase/decrease
    Histogram,  // Distribution of values
    Timer,      // Duration measurements
    Rate,       // Events per time unit
}
```

#### `MetricCategory`

```rust
pub enum MetricCategory {
    Consensus,
    Network,
    Transaction,
    Storage,
    Crypto,
    Runtime,
    ServiceBus,
    System,
    Application,
}
```

#### `Metric`

```rust
pub struct Metric {
    pub name: String,
    pub category: MetricCategory,
    pub metric_type: MetricType,
    pub description: String,
    pub value: MetricValue,
    pub labels: HashMap<String, String>,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}
```

#### `MetricValue`

```rust
pub enum MetricValue {
    Counter(u64),
    Gauge(f64),
    Histogram(Histogram),
    Timer(Histogram),
    Rate(RateMeter),
}
```

### Core Functions

#### `init_metrics() -> CatalystResult<()>`

Initialize the global metrics registry.

#### `get_metrics_registry() -> Option<Arc<Mutex<MetricsRegistry>>>`

Get reference to the global metrics registry.

### MetricsRegistry Methods

```rust
impl MetricsRegistry {
    pub fn new() -> Self;
    pub fn set_node_id(&mut self, node_id: String);
    pub fn set_export_interval(&mut self, interval: Duration);
    
    // Metric registration
    pub fn register_metric(&mut self, metric: Metric) -> CatalystResult<()>;
    
    // Metric operations
    pub fn increment_counter(&mut self, name: &str, value: u64) -> CatalystResult<()>;
    pub fn set_gauge(&mut self, name: &str, value: f64) -> CatalystResult<()>;
    pub fn observe_histogram(&mut self, name: &str, value: f64) -> CatalystResult<()>;
    pub fn observe_timer(&mut self, name: &str, duration_seconds: f64) -> CatalystResult<()>;
    pub fn mark_rate(&mut self, name: &str, value: f64) -> CatalystResult<()>;
    
    // Timer creation
    pub fn start_timer(&self, name: &str, registry: Arc<Mutex<MetricsRegistry>>) -> Timer;
    
    // Data access
    pub fn get_metrics(&self) -> &HashMap<String, Metric>;
    pub fn export_metrics(&mut self) -> CatalystResult<()>;
}
```

### Metrics Macros

#### `increment_counter!(name, value)`

```rust
increment_counter!("transactions_processed_total", 1);
```

#### `set_gauge!(name, value)`

```rust
set_gauge!("mempool_size", current_size as f64);
```

#### `observe_histogram!(name, value)`

```rust
observe_histogram!("transaction_size_bytes", tx_size as f64);
```

#### `time_operation!(name, operation)`

```rust
let result = time_operation!("consensus_phase_duration", {
    run_construction_phase()
});
```

### Standard Catalyst Metrics

Pre-defined metrics for the Catalyst Network:

#### Consensus Metrics
- `consensus_cycles_total` (Counter)
- `consensus_phase_duration` (Timer)
- `consensus_producers_count` (Gauge)

#### Transaction Metrics
- `transactions_processed_total` (Counter)
- `transaction_validation_duration` (Timer)
- `mempool_size` (Gauge)

#### Network Metrics
- `network_peers_connected` (Gauge)
- `network_messages_sent_total` (Counter)
- `network_message_latency` (Histogram)

#### Storage Metrics
- `storage_operations_total` (Counter)
- `storage_operation_duration` (Timer)

#### Crypto Metrics
- `crypto_operations_total` (Counter)
- `signature_verification_duration` (Timer)

#### System Metrics
- `system_memory_usage` (Gauge)
- `system_cpu_usage` (Gauge)

### Histogram Analysis

```rust
impl Histogram {
    pub fn percentile(&self, p: f64) -> Option<f64>;
    pub fn mean(&self) -> f64;
    pub fn count(&self) -> u64;
    pub fn sum(&self) -> f64;
}
```

### Rate Meter

```rust
impl RateMeter {
    pub fn new(window_duration: Duration) -> Self;
    pub fn mark(&mut self, value: f64);
    pub fn rate(&mut self) -> f64;
}
```

## Serialization

### Core Traits

#### `CatalystSerialize`

```rust
pub trait CatalystSerialize {
    fn serialize(&self) -> CatalystResult<Vec<u8>>;
    fn serialize_to<W: Write>(&self, writer: &mut W) -> CatalystResult<()>;
    fn serialized_size(&self) -> usize;
}
```

#### `CatalystDeserialize`

```rust
pub trait CatalystDeserialize: Sized {
    fn deserialize(data: &[u8]) -> CatalystResult<Self>;
    fn deserialize_from<R: Read>(reader: &mut R) -> CatalystResult<Self>;
}
```

#### `CatalystCodec`

Combined trait: `CatalystSerialize + CatalystDeserialize`

#### `CatalystSerializeWithContext`

For protocol-aware serialization:

```rust
pub trait CatalystSerializeWithContext {
    fn serialize_with_context(&self, ctx: &SerializationContext) -> CatalystResult<Vec<u8>>;
    fn deserialize_with_context(data: &[u8], ctx: &SerializationContext) -> CatalystResult<Self>
    where Self: Sized;
}
```

### SerializationContext

```rust
pub struct SerializationContext {
    pub protocol_version: u32,
    pub network_id: u8,
    pub compressed: bool,
}
```

### Built-in Implementations

The following types implement `CatalystSerialize` and `CatalystDeserialize`:

- `u8`, `u32`, `u64`
- `Vec<u8>`
- `String`

### Utility Functions

#### `utils::serialize_bytes<W: Write>(writer: &mut W, data: &[u8]) -> CatalystResult<()>`

Serialize length-prefixed byte array.

#### `utils::deserialize_bytes<R: Read>(reader: &mut R) -> CatalystResult<Vec<u8>>`

Deserialize length-prefixed byte array.

#### `utils::serialize_string<W: Write>(writer: &mut W, s: &str) -> CatalystResult<()>`

Serialize length-prefixed string.

#### `utils::deserialize_string<R: Read>(reader: &mut R) -> CatalystResult<String>`

Deserialize length-prefixed string.

#### `utils::serialize_vec<W, T>(writer: &mut W, items: &[T]) -> CatalystResult<()>`

Serialize vector of serializable items.

#### `utils::deserialize_vec<R, T>(reader: &mut R) -> CatalystResult<Vec<T>>`

Deserialize vector of deserializable items.

### Implementation Macro

#### `impl_catalyst_serialize!(Type, field1, field2, ...)`

Automatically implement serialization for structs:

```rust
#[derive(Debug, PartialEq)]
struct TransactionEntry {
    public_key: Vec<u8>,
    amount: u64,
}

impl_catalyst_serialize!(TransactionEntry, public_key, amount);
```

## State Management

### Core Trait

#### `StateManager`

Async trait for managing ledger state:

```rust
#[async_trait]
pub trait StateManager: Send + Sync {
    // Basic operations
    async fn get_state(&self, key: &[u8]) -> CatalystResult<Option<Vec<u8>>>;
    async fn set_state(&self, key: &[u8], value: Vec<u8>) -> CatalystResult<()>;
    async fn delete_state(&self, key: &[u8]) -> CatalystResult<bool>;
    async fn contains_key(&self, key: &[u8]) -> CatalystResult<bool>;
    
    // Batch operations
    async fn get_many<'a>(&self, keys: impl Iterator<Item = &'a [u8]> + Send) 
        -> CatalystResult<Vec<Option<Vec<u8>>>>;
    async fn set_many<'a>(&self, pairs: impl Iterator<Item = (&'a [u8], Vec<u8>)> + Send) 
        -> CatalystResult<()>;
    
    // State management
    async fn commit(&self) -> CatalystResult<Hash>;
    async fn get_state_root(&self) -> CatalystResult<Hash>;
    
    // Transactions
    async fn begin_transaction(&self) -> CatalystResult<u64>;
    async fn commit_transaction(&self, transaction_id: u64) -> CatalystResult<()>;
    async fn rollback_transaction(&self, transaction_id: u64) -> CatalystResult<()>;
    
    // Snapshots
    async fn create_snapshot(&self) -> CatalystResult<Hash>;
    async fn restore_snapshot(&self, snapshot_id: &Hash) -> CatalystResult<()>;
}
```

### Account Types

#### `AccountType`

```rust
pub enum AccountType {
    NonConfidential,  // Visible balance
    Confidential,     // Hidden balance with commitments
    Contract,         // Smart contract account
}
```

#### `AccountState`

```rust
pub struct AccountState {
    pub address: [u8; 21],
    pub account_type: AccountType,
    pub balance: Vec<u8>,
    pub data: Option<Vec<u8>>,
    pub nonce: u64,
}
```

#### Account Creation Methods

```rust
impl AccountState {
    pub fn new_non_confidential(address: [u8; 21], balance: u64) -> Self;
    pub fn new_confidential(address: [u8; 21], commitment: [u8; 32]) -> Self;
    pub fn new_contract(address: [u8; 21], balance: u64, contract_data: Option<Vec<u8>>) -> Self;
    
    // Accessors
    pub fn get_balance_as_u64(&self) -> Option<u64>;
    pub fn get_commitment(&self) -> Option<[u8; 32]>;
}
```

### State Key Utilities

#### `state_keys` Module

```rust
pub mod state_keys {
    pub const ACCOUNT_PREFIX: &[u8] = b"acc:";
    pub const TRANSACTION_PREFIX: &[u8] = b"tx:";
    pub const METADATA_PREFIX: &[u8] = b"meta:";
    pub const PRODUCER_PREFIX: &[u8] = b"prod:";
    pub const WORKER_PREFIX: &[u8] = b"work:";
    
    pub fn account_key(address: &[u8; 21]) -> Vec<u8>;
    pub fn transaction_key(tx_hash: &[u8; 32]) -> Vec<u8>;
    pub fn metadata_key(name: &str) -> Vec<u8>;
}
```

## Network Primitives

### Core Traits

#### `NetworkMessage`

```rust
pub trait NetworkMessage: Send + Sync + Clone {
    fn serialize(&self) -> CatalystResult<Vec<u8>>;
    fn deserialize(data: &[u8]) -> CatalystResult<Self> where Self: Sized;
    fn message_type(&self) -> MessageType;
    fn protocol_version(&self) -> u32 { 1 }
    fn validate(&self) -> CatalystResult<()> { Ok(()) }
    fn priority(&self) -> u8 { MessagePriority::Normal as u8 }
    fn should_gossip(&self) -> bool { true }
    fn ttl(&self) -> u32 { 300 }
    fn estimated_size(&self) -> usize;
}
```

#### `MessageHandler`

```rust
pub trait MessageHandler: Send + Sync {
    fn handle_message(&self, envelope: MessageEnvelope) -> CatalystResult<Option<MessageEnvelope>>;
    fn supported_types(&self) -> Vec<MessageType>;
    fn can_handle(&self, message_type: MessageType) -> bool;
}
```

### Message Types

#### `MessageType`

```rust
pub enum MessageType {
    // Consensus messages
    ProducerQuantity,
    ProducerCandidate, 
    ProducerVote,
    ProducerOutput,
    ConsensusSync,
    
    // Transaction messages
    Transaction,
    TransactionBatch,
    TransactionRequest,
    
    // Network management
    PeerDiscovery,
    PeerHandshake,
    PeerHeartbeat,
    PeerDisconnect,
    
    // Storage and state
    StateRequest,
    StateResponse,
    StorageSync,
    FileRequest,
    FileResponse,
    
    // Service bus
    EventNotification,
    EventSubscription,
    EventUnsubscription,
    
    // Node management
    WorkerRegistration,
    WorkerSelection,
    ProducerSelection,
    NodeStatus,
    
    // System
    HealthCheck,
    ConfigUpdate,
    NetworkInfo,
    MetricsReport,
    ErrorResponse,
    
    // Custom
    Custom(u16),
}
```

#### Message Type Methods

```rust
impl MessageType {
    pub fn is_consensus_critical(&self) -> bool;
    pub fn is_transaction_related(&self) -> bool;
    pub fn default_priority(&self) -> MessagePriority;
}
```

#### `MessagePriority`

```rust
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}
```

### Message Envelope

#### `MessageEnvelope`

```rust
pub struct MessageEnvelope {
    pub id: String,
    pub message_type: MessageType,
    pub version: u32,
    pub sender: String,
    pub target: Option<String>,
    pub timestamp: u64,
    pub priority: u8,
    pub ttl: u32,
    pub payload: Vec<u8>,
    pub signature: Option<Vec<u8>>,
    pub routing_info: Option<RoutingInfo>,
}
```

#### Envelope Methods

```rust
impl MessageEnvelope {
    pub fn new(message_type: MessageType, sender: String, target: Option<String>, payload: Vec<u8>) -> Self;
    pub fn from_message<T: NetworkMessage>(message: &T, sender: String, target: Option<String>) -> CatalystResult<Self>;
    pub fn extract_message<T: NetworkMessage>(&self) -> CatalystResult<T>;
    pub fn is_expired(&self) -> bool;
    pub fn sign<F>(&mut self, sign_fn: F) -> CatalystResult<()>;
    pub fn verify<F>(&self, verify_fn: F) -> CatalystResult<bool>;
    pub fn with_routing_info(self, routing_info: RoutingInfo) -> Self;
}
```

### Routing Information

#### `RoutingInfo`

```rust
pub struct RoutingInfo {
    pub visited_nodes: Vec<String>,
    pub max_hops: u8,
    pub hop_count: u8,
    pub preferred_path: Option<Vec<String>>,
    pub ordered_delivery: bool,
}
```

#### Routing Methods

```rust
impl RoutingInfo {
    pub fn new(max_hops: u8) -> Self;
    pub fn add_hop(&mut self, node_id: String) -> CatalystResult<()>;
    pub fn has_visited(&self, node_id: &str) -> bool;
    pub fn can_hop(&self) -> bool;
}
```

### Message Router

#### `MessageRouter`

```rust
pub struct MessageRouter {
    handlers: HashMap<MessageType, Arc<dyn MessageHandler>>,
}

impl MessageRouter {
    pub fn new() -> Self;
    pub fn register_handler(&mut self, handler: Arc<dyn MessageHandler>);
    pub fn route_message(&self, envelope: MessageEnvelope) -> CatalystResult<Option<MessageEnvelope>>;
    pub fn registered_types(&self) -> Vec<MessageType>;
}
```

### Network Statistics

#### `NetworkStats`

```rust
pub struct NetworkStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub messages_by_type: HashMap<String, u64>,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub delivery_failures: u64,
    pub expired_messages: u64,
    pub avg_processing_time_ms: f64,
}
```

#### Stats Methods

```rust
impl NetworkStats {
    pub fn record_sent_message(&mut self, message_type: MessageType, size: usize);
    pub fn record_received_message(&mut self, message_type: MessageType, size: usize);
    pub fn record_delivery_failure(&mut self);
    pub fn record_expired_message(&mut self);
    pub fn update_processing_time(&mut self, processing_time_ms: f64);
}
```

## Utility Functions

### Time Functions

#### `utils::current_timestamp() -> u64`

Get current timestamp in seconds since Unix epoch.

#### `utils::current_timestamp_ms() -> u64`

Get current timestamp in milliseconds since Unix epoch.

### Encoding Functions

#### `utils::bytes_to_hex(bytes: &[u8]) -> String`

Convert bytes to hexadecimal string.

#### `utils::hex_to_bytes(hex: &str) -> Result<Vec<u8>, hex::FromHexError>`

Convert hexadecimal string to bytes.

### Random Generation

#### `utils::random_bytes(len: usize) -> Vec<u8>`

Generate cryptographically secure random bytes.

### Formatting

#### `utils::format_bytes(bytes: u64) -> String`

Format byte count as human-readable string (B, KB, MB, GB, TB).

### Rate Limiting

#### `utils::RateLimiter`

```rust
pub struct RateLimiter {
    // Private fields
}

impl RateLimiter {
    pub fn new(max_tokens: u32, refill_rate: u32) -> Self;
    pub fn try_acquire(&mut self, tokens: u32) -> bool;
}
```

## Type Aliases

### Common Catalyst Types

```rust
pub type Hash = [u8; 32];        // 32-byte hash
pub type Address = [u8; 21];     // 21-byte address (1 prefix + 20 hash)
pub type PublicKey = [u8; 32];   // 32-byte public key
pub type Signature = [u8; 64];   // 64-byte signature
```

### Result Types

```rust
pub type CatalystResult<T> = Result<T, CatalystError>;
```

## System Information

### `SystemInfo`

```rust
pub struct SystemInfo {
    pub version: String,
    pub build_timestamp: u64,
    pub target_triple: String,
    pub rust_version: String,
    pub git_commit: Option<String>,
}

impl SystemInfo {
    pub fn new() -> Self;
}
```

## Thread Safety

All public APIs in `catalyst-utils` are designed to be thread-safe:

- **Error types**: `Send + Sync`
- **Logging**: Thread-safe global logger with Arc/Mutex protection
- **Metrics**: Thread-safe registry with Arc/Mutex protection
- **Serialization**: Stateless traits, inherently thread-safe
- **State management**: Async trait requiring `Send + Sync`
- **Network types**: All implement `Send + Sync`

## Memory Management

### Performance Considerations

- **Zero-copy where possible**: Serialization traits minimize copying
- **Bounded collections**: Rate limiters and histograms have size limits
- **Efficient encoding**: Little-endian, length-prefixed formats
- **Resource cleanup**: RAII patterns for timers and loggers

### Memory Limits

- **Serialization**: 100MB limit for individual data items
- **Collections**: 1M item limit for vectors
- **Histograms**: 1000 sample limit (configurable)
- **Rate limiters**: Sliding window with automatic cleanup

## Error Handling Patterns

### Best Practices

1. **Use specific error types**: Choose the most appropriate `CatalystError` variant
2. **Include context**: Use formatting macros to include relevant details
3. **Chain errors**: Convert from utility errors to Catalyst errors
4. **Handle at appropriate level**: Don't catch and ignore errors unnecessarily

### Example Error Handling

```rust
fn process_transaction(tx_data: &[u8]) -> CatalystResult<String> {
    // Parse transaction
    let tx = Transaction::deserialize(tx_data)
        .map_err(|_| CatalystError::Invalid("Invalid transaction format".to_string()))?;
    
    // Validate signature
    if !verify_signature(&tx) {
        return Err(crypto_error!("Invalid signature for transaction {}", tx.id));
    }
    
    // Process transaction
    match process(&tx) {
        Ok(result) => Ok(result),
        Err(e) => Err(CatalystError::Runtime(format!("Processing failed: {}", e))),
    }
}
```