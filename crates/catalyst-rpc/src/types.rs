//! Update crates/catalyst-rpc/src/types.rs with all missing types

use serde::{Deserialize, Serialize};

// === Node Information Types ===

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcPeer {
    pub id: String,
    pub address: String,
    pub protocols: Vec<String>,
    pub version: String,
    pub best_hash: String,
    pub best_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcNode {
    pub id: String,
    pub name: String,
    pub version: String,
    pub network: String,
    pub protocol_version: String,
    pub listening_addresses: Vec<String>,
    pub uptime: u64,
}

// === Blockchain State Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcBlock {
    pub hash: String,
    pub number: u64,
    pub parent_hash: String,
    pub timestamp: u64,
    pub transactions: Vec<RpcTransactionSummary>,
    pub transaction_count: usize,
    pub size: u64,
    pub gas_limit: String,
    pub gas_used: String,
    pub validator: String,
    pub state_root: String,
    pub transactions_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTransactionSummary {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub gas: String,
    pub gas_price: String,
}

// === Transaction Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTransaction {
    pub hash: String,
    pub block_hash: Option<String>,
    pub block_number: Option<u64>,
    pub transaction_index: Option<u64>,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub data: String,
    pub gas_limit: String,
    pub gas_price: String,
    pub gas_used: Option<String>,
    pub status: Option<String>,
    pub nonce: u64,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTransactionRequest {
    pub from: Option<String>,
    pub to: Option<String>,
    pub gas: Option<String>,
    pub gas_price: Option<String>,
    pub value: Option<String>,
    pub data: Option<String>,
    pub nonce: Option<u64>,
}

// === Account Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcAccount {
    pub address: String,
    pub balance: String,
    pub nonce: u64,
    pub code_hash: Option<String>,
    pub storage_hash: Option<String>,
    pub account_type: String,
}

// === Log and Filter Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_number: u64,
    pub block_hash: String,
    pub transaction_hash: String,
    pub transaction_index: u64,
    pub log_index: u64,
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcFilter {
    pub from_block: Option<String>,
    pub to_block: Option<String>,
    pub address: Option<Vec<String>>,
    pub topics: Option<Vec<Option<String>>>,
}

// === System and Health Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcHealth {
    pub is_syncing: bool,
    pub peers: u64,
    pub should_have_peers: bool,
    pub is_healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcProperties {
    pub chain: String,
    pub implementation: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcMetrics {
    pub uptime: u64,
    pub block_height: u64,
    pub peer_count: u64,
    pub transaction_pool_size: u64,
    pub memory_usage: u64,
    pub disk_usage: u64,
}

// === Sync Status Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcSyncStatus {
    Synced(bool),
    Syncing {
        starting_block: u64,
        current_block: u64,
        highest_block: u64,
    },
}

// === Validation Types ===

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcValidator {
    pub address: String,
    pub stake: String,
    pub commission: String,
    pub is_active: bool,
    pub blocks_produced: u64,
    pub uptime: f64,
}

// === Fee and Gas Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcFeeEstimate {
    pub gas_estimate: String,
    pub gas_price: String,
    pub total_fee: String,
}

// === Error Response Types ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcErrorResponse {
    pub code: i32,
    pub message: String,
    pub data: Option<serde_json::Value>,
}
