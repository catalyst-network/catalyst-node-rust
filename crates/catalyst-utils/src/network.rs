// catalyst-utils/src/network.rs

use crate::{CatalystResult, CatalystError};
use serde::{Serialize, Deserialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Core trait for network messages in the Catalyst protocol
/// 
/// All messages passed between nodes must implement this trait to ensure
/// proper serialization, deserialization, and message type identification.
pub trait NetworkMessage: Send + Sync + Clone {
    /// Serialize the message to bytes for network transmission
    fn serialize(&self) -> CatalystResult<Vec<u8>>;
    
    /// Deserialize bytes into a message instance
    fn deserialize(data: &[u8]) -> CatalystResult<Self> where Self: Sized;
    
    /// Get the message type identifier
    fn message_type(&self) -> MessageType;
    
    /// Get the protocol version this message supports
    fn protocol_version(&self) -> u32 {
        1 // Default to version 1
    }
    
    /// Validate the message content
    fn validate(&self) -> CatalystResult<()> {
        Ok(()) // Default implementation accepts all messages
    }
    
    /// Get message priority (higher number = higher priority)
    fn priority(&self) -> u8 {
        MessagePriority::Normal as u8
    }
    
    /// Check if this message should be gossiped to peers
    fn should_gossip(&self) -> bool {
        true // Most messages should be gossiped by default
    }
    
    /// Get the maximum time this message should live in the network (seconds)
    fn ttl(&self) -> u32 {
        300 // 5 minutes default TTL
    }
    
    /// Get estimated size of the message when serialized
    fn estimated_size(&self) -> usize {
        self.serialize().map(|data| data.len()).unwrap_or(0)
    }
}

/// Message types used in the Catalyst network protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageType {
    // Consensus messages
    ProducerQuantity,
    ProducerCandidate, 
    ProducerVote,
    ProducerOutput,
    ConsensusSync,
    
    // Transaction messages
    Transaction,
    TransactionBatch,
    TransactionRequest,
    
    // Network management messages
    PeerDiscovery,
    PeerHandshake,
    PeerHeartbeat,
    PeerDisconnect,
    
    // Storage and state messages
    StateRequest,
    StateResponse,
    StorageSync,
    FileRequest,
    FileResponse,
    
    // Service bus messages
    EventNotification,
    EventSubscription,
    EventUnsubscription,
    
    // Node management messages
    WorkerRegistration,
    WorkerSelection,
    ProducerSelection,
    NodeStatus,
    
    // System messages
    HealthCheck,
    ConfigUpdate,
    NetworkInfo,
    MetricsReport,
    
    // Error handling
    ErrorResponse,
    
    // Custom/Extension types for future use
    Custom(u16),
}

impl MessageType {
    /// Check if this message type is critical for consensus
    pub fn is_consensus_critical(&self) -> bool {
        matches!(self, 
            MessageType::ProducerQuantity |
            MessageType::ProducerCandidate |
            MessageType::ProducerVote |
            MessageType::ProducerOutput |
            MessageType::ConsensusSync
        )
    }
    
    /// Check if this message type relates to transactions
    pub fn is_transaction_related(&self) -> bool {
        matches!(self,
            MessageType::Transaction |
            MessageType::TransactionBatch |
            MessageType::TransactionRequest
        )
    }
    
    /// Get the default priority for this message type
    pub fn default_priority(&self) -> MessagePriority {
        match self {
            // Consensus messages have highest priority
            MessageType::ProducerQuantity |
            MessageType::ProducerCandidate |
            MessageType::ProducerVote |
            MessageType::ProducerOutput => MessagePriority::Critical,
            
            // Transactions are high priority
            MessageType::Transaction |
            MessageType::TransactionBatch => MessagePriority::High,
            
            // Network management is normal priority
            MessageType::PeerDiscovery |
            MessageType::PeerHandshake |
            MessageType::PeerHeartbeat => MessagePriority::Normal,
            
            // Everything else is normal priority
            _ => MessagePriority::Normal,
        }
    }
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageType::ProducerQuantity => write!(f, "producer_quantity"),
            MessageType::ProducerCandidate => write!(f, "producer_candidate"),
            MessageType::ProducerVote => write!(f, "producer_vote"),
            MessageType::ProducerOutput => write!(f, "producer_output"),
            MessageType::ConsensusSync => write!(f, "consensus_sync"),
            MessageType::Transaction => write!(f, "transaction"),
            MessageType::TransactionBatch => write!(f, "transaction_batch"),
            MessageType::TransactionRequest => write!(f, "transaction_request"),
            MessageType::PeerDiscovery => write!(f, "peer_discovery"),
            MessageType::PeerHandshake => write!(f, "peer_handshake"),
            MessageType::PeerHeartbeat => write!(f, "peer_heartbeat"),
            MessageType::PeerDisconnect => write!(f, "peer_disconnect"),
            MessageType::StateRequest => write!(f, "state_request"),
            MessageType::StateResponse => write!(f, "state_response"),
            MessageType::StorageSync => write!(f, "storage_sync"),
            MessageType::FileRequest => write!(f, "file_request"),
            MessageType::FileResponse => write!(f, "file_response"),
            MessageType::EventNotification => write!(f, "event_notification"),
            MessageType::EventSubscription => write!(f, "event_subscription"),
            MessageType::EventUnsubscription => write!(f, "event_unsubscription"),
            MessageType::WorkerRegistration => write!(f, "worker_registration"),
            MessageType::WorkerSelection => write!(f, "worker_selection"),
            MessageType::ProducerSelection => write!(f, "producer_selection"),
            MessageType::NodeStatus => write!(f, "node_status"),
            MessageType::HealthCheck => write!(f, "health_check"),
            MessageType::ConfigUpdate => write!(f, "config_update"),
            MessageType::NetworkInfo => write!(f, "network_info"),
            MessageType::MetricsReport => write!(f, "metrics_report"),
            MessageType::ErrorResponse => write!(f, "error_response"),
            MessageType::Custom(id) => write!(f, "custom_{}", id),
        }
    }
}

/// Message priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Message envelope that wraps all network messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope {
    /// Unique message identifier
    pub id: String,
    /// Message type
    pub message_type: MessageType,
    /// Protocol version
    pub version: u32,
    /// Sender node identifier
    pub sender: String,
    /// Target node identifier (None for broadcast)
    pub target: Option<String>,
    /// Message timestamp (Unix timestamp in milliseconds)
    pub timestamp: u64,
    /// Message priority
    pub priority: u8,
    /// Time-to-live in seconds
    pub ttl: u32,
    /// Message payload
    pub payload: Vec<u8>,
    /// Message signature for authentication
    pub signature: Option<Vec<u8>>,
    /// Routing information
    pub routing_info: Option<RoutingInfo>,
}

impl MessageEnvelope {
    /// Create a new message envelope
    pub fn new(
        message_type: MessageType,
        sender: String,
        target: Option<String>,
        payload: Vec<u8>,
    ) -> Self {
        Self {
            id: generate_message_id(),
            message_type,
            version: 1,
            sender,
            target,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            priority: message_type.default_priority() as u8,
            ttl: 300, // 5 minutes default
            payload,
            signature: None,
            routing_info: None,
        }
    }
    
    /// Create envelope from a NetworkMessage
    pub fn from_message<T: NetworkMessage>(
        message: &T,
        sender: String,
        target: Option<String>,
    ) -> CatalystResult<Self> {
        let payload = message.serialize()?;
        let mut envelope = Self::new(message.message_type(), sender, target, payload);
        envelope.priority = message.priority();
        envelope.ttl = message.ttl();
        envelope.version = message.protocol_version();
        Ok(envelope)
    }
    
    /// Extract and deserialize the message from payload
    pub fn extract_message<T: NetworkMessage>(&self) -> CatalystResult<T> {
        T::deserialize(&self.payload)
    }
    
    /// Check if the message has expired
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        
        let expiry = self.timestamp + (self.ttl as u64 * 1000);
        now > expiry
    }
    
    /// Sign the message with the given signing function
    pub fn sign<F>(&mut self, sign_fn: F) -> CatalystResult<()>
    where
        F: FnOnce(&[u8]) -> CatalystResult<Vec<u8>>,
    {
        let data = self.signing_data()?;
        self.signature = Some(sign_fn(&data)?);
        Ok(())
    }
    
    /// Verify the message signature with the given verification function
    pub fn verify<F>(&self, verify_fn: F) -> CatalystResult<bool>
    where
        F: FnOnce(&[u8]) -> CatalystResult<bool>,
    {
        match &self.signature {
            Some(sig) => {
                let data = self.signing_data()?;
                // Create a combined data buffer with both message data and signature
                let mut combined_data = data;
                combined_data.extend_from_slice(sig);
                verify_fn(&combined_data)
            }
            None => Ok(false), // Unsigned message
        }
    }
    
    /// Get the data that should be signed
    fn signing_data(&self) -> CatalystResult<Vec<u8>> {
        let mut envelope = self.clone();
        envelope.signature = None; // Remove signature for signing
        serde_json::to_vec(&envelope)
            .map_err(|e| CatalystError::Serialization(format!("Failed to serialize envelope for signing: {}", e)))
    }
    
    /// Add routing information
    pub fn with_routing_info(mut self, routing_info: RoutingInfo) -> Self {
        self.routing_info = Some(routing_info);
        self
    }
}

/// Routing information for message delivery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingInfo {
    /// Nodes this message has already visited (to prevent loops)
    pub visited_nodes: Vec<String>,
    /// Maximum number of hops allowed
    pub max_hops: u8,
    /// Current hop count
    pub hop_count: u8,
    /// Preferred routing path
    pub preferred_path: Option<Vec<String>>,
    /// Whether this message requires ordered delivery
    pub ordered_delivery: bool,
}

impl RoutingInfo {
    /// Create new routing info
    pub fn new(max_hops: u8) -> Self {
        Self {
            visited_nodes: Vec::new(),
            max_hops,
            hop_count: 0,
            preferred_path: None,
            ordered_delivery: false,
        }
    }
    
    /// Add a visited node and increment hop count
    pub fn add_hop(&mut self, node_id: String) -> CatalystResult<()> {
        if self.hop_count >= self.max_hops {
            return Err(CatalystError::Network("Maximum hops exceeded".to_string()));
        }
        
        if self.visited_nodes.contains(&node_id) {
            return Err(CatalystError::Network("Routing loop detected".to_string()));
        }
        
        self.visited_nodes.push(node_id);
        self.hop_count += 1;
        Ok(())
    }
    
    /// Check if a node has been visited
    pub fn has_visited(&self, node_id: &str) -> bool {
        self.visited_nodes.contains(&node_id.to_string())
    }
    
    /// Check if more hops are allowed
    pub fn can_hop(&self) -> bool {
        self.hop_count < self.max_hops
    }
}

/// Generate a unique message ID
fn generate_message_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let id: u64 = rng.gen();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    format!("{:016x}{:016x}", timestamp, id)
}

/// Example implementation of NetworkMessage for basic messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicMessage {
    pub content: String,
    pub message_type: MessageType,
}

impl NetworkMessage for BasicMessage {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|e| CatalystError::Serialization(format!("Failed to serialize BasicMessage: {}", e)))
    }
    
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        serde_json::from_slice(data)
            .map_err(|e| CatalystError::Serialization(format!("Failed to deserialize BasicMessage: {}", e)))
    }
    
    fn message_type(&self) -> MessageType {
        self.message_type
    }
}

/// Trait for handling incoming network messages
pub trait MessageHandler: Send + Sync {
    /// Handle an incoming message
    fn handle_message(&self, envelope: MessageEnvelope) -> CatalystResult<Option<MessageEnvelope>>;
    
    /// Get the message types this handler can process
    fn supported_types(&self) -> Vec<MessageType>;
    
    /// Check if this handler can process a specific message type
    fn can_handle(&self, message_type: MessageType) -> bool {
        self.supported_types().contains(&message_type)
    }
}

/// Message router for dispatching messages to appropriate handlers
pub struct MessageRouter {
    handlers: std::collections::HashMap<MessageType, std::sync::Arc<dyn MessageHandler>>,
}

impl MessageRouter {
    /// Create a new message router
    pub fn new() -> Self {
        Self {
            handlers: std::collections::HashMap::new(),
        }
    }
    
    /// Register a handler for specific message types
    pub fn register_handler(&mut self, handler: std::sync::Arc<dyn MessageHandler>) {
        let supported_types = handler.supported_types();
        for msg_type in supported_types {
            self.handlers.insert(msg_type, handler.clone());
        }
    }
    
    /// Route a message to the appropriate handler
    pub fn route_message(&self, envelope: MessageEnvelope) -> CatalystResult<Option<MessageEnvelope>> {
        match self.handlers.get(&envelope.message_type) {
            Some(handler) => handler.handle_message(envelope),
            None => Err(CatalystError::Network(
                format!("No handler for message type: {}", envelope.message_type)
            )),
        }
    }
    
    /// Get all registered message types
    pub fn registered_types(&self) -> Vec<MessageType> {
        self.handlers.keys().cloned().collect()
    }
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Network statistics for monitoring
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkStats {
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Messages by type
    pub messages_by_type: std::collections::HashMap<String, u64>,
    /// Total bytes sent
    pub bytes_sent: u64,
    /// Total bytes received
    pub bytes_received: u64,
    /// Number of failed message deliveries
    pub delivery_failures: u64,
    /// Number of expired messages
    pub expired_messages: u64,
    /// Average message processing time (milliseconds)
    pub avg_processing_time_ms: f64,
}

impl NetworkStats {
    /// Record a sent message
    pub fn record_sent_message(&mut self, message_type: MessageType, size: usize) {
        self.messages_sent += 1;
        self.bytes_sent += size as u64;
        *self.messages_by_type.entry(message_type.to_string()).or_insert(0) += 1;
    }
    
    /// Record a received message
    pub fn record_received_message(&mut self, message_type: MessageType, size: usize) {
        self.messages_received += 1;
        self.bytes_received += size as u64;
        *self.messages_by_type.entry(message_type.to_string()).or_insert(0) += 1;
    }
    
    /// Record a delivery failure
    pub fn record_delivery_failure(&mut self) {
        self.delivery_failures += 1;
    }
    
    /// Record an expired message
    pub fn record_expired_message(&mut self) {
        self.expired_messages += 1;
    }
    
    /// Update average processing time
    pub fn update_processing_time(&mut self, processing_time_ms: f64) {
        // Simple exponential moving average
        let alpha = 0.1;
        self.avg_processing_time_ms = alpha * processing_time_ms + (1.0 - alpha) * self.avg_processing_time_ms;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_message_type_display() {
        assert_eq!(MessageType::Transaction.to_string(), "transaction");
        assert_eq!(MessageType::ProducerQuantity.to_string(), "producer_quantity");
        assert_eq!(MessageType::Custom(123).to_string(), "custom_123");
    }
    
    #[test]
    fn test_message_type_classification() {
        assert!(MessageType::ProducerQuantity.is_consensus_critical());
        assert!(MessageType::Transaction.is_transaction_related());
        assert!(!MessageType::PeerHeartbeat.is_consensus_critical());
    }
    
    #[test]
    fn test_basic_message() {
        let msg = BasicMessage {
            content: "Hello, World!".to_string(),
            message_type: MessageType::HealthCheck,
        };
        
        // Use explicit trait method to avoid ambiguity with serde::Serialize
        let serialized = NetworkMessage::serialize(&msg).unwrap();
        let deserialized = <BasicMessage as NetworkMessage>::deserialize(&serialized).unwrap();
        
        assert_eq!(msg.content, deserialized.content);
        assert_eq!(msg.message_type, deserialized.message_type);
    }
    
    #[test]
    fn test_message_envelope() {
        let payload = b"test payload".to_vec();
        let envelope = MessageEnvelope::new(
            MessageType::Transaction,
            "node1".to_string(),
            Some("node2".to_string()),
            payload.clone(),
        );
        
        assert_eq!(envelope.message_type, MessageType::Transaction);
        assert_eq!(envelope.sender, "node1");
        assert_eq!(envelope.target, Some("node2".to_string()));
        assert_eq!(envelope.payload, payload);
        assert!(!envelope.is_expired());
    }
    
    #[test]
    fn test_routing_info() {
        let mut routing = RoutingInfo::new(5);
        
        assert!(routing.can_hop());
        assert!(!routing.has_visited("node1"));
        
        routing.add_hop("node1".to_string()).unwrap();
        assert!(routing.has_visited("node1"));
        assert_eq!(routing.hop_count, 1);
        
        // Test loop detection
        let result = routing.add_hop("node1".to_string());
        assert!(result.is_err());
    }
    
    #[test]
    fn test_message_id_generation() {
        let id1 = generate_message_id();
        let id2 = generate_message_id();
        
        assert_ne!(id1, id2);
        assert_eq!(id1.len(), 32); // 16 hex chars for timestamp + 16 for random
    }
    
    #[test]
    fn test_network_stats() {
        let mut stats = NetworkStats::default();
        
        stats.record_sent_message(MessageType::Transaction, 100);
        stats.record_received_message(MessageType::ProducerQuantity, 50);
        
        assert_eq!(stats.messages_sent, 1);
        assert_eq!(stats.messages_received, 1);
        assert_eq!(stats.bytes_sent, 100);
        assert_eq!(stats.bytes_received, 50);
        
        let tx_count = stats.messages_by_type.get("transaction").unwrap();
        assert_eq!(*tx_count, 1);
    }
}