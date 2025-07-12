use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

// Core type aliases
pub type NodeId = [u8; 32];
pub type BlockHash = [u8; 32];
pub type Address = [u8; 20];
pub type TokenAmount = u64;
pub type LedgerCycle = u64;

// Additional types from original lib.rs
pub type BlockHeight = u64;
pub type Gas = u64;
pub type Timestamp = u64;

/// Transaction hash type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxHash(pub [u8; 32]);

impl fmt::Display for TxHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

/// Configuration for different node roles
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
        producer_id: [u8; 32], // Use consistent NodeId type
        cycle_id: u64,
    },
    /// Storage node - providing distributed file storage
    Storage {
        storage_capacity: u64,
        available_space: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    pub id: String,
    pub uptime: u64,
    pub sync_status: SyncStatus,
    pub metrics: ResourceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncStatus {
    Synced,
    Syncing { progress: f64 },
    NotSynced,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetrics {
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub disk_usage: u64,
}

/// Worker pass for participating in consensus
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WorkerPass {
    pub node_id: [u8; 32], // Use consistent NodeId type
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
    pub partition_id: Option<u32>, // Which ledger partition
}

// Missing ResourceEstimate type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceEstimate {
    pub compute_units: u64,
    pub memory_bytes: u64,
    pub storage_bytes: u64,
    pub network_bytes: u64,
}

impl ResourceEstimate {
    pub fn new(compute_units: u64, memory_bytes: u64, storage_bytes: u64, network_bytes: u64) -> Self {
        Self {
            compute_units,
            memory_bytes,
            storage_bytes,
            network_bytes,
        }
    }
    
    pub fn zero() -> Self {
        Self::new(0, 0, 0, 0)
    }
    
    pub fn total_cost(&self) -> u64 {
        // Simple cost model - can be made more sophisticated
        self.compute_units + self.memory_bytes / 1024 + self.storage_bytes / 1024 + self.network_bytes / 1024
    }
}

// Resource proof for validation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceProof {
    pub cpu_score: u32,      // CoreMark benchmark result
    pub memory_mb: u32,      // Available RAM in MB
    pub storage_gb: u32,     // Available storage in GB
    pub bandwidth_mbps: u32, // Network bandwidth
    pub timestamp: Timestamp,
    pub signature: Vec<u8>,  // Proof signature
}

// Transaction types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub from: Address,
    pub to: Option<Address>, // None for contract creation
    pub value: TokenAmount,
    pub data: Vec<u8>,
    pub nonce: u64,
    pub gas_limit: u64,
    pub gas_price: TokenAmount,
    pub signature: Option<Vec<u8>>, // Placeholder for actual signature
}

impl Transaction {
    pub fn new(from: Address, to: Option<Address>, value: TokenAmount, data: Vec<u8>) -> Self {
        Self {
            from,
            to,
            value,
            data,
            nonce: 0,
            gas_limit: 1_000_000,
            gas_price: 1,
            signature: None,
        }
    }
    
    pub fn estimate_resources(&self) -> ResourceEstimate {
        ResourceEstimate::new(
            self.data.len() as u64 * 10, // compute based on data size
            1024, // base memory
            self.data.len() as u64, // storage for data
            self.data.len() as u64 + 64, // network transmission
        )
    }
}

// Execution result
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub return_data: Vec<u8>,
    pub gas_used: u64,
    pub state_changes: Vec<StateChange>,
    pub events: Vec<Event>,
    pub error: Option<String>,
}

impl ExecutionResult {
    pub fn success(return_data: Vec<u8>, gas_used: u64) -> Self {
        Self {
            success: true,
            return_data,
            gas_used,
            state_changes: Vec::new(),
            events: Vec::new(),
            error: None,
        }
    }
    
    pub fn failure(error: String, gas_used: u64) -> Self {
        Self {
            success: false,
            return_data: Vec::new(),
            gas_used,
            state_changes: Vec::new(),
            events: Vec::new(),
            error: Some(error),
        }
    }
}

// State change tracking
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateChange {
    pub address: Address,
    pub key: Vec<u8>,
    pub old_value: Option<Vec<u8>>,
    pub new_value: Option<Vec<u8>>,
}

// Event system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub address: Address,
    pub topics: Vec<Vec<u8>>,
    pub data: Vec<u8>,
}

// Account state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub address: Address,
    pub balance: TokenAmount,
    pub nonce: u64,
    pub code: Vec<u8>,
    pub storage: HashMap<Vec<u8>, Vec<u8>>,
}

impl Account {
    pub fn new(address: Address) -> Self {
        Self {
            address,
            balance: 0,
            nonce: 0,
            code: Vec::new(),
            storage: HashMap::new(),
        }
    }
    
    pub fn with_balance(address: Address, balance: TokenAmount) -> Self {
        Self {
            address,
            balance,
            nonce: 0,
            code: Vec::new(),
            storage: HashMap::new(),
        }
    }
}

// Execution context
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub block_number: u64,
    pub block_timestamp: u64,
    pub block_hash: BlockHash,
    pub gas_limit: u64,
    pub gas_price: TokenAmount,
    pub caller: Address,
    pub origin: Address,
}

impl ExecutionContext {
    pub fn new(block_number: u64, caller: Address) -> Self {
        Self {
            block_number,
            block_timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            block_hash: [0u8; 32],
            gas_limit: 1_000_000,
            gas_price: 1,
            caller,
            origin: caller,
        }
    }
}

// Consensus message types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteType {
    Prevote,
    Precommit,
}

// Runtime type enumeration
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuntimeType {
    Native,
    WASM,
    SVM, // Solana Virtual Machine (for Phase 2)
}

impl Default for RuntimeType {
    fn default() -> Self {
        RuntimeType::Native
    }
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

/// Block structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub hash: BlockHash,
    pub height: BlockHeight,
    pub timestamp: Timestamp,
    pub transactions: Vec<Transaction>,
    pub previous_hash: BlockHash,
    pub merkle_root: BlockHash,
    pub producer_id: NodeId,
}