use crate::{CryptoResult, CryptoError};
use rand::{RngCore, CryptoRng};

/// Secure random number generation utilities
pub struct SecureRandom;

impl SecureRandom {
    /// Generate cryptographically secure random bytes
    pub fn generate_bytes<R: RngCore + CryptoRng>(rng: &mut R, len: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; len];
        rng.fill_bytes(&mut bytes);
        bytes
    }
    
    /// Generate random scalar for curve operations
    pub fn generate_scalar<R: RngCore + CryptoRng>(rng: &mut R) -> curve25519_dalek::scalar::Scalar {
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        curve25519_dalek::scalar::Scalar::from_bytes_mod_order(bytes)
    }
}

/// Constant-time comparison utilities
pub struct ConstantTime;

impl ConstantTime {
    /// Constant-time equality check for byte arrays
    pub fn eq(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        
        use subtle::ConstantTimeEq;
        a.ct_eq(b).into()
    }
}

/// Key derivation utilities
pub struct KeyDerivation;

impl KeyDerivation {
    /// Derive key from seed using HKDF-like construction
    pub fn derive_key(seed: &[u8], info: &[u8], output_len: usize) -> CryptoResult<Vec<u8>> {
        if output_len > 255 * 32 {
            return Err(CryptoError::SerializationError("Output too long".to_string()));
        }
        
        let mut output = Vec::new();
        let mut counter = 1u8;
        
        while output.len() < output_len {
            let mut data = Vec::new();
            data.extend_from_slice(seed);
            data.extend_from_slice(info);
            data.push(counter);
            
            let hash = crate::blake2b_hash(&data);
            let remaining = output_len - output.len();
            let to_take = std::cmp::min(32, remaining);
            
            output.extend_from_slice(&hash.as_bytes()[..to_take]);
            counter += 1;
        }
        
        Ok(output)
    }
}