//! Replace the ENTIRE crates/catalyst-rpc/src/lib.rs with this clean version:

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

/// Simple RPC API
#[rpc(server)]
pub trait CatalystRpc {
    /// Get the current block number
    #[method(name = "catalyst_blockNumber")]
    async fn block_number(&self) -> RpcResult<u64>;
    
    /// Get node status
    #[method(name = "catalyst_status")]
    async fn status(&self) -> RpcResult<RpcNodeStatus>;
    
    /// Get peer count
    #[method(name = "catalyst_peerCount")]
    async fn peer_count(&self) -> RpcResult<u64>;
    
    /// Get node version
    #[method(name = "catalyst_version")]
    async fn version(&self) -> RpcResult<String>;
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
        
        // Build the server - exactly like working test
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
        log::info!("🔍 Available methods:");
        log::info!("  - catalyst_blockNumber");
        log::info!("  - catalyst_status");  
        log::info!("  - catalyst_peerCount");
        log::info!("  - catalyst_version");
        
        // Give the server more time to bind (like successful test)
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
    async fn block_number(&self) -> RpcResult<u64> {
        let state = self.blockchain_state.read().await;
        println!("📞 RPC call received: catalyst_blockNumber -> {}", state.current_block);
        Ok(state.current_block)
    }
    
    async fn status(&self) -> RpcResult<RpcNodeStatus> {
        let state = self.blockchain_state.read().await;
        println!("📞 RPC call received: catalyst_status");
        
        let status = RpcNodeStatus {
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
        };
        
        Ok(status)
    }
    
    async fn peer_count(&self) -> RpcResult<u64> {
        let state = self.blockchain_state.read().await;
        println!("📞 RPC call received: catalyst_peerCount -> {}", state.peer_count);
        Ok(state.peer_count)
    }
    
    async fn version(&self) -> RpcResult<String> {
        println!("📞 RPC call received: catalyst_version");
        Ok("Catalyst/1.0.0".to_string())
    }
}

// Re-export for backwards compatibility
pub use CatalystRpcError as RpcError;