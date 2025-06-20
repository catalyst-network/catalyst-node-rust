#!/bin/bash

# Catalyst Node Project Setup Script
# This script creates the complete project structure and initializes all crates

set -e

echo "🚀 Setting up Catalyst Network Node project..."

# Create main project directory
PROJECT_DIR="catalyst-node"
if [ -d "$PROJECT_DIR" ]; then
    echo "❌ Directory $PROJECT_DIR already exists. Please remove it first or choose a different name."
    exit 1
fi

mkdir "$PROJECT_DIR"
cd "$PROJECT_DIR"

echo "📁 Creating project structure..."

# Create directories
mkdir -p {crates,docs,scripts,configs,tests}

# Initialize workspace
cat > Cargo.toml << 'EOF'
[workspace]
resolver = "2"
members = [
    "crates/catalyst-core",
    "crates/catalyst-consensus", 
    "crates/catalyst-storage",
    "crates/catalyst-network",
    "crates/catalyst-runtime-evm",
    "crates/catalyst-runtime-svm", 
    "crates/catalyst-service-bus",
    "crates/catalyst-dfs",
    "crates/catalyst-crypto",
    "crates/catalyst-cli",
    "crates/catalyst-rpc",
    "crates/catalyst-config",
    "crates/catalyst-utils",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["Catalyst Network Team"]
license = "MIT OR Apache-2.0"
repository = "https://github.com/catalyst-network/catalyst-node"
homepage = "https://catalyst.network"

[workspace.dependencies]
# Async runtime
tokio = { version = "1.35", features = ["full"] }
tokio-util = "0.7"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bincode = "1.3"

# Cryptography
curve25519-dalek = "4.1"
ed25519-dalek = { version = "2.0", features = ["rand_core"] }
blake2 = "0.10"
sha2 = "0.10"
rand = "0.8"
rand_core = "0.6"

# Networking
libp2p = { version = "0.53", features = ["tcp", "mdns", "noise", "yamux", "gossipsub", "kad"] }

# Database/Storage
rocksdb = "0.21"

# Logging and tracing
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Error handling
anyhow = "1.0"
thiserror = "1.0"

# Configuration
config = "0.14"
clap = { version = "4.4", features = ["derive"] }
toml = "0.8"

# Utilities
uuid = { version = "1.6", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
bytes = "1.5"
futures = "0.3"
async-trait = "0.1"

# HTTP/WebSocket
axum = "0.7"
tokio-tungstenite = "0.20"

# IPFS integration
ipfs-api-backend-hyper = { version = "0.6", features = ["with-hyper-rustls"] }

[profile.release]
lto = true
codegen-units = 1
panic = "abort"

[profile.dev]
opt-level = 0
debug = true
EOF

echo "📦 Creating individual crates..."

# Function to create a crate
create_crate() {
    local name=$1
    local crate_type=${2:-"--lib"}
    
    cd crates
    cargo new $crate_type "$name"
    cd ..
}

# Create all crates
create_crate "catalyst-core"
create_crate "catalyst-consensus"
create_crate "catalyst-storage"
create_crate "catalyst-network"
create_crate "catalyst-runtime-evm"
create_crate "catalyst-runtime-svm"
create_crate "catalyst-service-bus"
create_crate "catalyst-dfs"
create_crate "catalyst-crypto"
create_crate "catalyst-rpc"
create_crate "catalyst-config"
create_crate "catalyst-utils"
create_crate "catalyst-cli" "--bin"

echo "🔧 Setting up individual crate dependencies..."

# Set up catalyst-core dependencies
cat > crates/catalyst-core/Cargo.toml << 'EOF'
[package]
name = "catalyst-core"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
serde.workspace = true
async-trait.workspace = true
uuid.workspace = true
chrono.workspace = true
thiserror.workspace = true
futures.workspace = true
EOF

# Set up catalyst-consensus dependencies
cat > crates/catalyst-consensus/Cargo.toml << 'EOF'
[package]
name = "catalyst-consensus"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
catalyst-core = { path = "../catalyst-core" }
tokio.workspace = true
async-trait.workspace = true
tracing.workspace = true
serde.workspace = true
anyhow.workspace = true
thiserror.workspace = true
EOF

# Set up catalyst-cli dependencies
cat > crates/catalyst-cli/Cargo.toml << 'EOF'
[package]
name = "catalyst-cli"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[[bin]]
name = "catalyst-cli"
path = "src/main.rs"

[dependencies]
catalyst-core = { path = "../catalyst-core" }
catalyst-consensus = { path = "../catalyst-consensus" }
catalyst-storage = { path = "../catalyst-storage" }
catalyst-network = { path = "../catalyst-network" }
catalyst-runtime-evm = { path = "../catalyst-runtime-evm" }
catalyst-service-bus = { path = "../catalyst-service-bus" }
catalyst-dfs = { path = "../catalyst-dfs" }
catalyst-rpc = { path = "../catalyst-rpc" }

tokio.workspace = true
clap.workspace = true
anyhow.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
serde.workspace = true
toml.workspace = true
chrono.workspace = true
EOF

# Add basic implementations for other crates
for crate in storage network runtime-evm runtime-svm service-bus dfs crypto rpc config utils; do
    cat > "crates/catalyst-$crate/Cargo.toml" << EOF
[package]
name = "catalyst-$crate"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
catalyst-core = { path = "../catalyst-core" }
tokio.workspace = true
async-trait.workspace = true
tracing.workspace = true
serde.workspace = true
anyhow.workspace = true
thiserror.workspace = true
EOF
done

# Add specific dependencies for certain crates
echo 'rocksdb.workspace = true' >> crates/catalyst-storage/Cargo.toml
echo 'libp2p.workspace = true' >> crates/catalyst-network/Cargo.toml
echo 'axum.workspace = true' >> crates/catalyst-rpc/Cargo.toml
echo 'tokio-tungstenite.workspace = true' >> crates/catalyst-service-bus/Cargo.toml
echo 'ipfs-api-backend-hyper.workspace = true' >> crates/catalyst-dfs/Cargo.toml
echo 'curve25519-dalek.workspace = true' >> crates/catalyst-crypto/Cargo.toml
echo 'ed25519-dalek.workspace = true' >> crates/catalyst-crypto/Cargo.toml
echo 'blake2.workspace = true' >> crates/catalyst-crypto/Cargo.toml
echo 'config.workspace = true' >> crates/catalyst-config/Cargo.toml
echo 'toml.workspace = true' >> crates/catalyst-config/Cargo.toml

echo "📝 Creating basic implementations..."

# Create basic lib.rs files for each crate
for crate in catalyst-core catalyst-consensus catalyst-storage catalyst-network catalyst-runtime-evm catalyst-runtime-svm catalyst-service-bus catalyst-dfs catalyst-crypto catalyst-rpc catalyst-config catalyst-utils; do
    if [ "$crate" != "catalyst-core" ]; then
        cat > "crates/$crate/src/lib.rs" << EOF
//! $crate implementation for Catalyst Network

use catalyst_core::*;

// TODO: Implement $crate functionality

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
EOF
    fi
done

echo "📋 Creating configuration files..."

# Create default configuration
mkdir -p configs
cat > configs/default.toml << 'EOF'
[node]
name = "catalyst-node"
private_key_file = "node.key"
auto_generate_identity = true

[network]
listen_addresses = ["/ip4/0.0.0.0/tcp/30333"]
bootstrap_peers = []
max_peers = 50
min_peers = 5
protocol_version = "catalyst/1.0"
mdns_discovery = true
dht_enabled = true

[network.timeouts]
connection_timeout = 30
request_timeout = 10
keep_alive_interval = 30

[storage]
data_dir = "data"
enabled = false
capacity_gb = 10
cache_size_mb = 256
write_buffer_size_mb = 64
max_open_files = 1000
compression_enabled = true

[consensus]
cycle_duration_seconds = 60
freeze_time_seconds = 5
max_transactions_per_block = 1000
min_producer_count = 3
max_producer_count = 100

[consensus.phase_timeouts]
construction_timeout = 15
campaigning_timeout = 15
voting_timeout = 15
synchronization_timeout = 15

[runtimes.evm]
enabled = true
gas_limit = 8000000
gas_price = 1000000000
max_code_size = 24576
debug_enabled = false

[runtimes.svm]
enabled = false
compute_unit_limit = 200000
max_account_data_size = 10485760

[runtimes.wasm]
enabled = false
max_memory_pages = 1024
execution_timeout_ms = 5000

[service_bus]
enabled = true
websocket_address = "0.0.0.0"
websocket_port = 8546
max_connections = 1000
event_buffer_size = 10000
webhooks_enabled = true
webhook_timeout = 30
webhook_max_retries = 3

[dfs]
enabled = true
ipfs_api_url = "http://127.0.0.1:5001"
ipfs_gateway_url = "http://127.0.0.1:8080"
cache_dir = "dfs_cache"
cache_size_gb = 5
pinning_enabled = true
gc_enabled = true
gc_interval_hours = 24

[rpc]
enabled = false
address = "127.0.0.1"
port = 8545
cors_enabled = true
cors_origins = ["*"]
auth_enabled = false
rate_limit = 100
request_timeout = 30

[logging]
level = "info"
format = "text"
file_enabled = true
file_path = "logs/catalyst.log"
max_file_size_mb = 100
max_files = 10
console_enabled = true
EOF

# Copy for docker
cp configs/default.toml configs/docker.toml

echo "🐳 Creating Docker configuration..."

# Create Dockerfile
cat > Dockerfile << 'EOF'
# Multi-stage build for Catalyst Node
FROM rust:1.75-bullseye as builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    cmake \
    git \
    clang \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy Cargo files
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# Build the project
RUN cargo build --release --workspace

# Runtime stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl1.1 \
    curl \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Create app user
RUN useradd -m -u 1001 catalyst

# Create necessary directories
RUN mkdir -p /app/data /app/logs /app/configs /app/dfs_cache \
    && chown -R catalyst:catalyst /app

# Copy binary from builder stage
COPY --from=builder /app/target/release/catalyst-cli /usr/local/bin/catalyst

# Copy default configuration
COPY configs/docker.toml /app/catalyst.toml

# Switch to app user
USER catalyst
WORKDIR /app

# Expose ports
EXPOSE 30333 8545 8546

# Default command
CMD ["catalyst", "start", "--config", "/app/catalyst.toml"]
EOF

echo "📖 Creating documentation..."

# Create README
cat > README.md << 'EOF'
# Catalyst Network Node

A truly decentralized blockchain node implementation that prioritizes accessibility, fairness, and real-world utility.

## Quick Start

```bash
# Build the project
make build

# Run a basic node
make run

# Run as validator
make run-validator
```

## Development

```bash
# Set up development environment
make setup

# Run tests
make test

# Format code
make fmt
```

See the full documentation for more details.
EOF

echo "⚙️ Creating development tools..."

# Create Makefile
cat > Makefile << 'EOF'
.PHONY: help build test run clean setup dev

help:
	@echo "Catalyst Node Development Commands"
	@echo "================================="
	@echo "  setup     - Install dependencies"
	@echo "  build     - Build the project"
	@echo "  test      - Run tests"
	@echo "  run       - Run the node"
	@echo "  clean     - Clean build artifacts"

setup:
	rustup update
	rustup component add rustfmt clippy

build:
	cargo build --release --workspace

dev:
	cargo build --workspace

test:
	cargo test --workspace

run: build
	./target/release/catalyst-cli start

clean:
	cargo clean
EOF

# Create .gitignore
cat > .gitignore << 'EOF'
# Rust
/target/
**/*.rs.bk
Cargo.lock

# IDE
.vscode/
.idea/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db

# Catalyst specific
/data/
/logs/
/dfs_cache/
node.key
identity.json
*.log

# Docker
.dockerignore
EOF

# Create basic tests
mkdir -p tests
cat > tests/integration_test.rs << 'EOF'
//! Integration tests for Catalyst Network

#[tokio::test]
async fn test_node_startup() {
    // TODO: Add integration tests
    assert!(true);
}
EOF

echo "📄 Creating license..."

cat > LICENSE << 'EOF'
MIT License

Copyright (c) 2025 Catalyst Network

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
EOF

echo "🔧 Initializing Git repository..."
git init
git add .
git commit -m "Initial commit: Catalyst Network Node project structure"

echo ""
echo "✅ Catalyst Network Node project setup complete!"
echo ""
echo "📁 Project created in: $(pwd)"
echo ""
echo "🚀 Next steps:"
echo "1. cd catalyst-node"
echo "2. make setup      # Install development dependencies"
echo "3. make build      # Build the project"
echo "4. make test       # Run tests"
echo "5. make run        # Start the node"
echo ""
echo "📚 Key files:"
echo "- Cargo.toml                    # Workspace configuration"
echo "- crates/catalyst-core/         # Core traits and types"
echo "- crates/catalyst-cli/          # Command-line interface"
echo "- configs/default.toml          # Default configuration"
echo "- Makefile                      # Development commands"
echo "- README.md                     # Project documentation"
echo ""
echo "🌟 Happy building! The future of blockchain is in your hands."
EOF

chmod +x setup.sh