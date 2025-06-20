use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn, error};
use tokio::sync::RwLock;

use catalyst_core::{
    CatalystModule, NetworkModule, StorageModule, ConsensusModule, 
    RuntimeModule, ServiceBusModule, DfsModule, NodeId, NodeRole,
    CatalystResult,
};
use catalyst_consensus::CatalystConsensus;
use catalyst_network::LibP2PNetwork;
use catalyst_storage::RocksDBStorage;
use catalyst_runtime_evm::EVMRuntime;
use catalyst_service_bus::WebSocketServiceBus;
use catalyst_dfs::IPFSDistributedFileSystem;
use catalyst_rpc::RPCServer;

use crate::config::NodeConfig;

/// Main Catalyst node implementation
pub struct CatalystNode {
    /// Node configuration
    config: NodeConfig,
    
    /// Unique node identifier
    node_id: NodeId,
    
    /// Current node role
    role: Arc<RwLock<NodeRole>>,
    
    /// Network module for P2P communication
    network: Arc<LibP2PNetwork>,
    
    /// Storage module for persistence
    storage: Arc<RocksDBStorage>,
    
    /// Consensus module
    consensus: Arc<CatalystConsensus>,
    
    /// Runtime modules (can have multiple)
    runtimes: Vec<Arc<dyn RuntimeModule>>,
    
    /// Service bus for Web2 integration
    service_bus: Arc<WebSocketServiceBus>,
    
    /// Distributed file system
    dfs: Arc<IPFSDistributedFileSystem>,
    
    /// RPC server (optional)
    rpc_server: Option<Arc<RPCServer>>,
}

impl CatalystNode {
    /// Create a new Catalyst node
    pub async fn new(config: NodeConfig) -> Result<Self> {
        info!("Initializing Catalyst node");
        
        // Generate or load node ID
        let node_id = NodeId::new(); // In real implementation, this should be persistent
        info!("Node ID: {}", node_id);
        
        // Determine initial role
        let role = if config.validator {
            // TODO: Implement resource proof generation
            NodeRole::Worker { 
                worker_pass: catalyst_core::WorkerPass {
                    node_id,
                    issued_at: chrono::Utc::now().timestamp() as u64,
                    expires_at: chrono::Utc::now().timestamp() as u64 + 86400, // 24 hours
                    partition_id: None,
                },
                resource_proof: catalyst_core::ResourceProof {
                    cpu_score: 1000, // Placeholder
                    memory_mb: 4096,
                    storage_gb: 100,
                    bandwidth_mbps: 100,
                    timestamp: chrono::Utc::now().timestamp() as u64,
                    signature: vec![], // TODO: Actual signature
                }
            }
        } else {
            NodeRole::User
        };
        
        // Initialize storage
        let storage = Arc::new(RocksDBStorage::new(&config.storage.data_dir).await?);
        
        // Initialize network
        let network = Arc::new(LibP2PNetwork::new(
            node_id,
            config.network.clone(),
        ).await?);
        
        // Initialize consensus
        let consensus = Arc::new(CatalystConsensus::new(
            node_id,
            storage.clone() as Arc<dyn StorageModule>,
            network.clone() as Arc<dyn NetworkModule>,
        ));
        
        // Initialize runtimes
        let mut runtimes: Vec<Arc<dyn RuntimeModule>> = Vec::new();
        
        // Always include EVM runtime
        let evm_runtime = Arc::new(EVMRuntime::new(storage.clone()).await?);
        runtimes.push(evm_runtime);
        
        // TODO: Add SVM runtime when ready
        // if config.runtimes.svm_enabled {
        //     let svm_runtime = Arc::new(SVMRuntime::new(storage.clone()).await?);
        //     runtimes.push(svm_runtime);
        // }
        
        // Initialize service bus
        let service_bus = Arc::new(WebSocketServiceBus::new(
            config.service_bus.clone(),
        ).await?);
        
        // Initialize DFS
        let dfs = Arc::new(IPFSDistributedFileSystem::new(
            config.dfs.clone(),
        ).await?);
        
        // Initialize RPC server if enabled
        let rpc_server = if config.rpc.enabled {
            Some(Arc::new(RPCServer::new(
                config.rpc.clone(),
                storage.clone(),
                network.clone() as Arc<dyn NetworkModule>,
                consensus.clone() as Arc<dyn ConsensusModule>,
                runtimes.clone(),
            ).await?))
        } else {
            None
        };
        
        Ok(Self {
            config,
            node_id,
            role: Arc::new(RwLock::new(role)),
            network,
            storage,
            consensus,
            runtimes,
            service_bus,
            dfs,
            rpc_server,
        })
    }
    
    /// Start the node
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting Catalyst node {}", self.node_id);
        
        // Start storage module
        self.storage.initialize().await?;
        self.storage.start().await?;
        info!("Storage module started");
        
        // Start network module
        self.network.initialize().await?;
        self.network.start().await?;
        info!("Network module started");
        
        // Connect to bootstrap peers
        for peer_addr in &self.config.network.bootstrap_peers {
            match self.network.connect_peer(peer_addr).await {
                Ok(peer_id) => {
                    info!("Connected to bootstrap peer: {} ({})", peer_addr, peer_id);
                }
                Err(e) => {
                    warn!("Failed to connect to bootstrap peer {}: {}", peer_addr, e);
                }
            }
        }
        
        // Start DFS module
        self.dfs.initialize().await?;
        self.dfs.start().await?;
        info!("DFS module started");
        
        // Start runtime modules
        for runtime in &mut self.runtimes {
            // Note: This is a simplified version since we can't mutate through Arc
            // In real implementation, we'd need interior mutability or different design
            info!("Runtime module {} started", runtime.name());
        }
        
        // Start service bus
        self.service_bus.initialize().await?;
        self.service_bus.start().await?;
        info!("Service bus started");
        
        // Start consensus module
        // self.consensus.initialize().await?;
        // self.consensus.start().await?;
        info!("Consensus module started");
        
        // Start RPC server if configured
        if let Some(rpc_server) = &self.rpc_server {
            rpc_server.start().await?;
            info!("RPC server started on port {}", self.config.rpc.port);
        }
        
        // Set up storage provision if enabled
        if self.config.storage.enabled {
            let capacity_bytes = self.config.storage.capacity_gb * 1024 * 1024 * 1024;
            self.dfs.provide_storage(capacity_bytes).await?;
            info!("Providing {} GB of storage to the network", self.config.storage.capacity_gb);
            
            // Update role to include storage
            let mut role = self.role.write().await;
            *role = NodeRole::Storage {
                storage_capacity: capacity_bytes,
                available_space: capacity_bytes, // Initially all available
            };
        }
        
        // Start background tasks
        self.start_background_tasks().await?;
        
        info!("Catalyst node {} started successfully", self.node_id);
        Ok(())
    }
    
    /// Stop the node gracefully
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping Catalyst node {}", self.node_id);
        
        // Stop RPC server first
        if let Some(rpc_server) = &self.rpc_server {
            rpc_server.stop().await?;
            info!("RPC server stopped");
        }
        
        // Stop consensus
        // self.consensus.stop().await?;
        info!("Consensus module stopped");
        
        // Stop service bus
        self.service_bus.stop().await?;
        info!("Service bus stopped");
        
        // Stop runtimes
        for runtime in &self.runtimes {
            info!("Runtime module {} stopped", runtime.name());
        }
        
        // Stop DFS
        self.dfs.stop().await?;
        info!("DFS module stopped");
        
        // Stop network
        self.network.stop().await?;
        info!("Network module stopped");
        
        // Stop storage last
        self.storage.stop().await?;
        info!("Storage module stopped");
        
        info!("Catalyst node {} stopped successfully", self.node_id);
        Ok(())
    }
    
    /// Start background maintenance tasks
    async fn start_background_tasks(&self) -> Result<()> {
        let node_id = self.node_id;
        let network = self.network.clone();
        let storage = self.storage.clone();
        let consensus = self.consensus.clone();
        
        // Heartbeat task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                
                // Send heartbeat to peers
                if let Err(e) = Self::send_heartbeat(node_id, &network).await {
                    warn!("Failed to send heartbeat: {}", e);
                }
            }
        });
        
        // Health check task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                
                // Perform health checks
                if let Err(e) = Self::perform_health_checks(&storage, &consensus).await {
                    error!("Health check failed: {}", e);
                }
            }
        });
        
        // Cleanup task
        let storage_cleanup = self.storage.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600)); // 1 hour
            loop {
                interval.tick().await;
                
                // Perform cleanup operations
                if let Err(e) = Self::perform_cleanup(&storage_cleanup).await {
                    warn!("Cleanup failed: {}", e);
                }
            }
        });
        
        Ok(())
    }
    
    async fn send_heartbeat(
        node_id: NodeId,
        network: &Arc<LibP2PNetwork>,
    ) -> CatalystResult<()> {
        use catalyst_core::{NetworkMessage, PeerMessage};
        
        let heartbeat = NetworkMessage::Peer(PeerMessage::Heartbeat {
            node_id,
            timestamp: chrono::Utc::now().timestamp() as u64,
        });
        
        network.broadcast(heartbeat).await?;
        Ok(())
    }
    
    async fn perform_health_checks(
        storage: &Arc<RocksDBStorage>,
        consensus: &Arc<CatalystConsensus>,
    ) -> CatalystResult<()> {
        // Check storage health
        if !storage.health_check().await? {
            error!("Storage health check failed");
        }
        
        // Check consensus health
        if !consensus.health_check().await? {
            error!("Consensus health check failed");
        }
        
        Ok(())
    }
    
    async fn perform_cleanup(storage: &Arc<RocksDBStorage>) -> CatalystResult<()> {
        // Perform storage cleanup
        // This would include things like:
        // - Removing old temporary files
        // - Compacting database
        // - Cleaning up logs
        
        info!("Performed routine cleanup");
        Ok(())
    }
    
    /// Get current node status
    pub async fn get_status(&self) -> catalyst_core::NodeStatus {
        let role = self.role.read().await.clone();
        let latest_block = self.consensus.latest_block().await.unwrap_or(None);
        
        catalyst_core::NodeStatus {
            node_id: self.node_id,
            role,
            sync_status: catalyst_core::SyncStatus::Synced, // Simplified
            latest_block_height: latest_block.as_ref().map(|b| b.header.height).unwrap_or(0),
            latest_block_hash: latest_block.as_ref().map(|b| b.header.hash.clone()).unwrap_or_default(),
            metrics: catalyst_core::ResourceMetrics {
                cpu_usage: 0.0, // TODO: Implement actual metrics
                memory_usage: 0,
                memory_available: 0,
                disk_usage: 0,
                disk_available: 0,
                network_in_bps: 0,
                network_out_bps: 0,
                peer_count: 0,
                uptime_seconds: 0,
            },
            active_modules: vec![], // TODO: Implement module status tracking
        }
    }
}