//! Catalyst Node Manager - Simplified Version
//! Complete single-file implementation to get building

use async_trait::async_trait;
use catalyst_config::CatalystConfig;
use catalyst_core::NodeStatus;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn, error};

/// Node management errors
#[derive(Error, Debug)]
pub enum NodeError {
    #[error("Service error: {0}")]
    Service(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Consensus error: {0}")]
    Consensus(String),
    #[error("Runtime error: {0}")]
    Runtime(String),
}

/// Service types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ServiceType {
    Storage,
    Network,
    Consensus,
    Runtime,
    Rpc,
    Metrics,
    Custom(u32),
}

impl std::fmt::Display for ServiceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceType::Storage => write!(f, "Storage"),
            ServiceType::Network => write!(f, "Network"),
            ServiceType::Consensus => write!(f, "Consensus"),
            ServiceType::Runtime => write!(f, "Runtime"),
            ServiceType::Rpc => write!(f, "RPC"),
            ServiceType::Metrics => write!(f, "Metrics"),
            ServiceType::Custom(id) => write!(f, "Custom({})", id),
        }
    }
}

/// Service health status
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceHealth {
    Healthy,
    Unhealthy,
    Starting,
    Stopping,
    Unknown,
}

/// System events for inter-service communication
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Event {
    ServiceStarted {
        service_type: ServiceType,
        name: String,
    },
    ServiceStopped {
        service_type: ServiceType,
        name: String,
    },
    ServiceError {
        service_type: ServiceType,
        error: String,
    },
    BlockReceived {
        block_hash: String,
        block_number: u64,
    },
    TransactionReceived {
        tx_hash: String,
    },
    PeerConnected {
        peer_id: String,
    },
    PeerDisconnected {
        peer_id: String,
    },
    ConsensusReached {
        block_hash: String,
    },
}

/// Event bus for publishing and subscribing to events
#[derive(Clone)]
pub struct EventBus {
    sender: Arc<broadcast::Sender<Event>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self {
            sender: Arc::new(sender),
        }
    }

    pub async fn publish(&self, event: Event) {
        let _ = self.sender.send(event);
    }

    pub async fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Service interface that all node components must implement
#[async_trait]
pub trait CatalystService: Send + Sync {
    async fn start(&mut self) -> Result<(), NodeError>;
    async fn stop(&mut self) -> Result<(), NodeError>;
    async fn health_check(&self) -> ServiceHealth;
    fn service_type(&self) -> ServiceType;
    fn name(&self) -> &str;
}

/// Individual service status information
#[derive(Debug, Clone)]
pub struct ServiceStatus {
    pub service_type: ServiceType,
    pub name: String,
    pub health: ServiceHealth,
    pub uptime: std::time::Duration,
}

/// Complete node state information
#[derive(Debug, Clone)]
pub struct NodeState {
    pub node_id: String,
    pub status: NodeStatus,
    pub services: Vec<ServiceStatus>,
    pub network_peers: u64,
    pub block_height: u64,
    pub is_validator: bool,
}

/// Service manager - handles lifecycle of all services
pub struct ServiceManager {
    services: Vec<Box<dyn CatalystService>>,
}

impl ServiceManager {
    pub fn new() -> Self {
        Self {
            services: Vec::new(),
        }
    }

    pub async fn register_service(&mut self, service: Box<dyn CatalystService>) -> Result<(), NodeError> {
        info!("Registering service: {}", service.name());
        self.services.push(service);
        Ok(())
    }

    pub async fn start_all(&mut self) -> Result<(), NodeError> {
        info!("Starting all services...");
        for service in &mut self.services {
            info!("Starting service: {}", service.name());
            service.start().await?;
        }
        info!("✅ All services started");
        Ok(())
    }

    pub async fn stop_all(&mut self) -> Result<(), NodeError> {
        info!("Stopping all services...");
        // Stop in reverse order
        for service in self.services.iter_mut().rev() {
            info!("Stopping service: {}", service.name());
            if let Err(e) = service.stop().await {
                warn!("Error stopping service {}: {}", service.name(), e);
            }
        }
        info!("✅ All services stopped");
        Ok(())
    }

    pub async fn all_services_healthy(&self) -> bool {
        for service in &self.services {
            if service.health_check().await != ServiceHealth::Healthy {
                return false;
            }
        }
        true
    }

    pub async fn get_all_service_status(&self) -> Vec<ServiceStatus> {
        let mut statuses = Vec::new();
        for service in &self.services {
            statuses.push(ServiceStatus {
                service_type: service.service_type(),
                name: service.name().to_string(),
                health: service.health_check().await,
                uptime: std::time::Duration::from_secs(0), // TODO: Track actual uptime
            });
        }
        statuses
    }
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Main node manager - orchestrates all node components
pub struct NodeManager {
    config: CatalystConfig,
    service_manager: ServiceManager,
    event_bus: EventBus,
    state: Arc<RwLock<NodeState>>,
    shutdown_tx: Option<broadcast::Sender<()>>,
}

impl NodeManager {
    pub fn new(config: CatalystConfig) -> Self {
        let node_id = "catalyst-node".to_string();
        let initial_state = NodeState {
            node_id: node_id.clone(),
            status: NodeStatus {
                id: node_id,
                uptime: 0,
                sync_status: catalyst_core::SyncStatus::NotSynced,
                metrics: catalyst_core::ResourceMetrics {
                    cpu_usage: 0.0,
                    memory_usage: 0,
                    disk_usage: 0,
                },
            },
            services: Vec::new(),
            network_peers: 0,
            block_height: 0,
            is_validator: false,
        };

        Self {
            config,
            service_manager: ServiceManager::new(),
            event_bus: EventBus::new(),
            state: Arc::new(RwLock::new(initial_state)),
            shutdown_tx: None,
        }
    }

    pub async fn start(&mut self) -> Result<(), NodeError> {
        info!("🚀 Starting Catalyst node...");
        let (shutdown_tx, _) = broadcast::channel(1);
        self.shutdown_tx = Some(shutdown_tx);
        self.service_manager.start_all().await?;
        info!("✅ Catalyst node started successfully");
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<(), NodeError> {
        info!("🛑 Stopping Catalyst node...");
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(());
        }
        self.service_manager.stop_all().await?;
        info!("✅ Catalyst node stopped successfully");
        Ok(())
    }

    pub async fn get_state(&self) -> NodeState {
        self.state.read().await.clone()
    }

    pub async fn is_running(&self) -> bool {
        self.service_manager.all_services_healthy().await
    }

    pub async fn add_service(&mut self, service: Box<dyn CatalystService>) -> Result<(), NodeError> {
        self.service_manager.register_service(service).await
    }

    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }
}

/// Main Catalyst node implementation
pub struct CatalystNode {
    manager: NodeManager,
}

impl CatalystNode {
    pub fn new(config: CatalystConfig) -> Self {
        Self {
            manager: NodeManager::new(config),
        }
    }

    pub async fn start(&mut self) -> Result<(), NodeError> {
        self.manager.start().await
    }

    pub async fn stop(&mut self) -> Result<(), NodeError> {
        self.manager.stop().await
    }

    pub async fn is_running(&self) -> bool {
        self.manager.is_running().await
    }

    pub async fn get_state(&self) -> NodeState {
        self.manager.get_state().await
    }

    pub async fn wait_for_shutdown(&mut self) {
        // Simple implementation - just wait for shutdown signal
        if let Some(tx) = &self.manager.shutdown_tx {
            let mut rx = tx.subscribe();
            let _ = rx.recv().await;
        }
    }

    pub async fn add_service(&mut self, service: Box<dyn CatalystService>) -> Result<(), NodeError> {
        self.manager.add_service(service).await
    }
}

/// Builder pattern for constructing Catalyst nodes
pub struct NodeBuilder {
    config: Option<CatalystConfig>,
    services: Vec<Box<dyn CatalystService>>,
}

impl NodeBuilder {
    pub fn new() -> Self {
        Self {
            config: None,
            services: Vec::new(),
        }
    }

    pub fn with_config(mut self, config: CatalystConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_service(mut self, service: Box<dyn CatalystService>) -> Self {
        self.services.push(service);
        self
    }

    pub fn with_rpc(mut self, rpc_config: catalyst_rpc::RpcConfig) -> Self {
        // Create RPC service adapter
        let rpc_service = RpcServiceAdapter::new(catalyst_rpc::RpcServer::new(rpc_config));
        self.services.push(Box::new(rpc_service));
        self
    }

    pub async fn build(mut self) -> Result<CatalystNode, NodeError> {
        let config = self.config.ok_or_else(|| {
            NodeError::Config("No configuration provided".to_string())
        })?;

        let mut node = CatalystNode::new(config);
        
        // Add all configured services
        for service in self.services {
            node.add_service(service).await?;
        }

        Ok(node)
    }
}

impl Default for NodeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Adapter to make RpcServer compatible with CatalystService
pub struct RpcServiceAdapter {
    rpc_server: catalyst_rpc::RpcServer,
    started: bool,
}

impl RpcServiceAdapter {
    pub fn new(rpc_server: catalyst_rpc::RpcServer) -> Self {
        Self {
            rpc_server,
            started: false,
        }
    }
}

#[async_trait]
impl CatalystService for RpcServiceAdapter {
    async fn start(&mut self) -> Result<(), NodeError> {
        self.rpc_server.start().await
            .map_err(|e| NodeError::Service(format!("RPC service start failed: {}", e)))?;
        self.started = true;
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), NodeError> {
        self.rpc_server.stop().await
            .map_err(|e| NodeError::Service(format!("RPC service stop failed: {}", e)))?;
        self.started = false;
        Ok(())
    }

    async fn health_check(&self) -> ServiceHealth {
        if self.started && self.rpc_server.is_running().await {
            ServiceHealth::Healthy
        } else {
            ServiceHealth::Unhealthy
        }
    }

    fn service_type(&self) -> ServiceType {
        ServiceType::Rpc
    }

    fn name(&self) -> &str {
        "RPC Server"
    }
}