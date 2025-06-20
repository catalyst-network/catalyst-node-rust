use serde::{Deserialize, Serialize};
use thiserror::Error;

// Module declarations
pub mod keys;
pub mod commitment;
pub mod hash;
pub mod merkle;
pub mod proofs;
pub mod signature;

// Re-exports - fix naming conflicts
pub use keys::*;
pub use commitment::*;
pub use hash::*;
pub use merkle::{MerkleTree}; // Only export MerkleTree, not MerkleProof
pub use proofs::*; // This exports the MerkleProof from proofs
pub use signature::*;

// Type aliases
pub type Bytes32 = [u8; 32];
pub type Bytes64 = [u8; 64];

// Custom Hash type (not std::hash::Hash)
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

// Trait for hashable types
pub trait Hashable {
    fn crypto_hash(&self) -> Hash;
}

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