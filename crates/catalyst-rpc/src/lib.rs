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
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;
use thiserror::Error;
use tokio::sync::mpsc;

const META_PROTOCOL_CHAIN_ID: &str = "protocol:chain_id";
const META_PROTOCOL_NETWORK_ID: &str = "protocol:network_id";
const META_PROTOCOL_GENESIS_HASH: &str = "protocol:genesis_hash";

const RPC_ERR_RATE_LIMITED_CODE: i32 = -32029;

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
    #[error("Rate limited")]
    RateLimited,
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
            RpcServerError::RateLimited => {
                ErrorObjectOwned::owned(RPC_ERR_RATE_LIMITED_CODE, err.to_string(), None::<()>)
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
    /// Get the chain id (EVM domain separation; tooling identity).
    #[method(name = "catalyst_chainId")]
    async fn chain_id(&self) -> RpcResult<String>;

    /// Get the network id (human-readable network identity).
    #[method(name = "catalyst_networkId")]
    async fn network_id(&self) -> RpcResult<String>;

    /// Get the genesis hash (stable chain identity hash; best-effort for legacy DBs).
    #[method(name = "catalyst_genesisHash")]
    async fn genesis_hash(&self) -> RpcResult<String>;

    /// Get the current block number
    #[method(name = "catalyst_blockNumber")]
    async fn block_number(&self) -> RpcResult<u64>;
    
    /// Get block by hash
    #[method(name = "catalyst_getBlockByHash")]
    async fn get_block_by_hash(&self, hash: String, full_transactions: bool) -> RpcResult<Option<RpcBlock>>;
    
    /// Get block by number
    #[method(name = "catalyst_getBlockByNumber")]
    async fn get_block_by_number(&self, number: u64, full_transactions: bool) -> RpcResult<Option<RpcBlock>>;

    /// Get a range of blocks by number (pagination primitive).
    ///
    /// Returns up to `count` blocks starting at `start` (inclusive), in ascending order.
    #[method(name = "catalyst_getBlocksByNumberRange")]
    async fn get_blocks_by_number_range(
        &self,
        start: u64,
        count: u64,
        full_transactions: bool,
    ) -> RpcResult<Vec<RpcBlock>>;
    
    /// Get transaction by hash
    #[method(name = "catalyst_getTransactionByHash")]
    async fn get_transaction_by_hash(&self, hash: String) -> RpcResult<Option<RpcTransaction>>;

    /// Get a receipt-like view for a submitted transaction.
    #[method(name = "catalyst_getTransactionReceipt")]
    async fn get_transaction_receipt(&self, hash: String) -> RpcResult<Option<RpcTxReceipt>>;

    /// Get a deterministic inclusion proof for a transaction against the cycle's tx Merkle root.
    #[method(name = "catalyst_getTransactionInclusionProof")]
    async fn get_transaction_inclusion_proof(&self, hash: String) -> RpcResult<Option<RpcTxInclusionProof>>;

    /// List recent transactions involving an address (best-effort).
    ///
    /// Scans applied cycles backwards from `from_cycle` (defaults to head) and returns up to `limit`
    /// transaction summaries that involve the address (sender, recipient, or special tx owner).
    #[method(name = "catalyst_getTransactionsByAddress")]
    async fn get_transactions_by_address(
        &self,
        address: String,
        from_cycle: Option<u64>,
        limit: u64,
    ) -> RpcResult<Vec<RpcTransactionSummary>>;
    
    /// Get account balance
    #[method(name = "catalyst_getBalance")]
    async fn get_balance(&self, address: String) -> RpcResult<String>;

    /// Get account balance plus an inclusion proof against the authenticated state root.
    #[method(name = "catalyst_getBalanceProof")]
    async fn get_balance_proof(&self, address: String) -> RpcResult<RpcBalanceProof>;

    /// Get sender nonce (monotonic per-sender counter).
    #[method(name = "catalyst_getNonce")]
    async fn get_nonce(&self, address: String) -> RpcResult<u64>;
    
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

    /// Get EVM contract bytecode for a 20-byte hex address.
    #[method(name = "catalyst_getCode")]
    async fn get_code(&self, address: String) -> RpcResult<String>;

    /// Get last call return data for a 20-byte hex address (dev placeholder).
    #[method(name = "catalyst_getLastReturn")]
    async fn get_last_return(&self, address: String) -> RpcResult<String>;

    /// Get EVM storage slot (32-byte slot) for a contract address.
    #[method(name = "catalyst_getStorageAt")]
    async fn get_storage_at(&self, address: String, slot: String) -> RpcResult<String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcBalanceProof {
    pub balance: String,
    pub state_root: String,
    /// Each step is "L:<hex>" or "R:<hex>" meaning sibling is on the Left or Right.
    pub proof: Vec<String>,
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
    /// Optional full transaction objects when requested (see `full_transactions` arg).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transactions_full: Option<Vec<RpcTransaction>>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTxReceipt {
    pub tx_hash: String,
    pub status: String,
    pub received_at_ms: u64,
    pub from: Option<String>,
    pub nonce: u64,
    pub fees: String,
    pub selected_cycle: Option<u64>,
    pub applied_cycle: Option<u64>,
    pub applied_lsu_hash: Option<String>,
    pub applied_state_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_used: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTxInclusionProof {
    pub tx_hash: String,
    pub cycle: u64,
    pub tx_index: usize,
    pub merkle_root: String,
    /// Each step is "L:<hex>" or "R:<hex>" meaning sibling is on the Left or Right.
    pub proof: Vec<String>,
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

fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, RpcServerError> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(s).map_err(|e| RpcServerError::InvalidParams(e.to_string()))
}

fn parse_hex_20(s: &str) -> Result<[u8; 20], RpcServerError> {
    let b = parse_hex_bytes(s)?;
    if b.len() != 20 {
        return Err(RpcServerError::InvalidParams("expected 20-byte hex".to_string()));
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&b);
    Ok(out)
}

fn evm_code_key(addr20: &[u8; 20]) -> Vec<u8> {
    let mut k = b"evm:code:".to_vec();
    k.extend_from_slice(addr20);
    k
}

fn evm_last_return_key(addr20: &[u8; 20]) -> Vec<u8> {
    let mut k = b"evm:last_return:".to_vec();
    k.extend_from_slice(addr20);
    k
}

fn parse_hex_32bytes(s: &str) -> Result<[u8; 32], RpcServerError> {
    let b = parse_hex_bytes(s)?;
    if b.len() != 32 {
        return Err(RpcServerError::InvalidParams("expected 32-byte hex".to_string()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&b);
    Ok(out)
}

fn evm_storage_key(addr20: &[u8; 20], slot32: &[u8; 32]) -> Vec<u8> {
    let mut k = b"evm:storage:".to_vec();
    k.extend_from_slice(addr20);
    k.extend_from_slice(slot32);
    k
}

fn verify_tx_signature_with_domain(
    tx: &catalyst_core::protocol::Transaction,
    chain_id: u64,
    genesis_hash: [u8; 32],
) -> bool {
    use catalyst_crypto::signatures::SignatureScheme;

    // Determine sender:
    // - transfers: (single) pubkey with negative NonConfidential amount
    // - worker registration: entry[0].public_key
    let sender_pk_bytes: [u8; 32] = match tx.core.tx_type {
        catalyst_core::protocol::TransactionType::WorkerRegistration => {
            let Some(e0) = tx.core.entries.get(0) else { return false };
            e0.public_key
        }
        catalyst_core::protocol::TransactionType::SmartContract => {
            let Some(e0) = tx.core.entries.get(0) else { return false };
            e0.public_key
        }
        _ => {
            let mut sender: Option<[u8; 32]> = None;
            for e in &tx.core.entries {
                if let catalyst_core::protocol::EntryAmount::NonConfidential(v) = e.amount {
                    if v < 0 {
                        match sender {
                            None => sender = Some(e.public_key),
                            Some(pk) if pk == e.public_key => {}
                            Some(_) => return false,
                        }
                    }
                }
            }
            let Some(sender) = sender else { return false };
            sender
        }
    };

    if tx.signature.0.len() != 64 {
        return false;
    }
    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(&tx.signature.0);
    let sig = match catalyst_crypto::signatures::Signature::from_bytes(sig_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let sender_pk = match catalyst_crypto::PublicKey::from_bytes(sender_pk_bytes) {
        Ok(pk) => pk,
        Err(_) => return false,
    };

    // Prefer v1 domain-separated payload; fall back to legacy.
    if let Ok(p) = tx.signing_payload_v1(chain_id, genesis_hash) {
        if SignatureScheme::new().verify(&p, &sig, &sender_pk).unwrap_or(false) {
            return true;
        }
    }
    let legacy = match tx.signing_payload() {
        Ok(p) => p,
        Err(_) => return false,
    };
    SignatureScheme::new().verify(&legacy, &sig, &sender_pk).unwrap_or(false)
}

fn tx_sender_pubkey(tx: &catalyst_core::protocol::Transaction) -> Option<[u8; 32]> {
    match tx.core.tx_type {
        catalyst_core::protocol::TransactionType::WorkerRegistration => tx.core.entries.get(0).map(|e| e.public_key),
        catalyst_core::protocol::TransactionType::SmartContract => tx.core.entries.get(0).map(|e| e.public_key),
        _ => {
            let mut sender: Option<[u8; 32]> = None;
            for e in &tx.core.entries {
                if let catalyst_core::protocol::EntryAmount::NonConfidential(v) = e.amount {
                    if v < 0 {
                        match sender {
                            None => sender = Some(e.public_key),
                            Some(pk) if pk == e.public_key => {}
                            Some(_) => return None,
                        }
                    }
                }
            }
            sender
        }
    }
}

/// Minimal RPC implementation backed by `catalyst-storage`.
pub struct CatalystRpcImpl {
    storage: Arc<catalyst_storage::StorageManager>,
    network: Option<Arc<catalyst_network::NetworkService>>,
    tx_submit: Option<mpsc::UnboundedSender<catalyst_core::protocol::Transaction>>,
    limiter: Arc<RpcLimiters>,
    _global_rps: u32,
    sender_rps: u32,
}

struct RpcLimiters {
    global: Mutex<catalyst_utils::utils::RateLimiter>,
    by_sender: Mutex<std::collections::HashMap<[u8; 32], catalyst_utils::utils::RateLimiter>>,
    drops_global: AtomicU64,
    drops_sender: AtomicU64,
}

impl RpcLimiters {
    fn new(global_rps: u32, sender_rps: u32) -> Self {
        let global_rps = global_rps.max(1);
        Self {
            global: Mutex::new(catalyst_utils::utils::RateLimiter::new(global_rps, global_rps)),
            by_sender: Mutex::new(std::collections::HashMap::new()),
            drops_global: AtomicU64::new(0),
            drops_sender: AtomicU64::new(0),
        }
    }

    async fn acquire_global(&self, cost: u32) -> bool {
        let mut g = self.global.lock().await;
        let ok = g.try_acquire(cost.max(1));
        if !ok {
            self.drops_global.fetch_add(1, Ordering::Relaxed);
        }
        ok
    }

    async fn acquire_sender(&self, sender: [u8; 32], cost: u32, sender_rps: u32) -> bool {
        let mut map = self.by_sender.lock().await;
        let lim = map
            .entry(sender)
            .or_insert_with(|| catalyst_utils::utils::RateLimiter::new(sender_rps.max(1), sender_rps.max(1)));
        let ok = lim.try_acquire(cost.max(1));
        if !ok {
            self.drops_sender.fetch_add(1, Ordering::Relaxed);
        }
        ok
    }
}

impl CatalystRpcImpl {
    pub fn new(
        storage: Arc<catalyst_storage::StorageManager>,
        network: Option<Arc<catalyst_network::NetworkService>>,
        tx_submit: Option<mpsc::UnboundedSender<catalyst_core::protocol::Transaction>>,
        global_rps: u32,
        sender_rps: u32,
    ) -> Self {
        Self {
            storage,
            network,
            tx_submit,
            limiter: Arc::new(RpcLimiters::new(global_rps, sender_rps)),
            _global_rps: global_rps,
            sender_rps,
        }
    }
}

#[async_trait]
impl CatalystRpcServer for CatalystRpcImpl {
    async fn chain_id(&self) -> RpcResult<String> {
        let cid = self
            .storage
            .get_metadata(META_PROTOCOL_CHAIN_ID)
            .await
            .ok()
            .flatten()
            .map(|b| decode_u64_le(&b))
            .unwrap_or(31337);
        Ok(format!("0x{:x}", cid))
    }

    async fn network_id(&self) -> RpcResult<String> {
        let nid = self
            .storage
            .get_metadata(META_PROTOCOL_NETWORK_ID)
            .await
            .ok()
            .flatten()
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_else(|| "unknown".to_string());
        Ok(nid)
    }

    async fn genesis_hash(&self) -> RpcResult<String> {
        let gh = self
            .storage
            .get_metadata(META_PROTOCOL_GENESIS_HASH)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        if gh.len() == 32 {
            Ok(format!("0x{}", hex::encode(gh)))
        } else {
            Ok("0x0".to_string())
        }
    }

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
        let h = parse_hex_32bytes(&_hash).map_err(ErrorObjectOwned::from)?;
        let key = cycle_by_lsu_hash_key(&h);
        let Some(bytes) = self.storage.get_metadata(&key).await.ok().flatten() else {
            return Ok(None);
        };
        if bytes.len() != 8 {
            return Ok(None);
        }
        let mut arr = [0u8; 8];
        arr.copy_from_slice(&bytes);
        let cycle = u64::from_le_bytes(arr);
        self.get_block_by_number(cycle, _full_transactions).await
    }

    async fn get_block_by_number(&self, _number: u64, _full_transactions: bool) -> RpcResult<Option<RpcBlock>> {
        build_block(self.storage.as_ref(), _number, _full_transactions)
            .await
            .map_err(ErrorObjectOwned::from)
            .map(Some)
            .or_else(|e| {
                // If block missing, return Ok(None) instead of error.
                let _ = e;
                Ok(None)
            })
    }

    async fn get_blocks_by_number_range(
        &self,
        start: u64,
        count: u64,
        full_transactions: bool,
    ) -> RpcResult<Vec<RpcBlock>> {
        // Throttle proportional to requested work.
        let cost = 1u32.saturating_add((count.min(10_000) / 100) as u32);
        if !self.limiter.acquire_global(cost).await {
            return Err(ErrorObjectOwned::from(RpcServerError::RateLimited));
        }
        let mut out: Vec<RpcBlock> = Vec::new();
        let count = count.min(10_000); // hard cap to protect node
        for i in 0..count {
            let n = start.saturating_add(i);
            if let Ok(b) = build_block(self.storage.as_ref(), n, full_transactions).await {
                out.push(b);
            } else {
                // Stop at first missing to keep semantics simple for indexers.
                break;
            }
        }
        Ok(out)
    }

    async fn get_transaction_by_hash(&self, hash: String) -> RpcResult<Option<RpcTransaction>> {
        let txid = parse_hex_32bytes(&hash).map_err(ErrorObjectOwned::from)?;
        let key = tx_raw_key(&txid);
        let Some(bytes) = self
            .storage
            .get_metadata(&key)
            .await
            .ok()
            .flatten()
        else {
            return Ok(None);
        };
        let tx: catalyst_core::protocol::Transaction = bincode::deserialize(&bytes).map_err(
            |e: bincode::Error| ErrorObjectOwned::from(RpcServerError::Server(e.to_string())),
        )?;

        let meta = load_tx_meta(self.storage.as_ref(), &txid).await;
        let status = meta
            .as_ref()
            .map(|m| tx_status_string(&m.status))
            .unwrap_or_else(|| "unknown".to_string());

        let from = tx_sender_pubkey(&tx).map(|pk| format!("0x{}", hex::encode(pk)));

        // Map a minimal "to/value" view for non-confidential transfers.
        let (to, value) = match tx.core.tx_type {
            catalyst_core::protocol::TransactionType::NonConfidentialTransfer => {
                let sender = tx_sender_pubkey(&tx);
                let mut to_pk: Option<[u8; 32]> = None;
                let mut val: i64 = 0;
                for e in &tx.core.entries {
                    if let catalyst_core::protocol::EntryAmount::NonConfidential(v) = e.amount {
                        if v > 0 {
                            if sender.map(|s| s != e.public_key).unwrap_or(true) {
                                to_pk = Some(e.public_key);
                                val = val.saturating_add(v);
                            }
                        }
                    }
                }
                (to_pk.map(|pk| format!("0x{}", hex::encode(pk))), val.max(0) as u64)
            }
            _ => (None, 0u64),
        };

        let (block_number, block_hash) = meta
            .as_ref()
            .and_then(|m| m.applied_cycle.map(|c| (Some(c), m.applied_lsu_hash)))
            .map(|(c, h)| (c, h.map(|hh| format!("0x{}", hex::encode(hh)))))
            .unwrap_or((None, None));

        Ok(Some(RpcTransaction {
            hash,
            block_hash,
            block_number,
            from: from.unwrap_or_else(|| "0x0".to_string()),
            to,
            value: value.to_string(),
            data: format!("0x{}", hex::encode(&tx.core.data)),
            gas_limit: 0,
            gas_price: "0".to_string(),
            gas_used: None,
            status: Some(status),
        }))
    }

    async fn get_transaction_receipt(&self, hash: String) -> RpcResult<Option<RpcTxReceipt>> {
        let txid = parse_hex_32bytes(&hash).map_err(ErrorObjectOwned::from)?;
        let meta = load_tx_meta(self.storage.as_ref(), &txid).await;
        let Some(m) = meta else {
            return Ok(None);
        };
        Ok(Some(RpcTxReceipt {
            tx_hash: hash,
            status: tx_status_string(&m.status),
            received_at_ms: m.received_at_ms,
            from: m.sender.map(|pk| format!("0x{}", hex::encode(pk))),
            nonce: m.nonce,
            fees: m.fees.to_string(),
            selected_cycle: m.selected_cycle,
            applied_cycle: m.applied_cycle,
            applied_lsu_hash: m.applied_lsu_hash.map(|h| format!("0x{}", hex::encode(h))),
            applied_state_root: m.applied_state_root.map(|h| format!("0x{}", hex::encode(h))),
            success: m.applied_success,
            error: m.applied_error.clone(),
            gas_used: m.evm_gas_used,
            return_data: m.evm_return.as_ref().map(|b| format!("0x{}", hex::encode(b))),
        }))
    }

    async fn get_transaction_inclusion_proof(
        &self,
        hash: String,
    ) -> RpcResult<Option<RpcTxInclusionProof>> {
        let txid = parse_hex_32bytes(&hash).map_err(ErrorObjectOwned::from)?;
        let meta = load_tx_meta(self.storage.as_ref(), &txid).await;
        let Some(m) = meta else { return Ok(None) };
        let Some(cycle) = m.applied_cycle else { return Ok(None) };

        let txids = load_cycle_txids(self.storage.as_ref(), cycle).await;
        let Some((idx, _)) = txids.iter().enumerate().find(|(_, t)| **t == txid) else {
            return Ok(None);
        };
        let (root, proof) = merkle_root_and_proof(&txids, idx);
        Ok(Some(RpcTxInclusionProof {
            tx_hash: hash,
            cycle,
            tx_index: idx,
            merkle_root: format!("0x{}", hex::encode(root)),
            proof,
        }))
    }

    async fn get_transactions_by_address(
        &self,
        address: String,
        from_cycle: Option<u64>,
        limit: u64,
    ) -> RpcResult<Vec<RpcTransactionSummary>> {
        let cost = 1u32.saturating_add((limit.min(5_000) / 10) as u32);
        if !self.limiter.acquire_global(cost).await {
            return Err(ErrorObjectOwned::from(RpcServerError::RateLimited));
        }
        let pk = parse_hex_32(&address).map_err(ErrorObjectOwned::from)?;
        let head = self.block_number().await.unwrap_or(0);
        let mut cycle = from_cycle.unwrap_or(head);
        let mut out: Vec<RpcTransactionSummary> = Vec::new();

        let limit = limit.min(5_000) as usize; // protect node
        while out.len() < limit {
            // Stop if we've walked past genesis.
            if cycle == 0 && from_cycle.is_some() {
                // allow cycle 0 scan once
            } else if cycle == 0 && from_cycle.is_none() && head == 0 {
                // empty chain
            }

            let txids = load_cycle_txids(self.storage.as_ref(), cycle).await;
            for txid in txids {
                if out.len() >= limit {
                    break;
                }
                if tx_involves_address(self.storage.as_ref(), &txid, &pk).await {
                    out.push(load_tx_summary(self.storage.as_ref(), &txid).await);
                }
            }

            if cycle == 0 {
                break;
            }
            cycle = cycle.saturating_sub(1);
        }

        Ok(out)
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

    async fn get_balance_proof(&self, address: String) -> RpcResult<RpcBalanceProof> {
        let pk = parse_hex_32(&address).map_err(ErrorObjectOwned::from)?;
        let key = bal_key(&pk);

        let Some((root, value_bytes, proof)) = self
            .storage
            .get_account_proof(&key)
            .await
            .map_err(|e| ErrorObjectOwned::from(RpcServerError::Server(e.to_string())))?
        else {
            return Ok(RpcBalanceProof {
                balance: "0".to_string(),
                state_root: format!("0x{}", hex::encode([0u8; 32])),
                proof: Vec::new(),
            });
        };

        let bal = decode_i64(&value_bytes).to_string();
        let mut steps: Vec<String> = Vec::with_capacity(proof.steps.len());
        for s in proof.steps {
            let tag = if s.sibling_is_left { "L" } else { "R" };
            steps.push(format!("{}:0x{}", tag, hex::encode(s.sibling)));
        }

        Ok(RpcBalanceProof {
            balance: bal,
            state_root: format!("0x{}", hex::encode(root)),
            proof: steps,
        })
    }

    async fn get_nonce(&self, address: String) -> RpcResult<u64> {
        let pk = parse_hex_32(&address).map_err(ErrorObjectOwned::from)?;
        let mut k = b"nonce:".to_vec();
        k.extend_from_slice(&pk);
        let n = self
            .storage
            .engine()
            .get("accounts", &k)
            .map_err(|e| ErrorObjectOwned::from(RpcServerError::Server(e.to_string())))?
            .map(|b| decode_u64_le(&b))
            .unwrap_or(0);
        Ok(n)
    }

    async fn get_account(&self, _address: String) -> RpcResult<Option<RpcAccount>> {
        let pk = parse_hex_32(&_address).map_err(ErrorObjectOwned::from)?;
        let bal_key = bal_key(&pk);
        let bal = self
            .storage
            .engine()
            .get("accounts", &bal_key)
            .map_err(|e| ErrorObjectOwned::from(RpcServerError::Server(e.to_string())))?
            .map(|b| decode_i64(&b))
            .unwrap_or(0);

        let mut nonce_key = b"nonce:".to_vec();
        nonce_key.extend_from_slice(&pk);
        let nonce = self
            .storage
            .engine()
            .get("accounts", &nonce_key)
            .map_err(|e| ErrorObjectOwned::from(RpcServerError::Server(e.to_string())))?
            .map(|b| decode_u64_le(&b))
            .unwrap_or(0);

        Ok(Some(RpcAccount {
            address: _address,
            balance: bal.to_string(),
            account_type: "user".to_string(),
            nonce,
        }))
    }

    async fn send_raw_transaction(&self, _data: String) -> RpcResult<String> {
        // Global throttling: protect CPU/memory under load.
        if !self.limiter.acquire_global(1).await {
            return Err(ErrorObjectOwned::from(RpcServerError::RateLimited));
        }

        // Accept hex-encoded legacy bincode(Transaction) OR v1 canonical wire tx (CTX1...).
        let bytes = parse_hex_bytes(&_data).map_err(ErrorObjectOwned::from)?;
        let tx: catalyst_core::protocol::Transaction =
            catalyst_core::protocol::decode_wire_tx_any(&bytes)
                .map_err(|e| ErrorObjectOwned::from(RpcServerError::InvalidParams(e)))?;

        // Basic format + fee floor checks.
        tx.validate_basic()
            .map_err(|e| ErrorObjectOwned::from(RpcServerError::InvalidParams(e)))?;
        let min_fee = catalyst_core::protocol::min_fee(&tx);
        if tx.core.fees < min_fee {
            return Err(ErrorObjectOwned::from(RpcServerError::InvalidParams(format!(
                "fee too low: fees={} min_required={}",
                tx.core.fees, min_fee
            ))));
        }

        let chain_id = self
            .storage
            .get_metadata(META_PROTOCOL_CHAIN_ID)
            .await
            .ok()
            .flatten()
            .map(|b| decode_u64_le(&b))
            .unwrap_or(31337);
        let genesis_hash = self
            .storage
            .get_metadata(META_PROTOCOL_GENESIS_HASH)
            .await
            .ok()
            .flatten()
            .and_then(|b| {
                if b.len() != 32 {
                    return None;
                }
                let mut out = [0u8; 32];
                out.copy_from_slice(&b);
                Some(out)
            })
            .unwrap_or([0u8; 32]);

        if !verify_tx_signature_with_domain(&tx, chain_id, genesis_hash) {
            return Err(ErrorObjectOwned::from(RpcServerError::InvalidParams(
                "Invalid transaction signature".to_string(),
            )));
        }

        // Minimal replay protection at the RPC boundary:
        // - reject nonce <= committed nonce for sender
        // (node/mempool also enforces sequential nonces)
        if let Some(sender_pk) = tx_sender_pubkey(&tx) {
            // Per-sender throttling: stop single-key spam even behind NAT.
            if !self
                .limiter
                .acquire_sender(sender_pk, 1, self.sender_rps)
                .await
            {
                return Err(ErrorObjectOwned::from(RpcServerError::RateLimited));
            }

            let mut k = b"nonce:".to_vec();
            k.extend_from_slice(&sender_pk);
            let committed = self
                .storage
                .engine()
                .get("accounts", &k)
                .map_err(|e| ErrorObjectOwned::from(RpcServerError::Server(e.to_string())))?
                .map(|b| decode_u64_le(&b))
                .unwrap_or(0);
            if tx.core.nonce <= committed {
                return Err(ErrorObjectOwned::from(RpcServerError::InvalidParams(
                    "Nonce too low".to_string(),
                )));
            }
        }

        // tx_id = blake2b512(CTX1 || canonical_tx_bytes)[..32]
        let txid = catalyst_core::protocol::tx_id_v1(&tx)
            .map_err(|e| ErrorObjectOwned::from(RpcServerError::Server(e)))?;
        let tx_id = format!("0x{}", hex::encode(txid));

        // Persist tx raw + meta so clients can poll receipt immediately (best-effort).
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let meta = catalyst_core::protocol::TxMeta {
            tx_id: txid,
            status: catalyst_core::protocol::TxStatus::Pending,
            received_at_ms: now_ms,
            sender: tx_sender_pubkey(&tx),
            nonce: tx.core.nonce,
            fees: tx.core.fees,
            selected_cycle: None,
            applied_cycle: None,
            applied_lsu_hash: None,
            applied_state_root: None,
            applied_success: None,
            applied_error: None,
            evm_gas_used: None,
            evm_return: None,
        };
        // Persist in internal format (bincode) for node/RPC decoding.
        let ser = bincode::serialize(&tx).map_err(|e: bincode::Error| {
            ErrorObjectOwned::from(RpcServerError::Server(e.to_string()))
        })?;
        let _ = self.storage.set_metadata(&tx_raw_key(&txid), &ser).await;
        if let Ok(mbytes) = bincode::serialize(&meta) {
            let _ = self.storage.set_metadata(&tx_meta_key(&txid), &mbytes).await;
        }

        if let Some(sender) = &self.tx_submit {
            sender.send(tx).map_err(|_| {
                ErrorObjectOwned::from(RpcServerError::Server(
                    "tx submit channel closed".to_string(),
                ))
            })?;
            Ok(tx_id)
        } else {
            Err(ErrorObjectOwned::from(RpcServerError::Server(
                "tx submission not enabled".to_string(),
            )))
        }
    }

    async fn estimate_fee(&self, _transaction: RpcTransactionRequest) -> RpcResult<String> {
        let from = parse_hex_32(&_transaction.from).map_err(ErrorObjectOwned::from)?;
        let tx_type = if _transaction.data.as_deref().unwrap_or("").trim().is_empty() {
            catalyst_core::protocol::TransactionType::NonConfidentialTransfer
        } else {
            catalyst_core::protocol::TransactionType::SmartContract
        };

        let entries_len = match tx_type {
            catalyst_core::protocol::TransactionType::SmartContract => 1usize,
            catalyst_core::protocol::TransactionType::WorkerRegistration => 1usize,
            _ => 2usize,
        };

        // We only need the core shape for fee estimation.
        let core = catalyst_core::protocol::TransactionCore {
            tx_type,
            entries: (0..entries_len)
                .map(|i| catalyst_core::protocol::TransactionEntry {
                    public_key: if i == 0 { from } else { [0u8; 32] },
                    amount: catalyst_core::protocol::EntryAmount::NonConfidential(0),
                })
                .collect(),
            nonce: 1,
            lock_time: 0,
            fees: 0,
            data: Vec::new(),
        };
        let fee = catalyst_core::protocol::min_fee_for_core(&core);
        Ok(fee.to_string())
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

    async fn get_code(&self, address: String) -> RpcResult<String> {
        let addr20 = parse_hex_20(&address).map_err(ErrorObjectOwned::from)?;
        let key = evm_code_key(&addr20);
        let bytes = self
            .storage
            .engine()
            .get("accounts", &key)
            .map_err(|e| ErrorObjectOwned::from(RpcServerError::Server(e.to_string())))?
            .unwrap_or_default();
        Ok(format!("0x{}", hex::encode(bytes)))
    }

    async fn get_last_return(&self, address: String) -> RpcResult<String> {
        let addr20 = parse_hex_20(&address).map_err(ErrorObjectOwned::from)?;
        let key = evm_last_return_key(&addr20);
        let bytes = self
            .storage
            .engine()
            .get("accounts", &key)
            .map_err(|e| ErrorObjectOwned::from(RpcServerError::Server(e.to_string())))?
            .unwrap_or_default();
        Ok(format!("0x{}", hex::encode(bytes)))
    }

    async fn get_storage_at(&self, address: String, slot: String) -> RpcResult<String> {
        let addr20 = parse_hex_20(&address).map_err(ErrorObjectOwned::from)?;
        let slot32 = parse_hex_32bytes(&slot).map_err(ErrorObjectOwned::from)?;
        let key = evm_storage_key(&addr20, &slot32);
        let bytes = self
            .storage
            .engine()
            .get("accounts", &key)
            .map_err(|e| ErrorObjectOwned::from(RpcServerError::Server(e.to_string())))?
            .unwrap_or_else(|| vec![0u8; 32]);
        Ok(format!("0x{}", hex::encode(bytes)))
    }
}

async fn build_block(
    store: &catalyst_storage::StorageManager,
    cycle: u64,
    full_transactions: bool,
) -> Result<RpcBlock, RpcServerError> {
    let lsu_key = format!("consensus:lsu:{}", cycle);
    let lsu_hash_key = format!("consensus:lsu_hash:{}", cycle);

    let Some(lsu_bytes) = store.get_metadata(&lsu_key).await.ok().flatten() else {
        return Err(RpcServerError::BlockNotFound(format!("cycle={}", cycle)));
    };

    let lsu: catalyst_consensus::types::LedgerStateUpdate =
        <catalyst_consensus::types::LedgerStateUpdate as catalyst_utils::CatalystDeserialize>::deserialize(&lsu_bytes)
            .map_err(|e| RpcServerError::Server(e.to_string()))?;

    let lsu_hash = store
        .get_metadata(&lsu_hash_key)
        .await
        .ok()
        .flatten()
        .and_then(|b| if b.len() == 32 {
            let mut h = [0u8; 32];
            h.copy_from_slice(&b);
            Some(h)
        } else {
            None
        })
        .unwrap_or_else(|| catalyst_consensus::types::hash_data(&lsu).unwrap_or([0u8; 32]));

    let parent_hash = if cycle > 0 {
        store
            .get_metadata(&format!("consensus:lsu_hash:{}", cycle.saturating_sub(1)))
            .await
            .ok()
            .flatten()
            .and_then(|b| if b.len() == 32 {
                let mut h = [0u8; 32];
                h.copy_from_slice(&b);
                Some(h)
            } else {
                None
            })
            .unwrap_or([0u8; 32])
    } else {
        [0u8; 32]
    };

    let txids = load_cycle_txids(store, cycle).await;
    let mut txs: Vec<RpcTransactionSummary> = Vec::new();
    for txid in &txids {
        txs.push(load_tx_summary(store, txid).await);
    }

    let transactions_full = if full_transactions {
        let mut out: Vec<RpcTransaction> = Vec::new();
        for txid in &txids {
            if let Some(tx) = load_tx_full(store, txid, Some(cycle), Some(lsu_hash)).await {
                out.push(tx);
            }
        }
        Some(out)
    } else {
        None
    };

    Ok(RpcBlock {
        hash: format!("0x{}", hex::encode(lsu_hash)),
        number: cycle,
        parent_hash: format!("0x{}", hex::encode(parent_hash)),
        timestamp: lsu.partial_update.timestamp,
        transactions: txs,
        transactions_full,
        transaction_count: txids.len(),
        size: lsu_bytes.len() as u64,
    })
}

async fn tx_involves_address(
    store: &catalyst_storage::StorageManager,
    txid: &[u8; 32],
    pk: &[u8; 32],
) -> bool {
    let key = tx_raw_key(txid);
    let Some(bytes) = store.get_metadata(&key).await.ok().flatten() else {
        return false;
    };
    let Ok(tx) = bincode::deserialize::<catalyst_core::protocol::Transaction>(&bytes) else {
        return false;
    };
    tx_participants(&tx).iter().any(|p| p == pk)
}

fn tx_participants(tx: &catalyst_core::protocol::Transaction) -> Vec<[u8; 32]> {
    match tx.core.tx_type {
        catalyst_core::protocol::TransactionType::WorkerRegistration => tx
            .core
            .entries
            .get(0)
            .map(|e| vec![e.public_key])
            .unwrap_or_default(),
        catalyst_core::protocol::TransactionType::SmartContract => tx
            .core
            .entries
            .get(0)
            .map(|e| vec![e.public_key])
            .unwrap_or_default(),
        _ => {
            let mut out: Vec<[u8; 32]> = Vec::new();
            if let Some(sender) = tx_sender_pubkey(tx) {
                out.push(sender);
            }
            for e in &tx.core.entries {
                if let catalyst_core::protocol::EntryAmount::NonConfidential(v) = e.amount {
                    if v > 0 {
                        out.push(e.public_key);
                    }
                }
            }
            out.sort();
            out.dedup();
            out
        }
    }
}

fn tx_raw_key(txid: &[u8; 32]) -> String {
    format!("tx:raw:{}", hex::encode(txid))
}

fn tx_meta_key(txid: &[u8; 32]) -> String {
    format!("tx:meta:{}", hex::encode(txid))
}

fn cycle_txids_key(cycle: u64) -> String {
    format!("tx:cycle:{}:txids", cycle)
}

fn cycle_by_lsu_hash_key(lsu_hash: &[u8; 32]) -> String {
    format!("consensus:cycle_by_lsu_hash:{}", hex::encode(lsu_hash))
}

async fn load_cycle_txids(store: &catalyst_storage::StorageManager, cycle: u64) -> Vec<[u8; 32]> {
    let Some(bytes) = store.get_metadata(&cycle_txids_key(cycle)).await.ok().flatten() else {
        return Vec::new();
    };
    bincode::deserialize::<Vec<[u8; 32]>>(&bytes).unwrap_or_default()
}

async fn load_tx_summary(
    store: &catalyst_storage::StorageManager,
    txid: &[u8; 32],
) -> RpcTransactionSummary {
    let hash = format!("0x{}", hex::encode(txid));
    let key = tx_raw_key(txid);
    let Some(bytes) = store.get_metadata(&key).await.ok().flatten() else {
        return RpcTransactionSummary {
            hash,
            from: "0x0".to_string(),
            to: None,
            value: "0".to_string(),
        };
    };
    let Ok(tx) = bincode::deserialize::<catalyst_core::protocol::Transaction>(&bytes) else {
        return RpcTransactionSummary {
            hash,
            from: "0x0".to_string(),
            to: None,
            value: "0".to_string(),
        };
    };

    let from = tx_sender_pubkey(&tx)
        .map(|pk| format!("0x{}", hex::encode(pk)))
        .unwrap_or_else(|| "0x0".to_string());

    let (to, value) = match tx.core.tx_type {
        catalyst_core::protocol::TransactionType::NonConfidentialTransfer => {
            let sender = tx_sender_pubkey(&tx);
            let mut to_pk: Option<[u8; 32]> = None;
            let mut val: i64 = 0;
            for e in &tx.core.entries {
                if let catalyst_core::protocol::EntryAmount::NonConfidential(v) = e.amount {
                    if v > 0 {
                        if sender.map(|s| s != e.public_key).unwrap_or(true) {
                            to_pk = Some(e.public_key);
                            val = val.saturating_add(v);
                        }
                    }
                }
            }
            (to_pk.map(|pk| format!("0x{}", hex::encode(pk))), val.max(0) as u64)
        }
        _ => (None, 0u64),
    };

    RpcTransactionSummary {
        hash,
        from,
        to,
        value: value.to_string(),
    }
}

async fn load_tx_full(
    store: &catalyst_storage::StorageManager,
    txid: &[u8; 32],
    block_number: Option<u64>,
    block_hash: Option<[u8; 32]>,
) -> Option<RpcTransaction> {
    let hash = format!("0x{}", hex::encode(txid));
    let key = tx_raw_key(txid);
    let bytes = store.get_metadata(&key).await.ok().flatten()?;
    let tx: catalyst_core::protocol::Transaction = bincode::deserialize(&bytes).ok()?;

    let meta = load_tx_meta(store, txid).await;
    let status = meta
        .as_ref()
        .map(|m| tx_status_string(&m.status))
        .unwrap_or_else(|| "unknown".to_string());

    let from = tx_sender_pubkey(&tx).map(|pk| format!("0x{}", hex::encode(pk)));
    let (to, value) = match tx.core.tx_type {
        catalyst_core::protocol::TransactionType::NonConfidentialTransfer => {
            let sender = tx_sender_pubkey(&tx);
            let mut to_pk: Option<[u8; 32]> = None;
            let mut val: i64 = 0;
            for e in &tx.core.entries {
                if let catalyst_core::protocol::EntryAmount::NonConfidential(v) = e.amount {
                    if v > 0 {
                        if sender.map(|s| s != e.public_key).unwrap_or(true) {
                            to_pk = Some(e.public_key);
                            val = val.saturating_add(v);
                        }
                    }
                }
            }
            (to_pk.map(|pk| format!("0x{}", hex::encode(pk))), val.max(0) as u64)
        }
        _ => (None, 0u64),
    };

    Some(RpcTransaction {
        hash,
        block_hash: block_hash.map(|h| format!("0x{}", hex::encode(h))),
        block_number,
        from: from.unwrap_or_else(|| "0x0".to_string()),
        to,
        value: value.to_string(),
        data: format!("0x{}", hex::encode(&tx.core.data)),
        gas_limit: 0,
        gas_price: "0".to_string(),
        gas_used: None,
        status: Some(status),
    })
}

async fn load_tx_meta(
    store: &catalyst_storage::StorageManager,
    txid: &[u8; 32],
) -> Option<catalyst_core::protocol::TxMeta> {
    let Some(bytes) = store.get_metadata(&tx_meta_key(txid)).await.ok().flatten() else {
        return None;
    };
    bincode::deserialize::<catalyst_core::protocol::TxMeta>(&bytes).ok()
}

fn tx_status_string(st: &catalyst_core::protocol::TxStatus) -> String {
    match st {
        catalyst_core::protocol::TxStatus::Pending => "pending",
        catalyst_core::protocol::TxStatus::Selected => "selected",
        catalyst_core::protocol::TxStatus::Applied => "applied",
        catalyst_core::protocol::TxStatus::Dropped => "dropped",
    }
    .to_string()
}

fn hash_pair(left: [u8; 32], right: [u8; 32]) -> [u8; 32] {
    use blake2::{Blake2b512, Digest};
    let mut h = Blake2b512::new();
    h.update(left);
    h.update(right);
    let out = h.finalize();
    let mut id = [0u8; 32];
    id.copy_from_slice(&out[..32]);
    id
}

fn merkle_root_and_proof(leaves: &[[u8; 32]], index: usize) -> ([u8; 32], Vec<String>) {
    if leaves.is_empty() {
        return ([0u8; 32], Vec::new());
    }
    if leaves.len() == 1 {
        return (leaves[0], Vec::new());
    }

    let mut idx = index;
    let mut level: Vec<[u8; 32]> = leaves.to_vec();
    let mut proof: Vec<String> = Vec::new();

    while level.len() > 1 {
        let is_right = idx % 2 == 1;
        let sib_idx = if is_right { idx - 1 } else { idx + 1 };
        let sib = if sib_idx < level.len() { level[sib_idx] } else { level[idx] };
        proof.push(format!(
            "{}:0x{}",
            if is_right { "L" } else { "R" },
            hex::encode(sib)
        ));

        let mut next: Vec<[u8; 32]> = Vec::with_capacity((level.len() + 1) / 2);
        let mut i = 0usize;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() { level[i + 1] } else { level[i] };
            next.push(hash_pair(left, right));
            i += 2;
        }
        level = next;
        idx /= 2;
    }

    (level[0], proof)
}

/// Start a minimal HTTP JSON-RPC server on `bind_address`.
pub async fn start_rpc_http(
    bind_address: SocketAddr,
    storage: Arc<catalyst_storage::StorageManager>,
    network: Option<Arc<catalyst_network::NetworkService>>,
    tx_submit: Option<mpsc::UnboundedSender<catalyst_core::protocol::Transaction>>,
    global_rps: u32,
    sender_rps: u32,
) -> Result<ServerHandle, RpcServerError> {
    let server = jsonrpsee::server::ServerBuilder::default()
        .build(bind_address)
        .await
        .map_err(|e| RpcServerError::Server(e.to_string()))?;

    let rpc = CatalystRpcImpl::new(storage, network, tx_submit, global_rps, sender_rps).into_rpc();
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

    #[test]
    fn merkle_proof_roundtrip() {
        let leaves: Vec<[u8; 32]> = vec![[1u8; 32], [2u8; 32], [3u8; 32], [4u8; 32], [5u8; 32]];
        let idx = 2usize;
        let (root, proof) = merkle_root_and_proof(&leaves, idx);

        // Verify by folding the proof.
        let mut cur = leaves[idx];
        for step in proof {
            let (side, hex_sib) = step.split_once(':').expect("bad proof step");
            let hex_sib = hex_sib.strip_prefix("0x").unwrap_or(hex_sib);
            let sib_bytes = hex::decode(hex_sib).expect("bad sibling hex");
            assert_eq!(sib_bytes.len(), 32);
            let mut sib = [0u8; 32];
            sib.copy_from_slice(&sib_bytes);
            cur = match side {
                "L" => hash_pair(sib, cur),
                "R" => hash_pair(cur, sib),
                _ => panic!("bad side"),
            };
        }
        assert_eq!(cur, root);
    }

    #[test]
    fn tx_participants_includes_sender_and_receiver() {
        let sender = [1u8; 32];
        let recv = [2u8; 32];
        let tx = catalyst_core::protocol::Transaction {
            core: catalyst_core::protocol::TransactionCore {
                tx_type: catalyst_core::protocol::TransactionType::NonConfidentialTransfer,
                entries: vec![
                    catalyst_core::protocol::TransactionEntry {
                        public_key: sender,
                        amount: catalyst_core::protocol::EntryAmount::NonConfidential(-5),
                    },
                    catalyst_core::protocol::TransactionEntry {
                        public_key: recv,
                        amount: catalyst_core::protocol::EntryAmount::NonConfidential(5),
                    },
                ],
                nonce: 1,
                lock_time: 0,
                fees: 1,
                data: Vec::new(),
            },
            signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
            timestamp: 0,
        };
        let parts = tx_participants(&tx);
        assert!(parts.contains(&sender));
        assert!(parts.contains(&recv));
    }

    #[test]
    fn chain_id_formats_as_hex() {
        assert_eq!(format!("0x{:x}", 31337u64), "0x7a69");
        assert_eq!(format!("0x{:x}", 1u64), "0x1");
    }
}