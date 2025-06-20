use crate::{Address, TxHash, BlockHash, TokenAmount, Timestamp, NodeId, NodeRole};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;

/// Event system for the Catalyst network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    /// Transaction events
    Transaction(TransactionEvent),
    /// Block events
    Block(BlockEvent),
    /// Consensus events
    Consensus(ConsensusEvent),
    /// Network events
    Network(NetworkEvent),
    /// Storage events
    Storage(StorageEvent),
    /// Custom application events
    Custom {
        event_type: String,
        data: Vec<u8>,
    },
}

/// Transaction-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionEvent {
    /// Transaction submitted to mempool
    Submitted {
        tx_hash: TxHash,
        from: Address,
        timestamp: Timestamp,
    },
    /// Transaction included in block
    Included {
        tx_hash: TxHash,
        block_hash: BlockHash,
        block_height: u64,
    },
    /// Transaction execution completed
    Executed {
        tx_hash: TxHash,
        success: bool,
        gas_used: u64,
        return_data: Vec<u8>,
    },
    /// Transaction failed
    Failed {
        tx_hash: TxHash,
        error: String,
        gas_used: u64,
    },
}

/// Block-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockEvent {
    /// New block proposed
    Proposed {
        block_hash: BlockHash,
        height: u64,
        producer: NodeId,
        timestamp: Timestamp,
    },
    /// Block finalized
    Finalized {
        block_hash: BlockHash,
        height: u64,
        transaction_count: u32,
    },
    /// Block reorganization
    Reorganization {
        old_chain: Vec<BlockHash>,
        new_chain: Vec<BlockHash>,
        fork_point: u64,
    },
}

/// Consensus-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusEvent {
    /// New consensus round started
    RoundStarted {
        height: u64,
        round: u32,
        leader: NodeId,
    },
    /// Vote cast in consensus
    VoteCast {
        voter: NodeId,
        block_hash: BlockHash,
        vote_type: VoteType,
    },
    /// Consensus reached
    ConsensusReached {
        block_hash: BlockHash,
        height: u64,
        votes: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoteType {
    Prevote,
    Precommit,
}

/// Network-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkEvent {
    /// Peer connected
    PeerConnected {
        peer_id: NodeId,
        role: NodeRole,
        address: String,
    },
    /// Peer disconnected
    PeerDisconnected {
        peer_id: NodeId,
        reason: String,
    },
    /// Message received
    MessageReceived {
        from: NodeId,
        message_type: String,
        size: usize,
    },
}

/// Storage-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageEvent {
    /// File stored
    FileStored {
        file_hash: String,
        size: u64,
        storage_nodes: Vec<NodeId>,
    },
    /// File retrieved
    FileRetrieved {
        file_hash: String,
        requester: NodeId,
    },
    /// Storage capacity changed
    CapacityChanged {
        node_id: NodeId,
        old_capacity: u64,
        new_capacity: u64,
    },
}

/// Event subscription interface
pub trait EventSubscription: Send + Sync {
    type Event;
    type Filter;

    /// Subscribe to events matching the filter
    fn subscribe(&mut self, filter: Self::Filter) -> Result<EventReceiver<Self::Event>, EventError>;

    /// Unsubscribe from events
    fn unsubscribe(&mut self, subscription_id: u64) -> Result<(), EventError>;

    /// Publish an event
    fn publish(&mut self, event: Self::Event) -> Result<(), EventError>;
}

/// Event filter interface
pub trait EventFilter: Send + Sync + Clone {
    type Event;

    /// Check if an event matches this filter
    fn matches(&self, event: &Self::Event) -> bool;
}

/// Event filter implementations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CatalystEventFilter {
    /// Match all events
    All,
    /// Match events by type
    ByType(String),
    /// Match transaction events for specific address
    ByAddress(Address),
    /// Match events by node
    ByNode(NodeId),
    /// Match events in block range
    ByBlockRange { from: u64, to: u64 },
    /// Combined filters
    And(Box<CatalystEventFilter>, Box<CatalystEventFilter>),
    Or(Box<CatalystEventFilter>, Box<CatalystEventFilter>),
}

impl EventFilter for CatalystEventFilter {
    type Event = Event;

    fn matches(&self, event: &Self::Event) -> bool {
        match self {
            CatalystEventFilter::All => true,
            CatalystEventFilter::ByType(event_type) => {
                match event {
                    Event::Transaction(_) => event_type == "transaction",
                    Event::Block(_) => event_type == "block",
                    Event::Consensus(_) => event_type == "consensus",
                    Event::Network(_) => event_type == "network",
                    Event::Storage(_) => event_type == "storage",
                    Event::Custom { event_type: et, .. } => event_type == et,
                }
            },
            CatalystEventFilter::ByAddress(address) => {
                match event {
                    Event::Transaction(TransactionEvent::Submitted { from, .. }) => from == address,
                    Event::Transaction(TransactionEvent::Executed { .. }) => false, // Would need tx details
                    _ => false,
                }
            },
            CatalystEventFilter::ByNode(node_id) => {
                match event {
                    Event::Block(BlockEvent::Proposed { producer, .. }) => producer == node_id,
                    Event::Consensus(ConsensusEvent::VoteCast { voter, .. }) => voter == node_id,
                    Event::Network(NetworkEvent::PeerConnected { peer_id, .. }) => peer_id == node_id,
                    Event::Network(NetworkEvent::PeerDisconnected { peer_id, .. }) => peer_id == node_id,
                    _ => false,
                }
            },
            CatalystEventFilter::ByBlockRange { from, to } => {
                match event {
                    Event::Block(BlockEvent::Proposed { height, .. }) => height >= from && height <= to,
                    Event::Block(BlockEvent::Finalized { height, .. }) => height >= from && height <= to,
                    _ => false,
                }
            },
            CatalystEventFilter::And(left, right) => {
                left.matches(event) && right.matches(event)
            },
            CatalystEventFilter::Or(left, right) => {
                left.matches(event) || right.matches(event)
            },
        }
    }
}

/// Event receiver type
pub type EventReceiver<T> = mpsc::Receiver<T>;

/// Event error types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventError {
    SubscriptionFailed(String),
    PublishFailed(String),
    FilterError(String),
    ChannelClosed,
}

impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventError::SubscriptionFailed(msg) => write!(f, "Subscription failed: {}", msg),
            EventError::PublishFailed(msg) => write!(f, "Publish failed: {}", msg),
            EventError::FilterError(msg) => write!(f, "Filter error: {}", msg),
            EventError::ChannelClosed => write!(f, "Event channel closed"),
        }
    }
}

impl std::error::Error for EventError {}

/// Event manager for handling subscriptions and publishing
pub struct EventManager {
    subscribers: HashMap<u64, mpsc::Sender<Event>>,
    next_id: u64,
}

impl EventManager {
    pub fn new() -> Self {
        Self {
            subscribers: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn subscribe(&mut self, filter: CatalystEventFilter) -> (u64, EventReceiver<Event>) {
        let (tx, rx) = mpsc::channel(1000);
        let id = self.next_id;
        self.next_id += 1;
        
        self.subscribers.insert(id, tx);
        (id, rx)
    }

    pub fn unsubscribe(&mut self, subscription_id: u64) -> Result<(), EventError> {
        self.subscribers.remove(&subscription_id);
        Ok(())
    }

    pub async fn publish(&mut self, event: Event) -> Result<(), EventError> {
        let mut failed_subscribers = Vec::new();
        
        for (id, sender) in &self.subscribers {
            if sender.send(event.clone()).await.is_err() {
                failed_subscribers.push(*id);
            }
        }
        
        // Remove failed subscribers
        for id in failed_subscribers {
            self.subscribers.remove(&id);
        }
        
        Ok(())
    }
}

impl Default for EventManager {
    fn default() -> Self {
        Self::new()
    }
}