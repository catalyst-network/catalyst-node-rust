use blake2::{Blake2b512, Digest};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const HASH_SIZE: usize = 32; // Blake2b-256

/// 256-bit hash output from Blake2b
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash256([u8; HASH_SIZE]);

impl Hash256 {
    /// Create a new Hash256 from bytes
    pub fn new(bytes: [u8; HASH_SIZE]) -> Self {
        Self(bytes)
    }

    /// Get the underlying bytes
    pub fn as_bytes(&self) -> &[u8; HASH_SIZE] {
        &self.0
    }

    /// Convert to vector
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    /// Create from slice (returns error if wrong length)
    pub fn from_slice(slice: &[u8]) -> Result<Self, crate::CryptoError> {
        if slice.len() != HASH_SIZE {
            return Err(crate::CryptoError::SerializationError(format!(
                "Expected {} bytes, got {}",
                HASH_SIZE,
                slice.len()
            )));
        }
        let mut bytes = [0u8; HASH_SIZE];
        bytes.copy_from_slice(slice);
        Ok(Self(bytes))
    }
}

impl fmt::Display for Hash256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl AsRef<[u8]> for Hash256 {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Blake2b-256 hash function as specified in the Catalyst Network protocol
pub fn blake2b_hash(data: &[u8]) -> Hash256 {
    let mut hasher = Blake2b512::new();
    hasher.update(data);
    let result = hasher.finalize();

    // Take first 32 bytes for Blake2b-256
    let mut hash_bytes = [0u8; HASH_SIZE];
    hash_bytes.copy_from_slice(&result[..HASH_SIZE]);
    Hash256(hash_bytes)
}

/// Hash multiple data pieces together
pub fn blake2b_hash_multiple(data_pieces: &[&[u8]]) -> Hash256 {
    let mut hasher = Blake2b512::new();
    for piece in data_pieces {
        hasher.update(piece);
    }
    let result = hasher.finalize();

    let mut hash_bytes = [0u8; HASH_SIZE];
    hash_bytes.copy_from_slice(&result[..HASH_SIZE]);
    Hash256(hash_bytes)
}
