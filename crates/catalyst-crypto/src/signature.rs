// Re-export from keys module for convenience
pub use crate::keys::{CryptoSignature, PrivateKey, PublicKey, KeyPair};
use crate::{CryptoError, Hash};
use serde::{Deserialize, Serialize};

/// Multi-signature threshold scheme (basic implementation)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSignature {
    pub signatures: Vec<CryptoSignature>,
    pub signers: Vec<PublicKey>,
    pub threshold: usize,
}

impl MultiSignature {
    pub fn new(threshold: usize) -> Self {
        Self {
            signatures: Vec::new(),
            signers: Vec::new(),
            threshold,
        }
    }
    
    pub fn add_signature(&mut self, signature: CryptoSignature, signer: PublicKey) {
        self.signatures.push(signature);
        self.signers.push(signer);
    }
    
    pub fn is_complete(&self) -> bool {
        self.signatures.len() >= self.threshold
    }
    
    pub fn verify(&self, message: &[u8]) -> Result<bool, CryptoError> {
        if !self.is_complete() {
            return Ok(false);
        }
        
        for (signature, signer) in self.signatures.iter().zip(self.signers.iter()) {
            signer.verify(message, signature)?;
        }
        
        Ok(true)
    }
}

/// Aggregate signature (simple concatenation for now)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateSignature {
    pub signatures: Vec<CryptoSignature>,
    pub public_keys: Vec<PublicKey>,
}

impl AggregateSignature {
    pub fn new() -> Self {
        Self {
            signatures: Vec::new(),
            public_keys: Vec::new(),
        }
    }
    
    pub fn add(&mut self, signature: CryptoSignature, public_key: PublicKey) {
        self.signatures.push(signature);
        self.public_keys.push(public_key);
    }
    
    pub fn verify(&self, message: &[u8]) -> Result<bool, CryptoError> {
        if self.signatures.len() != self.public_keys.len() {
            return Ok(false);
        }
        
        for (signature, public_key) in self.signatures.iter().zip(self.public_keys.iter()) {
            public_key.verify(message, signature)?;
        }
        
        Ok(true)
    }
}

/// Sign a hash directly
pub fn sign_hash(private_key: &PrivateKey, hash: &Hash) -> CryptoSignature {
    private_key.sign(hash.as_bytes())
}

/// Verify a hash signature
pub fn verify_hash(public_key: &PublicKey, hash: &Hash, signature: &CryptoSignature) -> Result<(), CryptoError> {
    public_key.verify(hash.as_bytes(), signature)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_signature() {
        let message = b"test message";
        let keypairs: Vec<KeyPair> = (0..3).map(|_| KeyPair::generate()).collect();
        
        let mut multisig = MultiSignature::new(2); // 2-of-3 threshold
        
        // Add two signatures
        for keypair in &keypairs[0..2] {
            let signature = keypair.sign(message);
            multisig.add_signature(signature, keypair.public_key);
        }
        
        assert!(multisig.is_complete());
        assert!(multisig.verify(message).unwrap());
    }
    
    #[test]
    fn test_aggregate_signature() {
        let message = b"test message";
        let keypairs: Vec<KeyPair> = (0..3).map(|_| KeyPair::generate()).collect();
        
        let mut agg_sig = AggregateSignature::new();
        
        for keypair in &keypairs {
            let signature = keypair.sign(message);
            agg_sig.add(signature, keypair.public_key);
        }
        
        assert!(agg_sig.verify(message).unwrap());
    }
}