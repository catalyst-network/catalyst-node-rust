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