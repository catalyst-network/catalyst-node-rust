pub mod types;
pub mod traits;
pub mod error;
pub mod events;

pub use types::*;
pub use traits::*;
pub use error::*;
pub use events::*;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Core result type used throughout the Catalyst system
pub type CatalystResult<T> = Result<T, CatalystError>;

/// Node identifier for network participants
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub Uuid);

impl NodeId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Block height type
pub type BlockHeight = u64;

/// Transaction hash type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxHash(pub [u8; 32]);

/// Block hash type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockHash(pub [u8; 32]);

/// Account address type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Address(pub [u8; 21]); // 20 bytes + 1 byte prefix as per your spec

/// Token amount type (using u128 for large values)
pub type TokenAmount = u128;

/// Gas/resource consumption type
pub type Gas = u64;

/// Timestamp type
pub type Timestamp = u64;

/// Configuration for different node roles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeRole {
    /// Basic user node - can create and relay transactions
    User,
    /// Worker node - eligible for producer selection
    Worker { 
        worker_pass: WorkerPass,
        resource_proof: ResourceProof,
    },
    /// Producer node - actively participating in consensus
    Producer { 
        producer_id: NodeId,
        cycle_id: u64,
    },
    /// Storage node - providing distributed file storage
    Storage {
        storage_capacity: u64,
        available_space: u64,
    },
}

/// Worker pass for participating in consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerPass {
    pub node_id: NodeId,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
    pub partition_id: Option<u32>, // Which ledger partition
}

/// Proof of available computing resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceProof {
    pub cpu_score: u32,      // CoreMark benchmark result
    pub memory_mb: u32,      // Available RAM in MB
    pub storage_gb: u32,     // Available storage in GB
    pub bandwidth_mbps: u32, // Network bandwidth
    pub timestamp: Timestamp,
    pub signature: Vec<u8>,  // Proof signature
}

/// Ledger cycle information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerCycle {
    pub cycle_id: u64,
    pub start_time: Timestamp,
    pub duration_ms: u32,
    pub partition_id: u32,
    pub producer_count: u32,
}

/// Network message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkMessage {
    /// Transaction broadcast
    Transaction(Transaction),
    /// Block proposal or update
    Block(Block),
    /// Consensus-related messages
    Consensus(ConsensusMessage),
    /// Service bus events
    Event(Event),
    /// Peer discovery and maintenance
    Peer(PeerMessage),
}

/// Consensus phase messages from your 4-phase protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusMessage {
    /// Construction phase - producer quantities
    ProducerQuantity {
        producer_id: NodeId,
        hash_value: BlockHash,
        cycle_id: u64,
    },
    /// Campaigning phase - producer candidates
    ProducerCandidate {
        producer_id: NodeId,
        candidate_hash: BlockHash,
        producer_list_hash: [u8; 32],
        cycle_id: u64,
    },
    /// Voting phase - producer votes
    ProducerVote {
        producer_id: NodeId,
        ledger_update_hash: BlockHash,
        voter_list_hash: [u8; 32],
        cycle_id: u64,
    },
    /// Synchronization phase - final output
    ProducerOutput {
        producer_id: NodeId,
        dfs_address: String,
        voter_list_hash: [u8; 32],
        cycle_id: u64,
    },
}

/// Peer networking messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerMessage {
    /// Node announcement
    Announce {
        node_id: NodeId,
        role: NodeRole,
        address: String,
    },
    /// Heartbeat/keep-alive
    Heartbeat {
        node_id: NodeId,
        timestamp: Timestamp,
    },
    /// Resource update
    ResourceUpdate {
        node_id: NodeId,
        proof: ResourceProof,
    },
}