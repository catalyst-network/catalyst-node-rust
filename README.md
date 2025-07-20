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
│ IPFS-based      │ EVM/SVM/WASM    │ WebSocket +     │
│ Distributed     │ Multi-runtime   │ Webhook Events  │
└─────────────────┴─────────────────┴─────────────────┘
```

## 🚀 Quick Start

### Prerequisites

- Rust 1.75+ ([install](https://rustup.rs/))
- Git
- IPFS ([install](https://ipfs.io/docs/install/))

### Build and Run

```bash
# Clone the repository
git clone https://github.com/catalyst-network/catalyst-node
cd catalyst-node

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

### Docker

```bash
# Build Docker image
make docker-build

# Run with Docker
make docker-run
```

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
catalyst-node/
├── crates/
│   ├── catalyst-core/         # Core traits and types
│   ├── catalyst-consensus/    # Collaborative consensus implementation
│   ├── catalyst-network/      # P2P networking (libp2p)
│   ├── catalyst-storage/      # RocksDB storage layer
│   ├── catalyst-runtime-evm/  # Ethereum Virtual Machine runtime
│   ├── catalyst-runtime-svm/  # Solana Virtual Machine runtime
│   ├── catalyst-service-bus/  # Web2 integration service bus
│   ├── catalyst-dfs/          # IPFS-based distributed file system
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
        println!("111111111111111111");
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
- **Work-Based Rewards**: Earn through network contribution
- **Dynamic Supply**: 1-2% annual inflation based on network needs
- **Low Fees**: Optimized for usage, not speculation

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
- **GitHub**: [Contribute on GitHub](https://github.com/catalyst-network/catalyst-node)
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