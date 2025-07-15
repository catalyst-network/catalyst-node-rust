//! Event System for Inter-Service Communication
//! Create as crates/catalyst-node/src/events/mod.rs

use crate::ServiceType;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::warn;

pub mod bus;
pub mod types;

/// Events that can occur in the Catalyst node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    // Service lifecycle events
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
    
    // Blockchain events
    BlockReceived {
        block_hash: String,
        block_number: u64,
    },
    BlockProduced {
        block_hash: String,
        block_number: u64,
        validator: String,
    },
    TransactionReceived {
        tx_hash: String,
    },
    TransactionExecuted {
        tx_hash: String,
        success: bool,
        gas_used: u64,
    },
    
    // Network events
    PeerConnected {
        peer_id: String,
    },
    PeerDisconnected {
        peer_id: String,
    },
    NetworkPartition {
        isolated_peers: Vec<String>,
    },
    
    // Consensus events
    ConsensusReached {
        block_hash: String,
    },
    ValidatorSlashed {
        validator: String,
        reason: String,
    },
    EpochChanged {
        epoch: u64,
        validators: Vec<String>,
    },
    
    // Storage events
    StateUpdated {
        block_hash: String,
        state_root: String,
    },
    DatabaseCompacted {
        size_before: u64,
        size_after: u64,
    },
    
    // RPC events
    RpcRequestReceived {
        method: String,
        request_id: String,
    },
    RpcRequestCompleted {
        method: String,
        request_id: String,
        success: bool,
    },
    
    // System events
    NodeStarted {
        node_id: String,
    },
    NodeStopping {
        node_id: String,
    },
    HealthCheckFailed {
        service_type: ServiceType,
        details: String,
    },
}

/// Event bus for publishing and subscribing to events
#[derive(Clone)]
pub struct EventBus {
    sender: Arc<broadcast::Sender<Event>>,
}

impl EventBus {
    /// Create a new event bus
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000); // Buffer 1000 events
        Self {
            sender: Arc::new(sender),
        }
    }

    /// Publish an event to all subscribers
    pub async fn publish(&self, event: Event) {
        if let Err(e) = self.sender.send(event) {
            warn!("Failed to publish event: {}", e);
        }
    }

    /// Subscribe to events
    pub async fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Get the number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

/// Event handler trait for services that want to handle events
#[async_trait::async_trait]
pub trait EventHandler: Send + Sync {
    /// Handle an incoming event
    async fn handle_event(&mut self, event: Event);
    
    /// Filter events this handler is interested in
    fn interested_in(&self, event: &Event) -> bool;
}

/// Event subscription helper
pub struct EventSubscription {
    receiver: broadcast::Receiver<Event>,
    handler: Box<dyn EventHandler>,
}

impl EventSubscription {
    /// Create a new event subscription
    pub fn new(receiver: broadcast::Receiver<Event>, handler: Box<dyn EventHandler>) -> Self {
        Self { receiver, handler }
    }

    /// Start processing events
    pub async fn start_processing(mut self) {
        while let Ok(event) = self.receiver.recv().await {
            if self.handler.interested_in(&event) {
                self.handler.handle_event(event).await;
            }
        }
    }
}

/// Utility macros for event publishing
#[macro_export]
macro_rules! publish_event {
    ($bus:expr, $event:expr) => {
        $bus.publish($event).await
    };
}

/// Event aggregator for collecting and analyzing events
pub struct EventAggregator {
    events: Arc<tokio::sync::RwLock<Vec<Event>>>,
    max_events: usize,
}

impl EventAggregator {
    /// Create a new event aggregator
    pub fn new(max_events: usize) -> Self {
        Self {
            events: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            max_events,
        }
    }

    /// Add an event to the aggregator
    pub async fn add_event(&self, event: Event) {
        let mut events = self.events.write().await;
        events.push(event);
        
        // Keep only the most recent events
        if events.len() > self.max_events {
            let excess = events.len() - self.max_events;
            events.drain(0..excess);
        }
    }

    /// Get recent events
    pub async fn get_recent_events(&self, count: usize) -> Vec<Event> {
        let events = self.events.read().await;
        let start = events.len().saturating_sub(count);
        events[start..].to_vec()
    }

    /// Get events by type
    pub async fn get_events_by_type(&self, event_type: &str) -> Vec<Event> {
        let events = self.events.read().await;
        events
            .iter()
            .filter(|event| Self::event_type_name(event) == event_type)
            .cloned()
            .collect()
    }

    /// Clear all stored events
    pub async fn clear(&self) {
        let mut events = self.events.write().await;
        events.clear();
    }

    /// Get event type name as string
    fn event_type_name(event: &Event) -> &'static str {
        match event {
            Event::ServiceStarted { .. } => "ServiceStarted",
            Event::ServiceStopped { .. } => "ServiceStopped",
            Event::ServiceError { .. } => "ServiceError",
            Event::BlockReceived { .. } => "BlockReceived",
            Event::BlockProduced { .. } => "BlockProduced",
            Event::TransactionReceived { .. } => "TransactionReceived",
            Event::TransactionExecuted { .. } => "TransactionExecuted",
            Event::PeerConnected { .. } => "PeerConnected",
            Event::PeerDisconnected { .. } => "PeerDisconnected",
            Event::NetworkPartition { .. } => "NetworkPartition",
            Event::ConsensusReached { .. } => "ConsensusReached",
            Event::ValidatorSlashed { .. } => "ValidatorSlashed",
            Event::EpochChanged { .. } => "EpochChanged",
            Event::StateUpdated { .. } => "StateUpdated",
            Event::DatabaseCompacted { .. } => "DatabaseCompacted",
            Event::RpcRequestReceived { .. } => "RpcRequestReceived",
            Event::RpcRequestCompleted { .. } => "RpcRequestCompleted",
            Event::NodeStarted { .. } => "NodeStarted",
            Event::NodeStopping { .. } => "NodeStopping",
            Event::HealthCheckFailed { .. } => "HealthCheckFailed",
        }
    }
}