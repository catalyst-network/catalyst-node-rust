// Core modules
pub mod types;
pub mod traits;
pub mod error;
pub mod events;
pub mod runtime;

// Re-exports - be specific to avoid ambiguity
pub use types::{
    NodeId, BlockHash, Address, TokenAmount, LedgerCycle,
    ResourceEstimate, ResourceProof, Transaction, ExecutionResult,
    Account, ExecutionContext, ConsensusMessage, RuntimeType,
    StateChange as TypesStateChange, Event as TypesEvent,
    // Additional types from the original lib.rs
    TxHash, Timestamp, Gas, BlockHeight, WorkerPass, NodeRole,
    NetworkMessage, PeerMessage,
};

pub use traits::{
    ConsensusProtocol, StateManager, NetworkInterface, 
    EventSubscription as TraitsEventSubscription,
    EventFilter as TraitsEventFilter,
};

pub use error::*;

pub use events::{
    EventSubscription as EventsEventSubscription,
    EventFilter as EventsEventFilter,
};

pub use runtime::{RuntimeManager, RuntimeConfig, RuntimeError};

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = env!("CARGO_PKG_NAME");

// Re-export async_trait for convenience when implementing traits
pub use async_trait::async_trait;

/// Core result type used throughout the Catalyst system
pub type CatalystResult<T> = Result<T, CatalystError>;

#[derive(Debug, Clone)]
pub struct NodeStatus {
    pub id: String,
    pub uptime: u64,
    pub sync_status: SyncStatus,
    pub metrics: ResourceMetrics,
}

#[derive(Debug, Clone)]
pub enum SyncStatus {
    Synced,
    Syncing { progress: f64 },
    NotSynced,
}

#[derive(Debug, Clone)]
pub struct ResourceMetrics {
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub disk_usage: u64,
}

// Add these module placeholders to catalyst-core/src/lib.rs

pub trait CatalystModule: Send + Sync {}
pub trait NetworkModule: Send + Sync {}
pub trait StorageModule: Send + Sync {} 
pub trait ConsensusModule: Send + Sync {}
pub trait RuntimeModule: Send + Sync {}
pub trait ServiceBusModule: Send + Sync {}
pub trait DfsModule: Send + Sync {}
