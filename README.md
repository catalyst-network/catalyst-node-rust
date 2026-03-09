# Catalyst Network Node

A truly decentralized blockchain node implementation that prioritizes accessibility, fairness, and real-world utility over speculation and wealth concentration.

## 🌟 Key Features

- **True Decentralization**: Anyone can participate without expensive hardware or large token stakes
- **Collaborative Consensus**: Energy-efficient, work-based consensus mechanism
- **Multi-Runtime Support**: EVM, SVM, and WASM smart contract execution
- **Web2 Integration**: Service bus for seamless traditional application integration
- **Modular Architecture**: Pluggable components for future extensibility
- **Fair Launch**: No pre-mine, no ICO, earn through contribution

## 🏗️ Architecture

```
┌─────────────────┬─────────────────┬─────────────────┐
│   Consensus     │    Database     │    Network      │
│   Module        │    Module       │    Module       │
├─────────────────┼─────────────────┼─────────────────┤
│ Collaborative   │ Multi-level     │ libp2p-based    │
│ 4-phase         │ RocksDB         │ Peer Discovery  │
└─────────────────┴─────────────────┴─────────────────┘
┌─────────────────┬─────────────────┬─────────────────┐
│   File System   │ Virtual Machine │    Service Bus  │
│   Module        │    Module       │    Module       │
├─────────────────┼─────────────────┼─────────────────┤
│ Content-addressed│ EVM/SVM/WASM   │ WebSocket +     │
│ DFS (CID sync)  │ Multi-runtime   │ Webhook Events  │
└─────────────────┴─────────────────┴─────────────────┘
```

## 🚀 Quick Start

### Prerequisites

- Rust 1.75+ ([install](https://rustup.rs/))
- Git
- IPFS: optional. The local testnet uses a built-in, local content store for CID-addressed data (no daemon required).

#### Linux system packages (required for building)

Some crates use native dependencies (e.g. RocksDB / zstd / OpenSSL) and require a C toolchain + headers.

- **Ubuntu/Debian**:
  - `sudo apt update && sudo apt install -y build-essential pkg-config libssl-dev clang libclang-dev cmake`
- **Fedora/RHEL**:
  - `sudo dnf install -y gcc gcc-c++ make pkgconf-pkg-config openssl-devel clang clang-devel cmake`

#### Working from a network filesystem (GVFS/SMB)

If your repo lives under a GVFS mount (paths containing `/gvfs/`), Cargo’s file locking inside `target/` may fail with **`Operation not supported (os error 95)`**.
This repo’s `makefile`/`Makefile` automatically sets `CARGO_TARGET_DIR` to a local cache dir in that case.

Additionally, some GVFS/SMB mounts prevent Cargo from updating `Cargo.lock`. In that case, `make build` will build from a **local rsynced mirror** under `~/.cache/catalyst-node/gvfs-workdir/src` to avoid lockfile write failures.

### Build and Run

```bash
# Clone the repository
git clone https://github.com/catalyst-network/catalyst-node-rust
cd catalyst-node-rust

# Set up development environment
make setup

# Build the project
make build

# Run a basic node
make run

# Run as validator
make run-validator

# Run with storage provision
make run-storage
```

## Local Testnet (recommended for validating current functionality)

This repo includes a working local testnet harness that runs **3 nodes** with:
- deterministic faucet funding
- RPC enabled on node1 (`http://127.0.0.1:8545`)
- signature + nonce validation
- LSU application into RocksDB state
- DFS-backed LSU sync (CID gossip + local content store)
- persistent mempool with deterministic re-broadcast on restart

### Start / stop

Start:

```bash
make testnet
```

Stop:

```bash
make stop-testnet
```

### Faster iteration helpers (non-blocking start + status + basic check)

`make testnet` blocks (it waits on node processes). For day-to-day testing, use:

```bash
make testnet-up
make testnet-status
make testnet-basic-test
make testnet-down
```

Tail logs:

```bash
make testnet-logs NODE=node1
```

### Contract sanity check (EVM execution)

This uses a deterministic initcode fixture (`testdata/evm/return_2a_initcode.hex`) that deploys a contract which returns `0x2a` on empty calldata.

```bash
make testnet-up
make testnet-contract-test
make testnet-down
```

### Smoke test (single command)

Runs an end-to-end test that:
- starts the 3-node testnet
- submits a faucet transaction to node1
- restarts node1 to test mempool persistence + rehydrate + deterministic re-broadcast
- verifies node1 balance increases
- stops the testnet

```bash
make smoke-testnet
```

Logs:
- `testnet/node1/logs/stdout.log`
- `testnet/node2/logs/stdout.log`
- `testnet/node3/logs/stdout.log`

### Send a transaction (faucet → node1)

Get node1 public key (used as the “address” in this scaffold):

```bash
NODE1_PUBKEY=$(grep -a "Node ID:" -m1 testnet/node1/logs/stdout.log | awk '{print $NF}')
echo "node1_pubkey=$NODE1_PUBKEY"
```

Send 25 units from the faucet:

```bash
cargo run -p catalyst-cli -- send $NODE1_PUBKEY 25 --key-file testnet/faucet.key --rpc-url http://127.0.0.1:8545
```

Check node1 balance:

```bash
cargo run -p catalyst-cli -- balance $NODE1_PUBKEY --rpc-url http://127.0.0.1:8545
```

### Query peers / chain head / nonce

Peers:

```bash
cargo run -p catalyst-cli -- peers --rpc-url http://127.0.0.1:8545
```

Head:

```bash
cargo run -p catalyst-cli -- status --rpc-url http://127.0.0.1:8545
```

Nonce (faucet):

```bash
FAUCET_PUBKEY=$(python3 -c 'print("fa"*32)')
curl -s -X POST http://127.0.0.1:8545 -H 'content-type: application/json' \
  -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"catalyst_getNonce\",\"params\":[\"0x${FAUCET_PUBKEY}\"]}"
echo
```

### Mempool persistence + deterministic re-broadcast

Pending protocol transactions are persisted in RocksDB and reloaded on startup. On restart, node1 will:
- rehydrate the mempool from persisted txs (after revalidation)
- deterministically rebroadcast them periodically (stable txid order)
- prune them once their sender nonce is applied (LSU application)

### Docker

```bash
# Build Docker image
make docker-build

# Run with Docker
make docker-run
```

## Stable Devnet (invite others to connect)

You can run a single-node “devnet” that:
- listens on a public P2P port
- exposes RPC externally (binds to `0.0.0.0`)
- prints the bootstrap multiaddr + RPC URL to share

Start (replace `HOST` with your public IP or DNS name):

```bash
make devnet-up HOST=<public_ip_or_dns> P2P_PORT=30333 RPC_PORT=8545
```

Stop:

```bash
make devnet-down
```

### How others connect

On a remote machine, they run their node and point it at your bootstrap address:

```bash
cargo run -p catalyst-cli -- start \
  --validator --rpc --rpc-address 127.0.0.1 --rpc-port 8545 \
  --bootstrap-peers "/ip4/<HOST>/tcp/30333"
```

### Bootstrap via DNS seeds (optional)

Instead of hard-coding `--bootstrap-peers`, operators can set **DNS seeds** in the node config. Seeds resolve via:
- **TXT records** (preferred): multiaddrs, `ip[:port]`, or host:port tokens
- **A/AAAA fallback**: resolved IPs mapped to `/ip{4,6}/.../tcp/<port>`

Config example:

```toml
[network]
bootstrap_peers = ["/ip4/<HOST>/tcp/30333"]
dns_seeds = ["seed1.example.org", "seed2.example.org"]
```

### Multi-hop WAN propagation

Nodes will **relay broadcast envelopes with deduplication** to improve propagation in non-fully-meshed WAN topologies (e.g. A ↔ B ↔ C).
Bootstrap dialing uses **per-peer exponential backoff with jitter** to avoid hammering unreachable peers.

### Smart contract execution (current state)

The repo supports **basic EVM execution** in the local testnet harness (see “Contract sanity check (EVM execution)”).
The CLI provides:
- `catalyst deploy <bytecode_file>`: deploy bytecode via RPC
- `catalyst call <contract> "<sig(args)>"`: call a function (read-only or state-changing if `--key-file` is provided)


## 📋 Usage

### Starting a Node

```bash
# Basic node (user role)
catalyst start

# Validator node with RPC
catalyst start --validator --rpc

# Storage provider node
catalyst start --storage --storage-capacity 100

# Full node (validator + storage + RPC)
catalyst start --validator --storage --rpc
```

### Configuration

Create a configuration file:

```bash
# Generate default configuration
catalyst start --config catalyst.toml

# Generate node identity
catalyst generate-identity --output identity.json

# Create genesis configuration
catalyst create-genesis --output genesis.json
```

### Interacting with the Network

```bash
# Check node status
catalyst status

# View connected peers
catalyst peers

# Send KAT tokens
catalyst send <recipient_address> <amount> --key-file wallet.key

# Check account balance
catalyst balance <address>

# Deploy a smart contract
catalyst deploy contract.bytecode --runtime evm --key-file wallet.key

# Call a smart contract
catalyst call <contract_address> "transfer(address,uint256)" --key-file wallet.key
```

## 🔧 Development

### Project Structure

```
catalyst-node-rust/
├── crates/
│   ├── catalyst-core/         # Core traits and types
│   ├── catalyst-consensus/    # Collaborative consensus implementation
│   ├── catalyst-network/      # P2P networking (libp2p)
│   ├── catalyst-storage/      # RocksDB storage layer
│   ├── catalyst-runtime-evm/  # Ethereum Virtual Machine runtime
│   ├── catalyst-runtime-svm/  # Solana Virtual Machine runtime
│   ├── catalyst-service-bus/  # Web2 integration service bus
│   ├── catalyst-dfs/          # Content-addressed DFS utilities
│   ├── catalyst-crypto/       # Cryptographic utilities
│   ├── catalyst-rpc/          # JSON-RPC server
│   ├── catalyst-config/       # Configuration management
│   ├── catalyst-utils/        # Common utilities
│   └── catalyst-cli/          # Command-line interface
├── configs/                   # Configuration files
├── docs/                      # Documentation
├── scripts/                   # Build and deployment scripts
└── tests/                     # Integration tests
```

### Development Commands

```bash
# Format code
make fmt

# Run lints
make clippy

# Run tests
make test

# Run benchmarks
make bench

# Generate documentation
make docs

# Start local testnet
make testnet

# Watch for changes
make watch
```

### Adding a New Module

1. Create a new crate: `cargo new --lib crates/catalyst-my-module`
2. Add to workspace in root `Cargo.toml`
3. Implement the appropriate trait from `catalyst-core`
4. Register in the main node configuration

Example module implementation:

```rust
use async_trait::async_trait;
use catalyst_core::{CatalystModule, CatalystResult};

pub struct MyModule {
    // Module state
}

#[async_trait]
impl CatalystModule for MyModule {
    fn name(&self) -> &'static str {
        "my-module"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    async fn initialize(&mut self) -> CatalystResult<()> {
        // Initialize module
        Ok(())
    }

    async fn start(&mut self) -> CatalystResult<()> {
        // Start module processing
        Ok(())
    }

    async fn stop(&mut self) -> CatalystResult<()> {
        // Stop module gracefully
        Ok(())
    }

    async fn health_check(&self) -> CatalystResult<bool> {
        // Check module health
        Ok(true)
    }
}
```

## 🌐 Web2 Integration

### Service Bus Example

Connect traditional applications to blockchain events:

```javascript
// Node.js example
const CatalystServiceBus = require('catalyst-service-bus');

const bus = new CatalystServiceBus('ws://localhost:8546');

// Listen for token transfers
bus.on('token_transfer', (event) => {
    console.log('Token transfer:', event);
    
    // Update your traditional database
    updateUserBalance(event.to_address, event.amount);
    
    // Send notification
    sendPushNotification(event.to_address, 'Payment received');
});

bus.connect();
```

```python
# Python example
from catalyst_service_bus import ServiceBus

bus = ServiceBus('ws://localhost:8546')

@bus.on('contract_event')
def handle_contract_event(event):
    if event.event_name == 'Transfer':
        # Process transfer event
        process_transfer(event.data)

bus.start()
```

### Smart Contract Deployment

Deploy existing Ethereum contracts:

```bash
# Deploy with Hardhat/Truffle configuration
npx hardhat deploy --network catalyst

# Or use Catalyst CLI directly
catalyst deploy MyContract.sol --runtime evm --args "constructor_arg"
```

## 📊 Economics

### Token Model (KAT)

- **Fair Launch**: No pre-mine, no ICO
- **Genesis Supply**: `0 KAT` (issuance starts after genesis)
- **Work-Based Rewards**: Earn through network contribution
- **Fixed Issuance Baseline**: `1 KAT` per successful cycle (v1 model)
- **Low Fees**: Optimized for usage, not speculation

Canonical parameter details live in `docs/tokenomics-model.md`.

### Earning Opportunities

1. **Validator Rewards**: Participate in consensus
2. **Storage Rewards**: Provide file storage
3. **Compute Rewards**: Execute smart contracts
4. **Development Rewards**: Contribute code

### Hardware Requirements

**Minimum (User Node):**
- 2 CPU cores
- 4 GB RAM
- 20 GB storage
- Broadband internet

**Recommended (Validator):**
- 4 CPU cores
- 8 GB RAM
- 100 GB SSD storage
- Stable internet connection

**No Special Hardware Required** - runs on commodity computers

## 🔐 Security

### Consensus Security

- **Byzantine Fault Tolerance**: Secure with <33% malicious nodes
- **Sybil Resistance**: Resource proofs prevent fake nodes
- **Economic Security**: Attack cost scales with network size

### Cryptographic Foundations

- **Curve25519**: Elliptic curve for signatures and keys
- **Blake2b**: Fast, secure hashing
- **Bulletproofs**: Confidential transaction privacy
- **Schnorr Signatures**: Efficient signature aggregation

## 🧪 Testing

### Unit Tests

```bash
# Run all tests
make test

# Run specific module tests
cargo test --package catalyst-consensus

# Run with output
cargo test -- --nocapture
```

### Integration Tests

```bash
# Run integration tests
make test-integration

# Start local testnet for testing
make testnet
```

### Benchmarks

```bash
# Run performance benchmarks
make bench

# Profile performance
make profile
```

## 📚 Documentation

- [Technical Consensus Paper](docs/consensus-protocol.pdf) - Detailed mathematical specification
- [API Documentation](https://docs.catalyst.network) - Complete API reference
- [Developer Guide](docs/developer-guide.md) - Building on Catalyst
- [Node Operator Guide](docs/node-operator-guide.md) - Running infrastructure

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md).

### Getting Started

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/amazing-feature`
3. Make your changes
4. Run tests: `make test`
5. Submit a pull request

### Code Standards

- Format code: `make fmt`
- Pass lints: `make clippy`
- Add tests for new features
- Update documentation

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🌍 Community

- **Discord**: [Join our Discord](https://discord.gg/catalyst)
- **GitHub**: [Contribute on GitHub](https://github.com/catalyst-network/catalyst-node-rust)
- **Forum**: [Community Forum](https://forum.catalyst.network)
- **Documentation**: [docs.catalyst.network](https://docs.catalyst.network)

## 🗺️ Roadmap

### Phase 1: Foundation (Q1-Q2 2025)
- ✅ Modular architecture
- ✅ Collaborative consensus
- ✅ EVM compatibility
- ✅ Basic networking
- 🔄 Fair launch preparation

### Phase 2: Integration (Q3-Q4 2025)
- 🔄 Service bus implementation
- 📋 SVM runtime integration
- 📋 Mobile applications
- 📋 Developer tools expansion

### Phase 3: Ecosystem (2026+)
- 📋 Cross-chain bridges
- 📋 Enterprise tools
- 📋 Additional runtimes
- 📋 Global scaling

## ❓ FAQ

**Q: How is Catalyst different from Ethereum?**
A: Catalyst prioritizes accessibility over artificial scarcity. No 32 ETH required to validate, collaborative consensus instead of competitive staking.

**Q: Can I run existing Ethereum contracts?**
A: Yes! Catalyst supports EVM compatibility, so existing Ethereum smart contracts can be deployed without modification.

**Q: How do I earn rewards?**
A: Run a node and contribute resources - validation, storage, or computation. Rewards are proportional to contribution, not capital.

**Q: What's the service bus?**
A: A WebSocket/webhook system that lets traditional web applications receive blockchain events like database triggers.

**Q: Is there a token sale?**
A: No. Catalyst launches fairly like Bitcoin - anyone can run a node and earn from day one.

---

*"The future of blockchain is not about who can afford to participate, but about who chooses to contribute."*