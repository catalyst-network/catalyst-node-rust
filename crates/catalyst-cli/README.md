# Catalyst CLI

The command-line interface for the Catalyst blockchain node. This crate provides a comprehensive CLI for managing and interacting with Catalyst nodes.

## Features

- **Node Management**: Start, stop, and monitor Catalyst nodes
- **Configuration**: Flexible configuration through files and environment variables
- **RPC Integration**: Built-in RPC server for external integrations
- **Logging**: Structured logging with configurable levels
- **Health Monitoring**: Node status and health checks

## Installation

### From Source
```bash
cargo build --release
./target/release/catalyst-node --help
```

### Binary Installation
```bash
# Copy to system path
sudo cp target/release/catalyst-node /usr/local/bin/
catalyst-node --help
```

## Usage

### Basic Commands

#### Start a Node
```bash
# Start with default configuration
catalyst-node start

# Start with custom RPC port
catalyst-node start --rpc-port 9944

# Start as validator
catalyst-node start --validator

# Start with custom config file
catalyst-node start --config /path/to/catalyst.toml
```

#### Check Node Status
```bash
# Get current node status
catalyst-node status

# Get detailed health information
catalyst-node status --verbose
```

#### Stop a Node
```bash
# Gracefully stop the node
catalyst-node stop
```

#### Generate Configuration
```bash
# Generate default config file
catalyst-node init

# Generate config in specific location
catalyst-node init --config /path/to/catalyst.toml
```

### Configuration Options

#### Command Line Flags
- `--config <PATH>`: Path to configuration file
- `--rpc-port <PORT>`: RPC server port (default: 9933)
- `--validator`: Run as validator node
- `--data-dir <PATH>`: Data directory path
- `--log-level <LEVEL>`: Logging level (error, warn, info, debug, trace)

#### Environment Variables
```bash
export CATALYST_CONFIG_PATH=/path/to/config.toml
export CATALYST_LOG_LEVEL=debug
export CATALYST_RPC_PORT=9933
export CATALYST_DATA_DIR=/var/lib/catalyst
```

## Configuration File

The CLI uses TOML configuration files. Generate a default config:

```bash
catalyst-node init --config catalyst.toml
```

### Example Configuration
```toml
[node]
name = "catalyst-node"
data_dir = "data"

[network]
listen_port = 30333
discovery_enabled = true
bootstrap_nodes = []

[rpc]
enabled = true
port = 9933
address = "127.0.0.1"
cors_enabled = true
cors_origins = ["*"]

[storage]
engine = "rocksdb"
data_dir = "data"
cache_size = 1024

[logging]
level = "info"
output = "console"
file_path = "logs/catalyst.log"

[consensus]
algorithm = "catalyst-pos"
validator = false

[metrics]
enabled = false
port = 9615
```

## Examples

### Development Setup
```bash
# 1. Initialize configuration
catalyst-node init

# 2. Start development node with RPC
catalyst-node start --rpc-port 9933 --log-level debug

# 3. Check status in another terminal
catalyst-node status
```

### Validator Setup
```bash
# 1. Generate validator configuration
catalyst-node init --config validator.toml

# 2. Edit configuration to enable validation
# Set consensus.validator = true in validator.toml

# 3. Start validator node
catalyst-node start --validator --config validator.toml
```

### Multi-Node Network
```bash
# Terminal 1 - Bootstrap node
catalyst-node start --rpc-port 9933

# Terminal 2 - Second node
catalyst-node start --rpc-port 9934 --data-dir data2

# Terminal 3 - Validator node  
catalyst-node start --validator --rpc-port 9935 --data-dir data3
```

## RPC Integration

The CLI automatically starts an RPC server when enabled:

```bash
# Start with RPC enabled
catalyst-node start --rpc-port 9933

# Test RPC endpoints
curl -X POST http://127.0.0.1:9933 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"catalyst_version","params":[],"id":1}'
```

See the [RPC Documentation](../catalyst-rpc/README.md) for available methods.

## Logging

### Log Levels
- `error`: Only errors
- `warn`: Warnings and errors
- `info`: General information (default)
- `debug`: Detailed debugging
- `trace`: Very verbose output

### Log Configuration
```bash
# Console logging
catalyst-node start --log-level info

# File logging (configure in catalyst.toml)
[logging]
output = "file"
file_path = "logs/catalyst.log"
```

## Data Directory

The node stores data in the configured data directory:

```
data/
├── blocks/          # Blockchain data
├── state/           # World state
├── transactions/    # Transaction pool
├── network/         # Network state
└── config/          # Runtime configuration
```

### Backup and Recovery
```bash
# Backup node data
tar -czf catalyst-backup.tar.gz data/

# Restore from backup
tar -xzf catalyst-backup.tar.gz
catalyst-node start
```

## Troubleshooting

### Common Issues

#### Port Already in Use
```bash
# Check what's using the port
lsof -i :9933

# Use different port
catalyst-node start --rpc-port 9944
```

#### Permission Denied
```bash
# Check data directory permissions
ls -la data/

# Fix permissions
chmod -R 755 data/
```

#### Configuration Errors
```bash
# Validate configuration
catalyst-node start --config catalyst.toml --dry-run

# Use default config
catalyst-node start  # Uses built-in defaults
```

### Debug Mode
```bash
# Start with verbose logging
RUST_LOG=debug catalyst-node start --log-level debug

# Get detailed status
catalyst-node status --verbose
```

## Development

### Building from Source
```bash
# Development build
cargo build

# Release build
cargo build --release

# Run tests
cargo test

# Run with logging
RUST_LOG=catalyst_cli=debug cargo run -- start
```

### Integration with Other Crates
```rust
use catalyst_cli::{NodeConfig, CatalystNode};

// Programmatic node control
let config = NodeConfig::default();
let node = CatalystNode::new(config).await?;
node.start().await?;
```

## Security Considerations

- **Configuration Files**: Store sensitive config securely
- **RPC Access**: Limit RPC access to trusted networks
- **Data Directory**: Protect blockchain data with appropriate permissions
- **Validator Keys**: Secure validator private keys

## Performance Tuning

### System Requirements
- **CPU**: 2+ cores recommended
- **Memory**: 4GB+ RAM recommended  
- **Storage**: SSD recommended, 100GB+ free space
- **Network**: Stable internet connection

### Optimization
```toml
# In catalyst.toml
[storage]
cache_size = 2048  # Increase for more RAM

[network]
max_peers = 50     # Adjust based on bandwidth

[rpc]
max_connections = 100  # Limit RPC connections
```

## Support

- **Documentation**: [docs.catalyst.network](https://docs.catalyst.network)
- **GitHub Issues**: [github.com/catalyst-network/catalyst](https://github.com/catalyst-network/catalyst)
- **Discord**: [discord.gg/catalyst](https://discord.gg/catalyst)

## License

This crate is part of the Catalyst Network project and is licensed under [LICENSE](../../LICENSE).