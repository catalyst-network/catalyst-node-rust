use crate::{Hash, CryptoError, hash::sha256};
use serde::{Deserialize, Serialize};

/// Pedersen commitment structure
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commitment {
    pub hash: Hash,
}

/// Commitment scheme for hiding values with later reveal
#[derive(Debug, Clone, Serialize, Deserialize)]  
pub struct CommitReveal {
    pub value: Vec<u8>,
    pub nonce: [u8; 32],
}

impl CommitReveal {
    /// Create a new commitment with random nonce
    pub fn new(value: Vec<u8>) -> Self {
        use rand::RngCore;
        let mut nonce = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut nonce);
        
        Self { value, nonce }
    }
    
    /// Create commitment with specific nonce (for testing)
    pub fn with_nonce(value: Vec<u8>, nonce: [u8; 32]) -> Self {
        Self { value, nonce }
    }
    
    /// Generate the commitment hash
    pub fn commit(&self) -> Commitment {
        let mut data = Vec::new();
        data.extend_from_slice(&self.value);
        data.extend_from_slice(&self.nonce);
        
        Commitment {
            hash: sha256(&data)
        }
    }
    
    /// Verify a commitment matches this value and nonce
    pub fn verify(&self, commitment: &Commitment) -> bool {
        self.commit().hash == commitment.hash
    }
}

impl Commitment {
    /// Create commitment from hash
    pub fn new(hash: Hash) -> Self {
        Self { hash }
    }
    
    /// Verify a reveal against this commitment
    pub fn verify_reveal(&self, reveal: &CommitReveal) -> Result<bool, CryptoError> {
        Ok(reveal.verify(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_reveal() {
        let value = b"secret value".to_vec();
        let commit_reveal = CommitReveal::new(value.clone());
        
        let commitment = commit_reveal.commit();
        assert!(commit_reveal.verify(&commitment));
        
        // Different value should not verify
        let different_value = CommitReveal::new(b"different value".to_vec());
        assert!(!different_value.verify(&commitment));
    }
    
    #[test]
    fn test_deterministic_commitment() {
        let value = b"test".to_vec();
        let nonce = [1u8; 32];
        
        let cr1 = CommitReveal::with_nonce(value.clone(), nonce);
        let cr2 = CommitReveal::with_nonce(value, nonce);
        
        assert_eq!(cr1.commit().hash, cr2.commit().hash);
    }
}