use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{
    NodeId, Address,
    ConsensusMessage, Transaction
};

/// Core consensus protocol trait
#[async_trait]
pub trait ConsensusProtocol: Send + Sync {
    type Block;
    type Proposal;
    type Vote;

    /// Propose a new block
    async fn propose_block(&mut self, transactions: Vec<Transaction>) -> Result<Self::Proposal, ConsensusError>;

    /// Validate a proposed block
    async fn validate_proposal(&self, proposal: &Self::Proposal) -> Result<bool, ConsensusError>;

    /// Submit a vote for a proposal
    async fn vote(&mut self, proposal_id: &[u8], vote: Self::Vote) -> Result<(), ConsensusError>;

    /// Process consensus messages
    async fn process_message(&mut self, message: ConsensusMessage) -> Result<(), ConsensusError>;

    /// Get the current consensus state
    async fn get_state(&self) -> Result<ConsensusState, ConsensusError>;
}

/// State management interface
#[async_trait]
pub trait StateManager: Send + Sync {
    /// Get account state
    async fn get_account(&self, address: &Address) -> Result<Option<Vec<u8>>, StateError>;

    /// Update account state
    async fn set_account(&mut self, address: &Address, data: Vec<u8>) -> Result<(), StateError>;

    /// Get storage value
    async fn get_storage(&self, address: &Address, key: &[u8]) -> Result<Option<Vec<u8>>, StateError>;

    /// Set storage value
    async fn set_storage(&mut self, address: &Address, key: Vec<u8>, value: Vec<u8>) -> Result<(), StateError>;

    /// Create a state snapshot
    async fn create_snapshot(&self) -> Result<StateSnapshot, StateError>;

    /// Restore from a snapshot
    async fn restore_snapshot(&mut self, snapshot: StateSnapshot) -> Result<(), StateError>;

    /// Get state root hash
    async fn get_state_root(&self) -> Result<[u8; 32], StateError>;
}

/// Network interface for peer-to-peer communication
#[async_trait]
pub trait NetworkInterface: Send + Sync {
    /// Broadcast a message to all peers
    async fn broadcast(&self, message: Vec<u8>) -> Result<(), NetworkError>;

    /// Send a message to a specific peer
    async fn send_to_peer(&self, peer_id: &NodeId, message: Vec<u8>) -> Result<(), NetworkError>;

    /// Get list of connected peers
    async fn get_peers(&self) -> Result<Vec<NodeId>, NetworkError>;

    /// Subscribe to network events
    async fn subscribe_events(&self) -> Result<NetworkEventReceiver, NetworkError>;
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

    /// Combine with another filter using AND logic
    fn and(self, other: Self) -> AndFilter<Self> where Self: Sized {
        AndFilter { left: self, right: other }
    }

    /// Combine with another filter using OR logic  
    fn or(self, other: Self) -> OrFilter<Self> where Self: Sized {
        OrFilter { left: self, right: other }
    }
}

// Filter combinators
#[derive(Clone)]
pub struct AndFilter<F> {
    left: F,
    right: F,
}

impl<F: EventFilter> EventFilter for AndFilter<F> {
    type Event = F::Event;

    fn matches(&self, event: &Self::Event) -> bool {
        self.left.matches(event) && self.right.matches(event)
    }
}

#[derive(Clone)]
pub struct OrFilter<F> {
    left: F,
    right: F,
}

impl<F: EventFilter> EventFilter for OrFilter<F> {
    type Event = F::Event;

    fn matches(&self, event: &Self::Event) -> bool {
        self.left.matches(event) || self.right.matches(event)
    }
}

// Type aliases for event handling
pub type EventReceiver<T> = tokio::sync::mpsc::Receiver<T>;
pub type NetworkEventReceiver = EventReceiver<NetworkEvent>;

// Supporting types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsensusState {
    pub current_height: u64,
    pub current_round: u32,
    pub leader: Option<NodeId>,
    pub locked_block: Option<[u8; 32]>,
    pub votes: HashMap<[u8; 32], u32>, // block_hash -> vote_count
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub root_hash: [u8; 32],
    pub height: u64,
    pub timestamp: u64,
    pub metadata: HashMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkEvent {
    PeerConnected(NodeId),
    PeerDisconnected(NodeId),
    MessageReceived { from: NodeId, data: Vec<u8> },
    NetworkError(String),
}

// Error types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsensusError {
    InvalidProposal(String),
    InvalidVote(String),
    NetworkError(String),
    StateError(String),
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateError {
    AccountNotFound,
    InvalidAddress,
    SerializationError(String),
    StorageError(String),
    SnapshotError(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkError {
    PeerNotFound,
    ConnectionFailed(String),
    SendFailed(String),
    ReceiveFailed(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventError {
    SubscriptionFailed(String),
    PublishFailed(String),
    FilterError(String),
}

// Implement std::error::Error for all error types
impl std::fmt::Display for ConsensusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsensusError::InvalidProposal(msg) => write!(f, "Invalid proposal: {}", msg),
            ConsensusError::InvalidVote(msg) => write!(f, "Invalid vote: {}", msg),
            ConsensusError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            ConsensusError::StateError(msg) => write!(f, "State error: {}", msg),
            ConsensusError::Timeout => write!(f, "Consensus timeout"),
        }
    }
}

impl std::error::Error for ConsensusError {}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::AccountNotFound => write!(f, "Account not found"),
            StateError::InvalidAddress => write!(f, "Invalid address"),
            StateError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            StateError::StorageError(msg) => write!(f, "Storage error: {}", msg),
            StateError::SnapshotError(msg) => write!(f, "Snapshot error: {}", msg),
        }
    }
}

impl std::error::Error for StateError {}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::PeerNotFound => write!(f, "Peer not found"),
            NetworkError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            NetworkError::SendFailed(msg) => write!(f, "Send failed: {}", msg),
            NetworkError::ReceiveFailed(msg) => write!(f, "Receive failed: {}", msg),
        }
    }
}

impl std::error::Error for NetworkError {}

impl std::fmt::Display for EventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventError::SubscriptionFailed(msg) => write!(f, "Subscription failed: {}", msg),
            EventError::PublishFailed(msg) => write!(f, "Publish failed: {}", msg),
            EventError::FilterError(msg) => write!(f, "Filter error: {}", msg),
        }
    }
}

impl std::error::Error for EventError {}