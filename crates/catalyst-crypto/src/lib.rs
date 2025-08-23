// Update crates/catalyst-crypto/src/lib.rs to export blake2b_hash_multiple

//! Catalyst Network Cryptographic Primitives
//!
//! This crate provides the core cryptographic functionality for the Catalyst Network:
//! - Twisted Edwards Curve25519 elliptic curve operations
//! - Blake2b-256 hashing
//! - MuSig aggregated signatures (Schnorr-based)
//! - Pedersen Commitments for confidential transactions
//!
//! All implementations follow the specifications in the Catalyst Network whitepaper.

pub mod commitments;
pub mod errors;
pub mod hash;
pub mod keys;
pub mod signatures;
pub mod utils;

pub use commitments::{CommitmentScheme, PedersenCommitment};
pub use errors::{CryptoError, CryptoResult};
pub use hash::{blake2b_hash, blake2b_hash_multiple, Hash256, HASH_SIZE}; // Added blake2b_hash_multiple
pub use keys::{Curve25519KeyPair, KeyPair, PrivateKey, PublicKey};
pub use signatures::{MuSigSignature, Signature, SignatureScheme};

// Re-export commonly used types
pub use curve25519_dalek::{
    constants::RISTRETTO_BASEPOINT_POINT,
    ristretto::{CompressedRistretto, RistrettoPoint},
    scalar::Scalar,
};
