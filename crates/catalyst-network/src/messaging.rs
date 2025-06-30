//! Network message handling and routing

use catalyst_utils::{
    network::{NetworkMessage as CatalystNetworkMessage, MessageType, MessageEnvelope, MessagePriority},
    serialization::{CatalystSerialize, CatalystDeserialize},
    CatalystResult,
    logging::*,
};
use libp2p::{PeerId, gossipsub::Message as GossipMessage};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, RwLock, oneshot};
use crate::error::{NetworkError, NetworkResult};

/// Network message wrapper for libp2p transport
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMessage {
    /// Message envelope from catalyst-utils
    pub envelope: MessageEnvelope,
    
    /// Routing metadata
    pub routing: RoutingMetadata,
    
    /// Transport metadata
    pub transport: TransportMetadata,
}

/// Message routing information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingMetadata {
    /// Hop count for TTL
    pub hop_count: u8,
    
    /// Maximum hops before dropping
    pub max_hops: u8,
    
    /// Routing strategy
    pub strategy: RoutingStrategy,
    
    /// Target peers (for directed messages)
    pub targets: Vec<String>, // PeerId as string
    
    /// Exclude peers (already seen)
    pub exclude: Vec<String>,
}

/// Transport-specific metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportMetadata {
    /// Transport type used
    pub transport_type: TransportType,
    
    /// Compression enabled
    pub compressed: bool,
    
    /// Encryption enabled
    pub encrypted: bool,
    
    /// Message size in bytes
    pub size_bytes: usize,
    
    /// Creation timestamp
    pub created_at: u64,
}

/// Routing strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoutingStrategy {
    /// Broadcast to all peers
    Broadcast,
    
    /// Send to specific peer
    Direct(String), // PeerId as string
    
    /// Gossip protocol propagation
    Gossip,
    
    /// Send to closest peers (Kademlia-based)
    Closest(usize),
    
    /// Random subset of peers
    Random(usize),
}

/// Transport types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportType {
    Tcp,
    WebSocket,
    Quic,
    Memory, // For testing
}

/// Message handler trait for processing incoming messages
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// Handle an incoming message
    async fn handle_message(
        &self,
        message: &NetworkMessage,
        sender: PeerId,
    ) -> NetworkResult<Option<NetworkMessage>>;
    
    /// Get the message types this handler supports
    fn supported_types(&self) -> Vec<MessageType>;
    
    /// Handler priority (higher = more priority)
    fn priority(&self) -> u8 {
        0
    }
}

/// Message router for dispatching messages to handlers
pub struct MessageRouter {
    /// Registered message handlers
    handlers: RwLock<HashMap<MessageType, Vec<Arc<dyn MessageHandler>>>>,
    
    /// Default handler for unrouted messages
    default_handler: Option<Arc<dyn MessageHandler>>,
    
    /// Message statistics
    stats: RwLock<MessageStats>,
}

/// Message processing statistics
#[derive(Debug, Default)]
pub struct MessageStats {
    pub messages_received: u64,
    pub messages_sent: u64,
    pub messages_dropped: u64,
    pub messages_routed: u64,
    pub bytes_received: u64,
    pub bytes_sent: u64,
    pub errors: u64,
    pub avg_processing_time: Duration,
}

/// Response channel for request-response patterns
pub type ResponseSender<T> = oneshot::Sender<NetworkResult<T>>;
pub type ResponseReceiver<T> = oneshot::Receiver<NetworkResult<T>>;

/// Message with response channel
pub struct RequestMessage<T> {
    pub message: NetworkMessage,
    pub response_tx: ResponseSender<T>,
}

impl NetworkMessage {
    /// Create new network message
    pub fn new<T: CatalystNetworkMessage>(
        message: &T,
        sender: String,
        target: Option<String>,
        strategy: RoutingStrategy,
    ) -> NetworkResult<Self> {
        let envelope = MessageEnvelope::from_message(message, sender, target)
            .map_err(|e| NetworkError::SerializationFailed(e.to_string()))?;
        
        let routing = RoutingMetadata {
            hop_count: 0,
            max_hops: 10, // Default TTL
            strategy,
            targets: Vec::new(),
            exclude: Vec::new(),
        };
        
        let transport = TransportMetadata {
            transport_type: TransportType::Tcp, // Default
            compressed: false,
            encrypted: true,
            size_bytes: envelope.serialized_size(),
            created_at: catalyst_utils::utils::current_timestamp(),
        };
        
        Ok(Self {
            envelope,
            routing,
            transport,
        })
    }
    
    /// Extract the inner message
    pub fn extract_message<T: CatalystNetworkMessage>(&self) -> NetworkResult<T> {
        self.envelope.extract_message()
            .map_err(|e| NetworkError::DeserializationFailed(e.to_string()))
    }
    
    /// Check if message should be forwarded
    pub fn should_forward(&self) -> bool {
        self.routing.hop_count < self.routing.max_hops
    }
    
    /// Increment hop count
    pub fn increment_hops(&mut self) {
        self.routing.hop_count += 1;
    }
    
    /// Add peer to exclude list
    pub fn add_exclude_peer(&mut self, peer_id: &PeerId) {
        self.routing.exclude.push(peer_id.to_string());
    }
    
    /// Check if peer is excluded
    pub fn is_peer_excluded(&self, peer_id: &PeerId) -> bool {
        self.routing.exclude.contains(&peer_id.to_string())
    }
    
    /// Get message priority
    pub fn priority(&self) -> u8 {
        self.envelope.priority()
    }
    
    /// Get message type
    pub fn message_type(&self) -> MessageType {
        self.envelope.message_type()
    }
    
    /// Get message age
    pub fn age(&self) -> Duration {
        let now = catalyst_utils::utils::current_timestamp();
        Duration::from_secs(now.saturating_sub(self.transport.created_at))
    }
    
    /// Check if message is expired
    pub fn is_expired(&self, max_age: Duration) -> bool {
        self.age() > max_age
    }
}

impl CatalystSerialize for NetworkMessage {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        bincode::serialize(self)
            .map_err(|e| catalyst_utils::CatalystError::Serialization(e.to_string()))
    }
    
    fn serialized_size(&self) -> usize {
        bincode::serialized_size(self).unwrap_or(0) as usize
    }
}

impl CatalystDeserialize for NetworkMessage {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        bincode::deserialize(data)
            .map_err(|e| catalyst_utils::CatalystError::Serialization(e.to_string()))
    }
}

impl MessageRouter {
    /// Create new message router
    pub fn new() -> Self {
        Self {
            handlers: RwLock::new(HashMap::new()),
            default_handler: None,
            stats: RwLock::new(MessageStats::default()),
        }
    }
    
    /// Register a message handler
    pub async fn register_handler<H>(&self, handler: H) -> NetworkResult<()>
    where
        H: MessageHandler + 'static,
    {
        let handler = Arc::new(handler);
        let mut handlers = self.handlers.write().await;
        
        for msg_type in handler.supported_types() {
            handlers
                .entry(msg_type)
                .or_insert_with(Vec::new)
                .push(handler.clone());
        }
        
        log_info!(
            LogCategory::Network,
            "Registered message handler for types: {:?}",
            handler.supported_types()
        );
        
        Ok(())
    }
    
    /// Set default handler for unrouted messages
    pub fn set_default_handler<H>(&mut self, handler: H)
    where
        H: MessageHandler + 'static,
    {
        self.default_handler = Some(Arc::new(handler));
    }
    
    /// Route incoming message to appropriate handlers
    pub async fn route_message(
        &self,
        message: NetworkMessage,
        sender: PeerId,
    ) -> NetworkResult<Vec<NetworkMessage>> {
        let start_time = Instant::now();
        let message_type = message.message_type();
        
        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.messages_received += 1;
            stats.bytes_received += message.transport.size_bytes as u64;
        }
        
        log_debug!(
            LogCategory::Network,
            "Routing message type {:?} from peer {}",
            message_type,
            sender
        );
        
        let handlers = self.handlers.read().await;
        let mut responses = Vec::new();
        
        if let Some(type_handlers) = handlers.get(&message_type) {
            // Sort handlers by priority
            let mut sorted_handlers = type_handlers.clone();
            sorted_handlers.sort_by(|a, b| b.priority().cmp(&a.priority()));
            
            for handler in sorted_handlers {
                match handler.handle_message(&message, sender).await {
                    Ok(Some(response)) => {
                        responses.push(response);
                        log_debug!(
                            LogCategory::Network,
                            "Handler produced response for message type {:?}",
                            message_type
                        );
                    }
                    Ok(None) => {
                        log_debug!(
                            LogCategory::Network,
                            "Handler processed message type {:?} without response",
                            message_type
                        );
                    }
                    Err(e) => {
                        log_warn!(
                            LogCategory::Network,
                            "Handler error for message type {:?}: {}",
                            message_type,
                            e
                        );
                        
                        let mut stats = self.stats.write().await;
                        stats.errors += 1;
                    }
                }
            }
        } else if let Some(default_handler) = &self.default_handler {
            // Use default handler
            match default_handler.handle_message(&message, sender).await {
                Ok(Some(response)) => responses.push(response),
                Ok(None) => {}
                Err(e) => {
                    log_warn!(
                        LogCategory::Network,
                        "Default handler error for message type {:?}: {}",
                        message_type,
                        e
                    );
                    
                    let mut stats = self.stats.write().await;
                    stats.errors += 1;
                }
            }
        } else {
            log_warn!(
                LogCategory::Network,
                "No handler found for message type {:?}",
                message_type
            );
            
            let mut stats = self.stats.write().await;
            stats.messages_dropped += 1;
        }
        
        // Update processing time statistics
        let processing_time = start_time.elapsed();
        {
            let mut stats = self.stats.write().await;
            stats.messages_routed += 1;
            
            // Update rolling average
            let alpha = 0.1; // Smoothing factor
            let current_avg = stats.avg_processing_time.as_nanos() as f64;
            let new_time = processing_time.as_nanos() as f64;
            let new_avg = alpha * new_time + (1.0 - alpha) * current_avg;
            stats.avg_processing_time = Duration::from_nanos(new_avg as u64);
        }
        
        Ok(responses)
    }
    
    /// Get message processing statistics
    pub async fn get_stats(&self) -> MessageStats {
        let stats = self.stats.read().await;
        MessageStats {
            messages_received: stats.messages_received,
            messages_sent: stats.messages_sent,
            messages_dropped: stats.messages_dropped,
            messages_routed: stats.messages_routed,
            bytes_received: stats.bytes_received,
            bytes_sent: stats.bytes_sent,
            errors: stats.errors,
            avg_processing_time: stats.avg_processing_time,
        }
    }
    
    /// Reset statistics
    pub async fn reset_stats(&self) {
        let mut stats = self.stats.write().await;
        *stats = MessageStats::default();
    }
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Example message handler for ping messages
pub struct PingHandler;

#[async_trait::async_trait]
impl MessageHandler for PingHandler {
    async fn handle_message(
        &self,
        message: &NetworkMessage,
        sender: PeerId,
    ) -> NetworkResult<Option<NetworkMessage>> {
        log_debug!(LogCategory::Network, "Received ping from {}", sender);
        
        // Create pong response
        let pong_message = NetworkMessage::new(
            &catalyst_utils::network::PingMessage {
                timestamp: catalyst_utils::utils::current_timestamp(),
                data: Vec::new(),
            },
            "local_node".to_string(),
            Some(sender.to_string()),
            RoutingStrategy::Direct(sender.to_string()),
        )?;
        
        Ok(Some(pong_message))
    }
    
    fn supported_types(&self) -> Vec<MessageType> {
        vec![MessageType::PeerHandshake] // Using existing type for ping
    }
    
    fn priority(&self) -> u8 {
        5 // Medium priority
    }
}

/// Message queue for outgoing messages
pub struct MessageQueue {
    /// Priority queues for different message types
    queues: RwLock<HashMap<u8, mpsc::UnboundedSender<NetworkMessage>>>,
    
    /// Queue statistics
    stats: RwLock<QueueStats>,
}

#[derive(Debug, Default)]
pub struct QueueStats {
    pub messages_queued: u64,
    pub messages_sent: u64,
    pub queue_depth: usize,
    pub average_wait_time: Duration,
}

impl MessageQueue {
    /// Create new message queue
    pub fn new() -> Self {
        Self {
            queues: RwLock::new(HashMap::new()),
            stats: RwLock::new(QueueStats::default()),
        }
    }
    
    /// Queue message for sending
    pub async fn queue_message(&self, message: NetworkMessage) -> NetworkResult<()> {
        let priority = message.priority();
        let queues = self.queues.read().await;
        
        if let Some(sender) = queues.get(&priority) {
            sender.send(message)
                .map_err(|_| NetworkError::RoutingFailed("Queue channel closed".to_string()))?;
            
            let mut stats = self.stats.write().await;
            stats.messages_queued += 1;
            stats.queue_depth += 1;
        } else {
            return Err(NetworkError::RoutingFailed(
                format!("No queue for priority {}", priority)
            ));
        }
        
        Ok(())
    }
    
    /// Get receiver for specific priority level
    pub async fn get_receiver(&self, priority: u8) -> mpsc::UnboundedReceiver<NetworkMessage> {
        let mut queues = self.queues.write().await;
        let (tx, rx) = mpsc::unbounded_channel();
        queues.insert(priority, tx);
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyst_utils::network::PingMessage;
    use tokio::time::timeout;
    
    #[tokio::test]
    async fn test_message_creation() {
        let ping = PingMessage {
            timestamp: catalyst_utils::utils::current_timestamp(),
            data: vec![1, 2, 3],
        };
        
        let message = NetworkMessage::new(
            &ping,
            "sender".to_string(),
            Some("target".to_string()),
            RoutingStrategy::Direct("target".to_string()),
        ).unwrap();
        
        assert_eq!(message.routing.hop_count, 0);
        assert!(message.should_forward());
    }
    
    #[tokio::test]
    async fn test_message_routing() {
        let router = MessageRouter::new();
        router.register_handler(PingHandler).await.unwrap();
        
        let ping = PingMessage {
            timestamp: catalyst_utils::utils::current_timestamp(),
            data: Vec::new(),
        };
        
        let message = NetworkMessage::new(
            &ping,
            "sender".to_string(),
            None,
            RoutingStrategy::Broadcast,
        ).unwrap();
        
        let sender = PeerId::random();
        let responses = router.route_message(message, sender).await.unwrap();
        
        // Should get one pong response
        assert_eq!(responses.len(), 1);
    }
    
    #[tokio::test]
    async fn test_message_serialization() {
        let ping = PingMessage {
            timestamp: catalyst_utils::utils::current_timestamp(),
            data: vec![1, 2, 3],
        };
        
        let message = NetworkMessage::new(
            &ping,
            "sender".to_string(),
            None,
            RoutingStrategy::Broadcast,
        ).unwrap();
        
        let serialized = message.serialize().unwrap();
        let deserialized = NetworkMessage::deserialize(&serialized).unwrap();
        
        assert_eq!(message.routing.hop_count, deserialized.routing.hop_count);
        assert_eq!(message.transport.size_bytes, deserialized.transport.size_bytes);
    }
    
    #[tokio::test]
    async fn test_message_queue() {
        let queue = MessageQueue::new();
        let mut receiver = queue.get_receiver(5).await;
        
        let ping = PingMessage {
            timestamp: catalyst_utils::utils::current_timestamp(),
            data: Vec::new(),
        };
        
        let message = NetworkMessage::new(
            &ping,
            "sender".to_string(),
            None,
            RoutingStrategy::Broadcast,
        ).unwrap();
        
        queue.queue_message(message).await.unwrap();
        
        let received = timeout(Duration::from_millis(100), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        
        assert_eq!(received.routing.hop_count, 0);
    }
}