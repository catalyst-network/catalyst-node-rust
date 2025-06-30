//! Event filtering engine for the Service Bus

use crate::events::{BlockchainEvent, EventFilter, EventSubscription};
use catalyst_utils::{CatalystResult, CatalystError, logging::LogCategory};
use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;
use uuid::Uuid;

/// Maximum number of events to keep in replay buffer
const MAX_REPLAY_BUFFER_SIZE: usize = 10000;

/// Event filtering and distribution engine
#[derive(Debug)]
pub struct FilterEngine {
    /// Active subscriptions by ID
    subscriptions: Arc<DashMap<Uuid, EventSubscription>>,
    
    /// Event replay buffer
    replay_buffer: Arc<DashMap<u64, Vec<BlockchainEvent>>>, // block_number -> events
    
    /// Event broadcasters by subscription ID
    broadcasters: Arc<DashMap<Uuid, broadcast::Sender<BlockchainEvent>>>,
    
    /// Configuration
    max_filters_per_connection: usize,
    enable_replay: bool,
    replay_buffer_size: usize,
    
    /// Connection to subscription mapping
    connection_subscriptions: Arc<DashMap<String, Vec<Uuid>>>, // connection_id -> subscription_ids
}

impl FilterEngine {
    /// Create a new filter engine
    pub fn new(
        max_filters_per_connection: usize,
        enable_replay: bool,
        replay_buffer_size: usize,
    ) -> Self {
        Self {
            subscriptions: Arc::new(DashMap::new()),
            replay_buffer: Arc::new(DashMap::new()),
            broadcasters: Arc::new(DashMap::new()),
            max_filters_per_connection,
            enable_replay,
            replay_buffer_size: replay_buffer_size.min(MAX_REPLAY_BUFFER_SIZE),
            connection_subscriptions: Arc::new(DashMap::new()),
        }
    }
    
    /// Add a new event subscription
    pub fn add_subscription(
        &self,
        connection_id: String,
        filter: EventFilter,
    ) -> CatalystResult<(Uuid, broadcast::Receiver<BlockchainEvent>)> {
        // Check connection filter limit
        let mut connection_subs = self.connection_subscriptions.entry(connection_id.clone())
            .or_insert_with(Vec::new);
        
        if connection_subs.len() >= self.max_filters_per_connection {
            return Err(CatalystError::Invalid(format!(
                "Connection {} has reached maximum filter limit of {}",
                connection_id, self.max_filters_per_connection
            )));
        }
        
        let subscription = EventSubscription::new(filter);
        let subscription_id = subscription.id;
        
        // Create broadcast channel for this subscription
        let (tx, rx) = broadcast::channel(1000);
        
        // Store subscription and broadcaster
        self.subscriptions.insert(subscription_id, subscription);
        self.broadcasters.insert(subscription_id, tx);
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
        self.subscriptions.remove(&subscription_id);
        self.broadcasters.remove(&subscription_id);
        
        // Remove from connection tracking
        if let Some(mut connection_subs) = self.connection_subscriptions.get_mut(connection_id) {
            connection_subs.retain(|&id| id != subscription_id);
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
        if let Some((_, subscription_ids)) = self.connection_subscriptions.remove(connection_id) {
            for subscription_id in subscription_ids {
                self.subscriptions.remove(&subscription_id);
                self.broadcasters.remove(&subscription_id);
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
            self.add_to_replay_buffer(&event).await?;
        }
        
        // Find matching subscriptions and broadcast event
        let mut matched_count = 0;
        
        for subscription_ref in self.subscriptions.iter() {
            let subscription = subscription_ref.value();
            
            if subscription.matches(&event) {
                if let Some(broadcaster) = self.broadcasters.get(&subscription.id) {
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
        for entry in self.replay_buffer.iter() {
            let block_number = *entry.key();
            
            if block_number >= from_block && block_number <= to_block {
                for event in entry.value().iter() {
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
    async fn add_to_replay_buffer(&self, event: &BlockchainEvent) -> CatalystResult<()> {
        let block_number = event.block_number;
        
        // Add event to the appropriate block bucket
        let mut block_events = self.replay_buffer.entry(block_number).or_insert_with(Vec::new);
        block_events.push(event.clone());
        
        // Maintain buffer size - remove old blocks if needed
        if self.replay_buffer.len() > self.replay_buffer_size {
            // Find oldest block and remove it
            if let Some(oldest_block) = self.replay_buffer.iter().map(|entry| *entry.key()).min() {
                self.replay_buffer.remove(&oldest_block);
            }
        }
        
        Ok(())
    }
    
    /// Get subscription statistics
    pub fn get_stats(&self) -> FilterEngineStats {
        let total_subscriptions = self.subscriptions.len();
        let total_connections = self.connection_subscriptions.len();
        let replay_buffer_blocks = self.replay_buffer.len();
        let replay_buffer_events: usize = self.replay_buffer.iter()
            .map(|entry| entry.value().len())
            .sum();
        
        FilterEngineStats {
            total_subscriptions,
            total_connections,
            replay_buffer_blocks,
            replay_buffer_events,
        }
    }
    
    /// Get active subscriptions for a connection
    pub fn get_connection_subscriptions(&self, connection_id: &str) -> Vec<EventSubscription> {
        if let Some(subscription_ids) = self.connection_subscriptions.get(connection_id) {
            subscription_ids
                .iter()
                .filter_map(|&id| self.subscriptions.get(&id).map(|sub| sub.clone()))
                .collect()
        } else {
            Vec::new()
        }
    }
    
    /// Update subscription filter
    pub fn update_subscription_filter(
        &self,
        subscription_id: Uuid,
        new_filter: EventFilter,
    ) -> CatalystResult<()> {
        if let Some(mut subscription) = self.subscriptions.get_mut(&subscription_id) {
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
#[derive(Debug, Clone)]
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
    use tokio::time::{sleep, Duration};

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
        
        engine.process_event(event.clone()).await.unwrap();
        
        // Should receive the event
        let received_event = rx.recv().await.unwrap();
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
            engine.process_event(event).await.unwrap();
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
            engine.process_event(event).await.unwrap();
        }
        
        // Query historical events
        let filter = EventFilter::new()
            .with_event_types(vec![EventType::TransactionConfirmed])
            .with_block_range(Some(2), Some(4));
        
        let historical = engine.get_historical_events(&filter, Some(10)).await.unwrap();
        
        // Should get events from blocks 2, 3, 4
        assert_eq!(historical.len(), 3);
        assert!(historical.iter().all(|e| e.block_number >= 2 && e.block_number <= 4));
    }
}