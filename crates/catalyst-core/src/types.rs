use serde::{Deserialize, Serialize};
use crate::{Address, TxHash, BlockHash, TokenAmount, Gas, Timestamp, NodeId};

/// Transaction structure supporting multiple types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    /// Transaction type and specific data
    pub tx_type: TransactionType,
    /// Transaction entries (inputs/outputs)
    pub entries: Vec<TransactionEntry>,
    /// Aggregated signature for all entries
    pub signature: TransactionSignature,
    /// Locking time (when transaction can be processed)
    pub lock_time: Timestamp,
    /// Transaction fees
    pub fees: TokenAmount,
    /// Timestamp when transaction was created
    pub timestamp: Timestamp,
    /// Optional data field (up to 60 bytes as per spec)
    pub data: Option<Vec<u8>>,
    /// Computed transaction hash
    pub hash: TxHash,
}

/// Transaction types supported by Catalyst
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionType {
    /// Standard token transfer (visible amounts)
    Standard,
    /// Confidential transfer (hidden amounts)
    Confidential,
    /// Smart contract deployment
    ContractDeploy {
        runtime_type: String,
        code: Vec<u8>,
        constructor_args: Vec<u8>,
    },
    /// Smart contract function call
    ContractCall {
        contract_address: Address,
        function_data: Vec<u8>,
        gas_limit: Gas,
    },
    /// Data storage request
    DataStorage {
        data_hash: String,
        storage_duration: u64,
    },
    /// File retrieval request
    DataRetrieval {
        data_hash: String,
    },
}

/// Transaction entry (input or output)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionEntry {
    /// Public key from which account address is derived
    pub public_key: [u8; 32],
    /// Amount component (positive for receiving, negative for spending)
    pub amount: AmountComponent,
}

/// Amount component (visible or hidden)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AmountComponent {
    /// Clear text amount (8 bytes)
    Visible(TokenAmount),
    /// Confidential amount with range proof
    Confidential {
        /// Pedersen commitment (32 bytes)
        commitment: [u8; 32],
        /// Range proof (using Bulletproofs)
        range_proof: Vec<u8>,
    },
}

/// Transaction signature (aggregated Schnorr signature)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionSignature {
    /// Signature scalar (32 bytes)
    pub signature: [u8; 32],
    /// Aggregated public key point (32 bytes)
    pub public_key: [u8; 32],
}

/// Block structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// Block header
    pub header: BlockHeader,
    /// Transactions in this block
    pub transactions: Vec<Transaction>,
    /// Compensation entries for producers
    pub compensation_entries: Vec<CompensationEntry>,
    /// Block signature from producers
    pub signature: BlockSignature,
}

/// Block header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    /// Block hash
    pub hash: BlockHash,
    /// Previous block hash
    pub previous_hash: BlockHash,
    /// Block height/number
    pub height: u64,
    /// Timestamp when block was created
    pub timestamp: Timestamp,
    /// Merkle root of transactions
    pub tx_merkle_root: [u8; 32],
    /// Merkle root of state changes
    pub state_merkle_root: [u8; 32],
    /// Ledger cycle ID this block belongs to
    pub cycle_id: u64,
    /// Partition ID for this block
    pub partition_id: u32,
    /// Total transaction fees collected
    pub total_fees: TokenAmount,
    /// Number of transactions in block
    pub tx_count: u32,
}

/// Compensation entry for rewarding producers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationEntry {
    /// Producer's public key
    pub public_key: [u8; 32],
    /// Reward amount (always positive)
    pub amount: TokenAmount,
    /// Type of work being rewarded
    pub work_type: WorkType,
}

/// Types of work that earn rewards
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkType {
    /// Consensus participation (producer work)
    Consensus,
    /// Voting participation
    Voting,
    /// Storage provision
    Storage { bytes_stored: u64 },
    /// Compute provision
    Compute { cycles_provided: u64 },
}

/// Block signature from consensus process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockSignature {
    /// List of producer IDs that signed
    pub signers: Vec<NodeId>,
    /// Aggregated signature
    pub signature: [u8; 64],
    /// Timestamp of signature
    pub timestamp: Timestamp,
}

/// Account state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// Account address (21 bytes: 20 bytes hash + 1 byte prefix)
    pub address: Address,
    /// Account balance
    pub balance: AccountBalance,
    /// Account nonce (for replay protection)
    pub nonce: u64,
    /// Smart contract code hash (if this is a contract account)
    pub code_hash: Option<[u8; 32]>,
    /// Storage root for contract accounts
    pub storage_root: Option<[u8; 32]>,
    /// Account type
    pub account_type: AccountType,
}

/// Account balance (visible or hidden)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountBalance {
    /// Visible balance (8 bytes)
    Visible(TokenAmount),
    /// Hidden balance as Pedersen commitment (32 bytes)
    Confidential([u8; 32]),
}

/// Account types as per specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountType {
    /// Non-confidential user account
    Standard,
    /// Confidential user account
    Confidential,
    /// Smart contract account
    Contract {
        runtime_type: String,
    },
}

/// Ledger state update structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerStateUpdate {
    /// Update ID
    pub update_id: u64,
    /// Cycle ID this update belongs to
    pub cycle_id: u64,
    /// Partition ID
    pub partition_id: u32,
    /// List of account state changes
    pub state_changes: Vec<StateChange>,
    /// Transaction entries processed
    pub transaction_entries: Vec<TransactionEntry>,
    /// Hash tree of transaction signatures
    pub signature_tree_hash: [u8; 32],
    /// Compensation entries for this cycle
    pub compensation_entries: Vec<CompensationEntry>,
    /// Timestamp of update
    pub timestamp: Timestamp,
    /// Hash of this update
    pub hash: [u8; 32],
}

/// Individual state change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    /// Account address affected
    pub address: Address,
    /// Type of change
    pub change_type: StateChangeType,
    /// Previous value (if any)
    pub previous_value: Option<Vec<u8>>,
    /// New value
    pub new_value: Vec<u8>,
}

/// Types of state changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateChangeType {
    /// Account balance change
    Balance,
    /// Account nonce change
    Nonce,
    /// Contract code deployment
    Code,
    /// Contract storage change
    Storage { key: Vec<u8> },
}

/// Network peer information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Unique peer identifier
    pub node_id: NodeId,
    /// Network addresses (multiaddr format)
    pub addresses: Vec<String>,
    /// Peer role and capabilities
    pub role: crate::NodeRole,
    /// Connection status
    pub status: PeerStatus,
    /// Last seen timestamp
    pub last_seen: Timestamp,
    /// Reputation score
    pub reputation: f64,
}

/// Peer connection status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PeerStatus {
    Connected,
    Connecting,
    Disconnected,
    Failed,
    Banned,
}

/// Event emitted by the blockchain for service bus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Event ID
    pub id: String,
    /// Event type
    pub event_type: String,
    /// Block height where event occurred
    pub block_height: u64,
    /// Transaction hash that generated this event
    pub tx_hash: Option<TxHash>,
    /// Contract address that emitted the event (if applicable)
    pub contract_address: Option<Address>,
    /// Event topics (indexed parameters)
    pub topics: Vec<[u8; 32]>,
    /// Event data (non-indexed parameters)
    pub data: Vec<u8>,
    /// Timestamp when event was emitted
    pub timestamp: Timestamp,
}

/// Resource metrics for node monitoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetrics {
    /// CPU usage percentage (0-100)
    pub cpu_usage: f32,
    /// Memory usage in bytes
    pub memory_usage: u64,
    /// Available memory in bytes
    pub memory_available: u64,
    /// Disk usage in bytes
    pub disk_usage: u64,
    /// Available disk space in bytes
    pub disk_available: u64,
    /// Network bandwidth usage (bytes per second)
    pub network_in_bps: u64,
    pub network_out_bps: u64,
    /// Number of connected peers
    pub peer_count: u32,
    /// Node uptime in seconds
    pub uptime_seconds: u64,
}

/// Node status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeStatus {
    /// Node ID
    pub node_id: NodeId,
    /// Current role
    pub role: crate::NodeRole,
    /// Sync status
    pub sync_status: SyncStatus,
    /// Latest block height known
    pub latest_block_height: u64,
    /// Latest block hash
    pub latest_block_hash: BlockHash,
    /// Resource metrics
    pub metrics: ResourceMetrics,
    /// Active modules
    pub active_modules: Vec<ModuleStatus>,
}

/// Blockchain sync status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncStatus {
    /// Fully synced with network
    Synced,
    /// Currently syncing
    Syncing {
        current_block: u64,
        target_block: u64,
        progress_percent: f32,
    },
    /// Not synced (behind network)
    Behind {
        blocks_behind: u64,
    },
    /// Sync failed
    Failed {
        error: String,
    },
}

/// Module status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleStatus {
    /// Module name
    pub name: String,
    /// Module version
    pub version: String,
    /// Module state
    pub state: ModuleState,
    /// Last health check result
    pub healthy: bool,
    /// Module-specific metrics
    pub metrics: std::collections::HashMap<String, serde_json::Value>,
}

/// Module state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModuleState {
    Uninitialized,
    Initializing,
    Running,
    Stopping,
    Stopped,
    Failed { error: String },
}