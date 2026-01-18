//! Catalyst consensus: collaborative 4‑phase protocol (Construction → Campaigning → Voting → Synchronisation).
//!
//! Reference: `https://catalystnet.org/media/CatalystConsensusPaper.pdf` (v1.2).

pub mod error;
pub mod phases;
pub mod producer;
pub mod types;

// The executable engine.
pub mod consensus;

pub use consensus::CollaborativeConsensus;
pub use types::{
    ConsensusConfig, CycleNumber, ProducerCandidate, ProducerId, ProducerOutput, ProducerQuantity, ProducerVote,
    LedgerStateUpdate, PartialLedgerStateUpdate, TransactionEntry,
};