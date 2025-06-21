use serde::{Deserialize, Serialize};
use thiserror::Error;

// Module declarations
pub mod keys;
pub mod commitment;
pub mod hash;
pub mod merkle;
pub mod proofs;
pub mod signature;

// Re‑exports – resolve naming nicely
pub use keys::*;
pub use commitment::*;
pub use hash::*;
pub use merkle::MerkleTree; // Only export MerkleTree (not MerkleProof)
pub use proofs::*; // This exports MerkleProof from proofs
pub use signature::*;

// Type aliases
pub type Bytes32 = [u8; 32];
pub type Bytes64 = [u8; 64];

/// Canonical 32‑byte hash wrapper (not std::hash::Hash)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hash(pub Bytes32);

impl Hash {
    pub fn new(bytes: Bytes32) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

// Error types
#[derive(Error, Debug)]
pub enum CryptoError {
    #[error("Invalid key format")]
    InvalidKey,
    #[error("Signature verification failed")]
    SignatureVerificationFailed,
    #[error("Invalid signature format")]
    InvalidSignature,
    #[error("Serialization error: {0}")]
    SerializationError(String),
    #[error("Hash computation error")]
    HashError,
}

// Trait for hashable data structures
pub trait Hashable {
    fn crypto_hash(&self) -> Hash;
}

// ────────────────────────────────────────────────────────────────────────────────
// Blanket impl so *any* fixed‑size byte array works with `Hashable`.
// This lets you call `[u8; N]::crypto_hash()` directly in tests & code without
// first slicing, e.g. `b"hello".crypto_hash()`.
//
// We simply coerce the array to a slice and delegate to the existing slice impl,
// so there’s no duplicated hashing logic.
//
// `as_slice()` on arrays has been stable since Rust 1.55.
// ────────────────────────────────────────────────────────────────────────────────
impl<const N: usize> Hashable for [u8; N] {
    #[inline]
    fn crypto_hash(&self) -> Hash {
        self.as_slice().crypto_hash()
    }
}

// Constants
pub mod constants {
    pub const HASH_SIZE: usize = 32;
    pub const SIGNATURE_SIZE: usize = 64;
    pub const PUBLIC_KEY_SIZE: usize = 32;
    pub const PRIVATE_KEY_SIZE: usize = 32;
}

// Utility functions
pub fn secure_random_bytes<const N: usize>() -> [u8; N] {
    use rand::RngCore;
    let mut bytes = [0u8; N];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}
