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
use std::sync::Arc;
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

    /// Get applied head info (cycle/hash/state_root)
    #[method(name = "catalyst_head")]
    async fn head(&self) -> RpcResult<RpcHead>;
}

/// Minimal head info based on applied LSU/state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcHead {
    pub applied_cycle: u64,
    pub applied_lsu_hash: String,
    pub applied_state_root: String,
    pub last_lsu_cid: Option<String>,
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

fn parse_hex_32(s: &str) -> Result<[u8; 32], RpcServerError> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).map_err(|e| RpcServerError::InvalidParams(e.to_string()))?;
    if bytes.len() != 32 {
        return Err(RpcServerError::InvalidParams(
            "Expected 32-byte hex (public key)".to_string(),
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn decode_i64(bytes: &[u8]) -> i64 {
    if bytes.len() != 8 {
        return 0;
    }
    let mut b = [0u8; 8];
    b.copy_from_slice(bytes);
    i64::from_le_bytes(b)
}

fn bal_key(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut k = b"bal:".to_vec();
    k.extend_from_slice(pubkey);
    k
}

fn decode_u64_le(bytes: &[u8]) -> u64 {
    if bytes.len() != 8 {
        return 0;
    }
    let mut b = [0u8; 8];
    b.copy_from_slice(bytes);
    u64::from_le_bytes(b)
}

/// Minimal RPC implementation backed by `catalyst-storage`.
pub struct CatalystRpcImpl {
    storage: Arc<catalyst_storage::StorageManager>,
    network: Option<Arc<catalyst_network::NetworkService>>,
}

impl CatalystRpcImpl {
    pub fn new(
        storage: Arc<catalyst_storage::StorageManager>,
        network: Option<Arc<catalyst_network::NetworkService>>,
    ) -> Self {
        Self { storage, network }
    }
}

#[async_trait]
impl CatalystRpcServer for CatalystRpcImpl {
    async fn block_number(&self) -> RpcResult<u64> {
        let n = self
            .storage
            .get_metadata("consensus:last_applied_cycle")
            .await
            .ok()
            .flatten()
            .map(|b| decode_u64_le(&b))
            .unwrap_or(0);
        Ok(n)
    }

    async fn get_block_by_hash(&self, _hash: String, _full_transactions: bool) -> RpcResult<Option<RpcBlock>> {
        Ok(None)
    }

    async fn get_block_by_number(&self, _number: u64, _full_transactions: bool) -> RpcResult<Option<RpcBlock>> {
        Ok(None)
    }

    async fn get_transaction_by_hash(&self, _hash: String) -> RpcResult<Option<RpcTransaction>> {
        Ok(None)
    }

    async fn get_balance(&self, address: String) -> RpcResult<String> {
        let pk = parse_hex_32(&address).map_err(ErrorObjectOwned::from)?;
        let key = bal_key(&pk);
        let bal = self
            .storage
            .engine()
            .get("accounts", &key)
            .map_err(|e| ErrorObjectOwned::from(RpcServerError::Server(e.to_string())))?
            .map(|b| decode_i64(&b))
            .unwrap_or(0);
        Ok(bal.to_string())
    }

    async fn get_account(&self, _address: String) -> RpcResult<Option<RpcAccount>> {
        Ok(None)
    }

    async fn send_raw_transaction(&self, _data: String) -> RpcResult<String> {
        Err(ErrorObjectOwned::from(RpcServerError::Server(
            "sendRawTransaction not implemented".to_string(),
        )))
    }

    async fn estimate_fee(&self, _transaction: RpcTransactionRequest) -> RpcResult<String> {
        Ok("0".to_string())
    }

    async fn network_info(&self) -> RpcResult<RpcNetworkInfo> {
        let peer_count = self.peer_count().await.unwrap_or(0);
        Ok(RpcNetworkInfo {
            chain_id: 0,
            network_id: 0,
            protocol_version: "catalyst/0".to_string(),
            genesis_hash: "0x0".to_string(),
            current_block: self.block_number().await?,
            highest_block: self.block_number().await?,
            peer_count,
        })
    }

    async fn syncing(&self) -> RpcResult<RpcSyncStatus> {
        Ok(RpcSyncStatus::Synced(true))
    }

    async fn peer_count(&self) -> RpcResult<u64> {
        if let Some(net) = &self.network {
            let st = net.get_stats().await;
            return Ok(st.connected_peers as u64);
        }
        Ok(0)
    }

    async fn version(&self) -> RpcResult<String> {
        Ok(format!("catalyst-rpc/{}", env!("CARGO_PKG_VERSION")))
    }

    async fn head(&self) -> RpcResult<RpcHead> {
        let applied_cycle = self.block_number().await?;
        let applied_lsu_hash = self
            .storage
            .get_metadata("consensus:last_applied_lsu_hash")
            .await
            .ok()
            .flatten()
            .map(|b| format!("0x{}", hex::encode(b)))
            .unwrap_or_else(|| "0x0".to_string());
        let applied_state_root = self
            .storage
            .get_metadata("consensus:last_applied_state_root")
            .await
            .ok()
            .flatten()
            .map(|b| format!("0x{}", hex::encode(b)))
            .unwrap_or_else(|| "0x0".to_string());
        let last_lsu_cid = self
            .storage
            .get_metadata("consensus:last_lsu_cid")
            .await
            .ok()
            .flatten()
            .and_then(|b| String::from_utf8(b).ok());

        Ok(RpcHead {
            applied_cycle,
            applied_lsu_hash,
            applied_state_root,
            last_lsu_cid,
        })
    }
}

/// Start a minimal HTTP JSON-RPC server on `bind_address`.
pub async fn start_rpc_http(
    bind_address: SocketAddr,
    storage: Arc<catalyst_storage::StorageManager>,
    network: Option<Arc<catalyst_network::NetworkService>>,
) -> Result<ServerHandle, RpcServerError> {
    let server = jsonrpsee::server::ServerBuilder::default()
        .build(bind_address)
        .await
        .map_err(|e| RpcServerError::Server(e.to_string()))?;

    let rpc = CatalystRpcImpl::new(storage, network).into_rpc();
    let handle = server.start(rpc);

    Ok(handle)
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