//! JSON-RPC server for Catalyst Network
//! Complete corrected version

use async_trait::async_trait;
use catalyst_core::{NodeStatus, SyncStatus, ResourceMetrics};
use jsonrpsee::{
    core::RpcResult,
    proc_macros::rpc,
    server::{ServerBuilder, ServerHandle},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::RwLock;

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
    /// Server port
    pub port: u16,
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
            enabled: false,
            port: 9933,
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
    
    /// Get peer count
    #[method(name = "catalyst_peerCount")]
    async fn peer_count(&self) -> RpcResult<u64>;
    
    /// Get network information
    #[method(name = "catalyst_networkInfo")]
    async fn network_info(&self) -> RpcResult<RpcNetworkInfo>;
    
    /// Get synchronization status
    #[method(name = "catalyst_syncing")]
    async fn syncing(&self) -> RpcResult<RpcSyncStatus>;
    
    // === Blockchain State ===
    /// Get the current block number
    #[method(name = "catalyst_blockNumber")]
    async fn block_number(&self) -> RpcResult<u64>;
    
    /// Get block by hash
    #[method(name = "catalyst_getBlockByHash")]
    async fn get_block_by_hash(&self, hash: String, full_transactions: bool) -> RpcResult<Option<RpcBlock>>;
    
    /// Get block by number
    #[method(name = "catalyst_getBlockByNumber")]
    async fn get_block_by_number(&self, number: String, full_transactions: bool) -> RpcResult<Option<RpcBlock>>;
    
    /// Get latest block
    #[method(name = "catalyst_getLatestBlock")]
    async fn get_latest_block(&self, full_transactions: bool) -> RpcResult<RpcBlock>;
    
    // === Transactions ===
    /// Get transaction by hash
    #[method(name = "catalyst_getTransactionByHash")]
    async fn get_transaction_by_hash(&self, hash: String) -> RpcResult<Option<RpcTransaction>>;
    
    /// Get transaction receipt
    #[method(name = "catalyst_getTransactionReceipt")]
    async fn get_transaction_receipt(&self, hash: String) -> RpcResult<Option<RpcTransactionReceipt>>;
    
    /// Send raw transaction
    #[method(name = "catalyst_sendRawTransaction")]
    async fn send_raw_transaction(&self, data: String) -> RpcResult<String>;
    
    /// Estimate transaction fee
    #[method(name = "catalyst_estimateFee")]
    async fn estimate_fee(&self, transaction: RpcTransactionRequest) -> RpcResult<RpcFeeEstimate>;
    
    /// Get pending transactions
    #[method(name = "catalyst_pendingTransactions")]
    async fn pending_transactions(&self) -> RpcResult<Vec<RpcTransaction>>;
    
    // === Accounts & Balances ===
    /// Get account balance
    #[method(name = "catalyst_getBalance")]
    async fn get_balance(&self, address: String, block_number: Option<String>) -> RpcResult<String>;
    
    /// Get account information
    #[method(name = "catalyst_getAccount")]
    async fn get_account(&self, address: String, block_number: Option<String>) -> RpcResult<Option<RpcAccount>>;
    
    /// Get account transaction count (nonce)
    #[method(name = "catalyst_getTransactionCount")]
    async fn get_transaction_count(&self, address: String, block_number: Option<String>) -> RpcResult<u64>;
    
    // === Smart Contracts (EVM) ===
    /// Call contract method (read-only)
    #[method(name = "catalyst_call")]
    async fn call(&self, call_request: RpcCallRequest, block_number: Option<String>) -> RpcResult<String>;
    
    /// Estimate gas for transaction
    #[method(name = "catalyst_estimateGas")]
    async fn estimate_gas(&self, call_request: RpcCallRequest) -> RpcResult<String>;
    
    /// Get contract code
    #[method(name = "catalyst_getCode")]
    async fn get_code(&self, address: String, block_number: Option<String>) -> RpcResult<String>;
    
    /// Get storage at position
    #[method(name = "catalyst_getStorageAt")]
    async fn get_storage_at(&self, address: String, position: String, block_number: Option<String>) -> RpcResult<String>;
    
    // === Mining/Validation ===
    /// Get validator status
    #[method(name = "catalyst_validatorStatus")]
    async fn validator_status(&self) -> RpcResult<RpcValidatorStatus>;
    
    /// Get validator list
    #[method(name = "catalyst_getValidators")]
    async fn get_validators(&self, block_number: Option<String>) -> RpcResult<Vec<RpcValidator>>;
    
    /// Submit block (for validators)
    #[method(name = "catalyst_submitBlock")]
    async fn submit_block(&self, block_data: String) -> RpcResult<bool>;
    
    // === Chain Information ===
    /// Get chain ID
    #[method(name = "catalyst_chainId")]
    async fn chain_id(&self) -> RpcResult<String>;
    
    /// Get gas price
    #[method(name = "catalyst_gasPrice")]
    async fn gas_price(&self) -> RpcResult<String>;
    
    /// Get genesis block
    #[method(name = "catalyst_getGenesis")]
    async fn get_genesis(&self) -> RpcResult<RpcBlock>;
}

/// RPC server implementation with persistent handle
pub struct RpcServer {
    config: RpcConfig,
    server_handle: Arc<RwLock<Option<ServerHandle>>>,
    blockchain_state: Arc<RwLock<BlockchainState>>,
    keep_alive_task: Arc<RwLock<Option<tokio::task::JoinHandle<()>>>>,
}

/// Mock blockchain state for development
#[derive(Debug, Clone)]
pub struct BlockchainState {
    pub current_block: u64,
    pub peer_count: u64,
    pub sync_status: SyncStatus,
    pub node_status: NodeStatus,
}

impl Default for BlockchainState {
    fn default() -> Self {
        Self {
            current_block: 12345,
            peer_count: 8,
            sync_status: SyncStatus::Synced,
            node_status: NodeStatus {
                id: "catalyst-node".to_string(),
                uptime: 3600, // 1 hour
                sync_status: SyncStatus::Synced,
                metrics: ResourceMetrics {
                    cpu_usage: 25.5,
                    memory_usage: 512 * 1024 * 1024, // 512MB
                    disk_usage: 2 * 1024 * 1024 * 1024, // 2GB
                },
            },
        }
    }
}

impl RpcServer {
    pub fn new(config: RpcConfig) -> Self {
        Self {
            config,
            server_handle: Arc::new(RwLock::new(None)),
            blockchain_state: Arc::new(RwLock::new(BlockchainState::default())),
            keep_alive_task: Arc::new(RwLock::new(None)),
        }
    }
    
    pub async fn start(&self) -> Result<(), CatalystRpcError> {
        if !self.config.enabled {
            log::info!("RPC server is disabled in configuration");
            return Ok(());
        }

        let bind_address = format!("{}:{}", self.config.address, self.config.port);
        log::info!("🚀 Starting RPC server on {}", bind_address);
        
        // Create the RPC implementation
        let rpc_impl = CatalystRpcImpl::new(self.blockchain_state.clone());
        
        // Build the server
        let server = ServerBuilder::default()
            .build(&bind_address)
            .await
            .map_err(|e| {
                log::error!("❌ Failed to build RPC server: {}", e);
                CatalystRpcError::Server(format!("Failed to build server: {}", e))
            })?;
        
        log::info!("📡 RPC server built successfully on {}", bind_address);
        
        // Register the RPC methods
        let methods = rpc_impl.into_rpc();
        let method_count = methods.method_names().count();
        log::info!("📋 Registered {} RPC methods", method_count);
        
        // Start the server
        let handle = server.start(methods);
        
        // Create a task to keep the server alive
        let server_handle_clone = handle.clone();
        let keep_alive_task = tokio::spawn(async move {
            log::info!("🔧 Keep-alive task started for RPC server");
            server_handle_clone.stopped().await;
            log::info!("🛑 RPC server stopped");
        });
        
        // Store both the handle and the keep-alive task
        *self.server_handle.write().await = Some(handle);
        *self.keep_alive_task.write().await = Some(keep_alive_task);
        
        log::info!("✅ RPC server started and listening on {}", bind_address);
        log::info!("🔍 Available methods: {} methods registered", method_count);
        
        // Give the server more time to bind
        log::info!("⏳ Waiting for server to fully start...");
        tokio::time::sleep(tokio::time::Duration::from_millis(2000)).await;
        
        // Test port binding multiple times
        for attempt in 1..=3 {
            match tokio::net::TcpStream::connect(&bind_address).await {
                Ok(_) => {
                    log::info!("✅ Port {} is accessible on attempt {}!", self.config.port, attempt);
                    log::info!("🎯 RPC server is ready for connections!");
                    return Ok(());
                }
                Err(e) => {
                    log::warn!("⚠️ Attempt {}: Port {} not accessible: {}", attempt, self.config.port, e);
                    if attempt < 3 {
                        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
                    }
                }
            }
        }
        
        // If we get here, all attempts failed
        log::error!("❌ Failed to verify port accessibility after 3 attempts");
        Err(CatalystRpcError::Server("Server appears to start but port is not accessible".to_string()))
    }
    
    pub async fn stop(&self) -> Result<(), CatalystRpcError> {
        log::info!("Stopping RPC server...");
        
        // Stop the keep-alive task first
        if let Some(task) = self.keep_alive_task.write().await.take() {
            task.abort();
        }
        
        if let Some(handle) = self.server_handle.write().await.take() {
            handle.stop().map_err(|e| CatalystRpcError::Server(format!("Failed to stop server: {}", e)))?;
            log::info!("✅ RPC server stopped");
        } else {
            log::warn!("RPC server was not running");
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

/// Implementation of the RPC methods
pub struct CatalystRpcImpl {
    blockchain_state: Arc<RwLock<BlockchainState>>,
}

impl CatalystRpcImpl {
    pub fn new(blockchain_state: Arc<RwLock<BlockchainState>>) -> Self {
        Self { blockchain_state }
    }
}

#[async_trait]
impl CatalystRpcServer for CatalystRpcImpl {
    // === Node Information ===
    async fn version(&self) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_version");
        Ok("Catalyst/1.0.0".to_string())
    }
    
    async fn status(&self) -> RpcResult<RpcNodeStatus> {
        let state = self.blockchain_state.read().await;
        println!("📞 RPC call: catalyst_status");
        
        Ok(RpcNodeStatus {
            node_id: state.node_status.id.clone(),
            uptime: state.node_status.uptime,
            sync_status: match state.node_status.sync_status {
                SyncStatus::Synced => "synced".to_string(),
                SyncStatus::Syncing { .. } => "syncing".to_string(),
                SyncStatus::NotSynced => "not_synced".to_string(),
            },
            block_height: state.current_block,
            peer_count: state.peer_count,
            cpu_usage: state.node_status.metrics.cpu_usage,
            memory_usage: state.node_status.metrics.memory_usage,
            disk_usage: state.node_status.metrics.disk_usage,
        })
    }
    
    async fn peer_count(&self) -> RpcResult<u64> {
        let state = self.blockchain_state.read().await;
        println!("📞 RPC call: catalyst_peerCount -> {}", state.peer_count);
        Ok(state.peer_count)
    }
    
    async fn network_info(&self) -> RpcResult<RpcNetworkInfo> {
        let state = self.blockchain_state.read().await;
        println!("📞 RPC call: catalyst_networkInfo");
        
        Ok(RpcNetworkInfo {
            chain_id: 1337,
            network_id: 1337,
            protocol_version: "catalyst/1.0".to_string(),
            genesis_hash: "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            current_block: state.current_block,
            highest_block: state.current_block,
            peer_count: state.peer_count,
        })
    }
    
    async fn syncing(&self) -> RpcResult<RpcSyncStatus> {
        let state = self.blockchain_state.read().await;
        println!("📞 RPC call: catalyst_syncing");
        
        match state.sync_status {
            SyncStatus::Synced => Ok(RpcSyncStatus::Synced(true)),
            SyncStatus::Syncing { progress } => Ok(RpcSyncStatus::Syncing {
                starting_block: 0,
                current_block: state.current_block,
                highest_block: (state.current_block as f64 / progress * 100.0) as u64,
            }),
            SyncStatus::NotSynced => Ok(RpcSyncStatus::Synced(false)),
        }
    }
    
    // === Blockchain State ===
    async fn block_number(&self) -> RpcResult<u64> {
        let state = self.blockchain_state.read().await;
        println!("📞 RPC call: catalyst_blockNumber -> {}", state.current_block);
        Ok(state.current_block)
    }
    
    async fn get_block_by_hash(&self, hash: String, _full_transactions: bool) -> RpcResult<Option<RpcBlock>> {
        println!("📞 RPC call: catalyst_getBlockByHash({})", hash);
        
        // Mock block
        Ok(Some(RpcBlock {
            hash: hash.clone(),
            number: 12345,
            parent_hash: "0x1234567890abcdef".to_string(),
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
        }))
    }
    
    async fn get_block_by_number(&self, number: String, full_transactions: bool) -> RpcResult<Option<RpcBlock>> {
        println!("📞 RPC call: catalyst_getBlockByNumber({}, {})", number, full_transactions);
        
        let block_num: u64 = if number == "latest" {
            self.blockchain_state.read().await.current_block
        } else {
            // Fix the error handling - just use unwrap_or for now
            number.parse().unwrap_or(0)
        };
        
        self.get_block_by_hash(format!("0x{:064x}", block_num), full_transactions).await
    }
    
    async fn get_latest_block(&self, _full_transactions: bool) -> RpcResult<RpcBlock> {
        println!("📞 RPC call: catalyst_getLatestBlock");
        
        let state = self.blockchain_state.read().await;
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
    
    // === Transactions ===
    async fn get_transaction_by_hash(&self, hash: String) -> RpcResult<Option<RpcTransaction>> {
        println!("📞 RPC call: catalyst_getTransactionByHash({})", hash);
        
        Ok(Some(RpcTransaction {
            hash: hash.clone(),
            block_hash: Some("0x1234567890abcdef".to_string()),
            block_number: Some(12345),
            transaction_index: Some(0),
            from: "0xabcdef1234567890abcdef1234567890abcdef12".to_string(),
            to: Some("0x9876543210fedcba9876543210fedcba98765432".to_string()),
            value: "1000000000000000000".to_string(),
            data: "0x".to_string(),
            gas_limit: "21000".to_string(),
            gas_price: "1000000000".to_string(),
            gas_used: Some("21000".to_string()),
            status: Some("success".to_string()),
            nonce: 1,
        }))
    }
    
    async fn get_transaction_receipt(&self, hash: String) -> RpcResult<Option<RpcTransactionReceipt>> {
        println!("📞 RPC call: catalyst_getTransactionReceipt({})", hash);
        
        Ok(Some(RpcTransactionReceipt {
            transaction_hash: hash,
            transaction_index: 0,
            block_hash: "0x1234567890abcdef".to_string(),
            block_number: 12345,
            from: "0xabcdef1234567890abcdef1234567890abcdef12".to_string(),
            to: Some("0x9876543210fedcba9876543210fedcba98765432".to_string()),
            cumulative_gas_used: "21000".to_string(),
            gas_used: "21000".to_string(),
            contract_address: None,
            logs: vec![],
            status: "success".to_string(),
        }))
    }
    
    async fn send_raw_transaction(&self, data: String) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_sendRawTransaction({})", data);
        
        // Generate mock transaction hash
        let tx_hash = format!("0x{:064x}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos());
        
        println!("Generated transaction hash: {}", tx_hash);
        Ok(tx_hash)
    }
    
    async fn estimate_fee(&self, _transaction: RpcTransactionRequest) -> RpcResult<RpcFeeEstimate> {
        println!("📞 RPC call: catalyst_estimateFee");
        
        Ok(RpcFeeEstimate {
            gas_estimate: "21000".to_string(),
            gas_price: "1000000000".to_string(),
            total_fee: "21000000000000".to_string(),
        })
    }
    
    async fn pending_transactions(&self) -> RpcResult<Vec<RpcTransaction>> {
        println!("📞 RPC call: catalyst_pendingTransactions");
        Ok(vec![]) // Empty for now
    }
    
    // === Accounts & Balances ===
    async fn get_balance(&self, address: String, _block_number: Option<String>) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_getBalance({})", address);
        Ok("10000000000000000000".to_string()) // 10 tokens
    }
    
    async fn get_account(&self, address: String, _block_number: Option<String>) -> RpcResult<Option<RpcAccount>> {
        println!("📞 RPC call: catalyst_getAccount({})", address);
        
        Ok(Some(RpcAccount {
            address: address.clone(),
            balance: "10000000000000000000".to_string(),
            nonce: 42,
            code_hash: None,
            storage_hash: None,
            account_type: "user".to_string(),
        }))
    }
    
    async fn get_transaction_count(&self, address: String, _block_number: Option<String>) -> RpcResult<u64> {
        println!("📞 RPC call: catalyst_getTransactionCount({})", address);
        Ok(42) // Mock nonce
    }
    
    // === Smart Contracts ===
    async fn call(&self, call_request: RpcCallRequest, _block_number: Option<String>) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_call({:?})", call_request.to);
        Ok("0x".to_string()) // Empty result
    }
    
    async fn estimate_gas(&self, call_request: RpcCallRequest) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_estimateGas({:?})", call_request.to);
        Ok("21000".to_string())
    }
    
    async fn get_code(&self, address: String, _block_number: Option<String>) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_getCode({})", address);
        Ok("0x".to_string()) // No code
    }
    
    async fn get_storage_at(&self, address: String, position: String, _block_number: Option<String>) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_getStorageAt({}, {})", address, position);
        Ok("0x0000000000000000000000000000000000000000000000000000000000000000".to_string())
    }
    
    // === Mining/Validation ===
    async fn validator_status(&self) -> RpcResult<RpcValidatorStatus> {
        println!("📞 RPC call: catalyst_validatorStatus");
        
        Ok(RpcValidatorStatus {
            is_validator: false,
            validator_address: None,
            stake: None,
            is_active: false,
            blocks_produced: 0,
            last_block_produced: None,
            next_block_slot: None,
        })
    }
    
    async fn get_validators(&self, _block_number: Option<String>) -> RpcResult<Vec<RpcValidator>> {
        println!("📞 RPC call: catalyst_getValidators");
        Ok(vec![]) // Empty for now
    }
    
    async fn submit_block(&self, block_data: String) -> RpcResult<bool> {
        println!("📞 RPC call: catalyst_submitBlock (data length: {})", block_data.len());
        Ok(true) // Accept for now
    }
    
    // === Chain Information ===
    async fn chain_id(&self) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_chainId");
        Ok("0x539".to_string()) // 1337 in hex
    }
    
    async fn gas_price(&self) -> RpcResult<String> {
        println!("📞 RPC call: catalyst_gasPrice");
        Ok("1000000000".to_string()) // 1 Gwei
    }
    
    async fn get_genesis(&self) -> RpcResult<RpcBlock> {
        println!("📞 RPC call: catalyst_getGenesis");
        
        Ok(RpcBlock {
            hash: "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            number: 0,
            parent_hash: "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            timestamp: 1640995200, // Jan 1, 2022
            transactions: vec![],
            transaction_count: 0,
            size: 1024,
            gas_limit: "8000000".to_string(),
            gas_used: "0".to_string(),
            validator: "0x0000000000000000000000000000000000000000".to_string(),
            state_root: "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421".to_string(),
            transactions_root: "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421".to_string(),
        })
    }
}

// Re-export for backwards compatibility
pub use CatalystRpcError as RpcError;