//! Catalyst consensus: collaborative 4‑phase protocol (Construction → Campaigning → Voting → Synchronisation).
//!
//! Reference: `https://catalystnet.org/media/CatalystConsensusPaper.pdf` (v1.2).

pub mod error;
pub mod phases;
pub mod producer;
pub mod types;

// The executable engine.
pub mod consensus;

/// LSU finality certificate (ADR 0001).
pub mod lsu_finality;

pub use consensus::CollaborativeConsensus;
pub use phases::hash_producer_list_merkle;
pub use lsu_finality::{
    bft_vote_threshold, committee_hash_ordered_producer_list, h_cert_v1, parse_producer_hex32,
    verify_finality_attestation, verify_lsu_finality_certificate, LsuFinalityCertificateV1,
    LsuFinalityVerifyError, ProducerFinalityVote, FINALITY_CERT_STYLE_INDIVIDUAL, MAX_FINALITY_VOTES,
};
pub use types::{
    ConsensusConfig, CycleNumber, ProducerCandidate, ProducerId, ProducerOutput, ProducerQuantity, ProducerVote,
    LedgerStateUpdate, PartialLedgerStateUpdate, TransactionEntry,
};