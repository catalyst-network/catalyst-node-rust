# Catalyst Config

Configuration management for the Catalyst Network blockchain platform.

## Overview

The `catalyst-config` crate provides comprehensive configuration management for all Catalyst Network components, including:

- **Network Configuration**: P2P networking, peer discovery, bandwidth management
- **Consensus Configuration**: 4-phase collaborative consensus parameters
- **Crypto Configuration**: Cryptographic settings, key derivation, signature schemes
- **Storage Configuration**: Database settings, DFS configuration, caching
- **Service Bus Configuration**: Web2 integration, event streaming, authentication
- **Logging Configuration**: Structured logging, output destinations, performance
- **Metrics Configuration**: Collection, storage, export, alerting

## Features

- 🌐 **Multi-Network Support**: Devnet, testnet, and mainnet configurations
- 📁 **Multiple Formats**: TOML and JSON configuration files
- 🔧 **Environment Overrides**: Override any setting via environment variables
- ✅ **Validation**: Comprehensive configuration validation and cross-checks
- 🔄 **Hot Reload**: Configuration changes without restart (for supported settings)
- 📊 **Templates**: Generate configuration templates for different networks
- 🛡️ **Security**: Built-in security validation for production environments

## Quick Start

### Basic Usage

```rust
use catalyst_config::{CatalystConfig, NetworkType};

// Create default configuration for development
let config = CatalystConfig::new_for_network(NetworkType::Devnet);

// Validate configuration
config.validate()?;

// Use configuration
println!("P2P Port: {}", config.network.p2p_port);
println!("Consensus Cycle: {}ms", config.consensus.cycle_duration_ms);
```

### Loading from File

```rust
use catalyst_config::loader::FileLoader;

// Load from TOML file
let config = FileLoader::load_toml("config/devnet.toml").await?;

// Auto-detect format
let config = FileLoader::load_auto("config/network.toml").await?;
```

### Environment Variable Overrides

```bash
# Set network type
export CATALYST_NETWORK=testnet

# Override P2P port
export CATALYST_NETWORK_P2P_PORT=7001

# Override consensus settings
export CATALYST_CONSENSUS_PRODUCER_COUNT=25
export CATALYST_CONSENSUS_CYCLE_DURATION_MS=30000
```

```rust
use catalyst_config::loader::EnvLoader;

// Load configuration with environment overrides
let config = EnvLoader::load_from_env()?;
```

## Configuration Structure

### Network Configuration

```toml
[network]
name = "devnet"
p2p_port = 7000
api_port = 8080
max_peers = 20
min_peers = 3
bootstrap_peers = ["127.0.0.1:7001", "127.0.0.1:7002"]
```

### Consensus Configuration

```toml
[consensus]
cycle_duration_ms = 15000  # Total cycle time
construction_phase_ms = 3750
campaigning_phase_ms = 3750
voting_phase_ms = 3750
synchronization_phase_ms = 3750
producer_count = 5
supermajority_threshold = 0.67
```

### Storage Configuration

```toml
[storage.database]
db_type = "RocksDb"
data_directory = "./data/devnet"
write_buffer_size = 67108864  # 64MB
block_cache_size = 134217728  # 128MB

[storage.dfs]
enabled = false
dfs_type = "Local"
replication_factor = 1
```

## Network Types

### Development Network (Devnet)
- **Purpose**: Local development and testing
- **Consensus**: Fast 15-second cycles, 5 producers
- **Security**: Relaxed settings for development ease
- **Storage**: Local storage, minimal caching

### Test Network (Testnet)
- **Purpose**: Integration testing and validation
- **Consensus**: Moderate 30-second cycles, 50 producers
- **Security**: Production-like security with monitoring
- **Storage**: DFS enabled, comprehensive caching

### Main Network (Mainnet)
- **Purpose**: Production blockchain network
- **Consensus**: Stable 60-second cycles, 200+ producers
- **Security**: Maximum security, all features enabled
- **Storage**: Full DFS, optimized performance

## Environment Variables

All configuration values can be overridden using environment variables with the prefix `CATALYST_`:

### Network Settings
- `CATALYST_NETWORK` - Network type (devnet/testnet/mainnet)
- `CATALYST_NETWORK_P2P_PORT` - P2P networking port
- `CATALYST_NETWORK_API_PORT` - REST API port
- `CATALYST_NETWORK_MAX_PEERS` - Maximum peer connections

### Consensus Settings
- `CATALYST_CONSENSUS_CYCLE_DURATION_MS` - Total consensus cycle time
- `CATALYST_CONSENSUS_PRODUCER_COUNT` - Number of producer nodes
- `CATALYST_CONSENSUS_SUPERMAJORITY_THRESHOLD` - Voting threshold

### Storage Settings
- `CATALYST_STORAGE_DATA_DIRECTORY` - Database storage path
- `CATALYST_STORAGE_DFS_ENABLED` - Enable distributed file system
- `CATALYST_STORAGE_COMPRESSION_ENABLED` - Enable storage compression

### Service Bus Settings
- `CATALYST_SERVICE_BUS_ENABLED` - Enable Web2 integration
- `CATALYST_SERVICE_BUS_WEBSOCKET_PORT` - WebSocket server port
- `CATALYST_SERVICE_BUS_AUTH_ENABLED` - Enable authentication

### Logging Settings
- `CATALYST_LOG_LEVEL` - Logging level (trace/debug/info/warn/error)
- `CATALYST_LOG_FORMAT` - Log format (text/json/compact)
- `CATALYST_LOG_ASYNC_ENABLED` - Enable asynchronous logging

### Metrics Settings
- `CATALYST_METRICS_ENABLED` - Enable metrics collection
- `CATALYST_METRICS_PORT` - Metrics server port
- `CATALYST_METRICS_COLLECTION_INTERVAL_SECONDS` - Collection frequency

## Configuration Validation

The configuration system includes comprehensive validation:

```rust
use catalyst_config::loader::validation::ConfigValidator;

// Basic validation
config.validate()?;

// Comprehensive validation with cross-checks
ConfigValidator::validate_comprehensive(&config)?;

// Network-specific validation
ConfigValidator::validate_for_network(&config, "mainnet")?;

// Generate validation report
let report = ConfigValidator::generate_report(&config);
println!("{}", report);
```

### Validation Checks

- **Port Conflicts**: Ensures no port conflicts between services
- **Resource Allocation**: Validates memory and thread allocations
- **Timing Constraints**: Checks consensus timing relationships
- **Security Requirements**: Enforces security settings for production
- **Performance Settings**: Validates performance-related configurations

## Configuration Templates

Generate configuration templates for different networks:

```rust
use catalyst_config::ConfigUtils;

// Generate devnet template
let template = ConfigUtils::generate_template("devnet")?;
std::fs::write("devnet.toml", template)?;

// Generate testnet template
let template = ConfigUtils::generate_template("testnet")?;
std::fs::write("testnet.toml", template)?;
```

## Hot Reload Support

Some configuration changes can be applied without restarting:

```rust
use catalyst_config::hot_reload::ConfigWatcher;

// Create configuration watcher
let mut watcher = ConfigWatcher::new("config.toml", &config)?;

// Check for changes
if watcher.check_for_changes().await? {
    println!("Configuration changed, reloading...");
    // Reload and apply non-critical changes
}
```

### Hot-Reloadable Settings
- Logging levels and output destinations
- Metrics collection intervals
- Service bus event filters
- Rate limiting parameters
- Non-critical security settings

### Settings Requiring Restart
- Network ports
- Consensus parameters
- Storage configuration
- Cryptographic settings

## Security Considerations

### Production Requirements

For mainnet deployments, the configuration system enforces:

- **Cryptographic Security**: Constant-time operations, secure memory cleanup
- **Network Security**: TLS encryption, authentication enabled
- **Storage Security**: Encryption at rest, secure file permissions
- **Access Control**: Authentication tokens, rate limiting
- **Audit Logging**: Comprehensive logging with retention

### Key Security Settings

```toml
[crypto.security]
constant_time_operations = true
secure_memory_cleanup = true
side_channel_resistance = true

[storage.security]
encryption_at_rest = true
secure_deletion = true

[service_bus.auth]
enabled = true
methods = ["JWT", "ApiKey"]

[metrics.server]
auth_enabled = true
auth_token = "secure_random_token"
```

## Examples

See the `examples/` directory for comprehensive usage examples:

- `basic_usage.rs` - Basic configuration loading and usage
- `env_override.rs` - Environment variable override examples
- `hot_reload.rs` - Hot reload functionality demonstration

## Integration with Other Crates

The catalyst-config crate is designed to be used by all other Catalyst Network crates:

```rust
// In other Catalyst crates
use catalyst_config::{CatalystConfig, NetworkType};

pub struct MyService {
    config: CatalystConfig,
}

impl MyService {
    pub fn new(network: NetworkType) -> Self {
        let config = CatalystConfig::new_for_network(network);
        Self { config }
    }
    
    pub fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Use configuration
        let port = self.config.network.p2p_port;
        let cycle_time = self.config.consensus.cycle_duration_ms;
        
        // Initialize service with configuration
        Ok(())
    }
}
```

## Contributing

When adding new configuration options:

1. **Add to appropriate config struct** in `src/config/`
2. **Update validation logic** in the `validate()` method
3. **Add environment variable support** in `src/loader/env.rs`
4. **Update network defaults** in `src/networks/`
5. **Add comprehensive tests** in `tests/`
6. **Update documentation** and examples

## License

This crate is part of the Catalyst Network project and follows the same licensing terms.