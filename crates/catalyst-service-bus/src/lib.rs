//! # Catalyst Service Bus
//!
//! Web2 integration layer that bridges blockchain events to traditional applications
//! through WebSocket connections and REST APIs. Designed to make blockchain accessible
//! to developers without blockchain expertise.

use catalyst_utils::{CatalystResult, logging::LogCategory};

pub mod config;
pub mod server;
pub mod websocket;
#[path = "reset.rs"]
pub mod rest;
pub mod events;
pub mod filters;
pub mod auth;
pub mod client;
pub mod error;

pub use config::ServiceBusConfig;
pub use server::ServiceBusServer;
pub use events::{BlockchainEvent, EventFilter, EventType};
pub use filters::FilterEngine;
pub use config::AuthConfig;
pub use auth::AuthToken;
pub use client::ServiceBusClient;
pub use error::ServiceBusError;

// Re-export commonly used types
pub use serde::{Deserialize, Serialize};
pub use uuid::Uuid;

/// Initialize the service bus with the provided configuration
pub async fn init_service_bus(config: ServiceBusConfig) -> CatalystResult<ServiceBusServer> {
    catalyst_utils::logging::log_info!(
        LogCategory::ServiceBus,
        "Initializing Catalyst Service Bus on {}:{}",
        config.host,
        config.port
    );

    let server = ServiceBusServer::new(config).await?;
    
    catalyst_utils::logging::log_info!(
        LogCategory::ServiceBus,
        "Service Bus initialized successfully"
    );

    Ok(server)
}

/// Service Bus API version
pub const API_VERSION: &str = "v1";

/// Default WebSocket path
pub const WS_PATH: &str = "/ws";

/// Default REST API path prefix
pub const API_PATH: &str = "/api/v1";

#[cfg(test)]
mod tests {
    use super::*;
    use catalyst_utils::utils;

    #[tokio::test]
    async fn test_service_bus_initialization() {
        let config = ServiceBusConfig::default();
        let result = init_service_bus(config).await;
        
        // Should initialize without errors
        assert!(result.is_ok());
    }
}