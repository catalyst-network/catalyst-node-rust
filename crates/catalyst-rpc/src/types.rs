//! RPC type definitions
//! Create this as crates/catalyst-rpc/src/types.rs

use serde::{Deserialize, Serialize};

/// RPC transaction request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTransactionRequest {
    pub from: String,
    pub to: Option<String>,
    pub value: Option<String>,
    pub data: Option<String>,
    pub gas_limit: Option<u64>,
    pub gas_price: Option<String>,
}

/// RPC block representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcBlock {
    pub hash: String,
    pub number: u64,
    pub parent_hash: String,
    pub timestamp: u64,
    pub transactions: Vec<RpcTransactionSummary>,
    pub transaction_count: usize,
    pub size: u64,
}

/// RPC transaction representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTransaction {
    pub hash: String,
    pub block_hash: Option<String>,
    pub block_number: Option<u64>,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
    pub data: String,
    pub gas_limit: u64,
    pub gas_price: String,
    pub gas_used: Option<u64>,
    pub status: Option<String>,
}

/// RPC transaction summary (in blocks)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTransactionSummary {
    pub hash: String,
    pub from: String,
    pub to: Option<String>,
    pub value: String,
}

/// RPC account representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcAccount {
    pub address: String,
    pub balance: String,
    pub account_type: String,
    pub nonce: u64,
}

/// RPC network information
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

/// RPC sync status
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

/// RPC node status for health checks
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