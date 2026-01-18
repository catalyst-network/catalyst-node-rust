//! JSON-RPC server for Catalyst Network
//! 
//! Provides standard blockchain RPC methods for interacting with the network,
//! including transaction submission, account queries, and network information.

use async_trait::async_trait;
use jsonrpsee::{
    core::RpcResult,
    proc_macros::rpc,
    server::ServerHandle,
    types::ErrorObjectOwned,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use thiserror::Error;

// Note: The initial scaffold referenced sub-modules (`methods`, `server`, `types`) that
// aren't present yet. Keeping the RPC types and traits in this file for now so the
// crate builds successfully.

#[derive(Error, Debug)]
pub enum RpcServerError {
    #[error("Server error: {0}")]
    Server(String),
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    #[error("Transaction not found: {0}")]
    TransactionNotFound(String),
    #[error("Block not found: {0}")]
    BlockNotFound(String),
    #[error("Account not found: {0}")]
    AccountNotFound(String),
    #[error("Network error: {0}")]
    Network(String),
}

impl From<RpcServerError> for ErrorObjectOwned {
    fn from(err: RpcServerError) -> Self {
        use jsonrpsee::types::error::{
            CALL_EXECUTION_FAILED_CODE, INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE,
        };

        match err {
            RpcServerError::InvalidParams(msg) => {
                ErrorObjectOwned::owned(INVALID_PARAMS_CODE, msg, None::<()>)
            }
            RpcServerError::TransactionNotFound(_) |
            RpcServerError::BlockNotFound(_) |
            RpcServerError::AccountNotFound(_) => {
                ErrorObjectOwned::owned(CALL_EXECUTION_FAILED_CODE, err.to_string(), None::<()>)
            }
            _ => ErrorObjectOwned::owned(INTERNAL_ERROR_CODE, err.to_string(), None::<()>),
        }
    }
}

/// RPC server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    /// Server bind address
    pub bind_address: SocketAddr,
    /// Maximum connections
    pub max_connections: u32,
    /// Enable HTTP
    pub enable_http: bool,
    /// Enable WebSocket
    pub enable_ws: bool,
    /// CORS allowed origins
    pub cors_origins: Vec<String>,
    /// Request timeout in seconds
    pub request_timeout: u64,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:9933".parse().unwrap(),
            max_connections: 100,
            enable_http: true,
            enable_ws: true,
            cors_origins: vec!["*".to_string()],
            request_timeout: 30,
        }
    }
}

/// Main RPC API trait defining all available methods
#[rpc(server)]
pub trait CatalystRpc {
    /// Get the current block number
    #[method(name = "catalyst_blockNumber")]
    async fn block_number(&self) -> RpcResult<u64>;
    
    /// Get block by hash
    #[method(name = "catalyst_getBlockByHash")]
    async fn get_block_by_hash(&self, hash: String, full_transactions: bool) -> RpcResult<Option<RpcBlock>>;
    
    /// Get block by number
    #[method(name = "catalyst_getBlockByNumber")]
    async fn get_block_by_number(&self, number: u64, full_transactions: bool) -> RpcResult<Option<RpcBlock>>;
    
    /// Get transaction by hash
    #[method(name = "catalyst_getTransactionByHash")]
    async fn get_transaction_by_hash(&self, hash: String) -> RpcResult<Option<RpcTransaction>>;
    
    /// Get account balance
    #[method(name = "catalyst_getBalance")]
    async fn get_balance(&self, address: String) -> RpcResult<String>;
    
    /// Get account information
    #[method(name = "catalyst_getAccount")]
    async fn get_account(&self, address: String) -> RpcResult<Option<RpcAccount>>;
    
    /// Send raw transaction
    #[method(name = "catalyst_sendRawTransaction")]
    async fn send_raw_transaction(&self, data: String) -> RpcResult<String>;
    
    /// Estimate transaction fee
    #[method(name = "catalyst_estimateFee")]
    async fn estimate_fee(&self, transaction: RpcTransactionRequest) -> RpcResult<String>;
    
    /// Get network information
    #[method(name = "catalyst_networkInfo")]
    async fn network_info(&self) -> RpcResult<RpcNetworkInfo>;
    
    /// Get node synchronization status
    #[method(name = "catalyst_syncing")]
    async fn syncing(&self) -> RpcResult<RpcSyncStatus>;
    
    /// Get peer count
    #[method(name = "catalyst_peerCount")]
    async fn peer_count(&self) -> RpcResult<u64>;
    
    /// Get node version
    #[method(name = "catalyst_version")]
    async fn version(&self) -> RpcResult<String>;
}

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

/// RPC server trait for managing the server lifecycle
#[async_trait]
pub trait RpcServer: Send + Sync {
    /// Start the RPC server
    async fn start(&mut self) -> Result<ServerHandle, RpcServerError>;
    
    /// Stop the RPC server
    async fn stop(&mut self) -> Result<(), RpcServerError>;
    
    /// Get server statistics
    async fn stats(&self) -> Result<RpcServerStats, RpcServerError>;
}

/// RPC server statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcServerStats {
    pub active_connections: u32,
    pub total_requests: u64,
    pub total_responses: u64,
    pub errors: u64,
    pub uptime_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_config_default() {
        let config = RpcConfig::default();
        assert!(config.enable_http);
        assert!(config.enable_ws);
        assert_eq!(config.max_connections, 100);
    }
    
    #[test]
    fn test_rpc_types_serialization() {
        let account = RpcAccount {
            address: "0x123".to_string(),
            balance: "1000000".to_string(),
            account_type: "user".to_string(),
            nonce: 0,
        };
        
        let json = serde_json::to_string(&account).unwrap();
        let deserialized: RpcAccount = serde_json::from_str(&json).unwrap();
        assert_eq!(account.address, deserialized.address);
    }
}