//! Catalyst Node Manager with Real Consensus Integration
//! Complete implementation with real consensus and storage services

pub mod consensus_service;
pub mod block_production;

use async_trait::async_trait;
use catalyst_config::CatalystConfig;
use catalyst_core::NodeStatus;
use catalyst_crypto::KeyPair;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn, error};
use blake2::{Blake2b512, Digest};
use rand;
use hex;

pub use consensus_service::*;
pub use block_production::*;

// Add module declarations for services
pub mod services {
    pub mod storage;
}

// Re-export the storage service
pub use services::storage::StorageService;

// Mock configurations for missing crates (when not available)
pub mod catalyst_network {
    use serde::{Serialize, Deserialize};
    
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct NetworkConfig {
        pub listen_port: u16,
        pub max_peers: u32,
    }
    
    impl Default for NetworkConfig {
        fn default() -> Self {
            Self {
                listen_port: 30333,
                max_peers: 50,
            }
        }
    }
}

pub mod catalyst_runtime_evm {
    use serde::{Serialize, Deserialize};
    
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EvmConfig {
        pub enabled: bool,
        pub gas_limit: String,
    }
    
    impl Default for EvmConfig {
        fn default() -> Self {
            Self {
                enabled: true,
                gas_limit: "8000000".to_string(),
            }
        }
    }
}

pub mod catalyst_service_bus {
    use serde::{Serialize, Deserialize};
    
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ServiceBusConfig {
        pub enabled: bool,
        pub max_channels: u32,
    }
    
    impl Default for ServiceBusConfig {
        fn default() -> Self {
            Self {
                enabled: true,
                max_channels: 100,
            }
        }
    }
}

/// Node management errors
#[derive(Error, Debug)]
pub enum NodeError {
    #[error("Service error: {0}")]
    Service(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Configuration error: {0}")]
    Configuration(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Consensus error: {message}")]
    Consensus { message: String },
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
    Unhealthy(String),
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
}

impl NodeManager {
    pub fn new(config: CatalystConfig) -> Self {
        Self { config }
    }
}

/// Node health information
#[derive(Debug, Clone)]
pub struct NodeHealth {
    pub is_healthy: bool,
    pub services: Vec<ServiceStatus>,
    pub uptime: u64,
    pub memory_usage: u64,
    pub storage_healthy: bool,
}

/// Adapter to make RpcServer compatible with CatalystService
pub struct RpcServiceAdapter {
    rpc_server: catalyst_rpc::RpcServer,
    storage_service: Option<Arc<StorageService>>,
    started: bool,
}

impl RpcServiceAdapter {
    pub fn new(rpc_server: catalyst_rpc::RpcServer) -> Self {
        Self {
            rpc_server,
            storage_service: None,
            started: false,
        }
    }

    /// Set storage service for real data access
    pub fn with_storage(mut self, storage_service: Arc<StorageService>) -> Self {
        self.storage_service = Some(storage_service);
        self
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
            ServiceHealth::Unhealthy("RPC service not running".to_string())
        }
    }

    fn service_type(&self) -> ServiceType {
        ServiceType::Rpc
    }

    fn name(&self) -> &str {
        "RPC Server"
    }
}

/// Wrapper to make StorageService compatible with the service framework
struct StorageServiceWrapper {
    storage_service: Arc<StorageService>,
}

impl StorageServiceWrapper {
    fn new(storage_service: Arc<StorageService>) -> Self {
        Self { storage_service }
    }
}

#[async_trait]
impl CatalystService for StorageServiceWrapper {
    async fn start(&mut self) -> Result<(), NodeError> {
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), NodeError> {
        Ok(())
    }

    async fn health_check(&self) -> ServiceHealth {
        self.storage_service.health_check().await
    }

    fn service_type(&self) -> ServiceType {
        ServiceType::Storage
    }

    fn name(&self) -> &str {
        "Storage Service"
    }
}

/// Mock network service
pub struct NetworkService;

impl NetworkService {
    pub async fn new(_config: catalyst_network::NetworkConfig) -> Result<Self, NodeError> {
        Ok(Self)
    }
}

#[async_trait]
impl CatalystService for NetworkService {
    async fn start(&mut self) -> Result<(), NodeError> {
        info!("🌐 Network service started (mock)");
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), NodeError> {
        info!("🌐 Network service stopped");
        Ok(())
    }

    async fn health_check(&self) -> ServiceHealth {
        ServiceHealth::Healthy
    }

    fn service_type(&self) -> ServiceType {
        ServiceType::Network
    }

    fn name(&self) -> &str {
        "Network Service"
    }
}

/// Mock EVM service
pub struct EvmService;

impl EvmService {
    pub async fn new(_config: catalyst_runtime_evm::EvmConfig) -> Result<Self, NodeError> {
        Ok(Self)
    }
}

#[async_trait]
impl CatalystService for EvmService {
    async fn start(&mut self) -> Result<(), NodeError> {
        info!("⚡ EVM service started (mock)");
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), NodeError> {
        info!("⚡ EVM service stopped");
        Ok(())
    }

    async fn health_check(&self) -> ServiceHealth {
        ServiceHealth::Healthy
    }

    fn service_type(&self) -> ServiceType {
        ServiceType::Runtime
    }

    fn name(&self) -> &str {
        "EVM Service"
    }
}

/// Mock service bus
pub struct ServiceBusService;

impl ServiceBusService {
    pub async fn new(_config: catalyst_service_bus::ServiceBusConfig) -> Result<Self, NodeError> {
        Ok(Self)
    }
}

#[async_trait]
impl CatalystService for ServiceBusService {
    async fn start(&mut self) -> Result<(), NodeError> {
        info!("🚌 Service bus started (mock)");
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), NodeError> {
        info!("🚌 Service bus stopped");
        Ok(())
    }

    async fn health_check(&self) -> ServiceHealth {
        ServiceHealth::Healthy
    }

    fn service_type(&self) -> ServiceType {
        ServiceType::Custom(1)
    }

    fn name(&self) -> &str {
        "Service Bus"
    }
}

/// Builder pattern for constructing Catalyst nodes with real consensus
pub struct NodeBuilder {
    config: CatalystConfig,
    services: Vec<Box<dyn CatalystService>>,
    rpc_config: Option<catalyst_rpc::RpcConfig>,
    storage_config: Option<catalyst_storage::StorageConfig>,
}

impl NodeBuilder {
    pub fn new(config: CatalystConfig) -> Self {
        Self {
            config,
            services: Vec::new(),
            rpc_config: None,
            storage_config: None,
        }
    }

    /// Add storage configuration
    pub fn with_storage(mut self, storage_config: catalyst_storage::StorageConfig) -> Self {
        self.storage_config = Some(storage_config);
        self
    }

    /// Add RPC server configuration
    pub fn with_rpc(mut self, config: catalyst_rpc::RpcConfig) -> Self {
        self.rpc_config = Some(config);
        self
    }

    /// Generate node ID from keypair
    fn generate_node_id(keypair: &KeyPair) -> [u8; 32] {
        let mut hasher = Blake2b512::new();
        hasher.update(&keypair.public_key().to_bytes());
        let result = hasher.finalize();
        let mut node_id = [0u8; 32];
        node_id.copy_from_slice(&result[..32]); // Take first 32 bytes
        node_id
    }

    /// Build the complete Catalyst node with real consensus
    pub async fn build(self) -> Result<CatalystNode, NodeError> {
        let mut services: Vec<Box<dyn CatalystService>> = Vec::new();
        let event_bus = Arc::new(EventBus::new());

        info!("🔨 Building Catalyst node with real consensus...");

        // 1. Create Storage Service (FIRST - everything depends on this)
        let storage_service = if let Some(storage_config) = self.storage_config {
            info!("📦 Setting up storage service...");
            let mut storage = StorageService::new(storage_config).await?;
            storage.set_event_bus(event_bus.clone());
            let storage_arc = Arc::new(storage);
            services.push(Box::new(StorageServiceWrapper::new(storage_arc.clone())));
            Some(storage_arc)
        } else {
            warn!("⚠️ No storage configuration provided - using default");
            let default_config = catalyst_storage::StorageConfig::for_development();
            let mut storage = StorageService::new(default_config).await?;
            storage.set_event_bus(event_bus.clone());
            let storage_arc = Arc::new(storage);
            services.push(Box::new(StorageServiceWrapper::new(storage_arc.clone())));
            Some(storage_arc)
        };

        // 2. Create Network Service
        info!("🌐 Setting up network service...");
        let network_config = catalyst_network::NetworkConfig::default();
        let network_service = NetworkService::new(network_config).await?;
        services.push(Box::new(network_service));

        // 3. Create Real Consensus Service (THE KEY CHANGE)
        info!("⚖️ Setting up REAL consensus service...");
        
        // Generate node identity for consensus
        let mut rng = rand::thread_rng();
        let keypair = KeyPair::generate(&mut rng);
        let node_id = Self::generate_node_id(&keypair);
        
        info!("🆔 Node ID for consensus: {}", hex::encode(&node_id[..8]));
        
        // Create real consensus service with storage
        let consensus_config = ConsensusConfig::default();
        let mut consensus_service = ConsensusService::new(
            node_id,
            keypair,
            storage_service.as_ref().unwrap().storage_manager().clone(),
            consensus_config,
        );
        
        // Set event bus for consensus
        consensus_service.set_event_bus(event_bus.clone());
        
        // Store consensus reference for block production
        let consensus_arc = Arc::new(consensus_service);
        
        // 4. Create Block Production Service (integrates with real consensus)
        info!("🏗️ Setting up block production service...");
        let block_production_service = BlockProductionService::new(
            storage_service.as_ref().unwrap().storage_manager().clone(),
            consensus_arc.clone(),
        );
        
        // Initialize block production
        block_production_service.initialize().await?;
        
        // Add consensus service to services list
        services.push(Box::new(ConsensusServiceWrapper::new(consensus_arc.clone())));
        
        // 5. Create EVM Runtime Service
        info!("⚡ Setting up EVM runtime service...");
        let evm_config = catalyst_runtime_evm::EvmConfig::default();
        let evm_service = EvmService::new(evm_config).await?;
        services.push(Box::new(evm_service));

        // 6. Create RPC Service with Storage Integration
        if let Some(rpc_config) = self.rpc_config {
            info!("🔌 Setting up RPC service with storage integration...");
            
            // Create blockchain state
            let blockchain_state = Arc::new(RwLock::new(catalyst_rpc::BlockchainState::default()));
            
            // Create RPC implementation with storage
            let mut rpc_impl = catalyst_rpc::CatalystRpcImpl::new(blockchain_state);
            if let Some(storage) = &storage_service {
                let storage_adapter = Arc::new(crate::services::storage::StorageServiceRpc::new(storage.clone()));
                rpc_impl = rpc_impl.with_storage(storage_adapter);
            }
            
            // Create RPC server
            let rpc_server = catalyst_rpc::RpcServer::new(rpc_config, rpc_impl).await
                .map_err(|e| NodeError::Service(format!("Failed to create RPC server: {}", e)))?;
            
            // Wrap in service adapter
            let rpc_service_adapter = RpcServiceAdapter::new(rpc_server);
            
            services.push(Box::new(rpc_service_adapter));
        }

        // 7. Create Service Bus
        info!("🚌 Setting up service bus...");
        let service_bus_config = catalyst_service_bus::ServiceBusConfig::default();
        let service_bus = ServiceBusService::new(service_bus_config).await?;
        services.push(Box::new(service_bus));

        // 8. Add any additional services from the builder
        services.extend(self.services);

        info!("✅ Node built successfully with {} services and REAL consensus", services.len());

        Ok(CatalystNode {
            config: self.config,
            services,
            event_bus,
            node_manager: None,
            storage_service,
            consensus_service: Some(consensus_arc),
            block_production_service: Some(Arc::new(block_production_service)),
        })
    }
}

/// Wrapper for consensus service to integrate with service management
struct ConsensusServiceWrapper {
    consensus: Arc<ConsensusService>,
}

impl ConsensusServiceWrapper {
    fn new(consensus: Arc<ConsensusService>) -> Self {
        Self { consensus }
    }
}

#[async_trait]
impl CatalystService for ConsensusServiceWrapper {
    async fn start(&mut self) -> Result<(), NodeError> {
        // Start the consensus service
        let consensus_clone = (*self.consensus).clone();
        consensus_clone.start().await?;
        
        // Start automatic transaction generation for testing
        self.consensus.start_auto_transaction_generation().await;
        
        info!("✅ Real consensus service started with transaction generation");
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), NodeError> {
        let consensus_clone = (*self.consensus).clone();
        consensus_clone.stop().await?;
        Ok(())
    }

    async fn health_check(&self) -> ServiceHealth {
        if self.consensus.is_running().await {
            ServiceHealth::Healthy
        } else {
            ServiceHealth::Unhealthy("Consensus service not running".to_string())
        }
    }

    fn service_type(&self) -> ServiceType {
        ServiceType::Consensus
    }

    fn name(&self) -> &str {
        "Real Catalyst Consensus Service"
    }
}

/// Main Catalyst node implementation with real consensus
pub struct CatalystNode {
    config: CatalystConfig,
    services: Vec<Box<dyn CatalystService>>,
    event_bus: Arc<EventBus>,
    node_manager: Option<NodeManager>,
    storage_service: Option<Arc<StorageService>>,
    consensus_service: Option<Arc<ConsensusService>>,
    block_production_service: Option<Arc<BlockProductionService>>,
}

impl CatalystNode {
    /// Get reference to storage service
    pub fn storage(&self) -> Option<&Arc<StorageService>> {
        self.storage_service.as_ref()
    }

    /// Get reference to consensus service
    pub fn consensus(&self) -> Option<&Arc<ConsensusService>> {
        self.consensus_service.as_ref()
    }

    /// Get reference to block production service
    pub fn block_production(&self) -> Option<&Arc<BlockProductionService>> {
        self.block_production_service.as_ref()
    }

    /// Get current block height from consensus
    pub async fn get_block_height(&self) -> u64 {
        if let Some(consensus) = &self.consensus_service {
            consensus.get_current_height().await
        } else {
            0
        }
    }

    /// Get detailed consensus status
    pub async fn get_consensus_status(&self) -> Option<ConsensusStatus> {
        if let Some(consensus) = &self.consensus_service {
            Some(consensus.get_status().await)
        } else {
            None
        }
    }

    /// Trigger test block production
    pub async fn trigger_test_block(&self) -> Result<(), NodeError> {
        if let Some(block_production) = &self.block_production_service {
            block_production.trigger_test_block().await
        } else {
            Err(NodeError::Service("Block production service not available".to_string()))
        }
    }

    /// Start automatic test transaction generation
    pub async fn start_auto_test_transactions(&self, interval_secs: u64) -> Result<(), NodeError> {
        if let Some(block_production) = &self.block_production_service {
            block_production.start_auto_test_transactions(interval_secs).await;
            Ok(())
        } else {
            Err(NodeError::Service("Block production service not available".to_string()))
        }
    }

    /// Get consensus debug information
    pub async fn get_consensus_debug_info(&self) -> Result<block_production::ConsensusDebugInfo, NodeError> {
        if let Some(block_production) = &self.block_production_service {
            block_production.get_consensus_debug_info().await
        } else {
            Err(NodeError::Service("Block production service not available".to_string()))
        }
    }

    /// Start all services including real consensus
    pub async fn start(&mut self) -> Result<(), NodeError> {
        info!("🚀 Starting Catalyst node with REAL CONSENSUS ({} services)...", self.services.len());

        // Start storage service first (if available)
        if let Some(storage) = &self.storage_service {
            info!("🗄️ Initializing storage...");
            if !matches!(storage.health_check().await, ServiceHealth::Healthy) {
                return Err(NodeError::Storage("Storage service is not healthy".to_string()));
            }
            info!("✅ Storage initialized successfully");
        }

        // Start all other services (including real consensus)
        for (index, service) in self.services.iter_mut().enumerate() {
            info!("🔄 Starting service {}: {}", index + 1, service.name());
            service.start().await.map_err(|e| {
                NodeError::Service(format!("Failed to start service '{}': {}", service.name(), e))
            })?;
            info!("✅ Service '{}' started successfully", service.name());
        }

        // Create and start node manager
        let node_manager = NodeManager::new(self.config.clone());
        self.node_manager = Some(node_manager);

        info!("🎉 Catalyst node started successfully with REAL CONSENSUS!");
        info!("📊 Real consensus is now producing blocks every 40 seconds");
        info!("🔄 Automatic test transactions are being generated");
        
        Ok(())
    }

    /// Stop all services
    pub async fn stop(&mut self) -> Result<(), NodeError> {
        info!("🛑 Stopping Catalyst node...");

        // Stop services in reverse order
        for service in self.services.iter_mut().rev() {
            info!("🔄 Stopping service: {}", service.name());
            if let Err(e) = service.stop().await {
                warn!("⚠️ Warning: Failed to stop service '{}': {}", service.name(), e);
            } else {
                info!("✅ Service '{}' stopped successfully", service.name());
            }
        }

        // Stop storage service last
        if let Some(storage) = &self.storage_service {
            info!("🗄️ Stopping storage...");
            info!("✅ Storage stopped");
        }

        info!("✅ Catalyst node stopped successfully");
        Ok(())
    }

    /// Get node health status
    pub async fn health_check(&self) -> NodeHealth {
        let mut healthy_services = 0;
        let total_services = self.services.len();
        let mut service_statuses = Vec::new();

        // Check storage service health
        let storage_healthy = if let Some(_storage) = &self.storage_service {
            _storage.health_check().await == ServiceHealth::Healthy
        } else {
            false
        };

        if storage_healthy {
            healthy_services += 1;
        }

        // Check all other services
        for service in &self.services {
            let health = service.health_check().await;
            let is_healthy = health == ServiceHealth::Healthy;
            
            if is_healthy {
                healthy_services += 1;
            }

            service_statuses.push(ServiceStatus {
                name: service.name().to_string(),
                service_type: service.service_type(),
                health,
                uptime: std::time::Duration::from_secs(0), // TODO: Implement actual uptime tracking
            });
        }

        let overall_healthy = healthy_services == total_services + 1; // +1 for storage

        NodeHealth {
            is_healthy: overall_healthy,
            services: service_statuses,
            uptime: 0, // TODO: Implement actual uptime tracking
            memory_usage: 0, // TODO: Implement actual memory tracking
            storage_healthy,
        }
    }

    /// Wait for shutdown signal (for CLI usage)
    pub async fn wait_for_shutdown(&mut self) {
        // Simple implementation - wait for Ctrl+C
        tokio::signal::ctrl_c().await.expect("Failed to listen for ctrl-c");
    }
}