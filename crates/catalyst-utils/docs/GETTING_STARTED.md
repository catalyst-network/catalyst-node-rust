# Getting Started with Catalyst Utils

This guide will help you quickly get up and running with the `catalyst-utils` crate.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
catalyst-utils = "0.1.0"
```

## Quick Start

### 1. Basic Usage

```rust
use catalyst_utils::*;
use catalyst_utils::utils;

fn main() -> CatalystResult<()> {
    // Generate random data
    let key = utils::random_bytes(32);
    println!("Generated key: {}", utils::bytes_to_hex(&key));
    
    // Format file sizes
    let size = utils::format_bytes(1048576);
    println!("File size: {}", size); // "1.00 MB"
    
    // Get timestamps
    let now = utils::current_timestamp_ms();
    println!("Current time: {}", now);
    
    Ok(())
}
```

### 2. Error Handling

```rust
use catalyst_utils::*;

fn validate_transaction(data: &[u8]) -> CatalystResult<()> {
    if data.is_empty() {
        return Err(CatalystError::Invalid("Empty transaction data".to_string()));
    }
    
    if data.len() > 1024 {
        return Err(CatalystError::Invalid(
            format!("Transaction too large: {} bytes", data.len())
        ));
    }
    
    // Use convenience macros
    if !verify_signature(data) {
        return Err(crypto_error!("Invalid signature"));
    }
    
    Ok(())
}

fn verify_signature(_data: &[u8]) -> bool {
    // Mock implementation
    true
}
```

### 3. Logging Setup

```rust
use catalyst_utils::logging::*;

#[tokio::main]
async fn main() -> CatalystResult<()> {
    // Initialize logging
    let config = LogConfig {
        min_level: LogLevel::Info,
        json_format: false,
        console_output: true,
        ..Default::default()
    };
    
    init_logger(config)?;
    set_node_id("my_node_1".to_string());
    
    // Basic logging
    log_info!(LogCategory::System, "Application started");
    log_warn!(LogCategory::Network, "High latency detected: {}ms", 150);
    
    // Structured logging
    log_with_fields!(
        LogLevel::Info,
        LogCategory::Transaction,
        "Transaction processed",
        "tx_id" => "0x123...",
        "amount" => 1000u64,
        "success" => true
    );
    
    Ok(())
}
```

### 4. Metrics Collection

```rust
use catalyst_utils::metrics::*;

#[tokio::main]
async fn main() -> CatalystResult<()> {
    // Initialize metrics
    init_metrics()?;
    
    // Use convenient macros
    increment_counter!("requests_total", 1);
    set_gauge!("active_connections", 42.0);
    observe_histogram!("request_duration_ms", 125.0);
    
    // Time operations automatically
    let result = time_operation!("database_query", {
        // Your database operation here
        std::thread::sleep(std::time::Duration::from_millis(50));
        "query_result"
    });
    
    println!("Query result: {}", result);
    
    Ok(())
}
```

### 5. Serialization

```rust
use catalyst_utils::serialization::*;

#[derive(Debug, PartialEq)]
struct MyData {
    id: u32,
    name: String,
    values: Vec<u8>,
}

// Implement serialization with the macro
impl_catalyst_serialize!(MyData, id, name, values);

fn main() -> CatalystResult<()> {
    let data = MyData {
        id: 42,
        name: "test".to_string(),
        values: vec![1, 2, 3, 4],
    };
    
    // Serialize
    let bytes = data.serialize()?;
    println!("Serialized {} bytes", bytes.len());
    
    // Deserialize
    let recovered = MyData::deserialize(&bytes)?;
    assert_eq!(data, recovered);
    println!("Round-trip successful!");
    
    Ok(())
}
```

## Common Patterns

### 1. Error Propagation

```rust
use catalyst_utils::*;

fn process_data() -> CatalystResult<String> {
    let data = load_data()?;  // CatalystResult<Vec<u8>>
    let parsed = parse_data(&data)?;  // CatalystResult<ParsedData>
    let result = transform(parsed)?;  // CatalystResult<String>
    Ok(result)
}

fn load_data() -> CatalystResult<Vec<u8>> {
    // Simulate file loading that might fail
    std::fs::read("data.bin")
        .map_err(|e| CatalystError::Storage(format!("Failed to load file: {}", e)))
}

fn parse_data(_data: &[u8]) -> CatalystResult<String> {
    // Simulate parsing that might fail
    Ok("parsed_data".to_string())
}

fn transform(_parsed: String) -> CatalystResult<String> {
    Ok("transformed_result".to_string())
}
```

### 2. Structured Logging in Components

```rust
use catalyst_utils::logging::*;

struct ConsensusEngine {
    node_id: String,
    cycle: u64,
}

impl ConsensusEngine {
    pub fn new(node_id: String) -> Self {
        Self { node_id, cycle: 0 }
    }
    
    pub async fn run_consensus_cycle(&mut self) -> CatalystResult<()> {
        self.cycle += 1;
        
        log_with_fields!(
            LogLevel::Info,
            LogCategory::Consensus,
            "Starting consensus cycle",
            "node_id" => self.node_id.clone(),
            "cycle" => self.cycle
        );
        
        // Construction phase
        self.log_phase_start("construction").await?;
        self.construction_phase().await?;
        self.log_phase_complete("construction").await?;
        
        // More phases...
        
        Ok(())
    }
    
    async fn log_phase_start(&self, phase: &str) -> CatalystResult<()> {
        log_with_fields!(
            LogLevel::Debug,
            LogCategory::Consensus,
            "Phase starting",
            "phase" => phase,
            "cycle" => self.cycle,
            "node_id" => self.node_id.clone()
        );
        Ok(())
    }
    
    async fn log_phase_complete(&self, phase: &str) -> CatalystResult<()> {
        log_with_fields!(
            LogLevel::Debug,
            LogCategory::Consensus,
            "Phase completed",
            "phase" => phase,
            "cycle" => self.cycle,
            "node_id" => self.node_id.clone()
        );
        Ok(())
    }
    
    async fn construction_phase(&self) -> CatalystResult<()> {
        // Simulate construction work
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        Ok(())
    }
}
```

### 3. Metrics in Network Components

```rust
use catalyst_utils::metrics::*;
use catalyst_utils::network::*;

struct NetworkManager {
    peer_count: usize,
    message_stats: std::collections::HashMap<MessageType, u64>,
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            peer_count: 0,
            message_stats: std::collections::HashMap::new(),
        }
    }
    
    pub async fn handle_peer_connection(&mut self, peer_id: &str) -> CatalystResult<()> {
        self.peer_count += 1;
        
        // Update metrics
        increment_counter!("network_connections_total", 1);
        set_gauge!("network_peers_connected", self.peer_count as f64);
        
        log_info!(LogCategory::Network, "Peer connected: {}", peer_id);
        
        Ok(())
    }
    
    pub async fn send_message<T: NetworkMessage>(&mut self, message: T, target: &str) -> CatalystResult<()> {
        let message_type = message.message_type();
        
        // Time the serialization
        let envelope = time_operation!("message_serialization", {
            MessageEnvelope::from_message(&message, "local_node".to_string(), Some(target.to_string()))
        })?;
        
        // Update stats
        increment_counter!("network_messages_sent_total", 1);
        *self.message_stats.entry(message_type).or_insert(0) += 1;
        
        // Observe message size
        observe_histogram!("network_message_size_bytes", envelope.payload.len() as f64);
        
        // Simulate network latency
        let latency = 0.015 + (rand::random::<f64>() * 0.020); // 15-35ms
        observe_histogram!("network_message_latency", latency);
        
        log_with_fields!(
            LogLevel::Debug,
            LogCategory::Network,
            "Message sent",
            "message_type" => message_type.to_string(),
            "target" => target,
            "size_bytes" => envelope.payload.len() as u64,
            "latency_ms" => (latency * 1000.0) as u64
        );
        
        Ok(())
    }
}

// Example message implementation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PingMessage {
    timestamp: u64,
    node_id: String,
}

impl NetworkMessage for PingMessage {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|e| CatalystError::Serialization(format!("Failed to serialize ping: {}", e)))
    }
    
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        serde_json::from_slice(data)
            .map_err(|e| CatalystError::Serialization(format!("Failed to deserialize ping: {}", e)))
    }
    
    fn message_type(&self) -> MessageType {
        MessageType::PeerHeartbeat
    }
}
```

### 4. Custom Metrics Collection

```rust
use catalyst_utils::metrics::*;

struct TransactionProcessor {
    registry: std::sync::Arc<std::sync::Mutex<MetricsRegistry>>,
}

impl TransactionProcessor {
    pub fn new() -> CatalystResult<Self> {
        let registry = get_metrics_registry()
            .ok_or_else(|| CatalystError::Internal("Metrics not initialized".to_string()))?;
        
        // Register custom metrics
        {
            let mut reg = registry.lock().unwrap();
            
            let validation_metric = Metric::new(
                "transaction_validation_steps".to_string(),
                MetricCategory::Transaction,
                MetricType::Histogram,
                "Number of validation steps per transaction".to_string(),
            );
            reg.register_metric(validation_metric)?;
            
            let error_metric = Metric::new(
                "transaction_validation_errors_total".to_string(),
                MetricCategory::Transaction,
                MetricType::Counter,
                "Total validation errors".to_string(),
            );
            reg.register_metric(error_metric)?;
        }
        
        Ok(Self { registry })
    }
    
    pub fn process_transaction(&self, tx_data: &[u8]) -> CatalystResult<String> {
        let start = std::time::Instant::now();
        let mut validation_steps = 0;
        
        // Step 1: Parse transaction
        validation_steps += 1;
        let _tx = self.parse_transaction(tx_data)?;
        
        // Step 2: Validate signature
        validation_steps += 1;
        if !self.validate_signature(&_tx) {
            increment_counter!("transaction_validation_errors_total", 1);
            return Err(crypto_error!("Invalid signature"));
        }
        
        // Step 3: Check balance
        validation_steps += 1;
        if !self.check_balance(&_tx) {
            increment_counter!("transaction_validation_errors_total", 1);
            return Err(CatalystError::Invalid("Insufficient balance".to_string()));
        }
        
        // Record metrics
        let duration = start.elapsed();
        observe_histogram!("transaction_validation_steps", validation_steps as f64);
        
        {
            let mut reg = self.registry.lock().unwrap();
            let _ = reg.observe_timer("transaction_processing_duration", duration.as_secs_f64());
        }
        
        increment_counter!("transactions_processed_total", 1);
        
        Ok("tx_success".to_string())
    }
    
    fn parse_transaction(&self, _data: &[u8]) -> CatalystResult<String> {
        // Mock parsing
        Ok("parsed_tx".to_string())
    }
    
    fn validate_signature(&self, _tx: &str) -> bool {
        // Mock validation
        rand::random::<f64>() > 0.1 // 90% success rate
    }
    
    fn check_balance(&self, _tx: &str) -> bool {
        // Mock balance check
        rand::random::<f64>() > 0.05 // 95% success rate
    }
}
```

## Testing Your Code

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use catalyst_utils::*;
    
    #[test]
    fn test_error_creation() {
        let error = crypto_error!("Test error: {}", 42);
        match error {
            CatalystError::Crypto(msg) => assert_eq!(msg, "Test error: 42"),
            _ => panic!("Wrong error type"),
        }
    }
    
    #[test]
    fn test_serialization_round_trip() {
        let original = "Hello, Catalyst!".to_string();
        let serialized = original.serialize().unwrap();
        let deserialized = String::deserialize(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }
    
    #[test]
    fn test_hex_conversion() {
        let bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let hex = utils::bytes_to_hex(&bytes);
        assert_eq!(hex, "deadbeef");
        
        let decoded = utils::hex_to_bytes(&hex).unwrap();
        assert_eq!(decoded, bytes);
    }
    
    #[tokio::test]
    async fn test_logging_integration() {
        let config = LogConfig {
            min_level: LogLevel::Debug,
            console_output: false, // Disable for tests
            ..Default::default()
        };
        
        // Note: In real tests, you might want to capture log output
        init_logger(config).ok(); // May already be initialized
        
        log_info!(LogCategory::System, "Test log message");
        // Test passes if no panic occurs
    }
    
    #[test]
    fn test_metrics_integration() {
        init_metrics().ok(); // May already be initialized
        
        increment_counter!("test_counter", 5);
        set_gauge!("test_gauge", 42.0);
        observe_histogram!("test_histogram", 1.5);
        
        // Test passes if no panic occurs
        // In real applications, you'd verify the metric values
    }
}
```

### Integration Tests

Create `tests/integration_test.rs`:

```rust
use catalyst_utils::*;
use catalyst_utils::{logging::*, metrics::*};

#[tokio::test]
async fn test_full_integration() {
    // Initialize all systems
    let log_config = LogConfig {
        min_level: LogLevel::Info,
        console_output: false,
        ..Default::default()
    };
    init_logger(log_config).ok();
    init_metrics().ok();
    
    set_node_id("test_node".to_string());
    
    // Simulate application workflow
    log_info!(LogCategory::System, "Starting integration test");
    
    for i in 1..=10 {
        increment_counter!("test_operations_total", 1);
        
        let result = time_operation!("test_operation_duration", {
            // Simulate work
            std::thread::sleep(std::time::Duration::from_millis(10));
            format!("result_{}", i)
        });
        
        log_with_fields!(
            LogLevel::Debug,
            LogCategory::Application,
            "Operation completed",
            "operation_id" => i,
            "result" => result
        );
        
        observe_histogram!("operation_sequence", i as f64);
    }
    
    set_gauge!("final_state", 100.0);
    log_info!(LogCategory::System, "Integration test completed");
}
```

## Performance Tips

### 1. Logging Performance

```rust
// ❌ Avoid expensive operations in log messages
log_debug!(LogCategory::Network, "Data: {:?}", expensive_computation());

// ✅ Use lazy evaluation or conditional logging
if log::log_enabled!(log::Level::Debug) {
    let data = expensive_computation();
    log_debug!(LogCategory::Network, "Data: {:?}", data);
}

// ✅ Or use structured fields to avoid formatting when not needed
log_with_fields!(
    LogLevel::Debug,
    LogCategory::Network,
    "Expensive data computed",
    "data" => expensive_computation().into()
);
```

### 2. Metrics Performance

```rust
// ✅ Metrics macros are designed for high-frequency use
for _ in 0..1000 {
    increment_counter!("high_frequency_counter", 1);
}

// ✅ Batch histogram observations when possible
let measurements: Vec<f64> = collect_measurements();
for measurement in measurements {
    observe_histogram!("batch_measurements", measurement);
}
```

### 3. Serialization Performance

```rust
// ✅ Pre-allocate buffers when size is known
let mut buffer = Vec::with_capacity(data.serialized_size());
data.serialize_to(&mut buffer)?;

// ✅ Use streaming serialization for large data
use std::io::Cursor;
let mut writer = Cursor::new(Vec::new());
large_data.serialize_to(&mut writer)?;
```

## Troubleshooting

### Common Issues

#### 1. "Metrics registry not initialized"

```rust
// ❌ Using metrics before initialization
increment_counter!("my_counter", 1); // Fails silently

// ✅ Initialize first
init_metrics()?;
increment_counter!("my_counter", 1); // Works
```

#### 2. "Logger already initialized"

```rust
// ❌ Multiple initializations
init_logger(config1)?;
init_logger(config2)?; // Error!

// ✅ Initialize once, or handle the error
init_logger(config).ok(); // Ignore if already initialized
```

#### 3. Serialization size limits

```rust
// ❌ Exceeding built-in limits
let huge_data = vec![0u8; 200_000_000]; // > 100MB limit
let result = huge_data.serialize(); // Error!

// ✅ Check sizes before serialization
if data.len() <= 100_000_000 {
    let result = data.serialize()?;
}
```

### Debugging Tips

#### 1. Enable debug logging

```rust
let config = LogConfig {
    min_level: LogLevel::Trace, // See everything
    include_source: true,       // Include file:line info
    ..Default::default()
};
```

#### 2. Monitor metrics export

```rust
// Force metrics export for debugging
if let Some(registry) = get_metrics_registry() {
    let mut reg = registry.lock().unwrap();
    reg.export_metrics()?;
}
```

#### 3. Validate serialization

```rust
// Test round-trip serialization
let original = my_data;
let serialized = original.serialize()?;
let recovered = MyData::deserialize(&serialized)?;
assert_eq!(original, recovered);
```

## Next Steps

1. **Explore Examples**: Check the `examples/` directory for comprehensive usage examples
2. **Read API Documentation**: See `docs/API.md` for detailed API reference
3. **Integration**: Start integrating with other Catalyst crates:
   - `catalyst-crypto` for cryptographic operations
   - `catalyst-network` for P2P networking
   - `catalyst-consensus` for consensus protocol
   - `catalyst-storage` for state management

4. **Monitoring**: Set up log aggregation and metrics dashboards for production use
5. **Testing**: Write comprehensive tests using the patterns shown above

## Getting Help

- **Documentation**: Check the API documentation in `docs/API.md`
- **Examples**: Review example code in the `examples/` directory
- **Issues**: Open issues on the GitHub repository
- **Community**: Join the Catalyst Network Discord for discussions

Happy coding with Catalyst Utils! 🚀