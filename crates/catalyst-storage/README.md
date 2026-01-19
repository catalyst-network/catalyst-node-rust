# Catalyst Storage

The storage layer for Catalyst Network, providing persistent state management, atomic transactions, snapshots, and database migrations built on RocksDB.

## Features

- **RocksDB Integration**: High-performance persistent storage with configurable column families
- **Atomic Transactions**: ACID-compliant transaction batching for state consistency
- **State Management**: Implements `StateManager` trait for account, transaction, and metadata storage
- **Snapshots**: Point-in-time database snapshots for backup and recovery
- **Database Migrations**: Versioned schema evolution with automatic migration system
- **Performance Monitoring**: Comprehensive metrics collection and monitoring
- **Backup & Restore**: Full database backup and restore capabilities

## Quick Start

```rust
use catalyst_storage::{StorageConfig, init_storage};
use catalyst_utils::state::{StateManager, AccountState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize storage
    let config = StorageConfig::default();
    let storage = init_storage(config).await?;
    
    // Use as StateManager
    let address = [1u8; 21];
    let account = AccountState::new_non_confidential(address, 1000);
    
    storage.set_account(&address, &account).await?;
    let retrieved = storage.get_account(&address).await?;
    
    println!("Account retrieved: {:?}", retrieved);
    Ok(())
}
```

## Architecture

### Core Components

#### StorageManager
The main interface providing high-level storage operations:
- Account state management
- Transaction and metadata storage
- Snapshot creation and management
- Database maintenance operations

#### RocksEngine
Low-level RocksDB wrapper providing:
- Column family management
- Atomic write operations
- Iterator support
- Performance optimization

#### TransactionBatch
Atomic transaction batching system:
- Multiple operation types (accounts, transactions, metadata, contracts, DFS refs)
- Size estimation and operation preview
- Checkpointing support
- Atomic commit/rollback

#### SnapshotManager
Point-in-time state snapshots:
- Compressed snapshot storage
- Snapshot export/import
- Automatic cleanup of old snapshots
- State root verification

#### MigrationManager
Database schema evolution:
- Versioned migration system
- Automatic migration execution
- Rollback support
- Migration history tracking

### Column Families

The storage layer uses separate column families for different data types:

- **accounts**: Account states and balances
- **transactions**: Transaction data and history
- **consensus**: Consensus-related state
- **metadata**: System metadata and configuration
- **contracts**: Smart contract code and storage
- **dfs_refs**: Distributed file system references

## Configuration

### Basic Configuration

```rust
use catalyst_storage::StorageConfig;

let config = StorageConfig {
    data_dir: "/path/to/catalyst/data".into(),
    max_open_files: 1000,
    write_buffer_size: 64 * 1024 * 1024, // 64MB
    block_cache_size: 256 * 1024 * 1024, // 256MB
    compression_enabled: true,
    ..Default::default()
};
```

### Column Family Optimization

```rust
use catalyst_storage::{StorageConfig, ColumnFamilyConfig};
use std::collections::HashMap;

let mut config = StorageConfig::default();

// Optimize accounts column family for frequent updates
let mut cf_configs = HashMap::new();
cf_configs.insert("accounts".to_string(), ColumnFamilyConfig {
    write_buffer_size: 128 * 1024 * 1024, // 128MB
    bloom_filter_bits_per_key: 15,
    ..ColumnFamilyConfig::for_accounts()
});

config.column_families = cf_configs;
```

### Performance Tuning

```rust
use catalyst_storage::{StorageConfig, PerformanceConfig};

let config = StorageConfig {
    performance: PerformanceConfig {
        parallel_compaction: true,
        use_direct_io: true,
        compaction_rate_limit: 100 * 1024 * 1024, // 100MB/s
        ..Default::default()
    },
    ..Default::default()
};
```

## Usage Examples

### Basic Operations

```rust
// Account management
let address = [1u8; 21];
let account = AccountState::new_non_confidential(address, 1000);

storage.set_account(&address, &account).await?;
let balance = storage.get_account(&address).await?;

// Transaction storage
let tx_hash = [2u8; 32];
let tx_data = b"transaction data";

storage.set_transaction(&tx_hash, tx_data).await?;
let tx = storage.get_transaction(&tx_hash).await?;

// Metadata operations
storage.set_metadata("key", b"value").await?;
let value = storage.get_metadata("key").await?;
```

### Atomic Transactions

```rust
// Create transaction batch
let tx_batch = storage.create_transaction("batch_id".to_string())?;

// Add multiple operations
let addr1 = [1u8; 21];
let addr2 = [2u8; 21];
let account1 = AccountState::new_non_confidential(addr1, 500);
let account2 = AccountState::new_non_confidential(addr2, 750);

tx_batch.set_account(&addr1, &account1).await?;
tx_batch.set_account(&addr2, &account2).await?;
tx_batch.set_metadata("batch_meta", b"batch_value").await?;

// Commit atomically
let commit_hash = storage.commit_transaction("batch_id").await?;
println!("Batch committed with hash: {:?}", commit_hash);
```

### Snapshots

```rust
// Create snapshot
let snapshot = storage.create_snapshot("backup_2024").await?;
println!("Created snapshot: {}", snapshot.name());

// List snapshots
let snapshots = storage.list_snapshots().await?;
for name in snapshots {
    println!("Available snapshot: {}", name);
}

// Restore from snapshot
storage.load_snapshot("backup_2024").await?;
println!("Restored from snapshot");
```

### Backup and Restore

```rust
use std::path::Path;

// Backup database
let backup_dir = Path::new("/backup/catalyst");
storage.backup_to_directory(backup_dir).await?;

// Restore database
storage.restore_from_directory(backup_dir).await?;
```

## State Manager Implementation

The storage layer implements the `StateManager` trait from `catalyst-utils`:

```rust
use catalyst_utils::state::StateManager;

async fn use_as_state_manager(storage: &dyn StateManager) -> Result<(), Box<dyn std::error::Error>> {
    // Basic state operations
    storage.set_state(b"key", b"value".to_vec()).await?;
    let value = storage.get_state(b"key").await?;
    
    // Account operations
    let address = [1u8; 21];
    let account = AccountState::new_non_confidential(address, 1000);
    storage.set_account(&address, &account).await?;
    
    // Transaction operations
    let tx_hash = [2u8; 32];
    storage.set_transaction(&tx_hash, b"tx_data".to_vec()).await?;
    
    // Commit changes and get state root
    let state_root = storage.commit().await?;
    println!("New state root: {:?}", state_root);
    
    Ok(())
}
```

## Metrics and Monitoring

Enable metrics collection:

```toml
[dependencies]
catalyst-storage = { path = "../catalyst-storage", features = ["metrics"] }
```

Available metrics:
- `catalyst_storage_operations_total` - Total storage operations
- `catalyst_storage_transactions_committed_total` - Successful commits
- `catalyst_storage_pending_transactions` - Current pending transactions
- `catalyst_storage_database_size_bytes` - Database size
- `catalyst_storage_operation_duration_seconds` - Operation latencies

## Performance Considerations

### Write Performance
- Use transaction batches for bulk operations
- Configure appropriate write buffer sizes
- Enable parallel compaction for write-heavy workloads

### Read Performance
- Tune block cache size for working set
- Use bloom filters for point lookups
- Configure row cache for frequently accessed data

### Storage Efficiency
- Enable compression (LZ4 for speed, ZSTD for size)
- Regular compaction to reduce space amplification
- Monitor and cleanup old snapshots

## Testing

Run the test suite:

```bash
# Unit tests
cargo test

# Integration tests
cargo test --test integration_tests

# Benchmarks
cargo bench
```

### Test Features

Enable testing utilities:

```toml
[dependencies]
catalyst-storage = { path = "../catalyst-storage", features = ["testing"] }
```

```rust
use catalyst_storage::StorageConfig;

// Create test configuration
let config = StorageConfig::for_testing();
let storage = init_storage(config).await?;
```

## Error Handling

The storage layer provides comprehensive error handling:

```rust
use catalyst_storage::{StorageError, StorageResult};

match storage.get_account(&address).await {
    Ok(Some(account)) => println!("Account found: {:?}", account),
    Ok(None) => println!("Account not found"),
    Err(StorageError::Corruption(msg)) => {
        eprintln!("Database corruption detected: {}", msg);
        // Handle corruption...
    },
    Err(StorageError::InsufficientSpace { required, available }) => {
        eprintln!("Insufficient disk space: need {}, have {}", required, available);
        // Handle space issues...
    },
    Err(e) => eprintln!("Storage error: {}", e),
}
```

## Migration System

Database migrations are automatically applied:

```rust
// Custom migration example
use catalyst_storage::{Migration, StorageResult, RocksEngine};

struct CustomMigration;

impl Migration for CustomMigration {
    fn version(&self) -> u32 { 2 }
    fn name(&self) -> &str { "add_custom_feature" }
    fn description(&self) -> &str { "Add support for custom feature" }
    
    async fn up(&self, engine: &RocksEngine) -> StorageResult<()> {
        // Migration logic here
        engine.put("metadata", b"custom_feature_enabled", b"true")?;
        Ok(())
    }
}

// Register and run migrations
let mut migration_manager = MigrationManager::new(engine).await?;
migration_manager.add_migration(Box::new(CustomMigration));
migration_manager.run_migrations().await?;
```

## Dependencies

- **RocksDB**: High-performance embedded database
- **catalyst-utils**: Core utilities and traits
- **tokio**: Async runtime
- **serde**: Serialization framework
- **prometheus**: Metrics collection (optional)

## Thread Safety

All storage components are thread-safe and can be safely shared across async tasks:

```rust
use std::sync::Arc;

let storage = Arc::new(init_storage(config).await?);

// Clone and use in different tasks
let storage_clone = storage.clone();
tokio::spawn(async move {
    storage_clone.get_account(&address).await
});
```

## Contributing

When contributing to the storage layer:

1. Ensure all tests pass: `cargo test --all-features`
2. Run benchmarks to check performance: `cargo bench`
3. Update documentation for new features
4. Add appropriate error handling and logging
5. Consider migration requirements for schema changes

## License

This crate is part of the Catalyst Network project and is licensed under the MIT License.