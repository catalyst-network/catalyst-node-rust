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

pub mod errors;
pub mod hash;
pub mod keys;
pub mod signatures;
pub mod commitments;
pub mod utils;

pub use errors::{CryptoError, CryptoResult};
pub use hash::{blake2b_hash, blake2b_hash_multiple, Hash256, HASH_SIZE}; // Added blake2b_hash_multiple
pub use keys::{PrivateKey, PublicKey, KeyPair, Curve25519KeyPair};
pub use signatures::{Signature, MuSigSignature, SignatureScheme};
pub use commitments::{PedersenCommitment, CommitmentScheme};

// Re-export commonly used types
pub use curve25519_dalek::{
    scalar::Scalar,
    ristretto::{RistrettoPoint, CompressedRistretto},
    constants::RISTRETTO_BASEPOINT_POINT,
};