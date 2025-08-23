//! Minimal Catalyst Service Bus implementation
//! This is a placeholder implementation to get the build working

use catalyst_utils::CatalystResult;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServiceBusError {
    #[error("Service bus error: {0}")]
    Generic(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Invalid message: {0}")]
    InvalidMessage(String),
}

impl From<ServiceBusError> for catalyst_utils::CatalystError {
    fn from(err: ServiceBusError) -> Self {
        catalyst_utils::CatalystError::Generic(err.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceBusConfig {
    pub enabled: bool,
    pub port: u16,
    pub max_connections: u32,
}

impl Default for ServiceBusConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 8080,
            max_connections: 100,
        }
    }
}

/// Minimal service bus
pub struct ServiceBus {
    config: ServiceBusConfig,
}

impl ServiceBus {
    pub fn new(config: ServiceBusConfig) -> Self {
        Self { config }
    }

    pub async fn start(&self) -> Result<(), ServiceBusError> {
        // Placeholder - no actual service
        Ok(())
    }

    pub async fn stop(&self) -> Result<(), ServiceBusError> {
        Ok(())
    }

    pub async fn send_event(&self, _event: &str) -> Result<(), ServiceBusError> {
        Ok(())
    }
}

// Placeholder event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub event_type: String,
    pub data: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    pub event_types: Vec<String>,
    pub addresses: Vec<String>,
}

impl EventFilter {
    pub fn new() -> Self {
        Self {
            event_types: Vec::new(),
            addresses: Vec::new(),
        }
    }
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::new()
    }
}

// Auth placeholder
#[derive(Debug, Clone)]
pub struct AuthToken {
    pub token: String,
}

impl AuthToken {
    pub fn new(token: String) -> Self {
        Self { token }
    }
}

// Export everything
pub use ServiceBus as CatalystServiceBus;
pub use ServiceBusError as CatalystServiceBusError;
