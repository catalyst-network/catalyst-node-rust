# Catalyst Utils

Core utility functions and foundational types for the Catalyst Network blockchain protocol.

## Overview

`catalyst-utils` provides the essential building blocks used across all Catalyst Network components, including error handling, logging, metrics, serialization, state management, and networking primitives.

## Features

- **🔧 Core Utilities**: Common functions for timestamps, hex conversion, byte formatting, and rate limiting
- **❌ Error Handling**: Comprehensive error types with structured error propagation
- **📝 Structured Logging**: Rich logging system with Catalyst-specific categories and fields
- **📊 Metrics Collection**: Built-in performance monitoring and analytics framework
- **🔄 Serialization**: Protocol-aware serialization for network messages and state
- **💾 State Management**: Abstractions for ledger state and account management
- **🌐 Network Primitives**: Core traits and types for P2P communication

## Quick Start

Add this to your `Cargo.toml`:

```toml
[dependencies]
catalyst-utils = "0.1.0"
```

### Basic Usage

```rust
use catalyst_utils::*;
use catalyst_utils::utils;

// Generate random bytes
let key_material = utils::random_bytes(32);

// Format bytes for display
let size_str = utils::format_bytes(1048576); // "1.00 MB"

// Create structured errors
let error = crypto_error!("Invalid key length: {}", key_material.len());

// Get current timestamp
let now = utils::current_timestamp_ms();
```

### Error Handling

```rust
use catalyst_utils::{CatalystResult, CatalystError};

fn process_transaction() -> CatalystResult<()> {
    // Your transaction processing logic
    if invalid_signature {
        return Err(crypto_error!("Signature verification failed"));
    }
    
    Ok(())
}
```

### Logging

```rust
use catalyst_utils::logging::*;

// Initialize the logging system
let config = LogConfig {
    min_level: LogLevel::Info,
    json_format: true,
    console_output: true,
    ..Default::default()
};
init_logger(config)?;

// Use structured logging
log_info!(LogCategory::Network, "Node connected: {}", peer_id);

// Log with structured fields
log_with_fields!(
    LogLevel::Info,
    LogCategory::Transaction,
    "Transaction processed",
    "tx_id" => "0x123...",
    "amount" => 1000u64,
    "valid" => true
);
```

### Metrics

```rust
use catalyst_utils::metrics::*;

// Initialize metrics
init_metrics()?;

// Use convenient macros
increment_counter!("transactions_processed_total", 1);
set_gauge!("mempool_size", current_size as f64);

// Time operations automatically
let result = time_operation!("consensus_phase_duration", {
    run_construction_phase()
});

// Observe distributions
observe_histogram!("transaction_size_bytes", tx_size as f64);
```

### Serialization

```rust
use catalyst_utils::serialization::*;

// Basic serialization
let data = 42u32;
let bytes = data.serialize()?;
let recovered = u32::deserialize(&bytes)?;

// For custom structs
#[derive(Debug, PartialEq)]
struct TransactionEntry {
    public_key: Vec<u8>,
    amount: u64,
}

impl_catalyst_serialize!(TransactionEntry, public_key, amount);

// Use with context for protocol versioning
let ctx = SerializationContext {
    protocol_version: 1,
    network_id: 0,
    compressed: true,
};
```

### State Management

```rust
use catalyst_utils::state::{StateManager, AccountState};

// Create account states
let account = AccountState::new_non_confidential([1u8; 21], 1000);
let balance = account.get_balance_as_u64(); // Some(1000)

// For confidential accounts
let commitment = [2u8; 32];
let conf_account = AccountState::new_confidential([1u8; 21], commitment);
```

### Network Messages

```rust
use catalyst_utils::network::*;

// Implement NetworkMessage for your types
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MyMessage {
    content: String,
}

impl NetworkMessage for MyMessage {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|e| CatalystError::Serialization(format!("Failed: {}", e)))
    }
    
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        serde_json::from_slice(data)
            .map_err(|e| CatalystError::Serialization(format!("Failed: {}", e)))
    }
    
    fn message_type(&self) -> MessageType {
        MessageType::Custom(1)
    }
}

// Create message envelopes
let envelope = MessageEnvelope::from_message(
    &my_message,
    "sender_node".to_string(),
    Some("target_node".to_string()),
)?;
```

## Architecture

### Error Types

The crate provides a hierarchical error system:

- `CatalystError`: Top-level error enum for all Catalyst operations
- `UtilError`: Utility-specific errors (IO, parsing, timeouts)
- Convenient macros: `crypto_error!`, `network_error!`, `consensus_error!`, etc.

### Logging Categories

Structured logging with Catalyst-specific categories:

- **Network**: P2P communications, gossip protocol
- **Consensus**: 4-phase consensus process tracking
- **Transaction**: Transaction processing and validation
- **Storage**: Distributed file system and state operations
- **Crypto**: Cryptographic operations
- **Runtime**: EVM and smart contract execution
- **ServiceBus**: Web2 integration layer
- **NodeManagement**: Worker pools and producer selection
- **Config**: Configuration and startup
- **Metrics**: Performance monitoring
- **System**: General system operations

### Metric Types

Built-in support for various metric types:

- **Counters**: Cumulative values (transactions processed, messages sent)
- **Gauges**: Current values (memory usage, connected peers)
- **Histograms**: Value distributions (latency, transaction sizes)
- **Timers**: Duration measurements (consensus phases, validation time)
- **Rate Meters**: Events per time unit (transactions per second)

### Standard Catalyst Metrics

Pre-defined metrics for the Catalyst Network:

```rust
// Consensus metrics
"consensus_cycles_total"          // Counter
"consensus_phase_duration"        // Timer
"consensus_producers_count"       // Gauge

// Transaction metrics  
"transactions_processed_total"    // Counter
"transaction_validation_duration" // Timer
"mempool_size"                   // Gauge

// Network metrics
"network_peers_connected"        // Gauge
"network_messages_sent_total"    // Counter
"network_message_latency"        // Histogram

// Storage metrics
"storage_operations_total"       // Counter
"storage_operation_duration"     // Timer

// Crypto metrics
"crypto_operations_total"        // Counter
"signature_verification_duration" // Timer

// System metrics
"system_memory_usage"           // Gauge
"system_cpu_usage"             // Gauge
```

## Advanced Usage

### Custom Serialization with Context

```rust
use catalyst_utils::serialization::*;

struct ProtocolMessage {
    version: u32,
    data: Vec<u8>,
}

impl CatalystSerializeWithContext for ProtocolMessage {
    fn serialize_with_context(&self, ctx: &SerializationContext) -> CatalystResult<Vec<u8>> {
        let mut buffer = Vec::new();
        
        // Use context for protocol-specific encoding
        if ctx.protocol_version >= 2 {
            // New format with compression
            if ctx.compressed {
                buffer = compress_data(&self.data)?;
            }
        }
        
        Ok(buffer)
    }
    
    fn deserialize_with_context(data: &[u8], ctx: &SerializationContext) -> CatalystResult<Self> {
        // Context-aware deserialization
        todo!("Implement based on protocol version")
    }
}
```

### Custom Metric Collection

```rust
use catalyst_utils::metrics::*;

// Register custom metrics
let mut registry = MetricsRegistry::new();
registry.set_node_id("producer_node_1".to_string());

let custom_metric = Metric::new(
    "custom_operation_duration".to_string(),
    MetricCategory::Application,
    MetricType::Timer,
    "Duration of custom operations".to_string(),
);

registry.register_metric(custom_metric)?;

// Use the registry
registry.observe_timer("custom_operation_duration", 0.150)?; // 150ms
```

### Advanced Logging Patterns

```rust
use catalyst_utils::logging::patterns::*;

// Consensus-specific logging
log_consensus_phase(
    &logger,
    "Construction",
    42,
    "node_producer_1",
    Some(additional_fields),
)?;

// Transaction logging
log_transaction(
    &logger,
    "tx_12345",
    "validated",
    Some(1000),
)?;

// Network event logging
log_network_event(
    &logger,
    "peer_connected",
    Some("peer_abc123"),
    Some(details),
)?;
```

## Performance Considerations

### Memory Usage

- Rate limiters use sliding windows with configurable sizes
- Histograms maintain sample limits (default: 1000 samples)
- Metrics registry exports periodically to prevent memory growth
- Serialization includes size limits to prevent DoS attacks

### Network Efficiency

- Little-endian encoding for consistency across platforms
- Length-prefixed encoding prevents buffer overruns
- Message envelopes include TTL and routing information
- Protocol versioning supports backward compatibility

### Consensus Optimization

- Specialized error types for each consensus phase
- Structured logging enables performance analysis
- Metrics collection with minimal overhead
- State management designed for frequent updates

## Contributing

This crate serves as the foundation for all Catalyst Network components. When adding new utilities:

1. **Error Types**: Add new error variants to `CatalystError` with descriptive messages
2. **Logging**: Use appropriate categories and include relevant structured fields  
3. **Metrics**: Define clear metric names following the naming convention
4. **Serialization**: Ensure compatibility with existing protocol versions
5. **Documentation**: Include comprehensive examples and performance notes

## Testing

Run the comprehensive test suite:

```bash
# All tests
cargo test

# Integration tests only
cargo test --test comprehensive_utils_tests

# With verbose output
cargo test --verbose

# Test coverage (requires cargo-tarpaulin)
cargo tarpaulin --verbose --all-features --workspace --timeout 120
```

## Examples

See the `examples/` directory for complete usage examples:

- `basic_usage.rs`: Core utilities and error handling
- `logging_example.rs`: Structured logging setup and usage
- `metrics_example.rs`: Metrics collection and export
- `serialization_example.rs`: Custom serialization implementation
- `consensus_integration.rs`: Integration with consensus components

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.

## Related Crates

- `catalyst-crypto`: Cryptographic primitives
- `catalyst-consensus`: Collaborative consensus implementation  
- `catalyst-network`: P2P networking and message handling
- `catalyst-storage`: State management and distributed file system
- `catalyst-runtime-evm`: EVM execution environment
- `catalyst-service-bus`: Web2 integration layer