//! Event filtering engine for the Service Bus

use crate::events::{BlockchainEvent, EventFilter, EventSubscription};
use catalyst_utils::{CatalystResult, CatalystError, logging::LogCategory};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Maximum number of events to keep in replay buffer
const MAX_REPLAY_BUFFER_SIZE: usize = 10000;

/// Event filtering and distribution engine
#[derive(Debug)]
pub struct FilterEngine {
    /// Active subscriptions by ID
    subscriptions: Arc<RwLock<HashMap<Uuid, EventSubscription>>>,
    
    /// Event replay buffer
    replay_buffer: Arc<RwLock<HashMap<u64, Vec<BlockchainEvent>>>>, // block_number -> events
    
    /// Event broadcasters by subscription ID
    broadcasters: Arc<RwLock<HashMap<Uuid, broadcast::Sender<BlockchainEvent>>>>,
    
    /// Configuration
    max_filters_per_connection: usize,
    enable_replay: bool,
    replay_buffer_size: usize,
    
    /// Connection to subscription mapping
    connection_subscriptions: Arc<RwLock<HashMap<String, Vec<Uuid>>>>, // connection_id -> subscription_ids
}

impl FilterEngine {
    /// Create a new filter engine
    pub fn new(
        max_filters_per_connection: usize,
        enable_replay: bool,
        replay_buffer_size: usize,
    ) -> Self {
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            replay_buffer: Arc::new(RwLock::new(HashMap::new())),
            broadcasters: Arc::new(RwLock::new(HashMap::new())),
            max_filters_per_connection,
            enable_replay,
            replay_buffer_size: replay_buffer_size.min(MAX_REPLAY_BUFFER_SIZE),
            connection_subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// Add a new event subscription
    pub fn add_subscription(
        &self,
        connection_id: String,
        filter: EventFilter,
    ) -> CatalystResult<(Uuid, broadcast::Receiver<BlockchainEvent>)> {
        // Check connection filter limit
        let mut conn_map = self.connection_subscriptions.write();
        let connection_subs = conn_map.entry(connection_id.clone()).or_insert_with(Vec::new);
        
        if connection_subs.len() >= self.max_filters_per_connection {
            return Err(CatalystError::Invalid(format!(
                "Connection {} has reached maximum filter limit of {}",
                connection_id, self.max_filters_per_connection
            )));
        }
        
        let subscription = EventSubscription::new(filter, connection_id.clone());
        let subscription_id = subscription.id;
        
        // Create broadcast channel for this subscription
        let (tx, rx) = broadcast::channel(1000);
        
        // Store subscription and broadcaster
        self.subscriptions.write().insert(subscription_id, subscription);
        self.broadcasters.write().insert(subscription_id, tx);
        connection_subs.push(subscription_id);
        
        catalyst_utils::logging::log_info!(
            LogCategory::ServiceBus,
            "Added subscription {} for connection {}",
            subscription_id,
            connection_id
        );
        
        Ok((subscription_id, rx))
    }
    
    /// Remove a subscription
    pub fn remove_subscription(&self, subscription_id: Uuid, connection_id: &str) -> CatalystResult<()> {
        // Remove from subscriptions
        self.subscriptions.write().remove(&subscription_id);
        self.broadcasters.write().remove(&subscription_id);
        
        // Remove from connection tracking
        if let Some(connection_subs) = self.connection_subscriptions.write().get_mut(connection_id) {
            connection_subs.retain(|id| *id != subscription_id);
        }
        
        catalyst_utils::logging::log_info!(
            LogCategory::ServiceBus,
            "Removed subscription {} for connection {}",
            subscription_id,
            connection_id
        );
        
        Ok(())
    }
    
    /// Remove all subscriptions for a connection
    pub fn remove_connection_subscriptions(&self, connection_id: &str) {
        let mut conn_map = self.connection_subscriptions.write();
        if let Some(subscription_ids) = conn_map.remove(connection_id) {
            for subscription_id in subscription_ids {
                self.subscriptions.write().remove(&subscription_id);
                self.broadcasters.write().remove(&subscription_id);
            }
            
            catalyst_utils::logging::log_info!(
                LogCategory::ServiceBus,
                "Removed all subscriptions for connection {}",
                connection_id
            );
        }
    }
    
    /// Process a new blockchain event
    pub async fn process_event(&self, event: BlockchainEvent) -> CatalystResult<()> {
        // Add to replay buffer if enabled
        if self.enable_replay {
            self.add_to_replay_buffer(&event)?;
        }
        
        // Find matching subscriptions and broadcast event
        let mut matched_count = 0;

        let subscriptions = self.subscriptions.read();
        let broadcasters = self.broadcasters.read();
        for subscription in subscriptions.values() {
            if subscription.matches(&event) {
                if let Some(broadcaster) = broadcasters.get(&subscription.id) {
                    // Send to all subscribers of this filter
                    match broadcaster.send(event.clone()) {
                        Ok(receiver_count) => {
                            matched_count += receiver_count;
                        }
                        Err(_) => {
                            // No receivers, subscription might be dead
                            catalyst_utils::logging::log_warn!(
                                LogCategory::ServiceBus,
                                "No receivers for subscription {}", 
                                subscription.id
                            );
                        }
                    }
                }
            }
        }
        
        if matched_count > 0 {
            catalyst_utils::logging::log_debug!(
                LogCategory::ServiceBus,
                "Event {} matched {} subscriptions",
                event.id,
                matched_count
            );
        }
        
        Ok(())
    }
    
    /// Get historical events for replay
    pub async fn get_historical_events(
        &self,
        filter: &EventFilter,
        limit: Option<usize>,
    ) -> CatalystResult<Vec<BlockchainEvent>> {
        if !self.enable_replay {
            return Ok(Vec::new());
        }
        
        let mut events = Vec::new();
        let limit = limit.unwrap_or(1000).min(10000); // Cap at 10k events
        
        // Determine block range for search
        let from_block = filter.from_block.unwrap_or(0);
        let to_block = filter.to_block.unwrap_or(u64::MAX);
        
        // Collect events from replay buffer
        let replay = self.replay_buffer.read();
        for (&block_number, block_events) in replay.iter() {
            if block_number >= from_block && block_number <= to_block {
                for event in block_events.iter() {
                    if event.matches_filter(filter) {
                        events.push(event.clone());
                        
                        if events.len() >= limit {
                            break;
                        }
                    }
                }
            }
            
            if events.len() >= limit {
                break;
            }
        }
        
        // Sort by timestamp
        events.sort_by_key(|e| e.timestamp);
        
        Ok(events)
    }
    
    /// Add event to replay buffer
    fn add_to_replay_buffer(&self, event: &BlockchainEvent) -> CatalystResult<()> {
        let block_number = event.block_number;
        
        // Add event to the appropriate block bucket
        let mut replay = self.replay_buffer.write();
        replay.entry(block_number).or_insert_with(Vec::new).push(event.clone());
        
        // Maintain buffer size - remove old blocks if needed
        if replay.len() > self.replay_buffer_size {
            // Find oldest block and remove it
            if let Some(oldest_block) = replay.keys().min().copied() {
                replay.remove(&oldest_block);
            }
        }
        
        Ok(())
    }
    
    /// Get subscription statistics
    pub fn get_stats(&self) -> FilterEngineStats {
        let total_subscriptions = self.subscriptions.read().len();
        let total_connections = self.connection_subscriptions.read().len();
        let replay = self.replay_buffer.read();
        let replay_buffer_blocks = replay.len();
        let replay_buffer_events: usize = replay.values().map(|v| v.len()).sum();
        
        FilterEngineStats {
            total_subscriptions,
            total_connections,
            replay_buffer_blocks,
            replay_buffer_events,
        }
    }
    
    /// Get active subscriptions for a connection
    pub fn get_connection_subscriptions(&self, connection_id: &str) -> Vec<EventSubscription> {
        let conn_map = self.connection_subscriptions.read();
        let subs = self.subscriptions.read();
        conn_map
            .get(connection_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| subs.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// Update subscription filter
    pub fn update_subscription_filter(
        &self,
        subscription_id: Uuid,
        new_filter: EventFilter,
    ) -> CatalystResult<()> {
        let mut subs = self.subscriptions.write();
        if let Some(subscription) = subs.get_mut(&subscription_id) {
            subscription.filter = new_filter;
            
            catalyst_utils::logging::log_info!(
                LogCategory::ServiceBus,
                "Updated filter for subscription {}",
                subscription_id
            );
            
            Ok(())
        } else {
            Err(CatalystError::NotFound(format!(
                "Subscription {} not found",
                subscription_id
            )))
        }
    }
}

/// Filter engine statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FilterEngineStats {
    pub total_subscriptions: usize,
    pub total_connections: usize,
    pub replay_buffer_blocks: usize,
    pub replay_buffer_events: usize,
}

/// Event matcher for complex filtering logic
pub struct EventMatcher;

impl EventMatcher {
    /// Check if event matches complex filter conditions
    pub fn matches_complex_filter(
        event: &BlockchainEvent,
        conditions: &HashMap<String, serde_json::Value>,
    ) -> bool {
        for (key, expected_value) in conditions {
            match key.as_str() {
                "min_amount" => {
                    if let Some(amount) = event.data.get("amount") {
                        if let (Some(actual), Some(expected)) = (amount.as_u64(), expected_value.as_u64()) {
                            if actual < expected {
                                return false;
                            }
                        }
                    }
                }
                "token_symbol" => {
                    if let Some(symbol) = event.data.get("symbol") {
                        if let (Some(actual), Some(expected)) = (symbol.as_str(), expected_value.as_str()) {
                            if actual != expected {
                                return false;
                            }
                        }
                    }
                }
                "gas_limit" => {
                    if let Some(gas_used) = event.metadata.gas_used {
                        if let Some(expected) = expected_value.as_u64() {
                            if gas_used > expected {
                                return false;
                            }
                        }
                    }
                }
                _ => {
                    // Custom field matching
                    if let Some(actual) = event.data.get(key) {
                        if actual != expected_value {
                            return false;
                        }
                    }
                }
            }
        }
        
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventType, events};
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_filter_engine_subscription() {
        let engine = FilterEngine::new(10, true, 100);
        
        let filter = EventFilter::new()
            .with_event_types(vec![EventType::TransactionConfirmed]);
        
        let (sub_id, mut rx) = engine.add_subscription(
            "test_connection".to_string(),
            filter,
        ).unwrap();
        
        // Create and process a matching event
        let event = events::transaction_confirmed(
            100,
            [1u8; 32],
            [2u8; 21],
            Some([3u8; 21]),
            1000,
            21000,
        );
        
        timeout(Duration::from_secs(2), engine.process_event(event.clone()))
            .await
            .expect("process_event timed out")
            .unwrap();
        
        // Should receive the event
        let received_event = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("rx.recv timed out")
            .unwrap();
        assert_eq!(received_event.id, event.id);
        
        // Clean up
        engine.remove_subscription(sub_id, "test_connection").unwrap();
    }

    #[tokio::test]
    async fn test_connection_filter_limit() {
        let engine = FilterEngine::new(2, false, 100); // Max 2 filters per connection
        
        let filter = EventFilter::new();
        
        // Should allow first two subscriptions
        let result1 = engine.add_subscription("test_conn".to_string(), filter.clone());
        assert!(result1.is_ok());
        
        let result2 = engine.add_subscription("test_conn".to_string(), filter.clone());
        assert!(result2.is_ok());
        
        // Third should fail
        let result3 = engine.add_subscription("test_conn".to_string(), filter);
        assert!(result3.is_err());
    }

    #[tokio::test]
    async fn test_replay_buffer() {
        let engine = FilterEngine::new(10, true, 5);
        
        // Add some events to replay buffer
        for i in 1..=10 {
            let event = events::block_finalized(i, [i as u8; 32]);
            timeout(Duration::from_secs(2), engine.process_event(event))
                .await
                .expect("process_event timed out")
                .unwrap();
        }
        
        let stats = engine.get_stats();
        assert!(stats.replay_buffer_blocks <= 5); // Should be limited by buffer size
    }

    #[tokio::test]
    async fn test_historical_events() {
        let engine = FilterEngine::new(10, true, 100);
        
        // Add some historical events
        for i in 1..=5 {
            let event = events::transaction_confirmed(
                i,
                [i as u8; 32],
                [2u8; 21],
                Some([3u8; 21]),
                1000 * i,
                21000,
            );
            timeout(Duration::from_secs(2), engine.process_event(event))
                .await
                .expect("process_event timed out")
                .unwrap();
        }
        
        // Query historical events
        let filter = EventFilter::new()
            .with_event_types(vec![EventType::TransactionConfirmed])
            .with_block_range(Some(2), Some(4));
        
        let historical = timeout(Duration::from_secs(2), engine.get_historical_events(&filter, Some(10)))
            .await
            .expect("get_historical_events timed out")
            .unwrap();
        
        // Should get events from blocks 2, 3, 4
        assert_eq!(historical.len(), 3);
        assert!(historical.iter().all(|e| e.block_number >= 2 && e.block_number <= 4));
    }
}