//! Service Management System
//! Create as crates/catalyst-node/src/services/mod.rs

use crate::{CatalystService, NodeError, ServiceStatus};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

pub mod traits;
pub mod registry;

/// Types of services in the Catalyst node
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

/// Health status of a service
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceHealth {
    Healthy,
    Unhealthy,
    Starting,
    Stopping,
    Unknown,
}

/// Internal service wrapper
struct ServiceWrapper {
    service: Box<dyn CatalystService>,
    start_time: Option<Instant>,
    health: ServiceHealth,
}

/// Manages the lifecycle of all node services
pub struct ServiceManager {
    services: Arc<RwLock<HashMap<ServiceType, ServiceWrapper>>>,
    startup_order: Vec<ServiceType>,
}

impl ServiceManager {
    /// Create a new service manager
    pub fn new() -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            startup_order: vec![
                ServiceType::Storage,
                ServiceType::Network,
                ServiceType::Consensus,
                ServiceType::Runtime,
                ServiceType::Rpc,
            ],
        }
    }

    /// Register a new service
    pub async fn register_service(&mut self, service: Box<dyn CatalystService>) -> Result<(), NodeError> {
        let service_type = service.service_type();
        let service_name = service.name().to_string();
        
        info!("Registering service: {} ({})", service_name, service_type);
        
        let wrapper = ServiceWrapper {
            service,
            start_time: None,
            health: ServiceHealth::Unknown,
        };
        
        self.services.write().await.insert(service_type, wrapper);
        
        info!("✅ Service registered: {} ({})", service_name, service_type);
        Ok(())
    }

    /// Start a specific service type
    pub async fn start_service_type(&mut self, service_type: ServiceType) -> Result<(), NodeError> {
        let mut services = self.services.write().await;
        
        if let Some(wrapper) = services.get_mut(&service_type) {
            info!("Starting service: {} ({})", wrapper.service.name(), service_type);
            
            wrapper.health = ServiceHealth::Starting;
            
            match wrapper.service.start().await {
                Ok(()) => {
                    wrapper.start_time = Some(Instant::now());
                    wrapper.health = ServiceHealth::Healthy;
                    info!("✅ Service started: {} ({})", wrapper.service.name(), service_type);
                    Ok(())
                }
                Err(e) => {
                    wrapper.health = ServiceHealth::Unhealthy;
                    error!("❌ Failed to start service {} ({}): {}", wrapper.service.name(), service_type, e);
                    Err(e)
                }
            }
        } else {
            Err(NodeError::Service(format!("Service type {:?} not registered", service_type)))
        }
    }

    /// Stop a specific service type
    pub async fn stop_service_type(&mut self, service_type: ServiceType) -> Result<(), NodeError> {
        let mut services = self.services.write().await;
        
        if let Some(wrapper) = services.get_mut(&service_type) {
            info!("Stopping service: {} ({})", wrapper.service.name(), service_type);
            
            wrapper.health = ServiceHealth::Stopping;
            
            match wrapper.service.stop().await {
                Ok(()) => {
                    wrapper.start_time = None;
                    wrapper.health = ServiceHealth::Unhealthy;
                    info!("✅ Service stopped: {} ({})", wrapper.service.name(), service_type);
                    Ok(())
                }
                Err(e) => {
                    error!("❌ Failed to stop service {} ({}): {}", wrapper.service.name(), service_type, e);
                    Err(e)
                }
            }
        } else {
            warn!("Service type {:?} not found for stopping", service_type);
            Ok(())
        }
    }

    /// Start all services in dependency order
    pub async fn start_all(&mut self) -> Result<(), NodeError> {
        info!("Starting all services...");
        
        for &service_type in &self.startup_order.clone() {
            if self.services.read().await.contains_key(&service_type) {
                self.start_service_type(service_type).await?;
                
                // Brief pause between service starts
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }
        }
        
        info!("✅ All services started");
        Ok(())
    }

    /// Stop all services in reverse dependency order
    pub async fn stop_all(&mut self) -> Result<(), NodeError> {
        info!("Stopping all services...");
        
        let mut stop_order = self.startup_order.clone();
        stop_order.reverse();
        
        for &service_type in &stop_order {
            if self.services.read().await.contains_key(&service_type) {
                if let Err(e) = self.stop_service_type(service_type).await {
                    warn!("Error stopping service {:?}: {}", service_type, e);
                }
                
                // Brief pause between service stops
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
        
        info!("✅ All services stopped");
        Ok(())
    }

    /// Check if all services are healthy
    pub async fn all_services_healthy(&self) -> bool {
        let services = self.services.read().await;
        
        if services.is_empty() {
            return false;
        }
        
        for wrapper in services.values() {
            if wrapper.health != ServiceHealth::Healthy {
                return false;
            }
        }
        
        true
    }

    /// Get health status of a specific service
    pub async fn get_service_health(&self, service_type: ServiceType) -> Option<ServiceHealth> {
        let services = self.services.read().await;
        services.get(&service_type).map(|wrapper| wrapper.health.clone())
    }

    /// Get status of all services
    pub async fn get_all_service_status(&self) -> Vec<ServiceStatus> {
        let services = self.services.read().await;
        let mut statuses = Vec::new();
        
        for (service_type, wrapper) in services.iter() {
            let uptime = wrapper.start_time
                .map(|start| start.elapsed())
                .unwrap_or_default();
            
            statuses.push(ServiceStatus {
                service_type: *service_type,
                name: wrapper.service.name().to_string(),
                health: wrapper.health.clone(),
                uptime,
            });
        }
        
        statuses
    }

    /// Perform health checks on all services
    pub async fn health_check_all(&self) -> HashMap<ServiceType, ServiceHealth> {
        let mut services = self.services.write().await;
        let mut results = HashMap::new();
        
        for (service_type, wrapper) in services.iter_mut() {
            let health = wrapper.service.health_check().await;
            wrapper.health = health.clone();
            results.insert(*service_type, health);
        }
        
        results
    }

    /// Get list of registered service types
    pub async fn get_registered_services(&self) -> Vec<ServiceType> {
        let services = self.services.read().await;
        services.keys().copied().collect()
    }

    /// Check if a service is registered
    pub async fn is_service_registered(&self, service_type: ServiceType) -> bool {
        let services = self.services.read().await;
        services.contains_key(&service_type)
    }
}

impl Default for ServiceManager {
    fn default() -> Self {
        Self::new()
    }
}