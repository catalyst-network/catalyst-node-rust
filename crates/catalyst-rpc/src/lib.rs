//! JSON-RPC server for Catalyst Network
//! Complete version with storage integration - Simplified error handling

use async_trait::async_trait;
use catalyst_core::{NodeStatus, ResourceMetrics, SyncStatus};
use catalyst_utils::Hash;
use jsonrpsee::{
    core::RpcResult,
    proc_macros::rpc,
    server::{ServerBuilder, ServerHandle},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

// Module declarations
pub mod types;
pub use types::*;

#[derive(Error, Debug)]
pub enum CatalystRpcError {
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
    #[error("Internal error: {0}")]
    Internal(String),
}

/// RPC server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    /// Enable HTTP
    pub enabled: bool,
    /// HTTP port
    pub http_port: u16,
    /// WebSocket port
    pub ws_port: u16,
    /// Server address
    pub address: String,
    /// Maximum connections
    pub max_connections: u32,
    /// Enable CORS
    pub cors_enabled: bool,
    /// CORS allowed origins
    pub cors_origins: Vec<String>,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            http_port: 9933,
            ws_port: 9944,
            address: "127.0.0.1".to_string(),
            max_connections: 100,
            cors_enabled: true,
            cors_origins: vec!["*".to_string()],
        }
    }
}

/// Comprehensive Catalyst RPC API
#[rpc(server)]
pub trait CatalystRpc {
    // === Node Information ===
    /// Get node version
    #[method(name = "catalyst_version")]
    async fn version(&self) -> RpcResult<String>;

    /// Get node status (health check)
    #[method(name = "catalyst_status")]
    async fn status(&self) -> RpcResult<RpcNodeStatus>;

    /// Get network information
    #[method(name = "catalyst_networkInfo")]
    async fn network_info(&self) -> RpcResult<RpcNetworkInfo>;

    /// Get peer information
    #[method(name = "catalyst_peerInfo")]
    async fn peer_info(&self) -> RpcResult<Vec<RpcPeer>>;

    /// Get node information
    #[method(name = "catalyst_nodeInfo")]
    async fn node_info(&self) -> RpcResult<RpcNode>;

    // === Blockchain State ===
    /// Get the current block number
    #[method(name = "catalyst_blockNumber")]
    async fn block_number(&self) -> RpcResult<u64>;

    /// Get block by hash
    #[method(name = "catalyst_getBlockByHash")]
    async fn get_block_by_hash(
        &self,
        hash: String,
        full_transactions: bool,
    ) -> RpcResult<Option<RpcBlock>>;

    /// Get block by number
    #[method(name = "catalyst_getBlockByNumber")]
    async fn get_block_by_number(
        &self,
        number: String,
        full_transactions: bool,
    ) -> RpcResult<Option<RpcBlock>>;

    /// Get latest block
    #[method(name = "catalyst_getLatestBlock")]
    async fn get_latest_block(&self, full_transactions: bool) -> RpcResult<RpcBlock>;

    // === Transactions ===
    /// Get transaction by hash
    #[method(name = "catalyst_getTransactionByHash")]
    async fn get_transaction_by_hash(&self, hash: String) -> RpcResult<Option<RpcTransaction>>;

    /// Get transaction receipt
    #[method(name = "catalyst_getTransactionReceipt")]
    async fn get_transaction_receipt(
        &self,
        hash: String,
    ) -> RpcResult<Option<RpcTransactionReceipt>>;

    /// Send raw transaction
    #[method(name = "catalyst_sendRawTransaction")]
    async fn send_raw_transaction(&self, signed_transaction_data: String) -> RpcResult<String>;

    // === Accounts & Balances ===
    /// Get account balance
    #[method(name = "catalyst_getBalance")]
    async fn get_balance(&self, address: String, block_number: Option<String>)
        -> RpcResult<String>;

    /// Get account information
    #[method(name = "catalyst_getAccount")]
    async fn get_account(
        &self,
        address: String,
        block_number: Option<String>,
    ) -> RpcResult<Option<RpcAccount>>;

    /// Get account transaction count (nonce)
    #[method(name = "catalyst_getTransactionCount")]
    async fn get_transaction_count(
        &self,
        address: String,
        block_number: Option<String>,
    ) -> RpcResult<u64>;

    // === Smart Contracts (EVM) ===
    /// Call contract method (read-only)
    #[method(name = "catalyst_call")]
    async fn call(
        &self,
        transaction: RpcTransactionRequest,
        block_number: Option<String>,
    ) -> RpcResult<String>;

    /// Estimate gas for transaction
    #[method(name = "catalyst_estimateGas")]
    async fn estimate_gas(&self, transaction: RpcTransactionRequest) -> RpcResult<String>;

    /// Get contract code
    #[method(name = "catalyst_getCode")]
    async fn get_code(&self, address: String, block_number: Option<String>) -> RpcResult<String>;

    /// Get storage at position
    #[method(name = "catalyst_getStorageAt")]
    async fn get_storage_at(
        &self,
        address: String,
        position: String,
        block_number: Option<String>,
    ) -> RpcResult<String>;

    /// Get logs
    #[method(name = "catalyst_getLogs")]
    async fn get_logs(&self, filter: RpcFilter) -> RpcResult<Vec<RpcLog>>;

    // === Chain Information ===
    /// Get chain ID
    #[method(name = "catalyst_chainId")]
    async fn chain_id(&self) -> RpcResult<String>;

    /// Get gas price
    #[method(name = "catalyst_gasPrice")]
    async fn gas_price(&self) -> RpcResult<String>;

    // === System & Metrics ===
    /// Get system health
    #[method(name = "catalyst_systemHealth")]
    async fn system_health(&self) -> RpcResult<RpcHealth>;

    /// Get system properties
    #[method(name = "catalyst_systemProperties")]
    async fn system_properties(&self) -> RpcResult<RpcProperties>;

    /// Get metrics
    #[method(name = "catalyst_metrics")]
    async fn metrics(&self) -> RpcResult<RpcMetrics>;
}

/// RPC server implementation with persistent handle
pub struct RpcServer {
    config: RpcConfig,
    server_handle: Arc<RwLock<Option<ServerHandle>>>,
    rpc_impl: Arc<CatalystRpcImpl>,
    keep_alive_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

/// Mock blockchain state for development
#[derive(Debug, Clone)]
pub struct BlockchainState {
    pub current_block: u64,
    pub node_status: NodeStatus,
    pub network_peers: Vec<RpcPeer>,
}

impl Default for BlockchainState {
    fn default() -> Self {
        Self {
            current_block: 0, // Start with genesis
            node_status: NodeStatus {
                id: "catalyst-node".to_string(),
                uptime: 0,
                sync_status: SyncStatus::Synced,
                metrics: ResourceMetrics {
                    cpu_usage: 25.5,
                    memory_usage: 512 * 1024 * 1024,    // 512MB
                    disk_usage: 2 * 1024 * 1024 * 1024, // 2GB
                },
            },
            network_peers: vec![],
        }
    }
}

impl RpcServer {
    pub async fn new(
        config: RpcConfig,
        rpc_impl: CatalystRpcImpl,
    ) -> Result<Self, CatalystRpcError> {
        Ok(Self {
            config,
            server_handle: Arc::new(RwLock::new(None)),
            rpc_impl: Arc::new(rpc_impl),
            keep_alive_task: Arc::new(RwLock::new(None)),
        })
    }

    pub async fn start(&self) -> Result<(), CatalystRpcError> {
        if !self.config.enabled {
            info!("RPC server is disabled in configuration");
            return Ok(());
        }

        let bind_address = format!("{}:{}", self.config.address, self.config.http_port);
        info!("🚀 Starting RPC server on {}", bind_address);

        // Build the server
        let server = ServerBuilder::default()
            .build(&bind_address)
            .await
            .map_err(|e| {
                error!("❌ Failed to build RPC server: {}", e);
                CatalystRpcError::Server(format!("Failed to build server: {}", e))
            })?;

        info!("📡 RPC server built successfully on {}", bind_address);

        // Register the RPC methods
        let methods = (*self.rpc_impl).clone().into_rpc();
        let method_count = methods.method_names().count();
        info!("📋 Registered {} RPC methods", method_count);

        // Start the server
        let handle = server.start(methods);

        // Create a task to keep the server alive
        let server_handle_clone = handle.clone();
        let keep_alive_task = tokio::spawn(async move {
            info!("🔧 Keep-alive task started for RPC server");
            server_handle_clone.stopped().await;
            info!("🛑 RPC server stopped");
        });

        // Store both the handle and the keep-alive task
        *self.server_handle.write().await = Some(handle);
        *self.keep_alive_task.write().await = Some(keep_alive_task);

        info!("✅ RPC server started and listening on {}", bind_address);

        // Give the server time to bind
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        Ok(())
    }

    pub async fn stop(&self) -> Result<(), CatalystRpcError> {
        info!("Stopping RPC server...");

        // Stop the keep-alive task first
        if let Some(task) = self.keep_alive_task.write().await.take() {
            task.abort();
        }

        if let Some(handle) = self.server_handle.write().await.take() {
            handle
                .stop()
                .map_err(|e| CatalystRpcError::Server(format!("Failed to stop server: {}", e)))?;
            info!("✅ RPC server stopped");
        } else {
            warn!("RPC server was not running");
        }

        Ok(())
    }

    pub async fn is_running(&self) -> bool {
        if let Some(handle) = self.server_handle.read().await.as_ref() {
            !handle.is_stopped()
        } else {
            false
        }
    }
}

/// Storage service types (simplified to avoid dependency issues)
pub mod storage_types {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct BlockData {
        pub hash: String,
        pub number: u64,
        pub parent_hash: String,
        pub timestamp: u64,
        pub transactions: Vec<String>,
        pub transaction_count: usize,
        pub size: u64,
        pub gas_limit: String,
        pub gas_used: String,
        pub validator: String,
        pub state_root: String,
        pub transactions_root: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct TransactionData {
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
}

/// Trait for storage service to avoid direct dependency
#[async_trait]
pub trait StorageServiceTrait: Send + Sync {
    async fn get_latest_block_number(&self) -> Result<u64, String>;
    async fn get_latest_block(&self) -> Result<Option<storage_types::BlockData>, String>;
    async fn get_block_by_number(
        &self,
        number: u64,
    ) -> Result<Option<storage_types::BlockData>, String>;
    async fn get_block_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<storage_types::BlockData>, String>;
    async fn get_account_balance(&self, address: &str) -> Result<String, String>;
    async fn get_account_nonce(&self, address: &str) -> Result<u64, String>;
    async fn get_transaction_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<storage_types::TransactionData>, String>;
    async fn health_check(&self) -> bool;
    async fn get_storage_statistics(&self) -> Result<(u64, u64), String>; // (total_blocks, database_size)
}

/// Implementation of the RPC methods
#[derive(Clone)] // Add Clone implementation
pub struct CatalystRpcImpl {
    blockchain_state: Arc<RwLock<BlockchainState>>,
    storage_service: Option<Arc<dyn StorageServiceTrait>>,
}

impl CatalystRpcImpl {
    pub fn new(blockchain_state: Arc<RwLock<BlockchainState>>) -> Self {
        Self {
            blockchain_state,
            storage_service: None,
        }
    }

    /// Set storage service for real data access
    pub fn with_storage(mut self, storage_service: Arc<dyn StorageServiceTrait>) -> Self {
        self.storage_service = Some(storage_service);
        self
    }

    /// Convert storage BlockData to RPC RpcBlock
    fn convert_block_data_to_rpc(&self, block_data: &storage_types::BlockData) -> RpcBlock {
        RpcBlock {
            hash: block_data.hash.clone(),
            number: block_data.number,
            parent_hash: block_data.parent_hash.clone(),
            timestamp: block_data.timestamp,
            transactions: block_data
                .transactions
                .iter()
                .map(|tx_hash| RpcTransactionSummary {
                    hash: tx_hash.clone(),
                    from: "0x0000000000000000000000000000000000000000".to_string(),
                    to: None,
                    value: "0".to_string(),
                    gas: "21000".to_string(),
                    gas_price: "1000000000".to_string(),
                })
                .collect(),
            transaction_count: block_data.transaction_count,
            size: block_data.size,
            gas_limit: block_data.gas_limit.clone(),
            gas_used: block_data.gas_used.clone(),
            validator: block_data.validator.clone(),
            state_root: block_data.state_root.clone(),
            transactions_root: block_data.transactions_root.clone(),
        }
    }

    /// Convert storage TransactionData to RPC RpcTransaction  
    fn convert_transaction_data_to_rpc(
        &self,
        tx_data: &storage_types::TransactionData,
    ) -> RpcTransaction {
        RpcTransaction {
            hash: tx_data.hash.clone(),
            block_hash: tx_data.block_hash.clone(),
            block_number: tx_data.block_number,
            transaction_index: tx_data.transaction_index,
            from: tx_data.from.clone(),
            to: tx_data.to.clone(),
            value: tx_data.value.clone(),
            data: tx_data.data.clone(),
            gas_limit: tx_data.gas_limit.clone(),
            gas_price: tx_data.gas_price.clone(),
            gas_used: tx_data.gas_used.clone(),
            status: tx_data.status.clone(),
            nonce: tx_data.nonce,
        }
    }
}

#[async_trait]
impl CatalystRpcServer for CatalystRpcImpl {
    // === Node Information ===
    async fn version(&self) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_version");
        Ok("Catalyst/1.0.0-storage".to_string())
    }

    async fn status(&self) -> RpcResult<RpcNodeStatus> {
        let state = self.blockchain_state.read().await;
        println!("📞 RPC call: catalyst_status");

        // Get real block height from storage
        let block_height = if let Some(storage) = &self.storage_service {
            storage.get_latest_block_number().await.unwrap_or(0)
        } else {
            state.current_block
        };

        Ok(RpcNodeStatus {
            node_id: state.node_status.id.clone(),
            uptime: state.node_status.uptime,
            sync_status: match state.node_status.sync_status {
                SyncStatus::Synced => "synced".to_string(),
                SyncStatus::Syncing { .. } => "syncing".to_string(),
                SyncStatus::NotSynced => "not_synced".to_string(),
            },
            block_height,
            peer_count: 0, // Hardcoded for now
            cpu_usage: state.node_status.metrics.cpu_usage,
            memory_usage: state.node_status.metrics.memory_usage,
            disk_usage: state.node_status.metrics.disk_usage,
        })
    }

    async fn network_info(&self) -> RpcResult<RpcNetworkInfo> {
        let state = self.blockchain_state.read().await;
        println!("📞 RPC call: catalyst_networkInfo");

        Ok(RpcNetworkInfo {
            chain_id: 1337,
            network_id: 1337,
            protocol_version: "catalyst/1.0".to_string(),
            genesis_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            current_block: state.current_block,
            highest_block: state.current_block,
            peer_count: 0,
        })
    }

    async fn peer_info(&self) -> RpcResult<Vec<RpcPeer>> {
        let state = self.blockchain_state.read().await;
        println!("📞 RPC call: catalyst_peerInfo");
        Ok(state.network_peers.clone())
    }

    async fn node_info(&self) -> RpcResult<RpcNode> {
        let state = self.blockchain_state.read().await;
        println!("📞 RPC call: catalyst_nodeInfo");

        Ok(RpcNode {
            id: state.node_status.id.clone(),
            name: "catalyst-node".to_string(),
            version: "1.0.0-storage".to_string(),
            network: "catalyst-local".to_string(),
            protocol_version: "1.0".to_string(),
            listening_addresses: vec!["/ip4/127.0.0.1/tcp/30333".to_string()],
            uptime: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }

    // === Blockchain State === (Now using real storage!)

    async fn block_number(&self) -> RpcResult<u64> {
        if let Some(storage) = &self.storage_service {
            match storage.get_latest_block_number().await {
                Ok(number) => {
                    println!(
                        "📞 RPC call: catalyst_blockNumber -> {} (from storage)",
                        number
                    );
                    Ok(number)
                }
                Err(_e) => {
                    println!("❌ RPC error: catalyst_blockNumber -> storage error");
                    // Fallback to mock data
                    let state = self.blockchain_state.read().await;
                    Ok(state.current_block)
                }
            }
        } else {
            let state = self.blockchain_state.read().await;
            println!(
                "📞 RPC call: catalyst_blockNumber -> {} (mock)",
                state.current_block
            );
            Ok(state.current_block)
        }
    }

    async fn get_latest_block(&self, _full_transactions: bool) -> RpcResult<RpcBlock> {
        if let Some(storage) = &self.storage_service {
            match storage.get_latest_block().await {
                Ok(Some(block_data)) => {
                    println!(
                        "📞 RPC call: catalyst_getLatestBlock -> #{} (from storage)",
                        block_data.number
                    );
                    return Ok(self.convert_block_data_to_rpc(&block_data));
                }
                Ok(None) => {
                    println!("❌ RPC error: catalyst_getLatestBlock -> No blocks found");
                }
                Err(_e) => {
                    println!("❌ RPC error: catalyst_getLatestBlock -> storage error");
                }
            }
        }

        // Fallback to mock data
        let state = self.blockchain_state.read().await;
        println!(
            "📞 RPC call: catalyst_getLatestBlock -> #{} (mock)",
            state.current_block
        );
        Ok(RpcBlock {
            hash: format!("0x{:064x}", state.current_block),
            number: state.current_block,
            parent_hash: format!("0x{:064x}", state.current_block.saturating_sub(1)),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            transactions: vec![],
            transaction_count: 0,
            size: 1024,
            gas_limit: "8000000".to_string(),
            gas_used: "0".to_string(),
            validator: "0x742d35cc6600c14f6d0c6bfb0c6c8e6b5e5b5e5e".to_string(),
            state_root: "0xabcdef1234567890".to_string(),
            transactions_root: "0x1234567890abcdef".to_string(),
        })
    }

    async fn get_block_by_number(
        &self,
        number: String,
        _full_transactions: bool,
    ) -> RpcResult<Option<RpcBlock>> {
        if let Some(storage) = &self.storage_service {
            let block_num: u64 = if number == "latest" {
                storage.get_latest_block_number().await.unwrap_or(0)
            } else {
                number.parse().unwrap_or(0)
            };

            match storage.get_block_by_number(block_num).await {
                Ok(Some(block_data)) => {
                    println!(
                        "📞 RPC call: catalyst_getBlockByNumber({}) -> #{} (from storage)",
                        number, block_data.number
                    );
                    return Ok(Some(self.convert_block_data_to_rpc(&block_data)));
                }
                Ok(None) => {
                    println!("📞 RPC call: catalyst_getBlockByNumber({}) -> None", number);
                    return Ok(None);
                }
                Err(_e) => {
                    println!(
                        "❌ RPC error: catalyst_getBlockByNumber({}) -> storage error",
                        number
                    );
                }
            }
        }

        println!("📞 RPC call: catalyst_getBlockByNumber({}) (mock)", number);
        Ok(None)
    }

    async fn get_block_by_hash(
        &self,
        hash: String,
        _full_transactions: bool,
    ) -> RpcResult<Option<RpcBlock>> {
        if let Some(storage) = &self.storage_service {
            match storage.get_block_by_hash(&hash).await {
                Ok(Some(block_data)) => {
                    println!(
                        "📞 RPC call: catalyst_getBlockByHash({}) -> #{} (from storage)",
                        hash, block_data.number
                    );
                    return Ok(Some(self.convert_block_data_to_rpc(&block_data)));
                }
                Ok(None) => {
                    println!("📞 RPC call: catalyst_getBlockByHash({}) -> None", hash);
                    return Ok(None);
                }
                Err(_e) => {
                    println!(
                        "❌ RPC error: catalyst_getBlockByHash({}) -> storage error",
                        hash
                    );
                }
            }
        }

        println!("📞 RPC call: catalyst_getBlockByHash({}) (mock)", hash);
        Ok(None)
    }

    // === Accounts & Balances === (Now using real storage!)

    async fn get_balance(
        &self,
        address: String,
        _block_number: Option<String>,
    ) -> RpcResult<String> {
        if let Some(storage) = &self.storage_service {
            match storage.get_account_balance(&address).await {
                Ok(balance) => {
                    println!(
                        "📞 RPC call: catalyst_getBalance({}) -> {} (from storage)",
                        address, balance
                    );
                    return Ok(balance);
                }
                Err(_e) => {
                    println!(
                        "❌ RPC error: catalyst_getBalance({}) -> storage error",
                        address
                    );
                }
            }
        }

        println!("📞 RPC call: catalyst_getBalance({}) (mock)", address);
        Ok("10000000000000000000".to_string())
    }

    async fn get_account(
        &self,
        address: String,
        _block_number: Option<String>,
    ) -> RpcResult<Option<RpcAccount>> {
        if let Some(storage) = &self.storage_service {
            match storage.get_account_balance(&address).await {
                Ok(balance) => {
                    let nonce = storage.get_account_nonce(&address).await.unwrap_or(0);

                    println!("📞 RPC call: catalyst_getAccount({}) -> balance: {}, nonce: {} (from storage)", address, balance, nonce);

                    return Ok(Some(RpcAccount {
                        address: address.clone(),
                        balance,
                        nonce,
                        code_hash: None,
                        storage_hash: None,
                        account_type: "user".to_string(),
                    }));
                }
                Err(_e) => {
                    println!(
                        "❌ RPC error: catalyst_getAccount({}) -> storage error",
                        address
                    );
                }
            }
        }

        println!("📞 RPC call: catalyst_getAccount({}) (mock)", address);
        Ok(Some(RpcAccount {
            address: address.clone(),
            balance: "10000000000000000000".to_string(),
            nonce: 42,
            code_hash: None,
            storage_hash: None,
            account_type: "user".to_string(),
        }))
    }

    async fn get_transaction_count(
        &self,
        address: String,
        _block_number: Option<String>,
    ) -> RpcResult<u64> {
        if let Some(storage) = &self.storage_service {
            match storage.get_account_nonce(&address).await {
                Ok(nonce) => {
                    println!(
                        "📞 RPC call: catalyst_getTransactionCount({}) -> {} (from storage)",
                        address, nonce
                    );
                    return Ok(nonce);
                }
                Err(_e) => {
                    println!(
                        "❌ RPC error: catalyst_getTransactionCount({}) -> storage error",
                        address
                    );
                }
            }
        }

        println!(
            "📞 RPC call: catalyst_getTransactionCount({}) (mock)",
            address
        );
        Ok(42)
    }

    // === Transactions === (Now using real storage!)

    async fn get_transaction_by_hash(&self, hash: String) -> RpcResult<Option<RpcTransaction>> {
        if let Some(storage) = &self.storage_service {
            match storage.get_transaction_by_hash(&hash).await {
                Ok(Some(tx_data)) => {
                    println!(
                        "📞 RPC call: catalyst_getTransactionByHash({}) -> found (from storage)",
                        hash
                    );
                    return Ok(Some(self.convert_transaction_data_to_rpc(&tx_data)));
                }
                Ok(None) => {
                    println!(
                        "📞 RPC call: catalyst_getTransactionByHash({}) -> None",
                        hash
                    );
                    return Ok(None);
                }
                Err(_e) => {
                    println!(
                        "❌ RPC error: catalyst_getTransactionByHash({}) -> storage error",
                        hash
                    );
                }
            }
        }

        println!(
            "📞 RPC call: catalyst_getTransactionByHash({}) (mock)",
            hash
        );
        Ok(None)
    }

    async fn get_transaction_receipt(
        &self,
        hash: String,
    ) -> RpcResult<Option<RpcTransactionReceipt>> {
        println!("📞 RPC call: catalyst_getTransactionReceipt({})", hash);
        Ok(None) // No receipts in basic implementation
    }

    async fn send_raw_transaction(&self, _signed_transaction_data: String) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_sendRawTransaction");
        // Generate a mock transaction hash using timestamp
        let time_bytes = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_le_bytes();

        let tx_hash: Hash = Sha256::digest(&time_bytes).into(); // uses Hash::from(GenericArray)
        let tx_hash_hex = format!("0x{}", hex::encode(tx_hash.as_bytes()));
        Ok(tx_hash_hex)
    }

    // === Smart Contracts ===
    async fn call(
        &self,
        _transaction: RpcTransactionRequest,
        _block_number: Option<String>,
    ) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_call");
        Ok("0x".to_string()) // Empty result
    }

    async fn estimate_gas(&self, _transaction: RpcTransactionRequest) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_estimateGas");
        Ok("21000".to_string()) // Standard gas limit
    }

    async fn get_code(&self, _address: String, _block_number: Option<String>) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_getCode");
        Ok("0x".to_string()) // No code (EOA)
    }

    async fn get_storage_at(
        &self,
        _address: String,
        _position: String,
        _block_number: Option<String>,
    ) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_getStorageAt");
        Ok("0x0000000000000000000000000000000000000000000000000000000000000000".to_string())
    }

    async fn get_logs(&self, _filter: RpcFilter) -> RpcResult<Vec<RpcLog>> {
        println!("📞 RPC call: catalyst_getLogs");
        Ok(vec![]) // No logs in basic implementation
    }

    // === Chain Information ===
    async fn chain_id(&self) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_chainId");
        Ok("0x539".to_string()) // 1337 in hex (local chain)
    }

    async fn gas_price(&self) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_gasPrice");
        Ok("1000000000".to_string()) // 1 Gwei
    }

    // === System & Metrics === (Enhanced with real storage stats)

    async fn system_health(&self) -> RpcResult<RpcHealth> {
        println!("📞 RPC call: catalyst_systemHealth");

        let storage_healthy = if let Some(storage) = &self.storage_service {
            storage.health_check().await
        } else {
            false
        };

        Ok(RpcHealth {
            is_syncing: false,
            peers: 0,
            should_have_peers: false,
            is_healthy: storage_healthy,
        })
    }

    async fn system_properties(&self) -> RpcResult<RpcProperties> {
        println!("📞 RPC call: catalyst_systemProperties");
        Ok(RpcProperties {
            chain: "Catalyst Local".to_string(),
            implementation: "catalyst-node".to_string(),
            version: "1.0.0-storage".to_string(),
        })
    }

    async fn metrics(&self) -> RpcResult<RpcMetrics> {
        println!("📞 RPC call: catalyst_metrics");

        // Get real storage statistics if available
        let (total_blocks, database_size) = if let Some(storage) = &self.storage_service {
            match storage.get_storage_statistics().await {
                Ok(stats) => stats,
                Err(_) => (0, 0),
            }
        } else {
            (0, 0)
        };

        Ok(RpcMetrics {
            uptime: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            block_height: total_blocks.saturating_sub(1), // Convert to 0-based
            peer_count: 0,
            transaction_pool_size: 0,
            memory_usage: database_size,
            disk_usage: database_size,
        })
    }
}

// Re-export for backwards compatibility
pub use CatalystRpcError as RpcError;
