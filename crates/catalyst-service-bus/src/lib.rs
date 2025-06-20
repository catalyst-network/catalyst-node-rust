//! Service Bus for Web2 integration with Catalyst Network
//! 
//! Provides WebSocket API and HTTP endpoints for traditional web applications
//! to interact with blockchain events without requiring blockchain expertise.

use async_trait::async_trait;
use catalyst_core::{Transaction, Block, Hash, Event};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::sync::broadcast;
use uuid::Uuid;

pub mod api;
pub mod client;
pub mod events;
pub mod filters;
pub mod server;
pub mod websocket;

pub use api::*;
pub use client::*;
pub use events::*;
pub use filters::*;
pub use server::*;
pub use websocket::*;

#[derive(Error, Debug)]
pub enum ServiceBusError {
    #[error("Server error: {0}")]
    Server(String),
    #[error("WebSocket error: {0}")]
    WebSocket(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid filter: {0}")]
    InvalidFilter(String),
    #[error("Client not found: {0}")]
    ClientNotFound(Uuid),
    #[error("Rate limit exceeded")]
    RateLimit,
}

/// Service bus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceBusConfig {
    /// HTTP server bind address
    pub bind_address: SocketAddr,
    /// Enable CORS for web applications
    pub enable_cors: bool,
    /// Maximum WebSocket connections
    pub max_connections: usize,
    /// Maximum events per second per client
    pub rate_limit: u32,
    /// Event buffer size
    pub event_buffer_size: usize,
    /// Enable metrics collection
    pub enable_metrics: bool,
}

impl Default for ServiceBusConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8080".parse().unwrap(),
            enable_cors: true,
            max_connections: 1000,
            rate_limit: 100,
            event_buffer_size: 10000,
            enable_metrics: true,
        }
    }
}

/// Blockchain event types that can be subscribed to
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EventType {
    /// Token transfer events
    TokenTransfer,
    /// Smart contract events
    ContractEvent,
    /// Block finalized
    BlockFinalized,
    /// Transaction confirmed
    TransactionConfirmed,
    /// Account balance changed
    BalanceChanged,
    /// Custom application events
    Custom(String),
}

/// Event filter for subscribing to specific events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    /// Event types to listen for
    pub event_types: Vec<EventType>,
    /// Specific contract addresses to monitor
    pub contracts: Option<Vec<String>>,
    /// Specific account addresses to monitor
    pub addresses: Option<Vec<String>>,
    /// Custom filter parameters
    pub parameters: Option<HashMap<String, serde_json::Value>>,
}

impl Default for EventFilter {
    fn default() -> Self {
        Self {
            event_types: vec![EventType::TokenTransfer, EventType::TransactionConfirmed],
            contracts: None,
            addresses: None,
            parameters: None,
        }
    }
}

impl EventFilter {
    /// Check if an event matches this filter
    pub fn matches(&self, event: &BlockchainEvent) -> bool {
        // Check event type
        if !self.event_types.contains(&event.event_type) {
            return false;
        }
        
        // Check contract filter
        if let Some(contracts) = &self.contracts {
            if let Some(contract) = &event.contract_address {
                if !contracts.contains(contract) {
                    return false;
                }
            }
        }
        
        // Check address filter
        if let Some(addresses) = &self.addresses {
            if let Some(address) = &event.address {
                if !addresses.contains(address) {
                    return false;
                }
            }
        }
        
        true
    }
}

/// Blockchain event structure sent to clients
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainEvent {
    /// Unique event ID
    pub id: Uuid,
    /// Event type
    pub event_type: EventType,
    /// Block number where event occurred
    pub block_number: u64,
    /// Transaction hash that generated the event
    pub transaction_hash: Hash,
    /// Contract address (if applicable)
    pub contract_address: Option<String>,
    /// Account address (if applicable)
    pub address: Option<String>,
    /// Event data as JSON
    pub data: serde_json::Value,
    /// Event metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// Timestamp when event occurred
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// WebSocket message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WebSocketMessage {
    /// Subscribe to events with filter
    Subscribe {
        id: Uuid,
        filter: EventFilter,
    },
    /// Unsubscribe from events
    Unsubscribe {
        id: Uuid,
    },
    /// Event notification
    Event {
        subscription_id: Uuid,
        event: BlockchainEvent,
    },
    /// Error message
    Error {
        message: String,
        code: u32,
    },
    /// Heartbeat/ping
    Ping,
    /// Heartbeat/pong
    Pong,
}

/// HTTP API response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: chrono::Utc::now(),
        }
    }
    
    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Client connection information
#[derive(Debug, Clone)]
pub struct ClientConnection {
    pub id: Uuid,
    pub address: SocketAddr,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub subscriptions: Vec<Uuid>,
    pub message_count: u64,
}

/// Service bus interface trait
#[async_trait]
pub trait ServiceBus: Send + Sync {
    /// Start the service bus server
    async fn start(&mut self) -> Result<(), ServiceBusError>;
    
    /// Stop the service bus server
    async fn stop(&mut self) -> Result<(), ServiceBusError>;
    
    /// Process a new blockchain event
    async fn process_event(&self, event: BlockchainEvent) -> Result<(), ServiceBusError>;
    
    /// Get connected clients
    async fn get_clients(&self) -> Result<Vec<ClientConnection>, ServiceBusError>;
    
    /// Get service bus statistics
    async fn get_stats(&self) -> Result<ServiceBusStats, ServiceBusError>;
}

/// Service bus statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceBusStats {
    pub connected_clients: usize,
    pub total_subscriptions: usize,
    pub events_processed: u64,
    pub messages_sent: u64,
    pub errors: u64,
    pub uptime_seconds: u64,
}

/// Event processor trait for transforming blockchain events
#[async_trait]
pub trait EventProcessor: Send + Sync {
    /// Process a raw blockchain event into a service bus event
    async fn process(&self, raw_event: Event) -> Result<Option<BlockchainEvent>, ServiceBusError>;
}

/// Rate limiter for client connections
#[derive(Debug)]
pub struct ClientRateLimiter {
    limits: dashmap::DashMap<Uuid, TokenBucket>,
    rate_limit: u32,
}

impl ClientRateLimiter {
    pub fn new(rate_limit: u32) -> Self {
        Self {
            limits: dashmap::DashMap::new(),
            rate_limit,
        }
    }
    
    pub fn check_rate(&self, client_id: Uuid) -> bool {
        let mut bucket = self.limits.entry(client_id).or_insert_with(|| {
            TokenBucket::new(self.rate_limit * 2, self.rate_limit)
        });
        bucket.consume(1)
    }
}

/// Simple token bucket implementation
#[derive(Debug)]
pub struct TokenBucket {
    capacity: u32,
    tokens: u32,
    refill_rate: u32,
    last_refill: std::time::Instant,
}

impl TokenBucket {
    pub fn new(capacity: u32, refill_rate: u32) -> Self {
        Self {
            capacity,
            tokens: capacity,
            refill_rate,
            last_refill: std::time::Instant::now(),
        }
    }
    
    pub fn consume(&mut self, tokens: u32) -> bool {
        self.refill();
        if self.tokens >= tokens {
            self.tokens -= tokens;
            true
        } else {
            false
        }
    }
    
    fn refill(&mut self) {
        let now = std::time::Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs();
        let new_tokens = (elapsed as u32) * self.refill_rate;
        self.tokens = (self.tokens + new_tokens).min(self.capacity);
        self.last_refill = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_filter_matching() {
        let filter = EventFilter {
            event_types: vec![EventType::TokenTransfer],
            contracts: Some(vec!["0x123".to_string()]),
            addresses: None,
            parameters: None,
        };
        
        let event = BlockchainEvent {
            id: Uuid::new_v4(),
            event_type: EventType::TokenTransfer,
            block_number: 100,
            transaction_hash: Hash::default(),
            contract_address: Some("0x123".to_string()),
            address: None,
            data: serde_json::Value::Null,
            metadata: HashMap::new(),
            timestamp: chrono::Utc::now(),
        };
        
        assert!(filter.matches(&event));
        
        let wrong_event = BlockchainEvent {
            contract_address: Some("0x456".to_string()),
            ..event
        };
        
        assert!(!filter.matches(&wrong_event));
    }
    
    #[test]
    fn test_token_bucket() {
        let mut bucket = TokenBucket::new(10, 5);
        
        assert!(bucket.consume(5));
        assert!(bucket.consume(5));
        assert!(!bucket.consume(1));
    }
}