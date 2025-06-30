//! Blockchain event definitions and processing for the Service Bus

use catalyst_utils::{CatalystResult, Hash, Address, utils};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Types of blockchain events that can be streamed
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    /// New block added to the chain
    BlockFinalized,
    
    /// Transaction included in a block
    TransactionConfirmed,
    
    /// Transaction added to mempool
    TransactionPending,
    
    /// Smart contract event
    ContractEvent,
    
    /// Token transfer
    TokenTransfer,
    
    /// Account balance change
    BalanceUpdate,
    
    /// Consensus phase change
    ConsensusPhase,
    
    /// New peer connected
    PeerConnected,
    
    /// Peer disconnected
    PeerDisconnected,
    
    /// Network statistics update
    NetworkStats,
    
    /// Custom application event
    Custom(String),
}

impl EventType {
    /// Check if this event type is a transaction-related event
    pub fn is_transaction_event(&self) -> bool {
        matches!(self, EventType::TransactionConfirmed | EventType::TransactionPending)
    }
    
    /// Check if this event type is a network-related event
    pub fn is_network_event(&self) -> bool {
        matches!(self, EventType::PeerConnected | EventType::PeerDisconnected | EventType::NetworkStats)
    }
    
    /// Check if this event type is a consensus-related event
    pub fn is_consensus_event(&self) -> bool {
        matches!(self, EventType::BlockFinalized | EventType::ConsensusPhase)
    }
    
    /// Check if this event type is a contract-related event
    pub fn is_contract_event(&self) -> bool {
        matches!(self, EventType::ContractEvent | EventType::TokenTransfer)
    }
    
    /// Get the string representation of the event type
    pub fn as_str(&self) -> &str {
        match self {
            EventType::BlockFinalized => "BlockFinalized",
            EventType::TransactionConfirmed => "TransactionConfirmed", 
            EventType::TransactionPending => "TransactionPending",
            EventType::ContractEvent => "ContractEvent",
            EventType::TokenTransfer => "TokenTransfer",
            EventType::BalanceUpdate => "BalanceUpdate",
            EventType::ConsensusPhase => "ConsensusPhase",
            EventType::PeerConnected => "PeerConnected",
            EventType::PeerDisconnected => "PeerDisconnected",
            EventType::NetworkStats => "NetworkStats",
            EventType::Custom(name) => name,
        }
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A blockchain event with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockchainEvent {
    /// Unique event ID
    pub id: Uuid,
    
    /// Event type
    pub event_type: EventType,
    
    /// Block number when the event occurred
    pub block_number: u64,
    
    /// Transaction hash (if applicable)
    pub transaction_hash: Option<Hash>,
    
    /// Contract address (if applicable)
    pub contract_address: Option<Address>,
    
    /// Account addresses involved
    pub addresses: Vec<Address>,
    
    /// Event data (JSON)
    pub data: serde_json::Value,
    
    /// Event metadata
    pub metadata: EventMetadata,
    
    /// Timestamp when the event occurred
    pub timestamp: u64,
    
    /// Topics for filtering (similar to Ethereum logs)
    pub topics: Vec<String>,
}

impl BlockchainEvent {
    /// Create a new blockchain event
    pub fn new(
        event_type: EventType,
        block_number: u64,
        data: serde_json::Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            event_type,
            block_number,
            transaction_hash: None,
            contract_address: None,
            addresses: Vec::new(),
            data,
            metadata: EventMetadata::default(),
            timestamp: utils::current_timestamp(),
            topics: Vec::new(),
        }
    }
    
    /// Add a transaction hash to the event
    pub fn with_transaction(mut self, tx_hash: Hash) -> Self {
        self.transaction_hash = Some(tx_hash);
        self
    }
    
    /// Add a contract address to the event
    pub fn with_contract(mut self, contract_addr: Address) -> Self {
        self.contract_address = Some(contract_addr);
        self
    }
    
    /// Add addresses to the event
    pub fn with_addresses(mut self, addresses: Vec<Address>) -> Self {
        self.addresses = addresses;
        self
    }
    
    /// Add topics for filtering
    pub fn with_topics(mut self, topics: Vec<String>) -> Self {
        self.topics = topics;
        self
    }
    
    /// Add metadata to the event
    pub fn with_metadata(mut self, metadata: EventMetadata) -> Self {
        self.metadata = metadata;
        self
    }
    
    /// Add custom data field
    pub fn with_custom_data(mut self, key: &str, value: serde_json::Value) -> Self {
        if let serde_json::Value::Object(ref mut map) = self.data {
            map.insert(key.to_string(), value);
        }
        self
    }
    
    /// Set the timestamp
    pub fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }
    
    /// Check if this event matches the given filter
    pub fn matches_filter(&self, filter: &EventFilter) -> bool {
        // Check event type
        if let Some(ref types) = filter.event_types {
            if !types.contains(&self.event_type) {
                return false;
            }
        }
        
        // Check addresses
        if let Some(ref filter_addresses) = filter.addresses {
            if !self.addresses.iter().any(|addr| filter_addresses.contains(addr)) {
                return false;
            }
        }
        
        // Check contract address
        if let Some(ref contracts) = filter.contracts {
            if let Some(contract_addr) = self.contract_address {
                if !contracts.contains(&contract_addr) {
                    return false;
                }
            } else {
                return false;
            }
        }
        
        // Check topics
        if let Some(ref filter_topics) = filter.topics {
            if !self.topics.iter().any(|topic| filter_topics.contains(topic)) {
                return false;
            }
        }
        
        // Check block range
        if let Some(from_block) = filter.from_block {
            if self.block_number < from_block {
                return false;
            }
        }
        
        if let Some(to_block) = filter.to_block {
            if self.block_number > to_block {
                return false;
            }
        }
        
        // Check timestamp range
        if let Some(from_time) = filter.from_timestamp {
            if self.timestamp < from_time {
                return false;
            }
        }
        
        if let Some(to_time) = filter.to_timestamp {
            if self.timestamp > to_time {
                return false;
            }
        }
        
        true
    }
    
    /// Get the event severity level
    pub fn severity(&self) -> EventSeverity {
        match &self.event_type {
            EventType::TransactionConfirmed | EventType::TokenTransfer => EventSeverity::Info,
            EventType::TransactionPending => EventSeverity::Debug,
            EventType::BlockFinalized => EventSeverity::Info,
            EventType::ContractEvent => EventSeverity::Info,
            EventType::BalanceUpdate => EventSeverity::Info,
            EventType::ConsensusPhase => EventSeverity::Info,
            EventType::PeerConnected | EventType::PeerDisconnected => EventSeverity::Debug,
            EventType::NetworkStats => EventSeverity::Debug,
            EventType::Custom(_) => EventSeverity::Info,
        }
    }
    
    /// Check if this is a high-value transaction (for alerting)
    pub fn is_high_value(&self, threshold: u64) -> bool {
        if let Some(amount) = self.data.get("amount") {
            if let Some(amount_val) = amount.as_u64() {
                return amount_val >= threshold;
            }
        }
        false
    }
    
    /// Extract the primary address involved in this event
    pub fn primary_address(&self) -> Option<Address> {
        match &self.event_type {
            EventType::TokenTransfer | EventType::TransactionConfirmed => {
                // For transfers, return the 'from' address
                self.addresses.first().copied()
            }
            EventType::ContractEvent => {
                self.contract_address
            }
            EventType::BalanceUpdate => {
                self.addresses.first().copied()
            }
            _ => None,
        }
    }
}

/// Event severity levels
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventSeverity {
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

/// Event metadata for additional context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMetadata {
    /// Gas used (for transaction events)
    pub gas_used: Option<u64>,
    
    /// Gas price (for transaction events)
    pub gas_price: Option<u64>,
    
    /// Transaction status
    pub status: Option<TransactionStatus>,
    
    /// Log index (for contract events)
    pub log_index: Option<u64>,
    
    /// Transaction index in block
    pub transaction_index: Option<u64>,
    
    /// Block hash
    pub block_hash: Option<Hash>,
    
    /// Network fee paid
    pub network_fee: Option<u64>,
    
    /// Additional custom fields
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for EventMetadata {
    fn default() -> Self {
        Self {
            gas_used: None,
            gas_price: None,
            status: None,
            log_index: None,
            transaction_index: None,
            block_hash: None,
            network_fee: None,
            custom: HashMap::new(),
        }
    }
}

impl EventMetadata {
    /// Add a custom field to the metadata
    pub fn with_custom(mut self, key: &str, value: serde_json::Value) -> Self {
        self.custom.insert(key.to_string(), value);
        self
    }
    
    /// Calculate the total transaction cost
    pub fn total_cost(&self) -> Option<u64> {
        match (self.gas_used, self.gas_price) {
            (Some(gas), Some(price)) => Some(gas * price),
            _ => self.network_fee,
        }
    }
}

/// Transaction status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransactionStatus {
    /// Transaction executed successfully
    Success,
    
    /// Transaction failed
    Failed,
    
    /// Transaction reverted
    Reverted,
    
    /// Transaction is pending
    Pending,
    
    /// Transaction was dropped
    Dropped,
    
    /// Transaction was replaced
    Replaced,
}

impl TransactionStatus {
    /// Check if the transaction was successful
    pub fn is_success(&self) -> bool {
        matches!(self, TransactionStatus::Success)
    }
    
    /// Check if the transaction failed
    pub fn is_failure(&self) -> bool {
        matches!(self, TransactionStatus::Failed | TransactionStatus::Reverted)
    }
    
    /// Check if the transaction is in a final state
    pub fn is_final(&self) -> bool {
        !matches!(self, TransactionStatus::Pending)
    }
}

/// Event filter for subscribing to specific events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    /// Filter by event types
    pub event_types: Option<Vec<EventType>>,
    
    /// Filter by addresses involved
    pub addresses: Option<Vec<Address>>,
    
    /// Filter by contract addresses
    pub contracts: Option<Vec<Address>>,
    
    /// Filter by topics
    pub topics: Option<Vec<String>>,
    
    /// Filter by block range (from)
    pub from_block: Option<u64>,
    
    /// Filter by block range (to)
    pub to_block: Option<u64>,
    
    /// Filter by timestamp range (from)
    pub from_timestamp: Option<u64>,
    
    /// Filter by timestamp range (to)
    pub to_timestamp: Option<u64>,
    
    /// Limit number of results
    pub limit: Option<usize>,
    
    /// Filter by transaction status
    pub status: Option<Vec<TransactionStatus>>,
    
    /// Minimum amount filter (for value transfers)
    pub min_amount: Option<u64>,
    
    /// Maximum amount filter (for value transfers)
    pub max_amount: Option<u64>,
    
    /// Include only events with specific custom fields
    pub custom_filters: Option<HashMap<String, serde_json::Value>>,
}

impl EventFilter {
    /// Create a new empty filter
    pub fn new() -> Self {
        Self {
            event_types: None,
            addresses: None,
            contracts: None,
            topics: None,
            from_block: None,
            to_block: None,
            from_timestamp: None,
            to_timestamp: None,
            limit: None,
            status: None,
            min_amount: None,
            max_amount: None,
            custom_filters: None,
        }
    }
    
    /// Filter by event types
    pub fn with_event_types(mut self, types: Vec<EventType>) -> Self {
        self.event_types = Some(types);
        self
    }
    
    /// Filter by addresses
    pub fn with_addresses(mut self, addresses: Vec<Address>) -> Self {
        self.addresses = Some(addresses);
        self
    }
    
    /// Filter by contract addresses
    pub fn with_contracts(mut self, contracts: Vec<Address>) -> Self {
        self.contracts = Some(contracts);
        self
    }
    
    /// Filter by topics
    pub fn with_topics(mut self, topics: Vec<String>) -> Self {
        self.topics = Some(topics);
        self
    }
    
    /// Filter by block range
    pub fn with_block_range(mut self, from: Option<u64>, to: Option<u64>) -> Self {
        self.from_block = from;
        self.to_block = to;
        self
    }
    
    /// Filter by timestamp range
    pub fn with_timestamp_range(mut self, from: Option<u64>, to: Option<u64>) -> Self {
        self.from_timestamp = from;
        self.to_timestamp = to;
        self
    }
    
    /// Limit number of results
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
    
    /// Filter by transaction status
    pub fn with_status(mut self, status: Vec<TransactionStatus>) -> Self {
        self.status = Some(status);
        self
    }
    
    /// Filter by amount range
    pub fn with_amount_range(mut self, min: Option<u64>, max: Option<u64>) -> Self {
        self.min_amount = min;
        self.max_amount = max;
        self
    }
    
    /// Add custom field filter
    pub fn with_custom_filter(mut self, key: &str, value: serde_json::Value) -> Self {
        self.custom_filters.get_or_insert_with(HashMap::new)
            .insert(key.to_string(), value);
        self
    }
    
    /// Check if this is an empty filter (matches everything)
    pub fn is_empty(&self) -> bool {
        self.event_types.is_none()
            && self.addresses.is_none()
            && self.contracts.is_none()
            && self.topics.is_none()
            && self.from_block.is_none()
            && self.to_block.is_none()
            && self.from_timestamp.is_none()
            && self.to_timestamp.is_none()
            && self.status.is_none()
            && self.min_amount.is_none()
            && self.max_amount.is_none()
            && self.custom_filters.is_none()
    }
    
    /// Validate the filter for correctness
    pub fn validate(&self) -> CatalystResult<()> {
        // Validate block range
        if let (Some(from), Some(to)) = (self.from_block, self.to_block) {
            if from > to {
                return Err(catalyst_utils::CatalystError::Invalid(
                    "from_block cannot be greater than to_block".to_string()
                ));
            }
        }
        
        // Validate timestamp range
        if let (Some(from), Some(to)) = (self.from_timestamp, self.to_timestamp) {
            if from > to {
                return Err(catalyst_utils::CatalystError::Invalid(
                    "from_timestamp cannot be greater than to_timestamp".to_string()
                ));
            }
        }
        
        // Validate amount range
        if let (Some(min), Some(max)) = (self.min_amount, self.max_amount) {
            if min > max {
                return Err(catalyst_utils::CatalystError::Invalid(
                    "min_amount cannot be greater than max_amount".to_string()
                ));
            }
        }
        
        // Validate limit
        if let Some(limit) = self.limit {
            if limit == 0 {
                return Err(catalyst_utils::CatalystError::Invalid(
                    "limit cannot be zero".to_string()
                ));
            }
            if limit > 10000 {
                return Err(catalyst_utils::CatalystError::Invalid(
                    "limit cannot exceed 10000".to_string()
                ));
            }
        }
        
        Ok(())
    }
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Event subscription handle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSubscription {
    /// Subscription ID
    pub id: Uuid,
    
    /// Event filter
    pub filter: EventFilter,
    
    /// When the subscription was created
    pub created_at: u64,
    
    /// Whether the subscription is active
    pub active: bool,
    
    /// Connection ID that owns this subscription
    pub connection_id: String,
    
    /// Last event processed timestamp
    pub last_event_timestamp: Option<u64>,
    
    /// Number of events processed
    pub events_processed: u64,
}

impl EventSubscription {
    /// Create a new event subscription
    pub fn new(filter: EventFilter, connection_id: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            filter,
            created_at: utils::current_timestamp(),
            active: true,
            connection_id,
            last_event_timestamp: None,
            events_processed: 0,
        }
    }
    
    /// Check if an event matches this subscription
    pub fn matches(&self, event: &BlockchainEvent) -> bool {
        self.active && event.matches_filter(&self.filter)
    }
    
    /// Update subscription statistics
    pub fn update_stats(&mut self, event: &BlockchainEvent) {
        self.last_event_timestamp = Some(event.timestamp);
        self.events_processed += 1;
    }
    
    /// Deactivate the subscription
    pub fn deactivate(&mut self) {
        self.active = false;
    }
}

/// Helper functions for creating common event types
pub mod events {
    use super::*;
    
    /// Create a block finalized event
    pub fn block_finalized(block_number: u64, block_hash: Hash) -> BlockchainEvent {
        let mut data = serde_json::Map::new();
        data.insert("block_hash".to_string(), 
                   serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&block_hash)));
        data.insert("block_number".to_string(), 
                   serde_json::Value::Number(block_number.into()));
        
        let metadata = EventMetadata {
            block_hash: Some(block_hash),
            ..Default::default()
        };
        
        BlockchainEvent::new(
            EventType::BlockFinalized,
            block_number,
            serde_json::Value::Object(data),
        ).with_metadata(metadata)
         .with_topics(vec!["block".to_string(), "finalized".to_string()])
    }
    
    /// Create a transaction confirmed event
    pub fn transaction_confirmed(
        block_number: u64,
        tx_hash: Hash,
        from: Address,
        to: Option<Address>,
        amount: u64,
        gas_used: u64,
    ) -> BlockchainEvent {
        let mut data = serde_json::Map::new();
        data.insert("from".to_string(), 
                   serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&from)));
        if let Some(to_addr) = to {
            data.insert("to".to_string(), 
                       serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&to_addr)));
        }
        data.insert("amount".to_string(), serde_json::Value::Number(amount.into()));
        data.insert("gas_used".to_string(), serde_json::Value::Number(gas_used.into()));
        
        let mut addresses = vec![from];
        if let Some(to_addr) = to {
            addresses.push(to_addr);
        }
        
        let metadata = EventMetadata {
            gas_used: Some(gas_used),
            status: Some(TransactionStatus::Success),
            ..Default::default()
        };
        
        BlockchainEvent::new(
            EventType::TransactionConfirmed,
            block_number,
            serde_json::Value::Object(data),
        )
        .with_transaction(tx_hash)
        .with_addresses(addresses)
        .with_metadata(metadata)
        .with_topics(vec!["transaction".to_string(), "confirmed".to_string()])
    }
    
    /// Create a transaction pending event
    pub fn transaction_pending(
        tx_hash: Hash,
        from: Address,
        to: Option<Address>,
        amount: u64,
        gas_limit: u64,
    ) -> BlockchainEvent {
        let mut data = serde_json::Map::new();
        data.insert("from".to_string(), 
                   serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&from)));
        if let Some(to_addr) = to {
            data.insert("to".to_string(), 
                       serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&to_addr)));
        }
        data.insert("amount".to_string(), serde_json::Value::Number(amount.into()));
        data.insert("gas_limit".to_string(), serde_json::Value::Number(gas_limit.into()));
        
        let mut addresses = vec![from];
        if let Some(to_addr) = to {
            addresses.push(to_addr);
        }
        
        let metadata = EventMetadata {
            status: Some(TransactionStatus::Pending),
            ..Default::default()
        };
        
        BlockchainEvent::new(
            EventType::TransactionPending,
            0, // Pending transactions don't have a block number yet
            serde_json::Value::Object(data),
        )
        .with_transaction(tx_hash)
        .with_addresses(addresses)
        .with_metadata(metadata)
        .with_topics(vec!["transaction".to_string(), "pending".to_string()])
    }
    
    /// Create a token transfer event
    pub fn token_transfer(
        block_number: u64,
        tx_hash: Hash,
        contract_address: Address,
        from: Address,
        to: Address,
        amount: u64,
        token_symbol: Option<String>,
    ) -> BlockchainEvent {
        let mut data = serde_json::Map::new();
        data.insert("from".to_string(), 
                   serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&from)));
        data.insert("to".to_string(), 
                   serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&to)));
        data.insert("amount".to_string(), serde_json::Value::Number(amount.into()));
        data.insert("contract".to_string(), 
                   serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&contract_address)));
        
        if let Some(symbol) = token_symbol {
            data.insert("symbol".to_string(), serde_json::Value::String(symbol));
        }
        
        BlockchainEvent::new(
            EventType::TokenTransfer,
            block_number,
            serde_json::Value::Object(data),
        )
        .with_transaction(tx_hash)
        .with_contract(contract_address)
        .with_addresses(vec![from, to])
        .with_topics(vec!["Transfer".to_string(), "token".to_string()])
    }
    
    /// Create a contract event
    pub fn contract_event(
        block_number: u64,
        tx_hash: Hash,
        contract_address: Address,
        event_name: &str,
        event_data: serde_json::Value,
        log_index: u64,
    ) -> BlockchainEvent {
        let mut data = serde_json::Map::new();
        data.insert("event_name".to_string(), serde_json::Value::String(event_name.to_string()));
        data.insert("contract".to_string(), 
                   serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&contract_address)));
        
        // Merge event data
        if let serde_json::Value::Object(event_map) = event_data {
            for (key, value) in event_map {
                data.insert(key, value);
            }
        }
        
        let metadata = EventMetadata {
            log_index: Some(log_index),
            status: Some(TransactionStatus::Success),
            ..Default::default()
        };
        
        BlockchainEvent::new(
            EventType::ContractEvent,
            block_number,
            serde_json::Value::Object(data),
        )
        .with_transaction(tx_hash)
        .with_contract(contract_address)
        .with_metadata(metadata)
        .with_topics(vec!["contract".to_string(), event_name.to_lowercase()])
    }
    
    /// Create a balance update event
    pub fn balance_update(
        block_number: u64,
        address: Address,
        old_balance: u64,
        new_balance: u64,
        reason: &str,
    ) -> BlockchainEvent {
        let mut data = serde_json::Map::new();
        data.insert("address".to_string(), 
                   serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&address)));
        data.insert("old_balance".to_string(), serde_json::Value::Number(old_balance.into()));
        data.insert("new_balance".to_string(), serde_json::Value::Number(new_balance.into()));
        data.insert("change".to_string(), 
                   serde_json::Value::Number((new_balance as i64 - old_balance as i64).into()));
        data.insert("reason".to_string(), serde_json::Value::String(reason.to_string()));
        
        BlockchainEvent::new(
            EventType::BalanceUpdate,
            block_number,
            serde_json::Value::Object(data),
        )
        .with_addresses(vec![address])
        .with_topics(vec!["balance".to_string(), "update".to_string()])
    }
    
    /// Create a consensus phase event
    pub fn consensus_phase(phase: &str, cycle: u64, producer: Option<String>) -> BlockchainEvent {
        let mut data = serde_json::Map::new();
        data.insert("phase".to_string(), serde_json::Value::String(phase.to_string()));
        data.insert("cycle".to_string(), serde_json::Value::Number(cycle.into()));
        
        if let Some(producer_id) = producer {
            data.insert("producer".to_string(), serde_json::Value::String(producer_id));
        }
        
        BlockchainEvent::new(
            EventType::ConsensusPhase,
            0, // Consensus events don't have a specific block number
            serde_json::Value::Object(data),
        ).with_topics(vec![format!("consensus.{}", phase.to_lowercase()), "consensus".to_string()])
    }
    
    /// Create a peer connected event
    pub fn peer_connected(peer_id: &str, ip_address: &str, port: u16) -> BlockchainEvent {
        let mut data = serde_json::Map::new();
        data.insert("peer_id".to_string(), serde_json::Value::String(peer_id.to_string()));
        data.insert("ip_address".to_string(), serde_json::Value::String(ip_address.to_string()));
        data.insert("port".to_string(), serde_json::Value::Number(port.into()));
        
        BlockchainEvent::new(
            EventType::PeerConnected,
            0,
            serde_json::Value::Object(data),
        ).with_topics(vec!["network".to_string(), "peer".to_string(), "connected".to_string()])
    }
    
    /// Create a peer disconnected event
    pub fn peer_disconnected(peer_id: &str, reason: &str) -> BlockchainEvent {
        let mut data = serde_json::Map::new();
        data.insert("peer_id".to_string(), serde_json::Value::String(peer_id.to_string()));
        data.insert("reason".to_string(), serde_json::Value::String(reason.to_string()));
        
        BlockchainEvent::new(
            EventType::PeerDisconnected,
            0,
            serde_json::Value::Object(data),
        ).with_topics(vec!["network".to_string(), "peer".to_string(), "disconnected".to_string()])
    }
    
    /// Create a network statistics event
    pub fn network_stats(
        peer_count: u32,
        total_supply: u64,
        circulating_supply: u64,
        hash_rate: u64,
    ) -> BlockchainEvent {
        let mut data = serde_json::Map::new();
        data.insert("peer_count".to_string(), serde_json::Value::Number(peer_count.into()));
        data.insert("total_supply".to_string(), serde_json::Value::Number(total_supply.into()));
        data.insert("circulating_supply".to_string(), serde_json::Value::Number(circulating_supply.into()));
        data.insert("hash_rate".to_string(), serde_json::Value::Number(hash_rate.into()));
        
        BlockchainEvent::new(
            EventType::NetworkStats,
            0,
            serde_json::Value::Object(data),
        ).with_topics(vec!["network".to_string(), "statistics".to_string()])
    }
    
    /// Create a custom event
    pub fn custom_event(
        event_name: &str,
        block_number: u64,
        data: serde_json::Value,
        topics: Vec<String>,
    ) -> BlockchainEvent {
        BlockchainEvent::new(
            EventType::Custom(event_name.to_string()),
            block_number,
            data,
        ).with_topics(topics)
    }
    
    /// Create a failed transaction event
    pub fn transaction_failed(
        block_number: u64,
        tx_hash: Hash,
        from: Address,
        error_message: &str,
        gas_used: u64,
    ) -> BlockchainEvent {
        let mut data = serde_json::Map::new();
        data.insert("from".to_string(), 
                   serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&from)));
        data.insert("error".to_string(), serde_json::Value::String(error_message.to_string()));
        data.insert("gas_used".to_string(), serde_json::Value::Number(gas_used.into()));
        
        let metadata = EventMetadata {
            gas_used: Some(gas_used),
            status: Some(TransactionStatus::Failed),
            ..Default::default()
        };
        
        BlockchainEvent::new(
            EventType::TransactionConfirmed,
            block_number,
            serde_json::Value::Object(data),
        )
        .with_transaction(tx_hash)
        .with_addresses(vec![from])
        .with_metadata(metadata)
        .with_topics(vec!["transaction".to_string(), "failed".to_string()])
    }
    
    /// Create a contract deployment event
    pub fn contract_deployed(
        block_number: u64,
        tx_hash: Hash,
        contract_address: Address,
        deployer: Address,
        contract_name: Option<String>,
    ) -> BlockchainEvent {
        let mut data = serde_json::Map::new();
        data.insert("contract".to_string(), 
                   serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&contract_address)));
        data.insert("deployer".to_string(), 
                   serde_json::Value::String(catalyst_utils::utils::bytes_to_hex(&deployer)));
        
        if let Some(name) = contract_name {
            data.insert("contract_name".to_string(), serde_json::Value::String(name));
        }
        
        let metadata = EventMetadata {
            status: Some(TransactionStatus::Success),
            ..Default::default()
        };
        
        BlockchainEvent::new(
            EventType::ContractEvent,
            block_number,
            serde_json::Value::Object(data),
        )
        .with_transaction(tx_hash)
        .with_contract(contract_address)
        .with_addresses(vec![deployer])
        .with_metadata(metadata)
        .with_topics(vec!["contract".to_string(), "deployed".to_string()])
    }
}

/// Event aggregation and statistics
pub mod analytics {
    use super::*;
    use std::collections::BTreeMap;
    
    /// Event statistics for a time period
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EventStats {
        /// Time period start
        pub period_start: u64,
        
        /// Time period end
        pub period_end: u64,
        
        /// Total events processed
        pub total_events: u64,
        
        /// Events by type
        pub events_by_type: HashMap<String, u64>,
        
        /// Transaction volume
        pub transaction_volume: u64,
        
        /// Unique addresses active
        pub active_addresses: u64,
        
        /// Average gas used
        pub avg_gas_used: f64,
        
        /// Failed transaction rate
        pub failed_tx_rate: f64,
    }
    
    /// Event aggregator for analytics
    pub struct EventAggregator {
        /// Events grouped by time buckets
        events: BTreeMap<u64, Vec<BlockchainEvent>>,
        
        /// Bucket size in seconds
        bucket_size: u64,
        
        /// Maximum number of buckets to keep
        max_buckets: usize,
    }
    
    impl EventAggregator {
        /// Create a new event aggregator
        pub fn new(bucket_size_seconds: u64, max_buckets: usize) -> Self {
            Self {
                events: BTreeMap::new(),
                bucket_size: bucket_size_seconds,
                max_buckets,
            }
        }
        
        /// Add an event to the aggregator
        pub fn add_event(&mut self, event: BlockchainEvent) {
            let bucket = (event.timestamp / self.bucket_size) * self.bucket_size;
            self.events.entry(bucket).or_insert_with(Vec::new).push(event);
            
            // Remove old buckets if we exceed the limit
            while self.events.len() > self.max_buckets {
                if let Some(oldest_bucket) = self.events.keys().next().copied() {
                    self.events.remove(&oldest_bucket);
                }
            }
        }
        
        /// Get statistics for a time period
        pub fn get_stats(&self, from: u64, to: u64) -> EventStats {
            let mut total_events = 0u64;
            let mut events_by_type = HashMap::new();
            let mut transaction_volume = 0u64;
            let mut active_addresses = std::collections::HashSet::new();
            let mut total_gas = 0u64;
            let mut gas_count = 0u64;
            let mut failed_txs = 0u64;
            let mut total_txs = 0u64;
            
            for (bucket_time, events) in &self.events {
                if *bucket_time >= from && *bucket_time <= to {
                    for event in events {
                        total_events += 1;
                        
                        // Count by type
                        let type_name = event.event_type.as_str().to_string();
                        *events_by_type.entry(type_name).or_insert(0) += 1;
                        
                        // Track addresses
                        for addr in &event.addresses {
                            active_addresses.insert(*addr);
                        }
                        
                        // Transaction-specific stats
                        if event.event_type.is_transaction_event() {
                            total_txs += 1;
                            
                            if let Some(amount) = event.data.get("amount") {
                                if let Some(amount_val) = amount.as_u64() {
                                    transaction_volume += amount_val;
                                }
                            }
                            
                            if let Some(gas) = event.metadata.gas_used {
                                total_gas += gas;
                                gas_count += 1;
                            }
                            
                            if let Some(status) = &event.metadata.status {
                                if status.is_failure() {
                                    failed_txs += 1;
                                }
                            }
                        }
                    }
                }
            }
            
            let avg_gas_used = if gas_count > 0 {
                total_gas as f64 / gas_count as f64
            } else {
                0.0
            };
            
            let failed_tx_rate = if total_txs > 0 {
                failed_txs as f64 / total_txs as f64
            } else {
                0.0
            };
            
            EventStats {
                period_start: from,
                period_end: to,
                total_events,
                events_by_type,
                transaction_volume,
                active_addresses: active_addresses.len() as u64,
                avg_gas_used,
                failed_tx_rate,
            }
        }
        
        /// Get recent events
        pub fn get_recent_events(&self, count: usize) -> Vec<&BlockchainEvent> {
            let mut recent_events = Vec::new();
            
            for (_, events) in self.events.iter().rev() {
                for event in events.iter().rev() {
                    recent_events.push(event);
                    if recent_events.len() >= count {
                        break;
                    }
                }
                if recent_events.len() >= count {
                    break;
                }
            }
            
            recent_events
        }
    }
}

/// Event validation utilities
pub mod validation {
    use super::*;
    
    /// Validate an event for correctness
    pub fn validate_event(event: &BlockchainEvent) -> CatalystResult<()> {
        // Validate basic fields
        if event.id.is_nil() {
            return Err(catalyst_utils::CatalystError::Invalid("Event ID cannot be nil".to_string()));
        }
        
        // Validate timestamp
        let now = utils::current_timestamp();
        if event.timestamp > now + 300 { // Allow 5 minute clock skew
            return Err(catalyst_utils::CatalystError::Invalid("Event timestamp is too far in the future".to_string()));
        }
        
        // Validate event-specific data
        match &event.event_type {
            EventType::TransactionConfirmed | EventType::TransactionPending => {
                validate_transaction_event(event)?;
            }
            EventType::TokenTransfer => {
                validate_token_transfer_event(event)?;
            }
            EventType::ContractEvent => {
                validate_contract_event(event)?;
            }
            EventType::BlockFinalized => {
                validate_block_event(event)?;
            }
            _ => {
                // Other events have minimal validation requirements
            }
        }
        
        Ok(())
    }
    
    fn validate_transaction_event(event: &BlockchainEvent) -> CatalystResult<()> {
        if event.transaction_hash.is_none() {
            return Err(catalyst_utils::CatalystError::Invalid("Transaction events must have a transaction hash".to_string()));
        }
        
        if !event.data.get("from").is_some() {
            return Err(catalyst_utils::CatalystError::Invalid("Transaction events must have a 'from' address".to_string()));
        }
        
        if let Some(amount) = event.data.get("amount") {
            if amount.as_u64().is_none() {
                return Err(catalyst_utils::CatalystError::Invalid("Transaction amount must be a valid number".to_string()));
            }
        }
        
        Ok(())
    }
    
    fn validate_token_transfer_event(event: &BlockchainEvent) -> CatalystResult<()> {
        if event.contract_address.is_none() {
            return Err(catalyst_utils::CatalystError::Invalid("Token transfer events must have a contract address".to_string()));
        }
        
        if event.addresses.len() < 2 {
            return Err(catalyst_utils::CatalystError::Invalid("Token transfer events must have at least 2 addresses (from, to)".to_string()));
        }
        
        if !event.data.get("amount").is_some() {
            return Err(catalyst_utils::CatalystError::Invalid("Token transfer events must have an amount".to_string()));
        }
        
        Ok(())
    }
    
    fn validate_contract_event(event: &BlockchainEvent) -> CatalystResult<()> {
        if event.contract_address.is_none() {
            return Err(catalyst_utils::CatalystError::Invalid("Contract events must have a contract address".to_string()));
        }
        
        if let Some(log_index) = event.metadata.log_index {
            if log_index > 1000 {
                return Err(catalyst_utils::CatalystError::Invalid("Log index seems unreasonably high".to_string()));
            }
        }
        
        Ok(())
    }
    
    fn validate_block_event(event: &BlockchainEvent) -> CatalystResult<()> {
        if !event.data.get("block_hash").is_some() {
            return Err(catalyst_utils::CatalystError::Invalid("Block events must have a block hash".to_string()));
        }
        
        if event.block_number == 0 {
            return Err(catalyst_utils::CatalystError::Invalid("Block number cannot be zero".to_string()));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyst_utils::utils;

    #[test]
    fn test_event_creation() {
        let event = BlockchainEvent::new(
            EventType::TransactionConfirmed,
            100,
            serde_json::json!({"amount": 1000}),
        );
        
        assert_eq!(event.event_type, EventType::TransactionConfirmed);
        assert_eq!(event.block_number, 100);
        assert_eq!(event.data["amount"], 1000);
        assert!(!event.id.is_nil());
    }

    #[test]
    fn test_event_filter_matching() {
        let event = events::transaction_confirmed(
            100,
            [1u8; 32],
            [2u8; 21],
            Some([3u8; 21]),
            1000,
            21000,
        );
        
        // Filter by event type
        let filter = EventFilter::new()
            .with_event_types(vec![EventType::TransactionConfirmed]);
        assert!(event.matches_filter(&filter));
        
        // Filter by address
        let filter = EventFilter::new()
            .with_addresses(vec![[2u8; 21]]);
        assert!(event.matches_filter(&filter));
        
        // Filter that doesn't match
        let filter = EventFilter::new()
            .with_event_types(vec![EventType::BlockFinalized]);
        assert!(!event.matches_filter(&filter));
    }

    #[test]
    fn test_event_subscription() {
        let filter = EventFilter::new()
            .with_event_types(vec![EventType::TokenTransfer]);
        
        let subscription = EventSubscription::new(filter, "test_connection".to_string());
        assert!(subscription.active);
        assert_eq!(subscription.connection_id, "test_connection");
        
        let token_event = events::token_transfer(
            100,
            [1u8; 32],
            [4u8; 21],
            [2u8; 21],
            [3u8; 21],
            500,
            Some("KAT".to_string()),
        );
        
        assert!(subscription.matches(&token_event));
    }

    #[test]
    fn test_event_type_checks() {
        assert!(EventType::TransactionConfirmed.is_transaction_event());
        assert!(EventType::PeerConnected.is_network_event());
        assert!(EventType::BlockFinalized.is_consensus_event());
        assert!(EventType::TokenTransfer.is_contract_event());
        assert!(!EventType::NetworkStats.is_transaction_event());
    }

    #[test]
    fn test_filter_validation() {
        let mut filter = EventFilter::new();
        assert!(filter.validate().is_ok());
        
        // Invalid block range
        filter.from_block = Some(100);
        filter.to_block = Some(50);
        assert!(filter.validate().is_err());
        
        // Reset and test amount range
        filter.from_block = None;
        filter.to_block = None;
        filter.min_amount = Some(1000);
        filter.max_amount = Some(500);
        assert!(filter.validate().is_err());
    }

    #[test]
    fn test_transaction_status() {
        assert!(TransactionStatus::Success.is_success());
        assert!(TransactionStatus::Failed.is_failure());
        assert!(TransactionStatus::Reverted.is_failure());
        assert!(!TransactionStatus::Pending.is_final());
        assert!(TransactionStatus::Success.is_final());
    }

    #[test]
    fn test_event_helpers() {
        // Test block finalized event
        let block_event = events::block_finalized(123, [1u8; 32]);
        assert_eq!(block_event.event_type, EventType::BlockFinalized);
        assert_eq!(block_event.block_number, 123);
        assert!(block_event.topics.contains(&"block".to_string()));
        
        // Test token transfer event
        let token_event = events::token_transfer(
            456,
            [2u8; 32],
            [3u8; 21],
            [4u8; 21],
            [5u8; 21],
            1000,
            Some("KAT".to_string()),
        );
        assert_eq!(token_event.event_type, EventType::TokenTransfer);
        assert_eq!(token_event.addresses.len(), 2);
        assert_eq!(token_event.data["symbol"], "KAT");
    }

    #[test]
    fn test_event_aggregator() {
        use analytics::EventAggregator;
        
        let mut aggregator = EventAggregator::new(60, 100); // 1 minute buckets, 100 max
        
        let event1 = events::transaction_confirmed(1, [1u8; 32], [2u8; 21], Some([3u8; 21]), 1000, 21000);
        let event2 = events::token_transfer(2, [2u8; 32], [4u8; 21], [5u8; 21], [6u8; 21], 500, Some("KAT".to_string()));
        
        aggregator.add_event(event1);
        aggregator.add_event(event2);
        
        let stats = aggregator.get_stats(0, utils::current_timestamp() + 1000);
        assert_eq!(stats.total_events, 2);
        assert!(stats.events_by_type.contains_key("TransactionConfirmed"));
        assert!(stats.events_by_type.contains_key("TokenTransfer"));
    }

    #[test]
    fn test_event_validation() {
        use validation::validate_event;
        
        let valid_event = events::transaction_confirmed(
            100,
            [1u8; 32],
            [2u8; 21],
            Some([3u8; 21]),
            1000,
            21000,
        );
        assert!(validate_event(&valid_event).is_ok());
        
        // Test invalid event (missing transaction hash)
        let mut invalid_event = valid_event.clone();
        invalid_event.transaction_hash = None;
        assert!(validate_event(&invalid_event).is_err());
    }

    #[test]
    fn test_high_value_detection() {
        let high_value_event = events::transaction_confirmed(
            100,
            [1u8; 32],
            [2u8; 21],
            Some([3u8; 21]),
            10000,
            21000,
        );
        
        assert!(high_value_event.is_high_value(5000));
        assert!(!high_value_event.is_high_value(15000));
    }

    #[test]
    fn test_custom_filter() {
        let mut filter = EventFilter::new()
            .with_custom_filter("symbol", serde_json::Value::String("KAT".to_string()));
        
        assert!(filter.custom_filters.is_some());
        assert_eq!(
            filter.custom_filters.as_ref().unwrap().get("symbol"),
            Some(&serde_json::Value::String("KAT".to_string()))
        );
    }
}