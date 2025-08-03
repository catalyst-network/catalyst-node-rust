//! Node Builder and Main Node Implementation
//! Create as crates/catalyst-node/src/node/mod.rs

use crate::{NodeManager, NodeError, CatalystService};
use catalyst_config::CatalystConfig;
use catalyst_rpc::{RpcServer, RpcConfig};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

pub mod builder;
pub mod runtime;

pub use builder::NodeBuilder;

/// Main Catalyst Node implementation
pub struct CatalystNode {
    manager: NodeManager,
    runtime: Option<tokio::task::JoinHandle<()>>,
}

impl CatalystNode {
    /// Create a new node with the given configuration
    pub fn new(config: CatalystConfig) -> Self {
        Self {
            manager: NodeManager::new(config),
            runtime: None,
        }
    }

    /// Start the node
    pub async fn start(&mut self) -> Result<(), NodeError> {
        info!("🚀 Starting Catalyst node...");
        
        // Start the node manager
        self.manager.start().await?;
        
        // Start the main runtime loop
        let manager = Arc::new(RwLock::new(&mut self.manager));
        let runtime_handle = tokio::spawn(async move {
            // Main node runtime loop
            Self::runtime_loop(manager).await;
        });
        
        self.runtime = Some(runtime_handle);
        
        info!("✅ Catalyst node is running");
        Ok(())
    }

    /// Stop the node
    pub async fn stop(&mut self) -> Result<(), NodeError> {
        info!("🛑 Stopping Catalyst node...");
        
        // Stop the runtime loop
        if let Some(handle) = self.runtime.take() {
            handle.abort();
        }
        
        // Stop the node manager
        self.manager.stop().await?;
        
        info!("✅ Catalyst node stopped");
        Ok(())
    }

    /// Check if the node is running
    pub async fn is_running(&self) -> bool {
        self.manager.is_running().await
    }

    /// Get node state
    pub async fn get_state(&self) -> crate::NodeState {
        self.manager.get_state().await
    }

    /// Wait for the node to be stopped
    pub async fn wait_for_shutdown(&mut self) {
        if let Some(handle) = &mut self.runtime {
            let _ = handle.await;
        }
    }

    /// Main runtime loop for the node
    async fn runtime_loop(_manager: Arc<RwLock<&mut NodeManager>>) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        
        loop {
            interval.tick().await;
            
            // Periodic health checks and maintenance
            // TODO: Add actual runtime logic here
            // - Health monitoring
            // - Metrics collection
            // - Periodic consensus tasks
            // - Network maintenance
        }
    }
}

/// Builder pattern for constructing Catalyst nodes
pub struct NodeBuilder {
    config: Option<CatalystConfig>,
    services: Vec<Box<dyn CatalystService>>,
}

impl NodeBuilder {
    /// Create a new node builder
    pub fn new() -> Self {
        Self {
            config: None,
            services: Vec::new(),
        }
    }

    /// Set the node configuration
    pub fn with_config(mut self, config: CatalystConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Add a custom service
    pub fn with_service(mut self, service: Box<dyn CatalystService>) -> Self {
        self.services.push(service);
        self
    }

    /// Enable RPC service
    pub fn with_rpc(mut self, rpc_config: RpcConfig) -> Self {
        let rpc_service = RpcServiceAdapter::new(RpcServer::new(rpc_config));
        self.services.push(Box::new(rpc_service));
        self
    }

    /// Build the node
    pub async fn build(mut self) -> Result<CatalystNode, NodeError> {
        let config = self.config.ok_or_else(|| {
            NodeError::Config("No configuration provided".to_string())
        })?;

        let mut node = CatalystNode::new(config);
        
        // Add all configured services
        for service in self.services {
            node.manager.add_service(service).await?;
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
    rpc_server: RpcServer,
    started: bool,
}

impl RpcServiceAdapter {
    pub fn new(rpc_server: RpcServer) -> Self {
        Self {
            rpc_server,
            started: false,
        }
    }
}

#[async_trait::async_trait]
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

    async fn health_check(&self) -> crate::ServiceHealth {
        if self.started && self.rpc_server.is_running().await {
            crate::ServiceHealth::Healthy
        } else {
            crate::ServiceHealth::Unhealthy
        }
    }

    fn service_type(&self) -> crate::ServiceType {
        crate::ServiceType::Rpc
    }

    fn name(&self) -> &str {
        "RPC Server"
    }
}