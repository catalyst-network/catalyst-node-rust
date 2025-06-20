use sha2::{Digest, Sha256};
use crate::{Hash, Hashable};

/// Compute SHA-256 hash of input data
pub fn sha256(data: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    
    let mut hash_bytes = [0u8; 32];
    hash_bytes.copy_from_slice(&result);
    Hash::new(hash_bytes)
}

/// Compute hash of multiple inputs
pub fn hash_multiple(inputs: &[&[u8]]) -> Hash {
    let mut hasher = Sha256::new();
    for input in inputs {
        hasher.update(input);
    }
    let result = hasher.finalize();
    
    let mut hash_bytes = [0u8; 32];
    hash_bytes.copy_from_slice(&result);
    Hash::new(hash_bytes)
}

impl Hashable for &[u8] {
    fn crypto_hash(&self) -> Hash {
        sha256(self)
    }
}

impl Hashable for Vec<u8> {
    fn crypto_hash(&self) -> Hash {
        sha256(self)
    }
}

impl Hashable for String {
    fn crypto_hash(&self) -> Hash {
        sha256(self.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256_basic() {
        let data = b"hello world";
        let hash = sha256(data);
        
        // Should be deterministic
        let hash2 = sha256(data);
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_hashable_trait() {
        let data = b"test data";
        let hash1 = data.crypto_hash();
        let hash2 = sha256(data);
        assert_eq!(hash1, hash2);
    }
}