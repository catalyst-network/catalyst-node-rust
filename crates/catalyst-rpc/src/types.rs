//! RPC type definitions
//! Enhanced version of crates/catalyst-rpc/src/types.rs

use serde::{Deserialize, Serialize};

// === EXISTING TYPES (Enhanced) ===

/// RPC transaction request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTransactionRequest {
    pub from: String,
    pub to: Option<String>,
    pub value: Option<String>,
    pub data: Option<String>,
    pub gas_limit: Option<String>, // Changed to String for consistency
    pub gas_price: Option<String>,
    pub nonce: Option<String>, // Added nonce field
}

/// RPC block representation (enhanced)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcBlock {
    pub hash: String,
    pub number: u64,
    pub parent_hash: String,
    pub timestamp: u64,
    pub transactions: Vec<RpcTransactionSummary>,
    pub transaction_count: usize,
    pub size: u64,
    // New fields added:
    pub gas_limit: String,
    pub gas_used: String,
    pub validator: String,
    pub state_root: String,
    pub transactions_root: String,
}

/// RPC transaction representation (enhanced)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTransaction {
    pub hash: String,
    pub block_hash: Option<String>,
    pub block_number: Option<u64>,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub data: String,
    pub gas_limit: String, // Changed to String for consistency
    pub gas_price: String,
    pub gas_used: Option<String>, // Changed to String for consistency
    pub status: Option<String>,
    // New fields added:
    pub transaction_index: Option<u64>,
    pub nonce: u64,
}

/// RPC transaction summary (in blocks) (enhanced)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTransactionSummary {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    // New fields added:
    pub gas: String,
    pub gas_price: String,
}

/// RPC account representation (enhanced)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcAccount {
    pub address: String,
    pub balance: String,
    pub account_type: String,
    pub nonce: u64,
    // New fields added:
    pub code_hash: Option<String>,
    pub storage_hash: Option<String>,
}

/// RPC network information (keeping existing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcNetworkInfo {
    pub chain_id: u64,
    pub network_id: u64,
    pub protocol_version: String,
    pub genesis_hash: String,
    pub current_block: u64,
    pub highest_block: u64,
    pub peer_count: u64,
}

/// RPC sync status (keeping existing)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcSyncStatus {
    /// Node is synced
    Synced(bool),
    /// Node is syncing
    Syncing {
        starting_block: u64,
        current_block: u64,
        highest_block: u64,
    },
}

/// RPC node status for health checks (keeping existing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcNodeStatus {
    pub node_id: String,
    pub uptime: u64,
    pub sync_status: String,
    pub block_height: u64,
    pub peer_count: u64,
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub disk_usage: u64,
}

// === NEW TYPES (Not conflicting) ===

/// Transaction receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTransactionReceipt {
    pub transaction_hash: String,
    pub transaction_index: u64,
    pub block_hash: String,
    pub block_number: u64,
    pub from: String,
    pub to: Option<String>,
    pub cumulative_gas_used: String,
    pub gas_used: String,
    pub contract_address: Option<String>,
    pub logs: Vec<RpcLog>,
    pub status: String,
}

/// Event log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: u64,
    pub transaction_hash: String,
    pub transaction_index: u64,
    pub log_index: u64,
    pub removed: bool,
}

/// Call request for contract calls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcCallRequest {
    pub from: Option<String>,
    pub to: String,
    pub data: Option<String>,
    pub gas: Option<String>,
    pub gas_price: Option<String>,
    pub value: Option<String>,
}

/// Fee estimation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcFeeEstimate {
    pub gas_estimate: String,
    pub gas_price: String,
    pub total_fee: String,
}

/// Validator information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcValidator {
    pub address: String,
    pub stake: String,
    pub is_active: bool,
    pub reputation_score: f64,
    pub blocks_produced: u64,
    pub last_block_time: u64,
}

/// Validator status for the current node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcValidatorStatus {
    pub is_validator: bool,
    pub validator_address: Option<String>,
    pub stake: Option<String>,
    pub is_active: bool,
    pub blocks_produced: u64,
    pub last_block_produced: Option<u64>,
    pub next_block_slot: Option<u64>,
}