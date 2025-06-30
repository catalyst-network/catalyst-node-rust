# Catalyst Network

P2P networking layer for the Catalyst Network blockchain platform.

## Overview

The `catalyst-network` crate provides a comprehensive libp2p-based networking solution for Catalyst Network nodes. It implements the networking requirements specified in the Catalyst Network protocol, including peer discovery, gossip protocols, bandwidth management, security, and reputation systems.

## Features

- **libp2p Integration**: Built on libp2p for robust P2P networking
- **Gossip Protocol**: Efficient message propagation across the network
- **Peer Discovery**: Multiple discovery methods (mDNS, Kademlia DHT, bootstrap)
- **Reputation System**: Track and manage peer reliability
- **Bandwidth Management**: Rate limiting and traffic shaping
- **Security**: Message validation, peer blocking, and access control
- **Health Monitoring**: Network health metrics and diagnostics
- **Metrics Collection**: Comprehensive performance monitoring

## Architecture

```
┌─────────────────┐
│  NetworkService │  ← Main entry point
└─────────┬───────┘
          │
    ┌─────▼─────┐
    │   Swarm   │  ← libp2p swarm management
    └─────┬─────┘
          │
    ┌─────▼─────┐
    │ Protocols │  ← Gossip, Kad, mDNS, etc.
    └───────────┘

Supporting Services:
├── Discovery      ← Peer discovery and management
├── Reputation     ← Peer scoring and trust
├── Bandwidth      ← Rate limiting and QoS
├── Security       ← Validation and access control
├── Monitoring     ← Health checks and metrics
└── Messaging      ← Message routing and handling
```

## Quick Start

### Basic Usage

```rust
use catalyst_network::{NetworkService, NetworkConfig};
use catalyst_utils::CatalystResult;

#[tokio::main]
async fn main() -> CatalystResult<()> {
    // Create network configuration
    let config = NetworkConfig::default();
    
    // Create network service
    let service = NetworkService::new(config).await?;
    
    // Start networking
    service.start().await?;
    
    // Service is now running and will handle:
    // - Peer discovery and connections
    // - Message routing and gossip
    // - Network health monitoring
    
    // Send a message to all peers
    let ping = catalyst_utils::network::PingMessage {
        timestamp: catalyst_utils::utils::current_timestamp(),
        data: vec![1, 2, 3],
    };
    
    service.send_message(&ping, None).await?;
    
    // Subscribe to network events
    let mut events = service.subscribe_events().await;
    
    // Handle events
    while let Some(event) = events.recv().await {
        match event {
            catalyst_network::NetworkEvent::PeerConnected { peer_id, .. } => {
                println!("Peer connected: {}", peer_id);
            }
            catalyst_network::NetworkEvent::MessageReceived { message, sender } => {
                println!("Message from {}: {:?}", sender, message.message_type());
            }
            _ => {}
        }
    }
    
    Ok(())
}
```

### Custom Message Handler

```rust
use catalyst_network::messaging::{MessageHandler, NetworkMessage};
use catalyst_utils::{
    network::{NetworkMessage as CatalystMessage, MessageType},
    logging::*,
};
use libp2p::PeerId;

pub struct MyMessageHandler;

#[async_trait::async_trait]
impl MessageHandler for MyMessageHandler {
    async fn handle_message(
        &self,
        message: &NetworkMessage,
        sender: PeerId,
    ) -> catalyst_network::NetworkResult<Option<NetworkMessage>> {
        log_info!(LogCategory::Network, "Received custom message from {}", sender);
        
        // Process message based on type
        match message.message_type() {
            MessageType::Transaction => {
                // Handle transaction message
                self.process_transaction(message, sender).await
            }
            MessageType::ProducerCandidate => {
                // Handle consensus message
                self.process_consensus(message, sender).await
            }
            _ => Ok(None), // Ignore other types
        }
    }
    
    fn supported_types(&self) -> Vec<MessageType> {
        vec![
            MessageType::Transaction,
            MessageType::ProducerCandidate,
            MessageType::ProducerVote,
        ]
    }
    
    fn priority(&self) -> u8 {
        10 // High priority for consensus messages
    }
}

impl MyMessageHandler {
    async fn process_transaction(
        &self,
        message: &NetworkMessage,
        sender: PeerId,
    ) -> catalyst_network::NetworkResult<Option<NetworkMessage>> {
        // Custom transaction processing logic
        log_debug!(LogCategory::Network, "Processing transaction from {}", sender);
        Ok(None)
    }
    
    async fn process_consensus(
        &self,
        message: &NetworkMessage,
        sender: PeerId,
    ) -> catalyst_network::NetworkResult<Option<NetworkMessage>> {
        // Custom consensus processing logic
        log_debug!(LogCategory::Network, "Processing consensus message from {}", sender);
        Ok(None)
    }
}

// Register the handler
async fn setup_custom_handler(service: &NetworkService) -> catalyst_utils::CatalystResult<()> {
    service.register_handler(MyMessageHandler).await?;
    Ok(())
}
```

## Configuration

### Network Configuration

```rust
use catalyst_network::NetworkConfig;
use std::time::Duration;

let mut config = NetworkConfig::default();

// Basic peer settings
config.peer.max_peers = 100;
config.peer.min_peers = 20;
config.peer.listen_addresses = vec![
    "/ip4/0.0.0.0/tcp/9000".parse().unwrap(),
    "/ip4/0.0.0.0/tcp/9001/ws".parse().unwrap(),
];

// Gossip configuration
config.gossip.topic_name = "my-network".to_string();
config.gossip.heartbeat_interval = Duration::from_secs(1);
config.gossip.mesh_n = 6;

// Discovery settings
config.discovery.enable_mdns = true;
config.discovery.enable_kademlia = true;
config.discovery.bootstrap_interval = Duration::from_secs(30);

// Security settings
config.security.max_message_size = 1024 * 1024; // 1MB
config.security.enable_encryption = true;

// Bandwidth limits
config.bandwidth.upload_limit = Some(10 * 1024 * 1024); // 10 MB/s
config.bandwidth.download_limit = Some(20 * 1024 * 1024); // 20 MB/s

// Reputation system
config.reputation.initial_reputation = 50.0;
config.reputation.min_reputation = 10.0;
config.reputation.decay_rate = 0.01;

// Validate configuration
config.validate()?;
```

### Loading from File

```rust
use catalyst_network::NetworkConfig;

// Load from TOML file
let config = NetworkConfig::from_file("network.toml").await?;

// Or from TOML string
let toml_content = r#"
[peer]
max_peers = 50
min_peers = 10
listen_addresses = ["/ip4/0.0.0.0/tcp/9000"]

[gossip]
topic_name = "catalyst"
heartbeat_interval = "1s"

[discovery]
enable_mdns = true
enable_kademlia = true
"#;

let config = NetworkConfig::from_toml(toml_content)?;
```

## Network Services

### Peer Discovery

```rust
use catalyst_network::discovery::{PeerDiscovery, DiscoveryMethod};

let config = catalyst_network::config::DiscoveryConfig::default();
let discovery = PeerDiscovery::new(&config);

// Add a peer manually
let peer_id = libp2p::PeerId::random();
let addresses = vec!["/ip4/192.168.1.100/tcp/9000".parse().unwrap()];

discovery.add_peer(peer_id, addresses, DiscoveryMethod::Manual).await;

// Get best peers for connection
let best_peers = discovery.get_best_peers(10).await;

// Force bootstrap discovery
discovery.force_bootstrap().await?;
```

### Reputation Management

```rust
use catalyst_network::reputation::ReputationManager;

let config = catalyst_network::config::ReputationConfig::default();
let reputation = ReputationManager::new(&config);

let peer_id = libp2p::PeerId::random();

// Update peer score based on behavior
reputation.update_peer_score(peer_id, 5.0, "successful message delivery").await;
reputation.update_peer_score(peer_id, -2.0, "slow response").await;

// Check if we should connect to peer
if reputation.should_connect_to_peer(&peer_id).await {
    println!("Peer has good reputation, connecting...");
}

// Get current score
let score = reputation.get_peer_score(&peer_id).await;
println!("Peer reputation: {:.2}", score);
```

### Bandwidth Management

```rust
use catalyst_network::bandwidth::BandwidthManager;

let config = catalyst_network::config::BandwidthConfig::default();
let bandwidth = BandwidthManager::new(&config);

// Check if we can send data
if bandwidth.can_send(1024).await {
    // Send data
    bandwidth.record_upload(1024).await;
}

// Get current usage statistics
let usage = bandwidth.get_usage().await;
println!("Upload rate: {:.2} MB/s", usage.upload_rate / 1_000_000.0);
println!("Download rate: {:.2} MB/s", usage.download_rate / 1_000_000.0);
```

### Health Monitoring

```rust
use catalyst_network::monitoring::{HealthMonitor, HealthStatus};

let config = catalyst_network::config::MonitoringConfig::default();
let monitor = HealthMonitor::new(&config);

// Perform health check
monitor.check_health().await?;

// Get current health status
let health = monitor.get_health().await;

match health.overall_status {
    HealthStatus::Healthy => println!("Network is healthy"),
    HealthStatus::Warning => println!("Network has warnings"),
    HealthStatus::Critical => println!("Network is in critical state"),
    HealthStatus::Unknown => println!("Network health unknown"),
}

println!("Connection health: {:.1}%", health.connection_health * 100.0);
println!("Bandwidth health: {:.1}%", health.bandwidth_health * 100.0);
```

## Protocol Integration

### Catalyst Network Messages

The network layer handles all Catalyst Network protocol messages:

```rust
use catalyst_utils::network::MessageType;

// Consensus messages
MessageType::ProducerQuantity      // Phase 1: Construction
MessageType::ProducerCandidate     // Phase 2: Campaigning  
MessageType::ProducerVote          // Phase 3: Voting
MessageType::ProducerOutput        // Phase 4: Synchronization

// Transaction messages
MessageType::Transaction           // Individual transactions
MessageType::TransactionBatch      // Batched transactions

// Network management
MessageType::PeerDiscovery         // Peer discovery
MessageType::PeerHandshake         // Connection establishment
MessageType::StateRequest          // State synchronization
```

### Custom Protocol Messages

```rust
use catalyst_utils::network::{NetworkMessage as CatalystMessage, MessageType};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomMessage {
    pub data: Vec<u8>,
    pub timestamp: u64,
}

impl CatalystMessage for CustomMessage {
    fn serialize(&self) -> catalyst_utils::CatalystResult<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|e| catalyst_utils::CatalystError::Serialization(e.to_string()))
    }
    
    fn deserialize(data: &[u8]) -> catalyst_utils::CatalystResult<Self> {
        serde_json::from_slice(data)
            .map_err(|e| catalyst_utils::CatalystError::Serialization(e.to_string()))
    }
    
    fn message_type(&self) -> MessageType {
        MessageType::Custom(42) // Custom message type
    }
    
    fn priority(&self) -> u8 {
        5 // Medium priority
    }
}
```

## Performance and Monitoring

### Metrics

The network crate provides comprehensive metrics through the `catalyst-utils` metrics system:

```rust
// Connection metrics
"network_connections_established_total"
"network_connections_closed_total"
"network_connected_peers"

// Message metrics  
"network_messages_sent_total"
"network_messages_received_total"
"network_bytes_sent_total"
"network_bytes_received_total"

// Gossip metrics
"network_gossip_published_total"
"network_gossip_received_total"

// Discovery metrics
"network_peers_discovered_total"
"network_bootstrap_attempts_total"
"network_bootstrap_duration_seconds"

// Health metrics
"network_health_connection"
"network_health_bandwidth"
"network_health_latency"

// Security metrics
"network_security_blocked_connections_total"
"network_security_rejected_messages_total"

// Reputation metrics
"network_reputation_average_score"
"network_reputation_tracked_peers"
```

### Performance Tuning

```rust
use catalyst_network::NetworkConfig;
use std::time::Duration;

let mut config = NetworkConfig::default();

// Optimize for high throughput
config.gossip.heartbeat_interval = Duration::from_millis(500);
config.gossip.mesh_n = 12; // Larger mesh for better connectivity
config.bandwidth.upload_limit = Some(100 * 1024 * 1024); // 100 MB/s
config.transport.max_connections = 200;

// Optimize for low latency
config.gossip.heartbeat_interval = Duration::from_millis(100);
config.transport.max_connections_per_ip = 10;
config.discovery.discovery_interval = Duration::from_secs(10);

// Optimize for resource-constrained environments
config.peer.max_peers = 20;
config.gossip.message_cache_size = 100;
config.bandwidth.upload_limit = Some(1 * 1024 * 1024); // 1 MB/s
config.reputation.enable_reputation = false; // Disable if not needed
```

## Testing

### Unit Tests

```bash
# Run all tests
cargo test

# Run network-specific tests
cargo test --package catalyst-network

# Run with logging
RUST_LOG=debug cargo test
```

### Integration Tests

```bash
# Run integration tests
cargo test --test integration_tests

# Run property-based tests
cargo test --test property_tests
```

### Benchmarks

```bash
# Run performance benchmarks
cargo bench

# Run specific benchmark
cargo bench message_serialization
```

### Test Network

```rust
use catalyst_network::test_utils::TestNetwork;

#[tokio::test]
async fn test_multi_node_communication() {
    // Create a test network with 5 nodes
    let network = TestNetwork::new(5).await;
    
    // Start all nodes
    network.start_all().await;
    
    // Connect peers
    network.connect_peers().await;
    
    // Test message propagation
    let ping = catalyst_utils::network::PingMessage {
        timestamp: catalyst_utils::utils::current_timestamp(),
        data: vec![1, 2, 3],
    };
    
    // Send from first node
    network.services[0].send_message(&ping, None).await?;
    
    // Verify message received by other nodes
    // ... test logic ...
    
    // Clean up
    network.stop_all().await;
}
```

## Security Considerations

### Network Security

- **Encryption**: All connections use Noise protocol encryption by default
- **Authentication**: Peer identity verification using cryptographic signatures
- **Message Validation**: Size limits, age checks, and content validation
- **Rate Limiting**: Protection against spam and DoS attacks
- **Reputation System**: Automatic peer scoring and blocking

### Configuration Security

```rust
use catalyst_network::NetworkConfig;
use std::collections::HashSet;

let mut config = NetworkConfig::default();

// Restrict to known good peers
config.security.allowed_peers = [
    "peer1_id".to_string(),
    "peer2_id".to_string(),
].into_iter().collect();

// Block malicious IPs
config.security.blocked_ips = [
    "192.168.1.100".parse().unwrap(),
    "10.0.0.50".parse().unwrap(),
].into_iter().collect();

// Message size limits
config.security.max_message_size = 512 * 1024; // 512 KB

// Enable encryption
config.security.enable_encryption = true;
config.security.verify_peer_identity = true;
```

## Troubleshooting

### Common Issues

1. **Connection Failures**
   ```rust
   // Check firewall and network configuration
   // Verify listen addresses are accessible
   // Check peer reputation scores
   ```

2. **High Memory Usage**
   ```rust
   // Reduce message cache size
   config.gossip.message_cache_size = 100;
   
   // Limit peer connections
   config.peer.max_peers = 50;
   
   // Enable garbage collection
   config.gossip.message_cache_duration = Duration::from_secs(60);
   ```

3. **Poor Performance**
   ```rust
   // Increase bandwidth limits
   config.bandwidth.upload_limit = Some(50 * 1024 * 1024);
   
   // Optimize gossip parameters
   config.gossip.heartbeat_interval = Duration::from_millis(500);
   config.gossip.mesh_n = 8;
   
   // Use faster transport
   config.transport.enable_websocket = false; // TCP only
   ```

### Debugging

```rust
// Enable debug logging
use catalyst_utils::logging::*;

let log_config = LogConfig {
    min_level: LogLevel::Debug,
    console_output: true,
    json_format: false,
    ..Default::default()
};

init_logger(log_config)?;

// Monitor network events
let mut events = service.subscribe_events().await;
while let Some(event) = events.recv().await {
    println!("Network event: {:?}", event);
}

// Check network statistics
let stats = service.get_stats().await?;
println!("Connected peers: {}", stats.connected_peers);
println!("Messages sent: {}", stats.messages_sent);
println!("Error rate: {:.2}%", stats.errors as f64 / stats.messages_sent as f64 * 100.0);
```

## Contributing

See the main [CONTRIBUTING.md](../CONTRIBUTING.md) for guidelines on contributing to Catalyst Network.

### Network-Specific Guidelines

- Follow the libp2p patterns and conventions
- Ensure all new protocols are properly tested
- Add metrics for new features
- Update configuration documentation
- Include integration tests for complex features

## License

This project is licensed under either of:

- Apache License, Version 2.0, ([LICENSE-APACHE](../LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](../LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.