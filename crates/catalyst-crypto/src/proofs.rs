use crate::{Hash, CryptoError, hash::sha256};
use serde::{Deserialize, Serialize};

/// Zero-knowledge proof of knowledge (placeholder implementation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZKProof {
    pub commitment: Hash,
    pub challenge: Hash,
    pub response: Vec<u8>,
}

/// Merkle proof for inclusion in a Merkle tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf: Hash,
    pub path: Vec<Hash>,
    pub indices: Vec<bool>, // true = right, false = left
}

impl MerkleProof {
    pub fn new(leaf: Hash) -> Self {
        Self {
            leaf,
            path: Vec::new(),
            indices: Vec::new(),
        }
    }
    
    pub fn add_sibling(&mut self, sibling: Hash, is_right: bool) {
        self.path.push(sibling);
        self.indices.push(is_right);
    }
    
    pub fn verify(&self, root: &Hash) -> bool {
        let mut current = self.leaf;
        
        for (sibling, &is_right) in self.path.iter().zip(self.indices.iter()) {
            current = if is_right {
                // Current is left, sibling is right
                hash_pair(&current, sibling)
            } else {
                // Current is right, sibling is left  
                hash_pair(sibling, &current)
            };
        }
        
        current == *root
    }
}

/// Range proof (simplified implementation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeProof {
    pub commitment: Hash,
    pub min_value: u64,
    pub max_value: u64,
    pub proof_data: Vec<u8>,
}

impl RangeProof {
    pub fn new(value: u64, min_value: u64, max_value: u64) -> Result<Self, CryptoError> {
        if value < min_value || value > max_value {
            return Err(CryptoError::InvalidSignature);
        }
        
        // Simplified proof - in practice this would use bulletproofs or similar
        let mut proof_data = Vec::new();
        proof_data.extend_from_slice(&value.to_le_bytes());
        proof_data.extend_from_slice(&min_value.to_le_bytes());
        proof_data.extend_from_slice(&max_value.to_le_bytes());
        
        let commitment = sha256(&proof_data);
        
        Ok(Self {
            commitment,
            min_value,
            max_value,
            proof_data,
        })
    }
    
    pub fn verify(&self) -> Result<bool, CryptoError> {
        // Simplified verification
        let expected_commitment = sha256(&self.proof_data);
        Ok(self.commitment == expected_commitment)
    }
}

/// Non-interactive zero-knowledge proof
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NIZKProof {
    pub statement: Vec<u8>,
    pub witness_commitment: Hash,
    pub proof: Vec<u8>,
}

impl NIZKProof {
    pub fn new(statement: Vec<u8>, witness: Vec<u8>) -> Self {
        let witness_commitment = sha256(&witness);
        
        // Simplified proof generation
        let mut proof_input = Vec::new();
        proof_input.extend_from_slice(&statement);
        proof_input.extend_from_slice(&witness);
        let proof = sha256(&proof_input).as_bytes().to_vec();
        
        Self {
            statement,
            witness_commitment,
            proof,
        }
    }
    
    pub fn verify(&self) -> bool {
        // Simplified verification - in practice this would use proper NIZK
        !self.proof.is_empty() && self.proof.len() == 32
    }
}

// Helper function for Merkle proof
fn hash_pair(left: &Hash, right: &Hash) -> Hash {
    let mut combined = Vec::new();
    combined.extend_from_slice(left.as_bytes());
    combined.extend_from_slice(right.as_bytes());
    sha256(&combined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_proof() {
        let leaf = sha256(b"leaf data");
        let sibling = sha256(b"sibling data");
        let root = hash_pair(&leaf, &sibling);
        
        let mut proof = MerkleProof::new(leaf);
        proof.add_sibling(sibling, true); // sibling is on the right
        
        assert!(proof.verify(&root));
    }
    
    #[test]
    fn test_range_proof() {
        let proof = RangeProof::new(50, 0, 100).unwrap();
        assert!(proof.verify().unwrap());
        
        // Should fail for out of range
        assert!(RangeProof::new(150, 0, 100).is_err());
    }
    
    #[test]
    fn test_nizk_proof() {
        let statement = b"I know the secret".to_vec();
        let witness = b"secret123".to_vec();
        
        let proof = NIZKProof::new(statement, witness);
        assert!(proof.verify());
    }
}