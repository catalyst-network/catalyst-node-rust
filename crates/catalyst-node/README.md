# Catalyst Node Manager

The orchestration layer for Catalyst blockchain nodes. This crate provides the core node management functionality that integrates all Catalyst components into a cohesive, production-ready blockchain node.

## 🏗️ Architecture

The Node Manager follows a service-oriented architecture with event-driven communication:

```
┌─────────────────────────────────────────────────────────────┐
│                    Catalyst Node                            │
├─────────────────────────────────────────────────────────────┤
│  Node Manager                                               │
│  ├── Service Orchestrator                                   │
│  ├── Event Bus                                              │
│  ├── Lifecycle Management                                   │
│  └── Health Monitoring                                      │
├─────────────────────────────────────────────────────────────┤
│  Services                                                   │
│  ├── Storage Service     ├── Network Service                │
│  ├── Consensus Service   ├── Runtime Service                │
│  └── RPC Service         └── Metrics Service                │
└─────────────────────────────────────────────────────────────┘
```

## 🚀 Features

- **Service Orchestration**: Manages startup/shutdown of all node components
- **Event-Driven Communication**: Inter-service communication via event bus
- **Health Monitoring**: Continuous health checks and recovery
- **Graceful Lifecycle**: Proper startup and shutdown sequences
- **Modular Design**: Easy to add new services and components
- **Production Ready**: Built for reliability and monitoring

## 📋 Quick Start

### Basic Node Setup

```rust
use catalyst_node::{NodeBuilder, CatalystNode};
use catalyst_config::CatalystConfig;
use catalyst_rpc::RpcConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration
    let config = CatalystConfig::default();
    
    // Configure RPC
    let rpc_config = RpcConfig {
        enabled: true,
        port: 9933,
        address: "127.0.0.1".to_string(),
        max_connections: 100,
        cors_enabled: true,
        cors_origins: vec!["*".to_string()],
    };
    
    // Build and start the node
    let mut node = NodeBuilder::new()
        .with_config(config)
        .with_rpc(rpc_config)
        .build()
        .await?;
    
    node.start().await?;
    
    println!("✅ Catalyst node is running!");
    
    // Keep running until shutdown
    node.wait_for_shutdown().await;
    
    Ok(())
}
```

### Service Integration

```rust
use catalyst_node::{CatalystService, ServiceType, ServiceHealth, NodeError};

// Implement custom service
struct MyCustomService {
    name: String,
    running: bool,
}

#[async_trait::async_trait]
impl CatalystService for MyCustomService {
    async fn start(&mut self) -> Result<(), NodeError> {
        println!("Starting {}", self.name);
        self.running = true;
        Ok(())
    }
    
    async fn stop(&mut self) -> Result<(), NodeError> {
        println!("Stopping {}", self.name);
        self.running = false;
        Ok(())
    }
    
    async fn health_check(&self) -> ServiceHealth {
        if self.running {
            ServiceHealth::Healthy
        } else {
            ServiceHealth::Unhealthy
        }
    }
    
    fn service_type(&self) -> ServiceType {
        ServiceType::Custom(1)
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

// Add to node
let custom_service = MyCustomService {
    name: "My Service".to_string(),
    running: false,
};

let mut node = NodeBuilder::new()
    .with_config(config)
    .with_service(Box::new(custom_service))
    .build()
    .await?;
```

## 🔧 Components

### NodeBuilder

Fluent API for constructing nodes with custom configurations:

```rust
let node = NodeBuilder::new()
    .with_config(catalyst_config)
    .with_rpc(rpc_config)
    .with_service(custom_service)
    .build()
    .await?;
```

### ServiceManager

Manages the lifecycle of all node services:

```rust
// Services start in dependency order:
// 1. Storage
// 2. Network  
// 3. Consensus
// 4. Runtime
// 5. RPC

// Services stop in reverse order for clean shutdown
```

### Event System

Event-driven communication between services:

```rust
use catalyst_node::{Event, EventBus};

let event_bus = EventBus::new();

// Publish events
event_bus.publish(Event::BlockReceived {
    block_hash: "0x123...".to_string(),
    block_number: 42,
}).await;

// Subscribe to events
let mut receiver = event_bus.subscribe().await;
while let Ok(event) = receiver.recv().await {
    match event {
        Event::BlockReceived { block_hash, block_number } => {
            println!("New block: #{} ({})", block_number, block_hash);
        }
        _ => {}
    }
}
```

## 📊 Service Types

The node supports these core service types:

- **Storage**: Persistent blockchain data (RocksDB)
- **Network**: P2P networking (libp2p)
- **Consensus**: Block production and validation
- **Runtime**: Transaction execution (EVM)
- **RPC**: JSON-RPC API server
- **Metrics**: Performance monitoring
- **Custom**: User-defined services

## 🎯 Events

The event system supports comprehensive blockchain events:

### Service Events
- `ServiceStarted` / `ServiceStopped`
- `ServiceError`
- `HealthCheckFailed`

### Blockchain Events
- `BlockReceived` / `BlockProduced`
- `TransactionReceived` / `TransactionExecuted`
- `StateUpdated`

### Network Events
- `PeerConnected` / `PeerDisconnected`
- `NetworkPartition`

### Consensus Events
- `ConsensusReached`
- `ValidatorSlashed`
- `EpochChanged`

## 🔍 Monitoring

### Service Health Checks

```rust
// Check individual service health
let health = node.get_service_health(ServiceType::Storage).await;
match health {
    Some(ServiceHealth::Healthy) => println!("Storage is healthy"),
    Some(ServiceHealth::Unhealthy) => println!("Storage needs attention"),
    None => println!("Storage service not found"),
}

// Check all services
let all_healthy = node.is_running().await;
```

### Node State

```rust
let state = node.get_state().await;
println!("Node ID: {}", state.node_id);
println!("Block height: {}", state.block_height);
println!("Peers: {}", state.network_peers);
println!("Validator: {}", state.is_validator);

for service in state.services {
    println!("Service {}: {:?}", service.name, service.health);
}
```

## 🏃 Running the Node

### Development Mode

```bash
# Start development node
cargo run -p catalyst-cli start --rpc-port 9933

# Start with custom config
cargo run -p catalyst-cli start --config my-config.toml

# Start as validator
cargo run -p catalyst-cli start --validator --rpc-port 9933
```

### Production Mode

```bash
# Build release binary
cargo build --release

# Start production node
./target/release/catalyst-node start --config production.toml
```

### Docker Deployment

```dockerfile
FROM rust:1.70 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /app/target/release/catalyst-node /usr/local/bin/
EXPOSE 9933 30333
CMD ["catalyst-node", "start"]
```

## 🔧 Configuration

### Node Configuration

```toml
[node]
name = "my-catalyst-node"
data_dir = "data"

[network]
listen_port = 30333
discovery_enabled = true
max_peers = 50

[rpc]
enabled = true
port = 9933
address = "127.0.0.1"
cors_enabled = true

[consensus]
validator = false
algorithm = "catalyst-pos"

[storage]
engine = "rocksdb"
cache_size = 1024
```

### Service Configuration

```rust
// Custom service startup order
let mut node_manager = NodeManager::new(config);
node_manager.set_startup_order(vec![
    ServiceType::Storage,
    ServiceType::Custom(1), // Your custom service
    ServiceType::Network,
    ServiceType::Consensus,
    ServiceType::Runtime,
    ServiceType::Rpc,
]);
```

## 🧪 Testing

### Unit Tests

```bash
cargo test -p catalyst-node
```

### Integration Tests

```bash
# Test full node startup/shutdown
cargo test -p catalyst-node --test integration

# Test service orchestration
cargo test -p catalyst-node --test services
```

### Multi-Node Testing

```rust
#[tokio::test]
async fn test_multi_node_network() {
    // Start bootstrap node
    let mut node1 = create_test_node(9933, 30333).await;
    node1.start().await.unwrap();
    
    // Start second node
    let mut node2 = create_test_node(9934, 30334).await;
    node2.start().await.unwrap();
    
    // Verify they connect
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    let state1 = node1.get_state().await;
    let state2 = node2.get_state().await;
    
    assert!(state1.network_peers > 0);
    assert!(state2.network_peers > 0);
    
    node1.stop().await.unwrap();
    node2.stop().await.unwrap();
}
```

## 📈 Performance

### Metrics Collection

```rust
use catalyst_node::Event;

// Subscribe to performance events
let mut receiver = event_bus.subscribe().await;
while let Ok(event) = receiver.recv().await {
    match event {
        Event::BlockProduced { block_number, .. } => {
            println!("Block production rate: {}/min", blocks_per_minute);
        }
        Event::TransactionExecuted { gas_used, .. } => {
            println!("Gas usage: {}", gas_used);
        }
        _ => {}
    }
}
```

### Resource Monitoring

```rust
// Monitor resource usage
let state = node.get_state().await;
for service in state.services {
    match service.health {
        ServiceHealth::Healthy => {
            println!("✅ {} running for {:?}", service.name, service.uptime);
        }
        ServiceHealth::Unhealthy => {
            println!("❌ {} needs attention", service.name);
        }
        _ => {}
    }
}
```

## 🔒 Security

### Service Isolation

Each service runs with minimal permissions and clear boundaries:

```rust
// Services cannot access each other directly
// All communication goes through the event bus
// Each service has its own error handling
```

### Configuration Security

```toml
# Secure RPC configuration
[rpc]
enabled = true
address = "127.0.0.1"  # Localhost only
cors_origins = ["https://trusted-domain.com"]
max_connections = 100
```

### Network Security

```toml
[network]
# Firewall-friendly configuration
listen_port = 30333
max_peers = 50
discovery_enabled = true
```

## 🐛 Troubleshooting

### Common Issues

#### Service Startup Failures

```bash
# Check service logs
RUST_LOG=catalyst_node=debug cargo run -p catalyst-cli start

# Check specific service
RUST_LOG=catalyst_storage=debug cargo run -p catalyst-cli start
```

#### Port Conflicts

```bash
# Check port usage
lsof -i :9933
lsof -i :30333

# Use different ports
cargo run -p catalyst-cli start --rpc-port 9944 --network-port 30334
```

#### Configuration Errors

```bash
# Validate configuration
cargo run -p catalyst-cli init --output test-config.toml
cargo run -p catalyst-cli start --config test-config.toml
```

### Debug Mode

```bash
# Maximum logging
RUST_LOG=trace cargo run -p catalyst-cli start

# Service-specific logging
RUST_LOG=catalyst_node::services=debug cargo run -p catalyst-cli start
```

## 🤝 Integration

### With Existing Services

```rust
// Integrate with external monitoring
struct MonitoringService {
    metrics_endpoint: String,
}

#[async_trait::async_trait]
impl CatalystService for MonitoringService {
    async fn start(&mut self) -> Result<(), NodeError> {
        // Connect to external monitoring system
        Ok(())
    }
    
    // ... implement other methods
}

// Add to node
let monitoring = MonitoringService {
    metrics_endpoint: "http://prometheus:9090".to_string(),
};

let mut node = NodeBuilder::new()
    .with_config(config)
    .with_service(Box::new(monitoring))
    .build()
    .await?;
```

### With External APIs

```rust
// React to blockchain events
let mut receiver = event_bus.subscribe().await;
while let Ok(event) = receiver.recv().await {
    match event {
        Event::BlockProduced { block_hash, .. } => {
            // Notify external API
            notify_external_system(&block_hash).await;
        }
        _ => {}
    }
}
```

## 🗺️ Roadmap

### Phase 1: Core Integration ✅
- [x] Service orchestration
- [x] Event system
- [x] Basic health monitoring
- [x] RPC integration

### Phase 2: Advanced Features (In Progress)
- [ ] Storage service integration
- [ ] Network service integration
- [ ] Consensus service integration
- [ ] EVM runtime integration

### Phase 3: Production Features
- [ ] Advanced metrics
- [ ] Service recovery
- [ ] Hot reloading
- [ ] Cluster management

## 📝 Contributing

1. **Add New Services**: Implement `CatalystService` trait
2. **Add Events**: Extend the `Event` enum
3. **Add Tests**: Cover service integration scenarios
4. **Update Documentation**: Keep README current

### Service Development

```rust
// Template for new services
pub struct MyService {
    config: MyServiceConfig,
    state: ServiceState,
}

#[async_trait::async_trait]
impl CatalystService for MyService {
    async fn start(&mut self) -> Result<(), NodeError> {
        // Initialize service
        Ok(())
    }
    
    async fn stop(&mut self) -> Result<(), NodeError> {
        // Clean shutdown
        Ok(())
    }
    
    async fn health_check(&self) -> ServiceHealth {
        // Check service health
        ServiceHealth::Healthy
    }
    
    fn service_type(&self) -> ServiceType {
        ServiceType::Custom(42)
    }
    
    fn name(&self) -> &str {
        "My Service"
    }
}
```

## 📄 License

This crate is part of the Catalyst Network project and is licensed under [LICENSE](../../LICENSE).