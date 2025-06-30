use crate::{
    config::GossipConfig,
    error::{NetworkError, NetworkResult},
    messaging::NetworkMessage,
};

use catalyst_utils::{
    logging::*,
    metrics::*,
};

use libp2p::{PeerId, gossipsub::{Message, TopicHash}};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

pub struct GossipProtocol {
    config: GossipConfig,
    message_cache: RwLock<HashMap<String, CachedMessage>>,
    subscribed_peers: RwLock<HashMap<TopicHash, HashSet<PeerId>>>,
    stats: RwLock<GossipStats>,
}

#[derive(Debug, Clone)]
pub struct CachedMessage {
    pub message: NetworkMessage,
    pub cached_at: Instant,
    pub propagation_count: u32,
}

#[derive(Debug, Default)]
pub struct GossipStats {
    pub messages_published: u64,
    pub messages_received: u64,
    pub messages_forwarded: u64,
    pub messages_dropped: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

impl GossipProtocol {
    pub fn new(config: &GossipConfig) -> Self {
        Self {
            config: config.clone(),
            message_cache: RwLock::new(HashMap::new()),
            subscribed_peers: RwLock::new(HashMap::new()),
            stats: RwLock::new(GossipStats::default()),
        }
    }
    
    pub async fn publish_message(&self, message: NetworkMessage) -> NetworkResult<()> {
        log_debug!(LogCategory::Network, "Publishing gossip message");
        
        // Add to cache
        let message_id = self.generate_message_id(&message);
        {
            let mut cache = self.message_cache.write().await;
            cache.insert(message_id.clone(), CachedMessage {
                message: message.clone(),
                cached_at: Instant::now(),
                propagation_count: 0,
            });
        }
        
        // Update stats
        {
            let mut stats = self.stats.write().await;
            stats.messages_published += 1;
        }
        
        increment_counter!("network_gossip_published_total", 1);
        Ok(())
    }
    
    pub async fn handle_received_message(&self, message: Message, sender: PeerId) -> NetworkResult<()> {
        let message_id = String::from_utf8_lossy(&message.data[..32.min(message.data.len())]).to_string();
        
        // Check cache for duplicates
        {
            let cache = self.message_cache.read().await;
            if cache.contains_key(&message_id) {
                let mut stats = self.stats.write().await;
                stats.cache_hits += 1;
                return Ok(()); // Duplicate message
            }
        }
        
        // Process new message
        {
            let mut stats = self.stats.write().await;
            stats.messages_received += 1;
            stats.cache_misses += 1;
        }
        
        log_debug!(LogCategory::Network, "Received new gossip message from {}", sender);
        increment_counter!("network_gossip_received_total", 1);
        
        Ok(())
    }
    
    fn generate_message_id(&self, message: &NetworkMessage) -> String {
        format!("{:?}_{}", message.message_type(), message.transport.created_at)
    }
    
    pub async fn cleanup_cache(&self) {
        let mut cache = self.message_cache.write().await;
        let now = Instant::now();
        let expiry = self.config.message_cache_duration;
        
        cache.retain(|_, cached| now.duration_since(cached.cached_at) < expiry);
    }
}