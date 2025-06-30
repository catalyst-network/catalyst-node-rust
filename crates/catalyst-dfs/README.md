# Catalyst DFS

A distributed file system for the Catalyst Network, providing IPFS-compatible storage with content addressing, peer-to-peer networking, and automatic replication.

## Features

- **Content-Addressed Storage**: Files are identified by cryptographic hashes (CIDs)
- **IPFS Compatibility**: Compatible with IPFS protocols and tools
- **P2P Networking**: Decentralized file sharing using libp2p
- **Automatic Replication**: Configurable content replication across the network
- **Local Storage**: Efficient local storage with RocksDB
- **Content Categories**: Organize content by type (contracts, ledger updates, media, etc.)
- **Garbage Collection**: Automatic cleanup of unpinned content
- **Provider Discovery**: DHT-based content provider discovery

## Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   DFS Service   │────│  NetworkedDfs   │────│ LocalDfsStorage │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         │                       │                       │
    ┌─────────┐           ┌─────────────┐         ┌─────────────┐
    │Provider │           │   DfsSwarm  │         │   RocksDB   │
    │System   │           │   (libp2p)  │         │  Metadata   │
    └─────────┘           └─────────────┘         └─────────────┘
```

## Quick Start

### Basic Usage

```rust
use catalyst_dfs::{DfsConfig, DfsFactory, DfsService};
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create configuration
    let temp_dir = TempDir::new()?;
    let config = DfsConfig::dev_config(temp_dir.path().to_path_buf());
    
    // Create DFS service
    let service = DfsService::new(config).await?;
    
    // Store content
    let data = b"Hello, distributed world!".to_vec();
    let cid = service.store_with_replication(data.clone()).await?;
    println!("Stored content with CID: {}", cid.to_string());
    
    // Retrieve content
    let retrieved = service.dfs().get(&cid).await?;
    assert_eq!(data, retrieved);
    println!("Retrieved: {}", String::from_utf8_lossy(&retrieved));
    
    Ok(())
}
```

### IPFS-Compatible Usage

```rust
use catalyst_dfs::{DfsConfig, ipfs::IpfsFactory};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = DfsConfig::default();
    
    // Create IPFS-compatible node
    let dfs = IpfsFactory::create_ipfs_node(config).await?;
    
    // Store and retrieve like IPFS
    let data = b"IPFS-compatible content".to_vec();
    let cid = dfs.put(data.clone()).await?;
    let retrieved = dfs.get(&cid).await?;
    
    println!("CID: {}", cid.to_string());
    
    Ok(())
}
```

### Networked DFS with Bootstrap Peers

```rust
use catalyst_dfs::{DfsConfig, ipfs::IpfsFactory};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = DfsConfig::default();
    config.enable_networking = true;
    
    let bootstrap_peers = vec![
        "/ip4/127.0.0.1/tcp/4001/p2p/12D3KooWExample".to_string(),
    ];
    
    let dfs = IpfsFactory::create_with_bootstrap(config, bootstrap_peers).await?;
    
    // Connect to network and start sharing content
    dfs.bootstrap().await?;
    
    Ok(())
}
```

## Configuration

```rust
use catalyst_dfs::DfsConfig;
use std::path::PathBuf;

let config = DfsConfig {
    storage_dir: PathBuf::from("./my-dfs"),
    max_storage_size: 50 * 1024 * 1024 * 1024, // 50GB
    enable_gc: true,
    gc_interval: 1800, // 30 minutes
    enable_discovery: true,
    provider_interval: 300, // 5 minutes
    replication_factor: 5,
    enable_networking: true,
    listen_addresses: vec!["/ip4/0.0.0.0/tcp/4001".to_string()],
    bootstrap_peers: vec![
        "/ip4/bootstrap.example.com/tcp/4001/p2p/12D3KooW...".to_string(),
    ],
};
```

## Content Categories

Organize content by type for better management:

```rust
use catalyst_dfs::{ContentCategory, CategorizedStorage};

// Store categorized content
let contract_data = b"contract bytecode".to_vec();
let cid = dfs.put_categorized(contract_data, ContentCategory::Contract).await?;

// List content by category
let contracts = dfs.list_by_category(ContentCategory::Contract).await?;
println!("Found {} contracts", contracts.len());
```

Available categories:
- `LedgerUpdate` - Blockchain state updates
- `Contract` - Smart contract bytecode
- `AppData` - Application data
- `Media` - Media files
- `File` - Generic files

## Content Management

### Pinning Content

Pinned content is protected from garbage collection:

```rust
// Pin important content
dfs.pin(&cid).await?;

// Check if content is pinned
let metadata = dfs.metadata(&cid).await?;
println!("Pinned: {}", metadata.pinned);

// Unpin when no longer needed
dfs.unpin(&cid).await?;
```

### Garbage Collection

```rust
// Manual garbage collection
let result = dfs.gc().await?;
println!("Removed {} objects, freed {} bytes", 
         result.objects_removed, result.bytes_freed);
```

### Statistics

```rust
let stats = dfs.stats().await?;
println!("Total objects: {}", stats.total_objects);
println!("Total bytes: {}", stats.total_bytes);
println!("Available space: {}", stats.available_space);
println!("Hit rate: {:.2}%", stats.hit_rate * 100.0);
```

## Network Operations

### Provider Discovery

```rust
use catalyst_dfs::ContentProvider;

// Find providers for content
let providers = provider.find_providers(&cid).await?;
println!("Found {} providers", providers.len());

// Announce that we provide content
provider.provide(&cid).await?;
```

### Network Statistics

```rust
let net_stats = networked_dfs.network_stats().await?;
println!("Connected peers: {}", net_stats.connected_peers);
println!("Provided content: {}", net_stats.provided_content_count);
```

## Replication

Monitor and ensure content replication:

```rust
use catalyst_dfs::{ContentReplicator, ReplicationLevel};

let replicator = ContentReplicator::new(provider, 3); // Target 3 replicas
let status = replicator.check_replication(&cid).await?;

match status.level {
    ReplicationLevel::Optimal => println!("Well replicated"),
    ReplicationLevel::Adequate => println!("Sufficiently replicated"),
    ReplicationLevel::Insufficient => println!("Needs more replicas"),
    ReplicationLevel::None => println!("No replicas found"),
}
```

## Integration with Catalyst Core

Store and retrieve ledger updates:

```rust
use catalyst_core::LedgerStateUpdate;
use catalyst_dfs::CategorizedStorage;

// Store ledger update
let update = LedgerStateUpdate::new(/* ... */);
let cid = dfs.store_ledger_update(&update).await?;

// Retrieve ledger update
let retrieved_update = dfs.get_ledger_update(&cid).await?;
```

## Testing

Run the test suite:

```bash
cargo test
```

Run tests with networking (requires additional setup):

```bash
cargo test --features networking
```

Run integration tests:

```bash
cargo test --test integration
```

## Performance

### Benchmarks

```bash
cargo bench
```

### Configuration for Performance

For high-throughput scenarios:

```rust
let config = DfsConfig {
    max_storage_size: 1_000_000_000_000, // 1TB
    gc_interval: 3600 * 24, // Daily GC
    provider_interval: 60, // Frequent announcements
    replication_factor: 2, // Lower replication for speed
    ..Default::default()
};
```

For high-reliability scenarios:

```rust
let config = DfsConfig {
    replication_factor: 7, // High replication
    gc_interval: 3600, // Frequent cleanup
    provider_interval: 300, // Regular announcements
    ..Default::default()
};
```

## Troubleshooting

### Common Issues

1. **Storage directory permissions**: Ensure the storage directory is writable
2. **Port conflicts**: Check that listen addresses are available
3. **Bootstrap peer connectivity**: Verify bootstrap peers are reachable
4. **Disk space**: Monitor available storage space

### Logging

Enable detailed logging:

```rust
env_logger::init();
```

Or use with specific log levels:

```bash
RUST_LOG=catalyst_dfs=debug cargo run
```

### Debugging Network Issues

```rust
// Check peer connectivity
let peer_id = dfs.local_peer_id().await;
println!("Local peer ID: {}", peer_id);

// Monitor network events
while let Some(event) = swarm.next_event().await {
    println!("Network event: {:?}", event);
}
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

### Development Setup

```bash
git clone https://github.com/catalyst-network/catalyst-dfs
cd catalyst-dfs
cargo build
cargo test
```

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Related Projects

- [IPFS](https://ipfs.io/) - The InterPlanetary File System
- [libp2p](https://libp2p.io/) - Modular network stack
- [Catalyst Network](https://github.com/catalyst-network) - The main Catalyst blockchain project