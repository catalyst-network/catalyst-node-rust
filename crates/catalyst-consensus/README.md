# Catalyst Consensus

The `catalyst-consensus` crate implements the collaborative consensus mechanism for the Catalyst Network. This is a novel 4-phase consensus protocol that enables democratic, energy-efficient consensus without the waste associated with Proof-of-Work mining.

## Overview

The Catalyst consensus protocol replaces competitive mining with collaborative consensus among selected producer nodes. Instead of racing to solve cryptographic puzzles, producers work together through four distinct phases to build and validate ledger state updates.

## Key Features

- **4-Phase Collaborative Process**: Construction → Campaigning → Voting → Synchronization
- **Energy Efficient**: No wasteful cryptographic puzzle solving
- **Democratic**: All selected producers participate and are rewarded
- **Deterministic Producer Selection**: Uses XOR-based randomization from previous state
- **Supermajority Thresholds**: Configurable confidence levels for each phase
- **Compensation System**: Automatic reward distribution to participating producers

## Architecture

# Catalyst Consensus

The `catalyst-consensus` crate implements the collaborative consensus mechanism for the Catalyst Network. This is a novel 4-phase consensus protocol that enables democratic, energy-efficient consensus without the waste associated with Proof-of-Work mining.

## Overview

The Catalyst consensus protocol replaces competitive mining with collaborative consensus among selected producer nodes. Instead of racing to solve cryptographic puzzles, producers work together through four distinct phases to build and validate ledger state updates.

## Key Features

- **4-Phase Collaborative Process**: Construction → Campaigning → Voting → Synchronization
- **Energy Efficient**: No wasteful cryptographic puzzle solving
- **Democratic**: All selected producers participate and are rewarded
- **Asynchronous Operation**: Built on tokio for high-performance async execution
- **Network Integration**: Uses catalyst-network for P2P communication
- **Cryptographic Security**: Leverages catalyst-crypto for signatures and hashing

## Architecture

### Core Components

- **`CatalystConsensus`**: Main consensus engine orchestrating the 4-phase process
- **`ConsensusProtocol`**: Trait defining consensus behavior for block proposals and voting
- **`ConsensusPhase`**: Enum representing the current phase of consensus
- **`LedgerCycle`**: Management of consensus cycles and timing
- **`ConsensusMessage`**: Message types for inter-node communication during consensus

### Message Types

- **`ProducerQuantity`**: Construction phase - hash of partial ledger update
- **`ProducerCandidate`**: Campaigning phase - proposed majority update
- **`ProducerVote`**: Voting phase - vote on final ledger state
- **`ProducerOutput`**: Synchronization phase - finalized update location

## The 4-Phase Protocol

### Phase 1: Construction (ConsensusPhase::Construction)
Producers collect transactions and create partial ledger state updates, broadcasting their producer quantities.

### Phase 2: Campaigning (ConsensusPhase::Campaigning)
Producers identify the most common partial update and broadcast candidates representing the majority consensus.

### Phase 3: Voting (ConsensusPhase::Voting)
Producers vote on final ledger states and create compensation entries for rewards.

### Phase 4: Synchronization (ConsensusPhase::Synchronization)
Final consensus is reached and the agreed ledger update is broadcast to the network.

## Usage

### Basic Setup

```rust
use catalyst_consensus::{CatalystConsensus, ConsensusConfig};
use catalyst_crypto::KeyPair;
use catalyst_network::{NetworkService, NetworkConfig};
use std::sync::Arc;

// Create consensus configuration
let config = ConsensusConfig {
    phase_duration_ms: 10_000,    // 10 seconds per phase
    min_producers: 3,
    max_producers: 100,
    message_timeout_ms: 5_000,
};

// Setup network and crypto
let mut rng = rand::thread_rng();
let keypair = KeyPair::generate(&mut rng);
let node_id = [1u8; 32];
let network_config = NetworkConfig::default();
let network = Arc::new(NetworkService::new(network_config).await?);

// Create consensus engine
let consensus = CatalystConsensus::new(node_id, keypair, network);
```

### Starting Consensus

```rust
// Start the consensus engine (runs indefinitely)
consensus.start().await?;
```

### Block Proposal and Voting

```rust
use catalyst_consensus::{Transaction, ConsensusProtocol};

// Create transactions
let transactions = vec![
    Transaction {
        id: [1u8; 32],
        data: b"Transfer 100 KAT".to_vec(),
    },
];

// Propose a block
let mut consensus_engine = consensus;
let proposal = consensus_engine.propose_block(transactions).await?;

// Validate the proposal
let is_valid = consensus_engine.validate_proposal(&proposal).await?;

// Vote on the proposal
if is_valid {
    let signature = catalyst_crypto::Signature::new([0u8; 64]);
    consensus_engine.vote(&proposal.hash, signature).await?;
}
```

## Configuration

### ConsensusConfig Options

```rust
pub struct ConsensusConfig {
    pub phase_duration_ms: u32,    // Duration of each phase
    pub min_producers: u32,        // Minimum producers required
    pub max_producers: u32,        // Maximum producers allowed
    pub message_timeout_ms: u32,   // Timeout for message collection
}
```

### Recommended Settings

- **Development**: 5-10 second phases, 2-5 producers
- **Testnet**: 10-15 second phases, 5-10 producers  
- **Mainnet**: 15-30 second phases, 10+ producers

## Message Flow

1. **Construction Phase**: Each producer broadcasts `ProducerQuantity` messages
2. **Campaigning Phase**: Producers broadcast `ProducerCandidate` messages
3. **Voting Phase**: Producers broadcast `ProducerVote` messages
4. **Synchronization Phase**: Final `ProducerOutput` messages are broadcast

## Error Handling

The consensus engine provides detailed error types:

```rust
pub enum ConsensusError {
    InvalidProposal(String),
    NetworkError(String),
    ValidationError(String),
}
```

## Examples

Run the simple consensus example:

```bash
cargo run --example simple_consensus
```

## Testing

Run the test suite:

```bash
# All tests
cargo test

# Specific test
cargo test test_consensus_creation

# With output
cargo test -- --nocapture
```

## Integration

The consensus crate integrates with:

- **`catalyst-network`**: P2P communication and message broadcasting
- **`catalyst-crypto`**: Cryptographic operations and key management
- **Future integration**: `catalyst-utils`, `catalyst-storage`, `catalyst-config`

## Dependencies

- `catalyst-network`: Network layer for P2P communication
- `catalyst-crypto`: Cryptographic primitives
- `tokio`: Async runtime
- `serde`: Serialization framework
- `tracing`: Logging and observability
- `hex`: Hexadecimal encoding utilities

## Implementation Status

✅ **Completed:**
- Core consensus engine structure
- 4-phase protocol framework
- Basic block proposal and voting
- Network integration
- Async operation support
- Configuration management

🚧 **In Progress:**
- Full message serialization/deserialization
- Producer selection algorithms
- Reward distribution mechanisms
- Enhanced validation logic

📋 **Planned:**
- DFS integration for ledger storage
- Advanced producer reputation
- Cross-shard consensus coordination
- Performance optimizations

## Performance

Current implementation provides:

- **Phase Duration**: 5-30 seconds per phase configurable
- **Producer Support**: 2-100+ producers
- **Async Operations**: Non-blocking network and consensus operations
- **Memory Efficient**: Minimal state retention between cycles

## Contributing

1. Follow existing code patterns and async/await usage
2. Add tests for new functionality
3. Use appropriate logging with tracing
4. Handle errors gracefully with detailed context
5. Update documentation for API changes

## License

This crate is part of the Catalyst Network project. See the project root for license information.