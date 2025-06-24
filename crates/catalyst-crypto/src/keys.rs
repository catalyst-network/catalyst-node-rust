use crate::{CryptoError, CryptoResult, blake2b_hash};
use curve25519_dalek::{
    scalar::Scalar,
    ristretto::{RistrettoPoint, CompressedRistretto},
    constants::RISTRETTO_BASEPOINT_POINT,
};
use rand::{RngCore, CryptoRng};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const PRIVATE_KEY_SIZE: usize = 32;
pub const PUBLIC_KEY_SIZE: usize = 32;

/// Private key for Curve25519 operations
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct PrivateKey {
    scalar: Scalar,
    #[serde(skip)]
    bytes: [u8; PRIVATE_KEY_SIZE],
}

impl PrivateKey {
    /// Generate a new random private key
    pub fn generate<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        let mut bytes = [0u8; PRIVATE_KEY_SIZE];
        rng.fill_bytes(&mut bytes);
        
        let scalar = Scalar::from_bytes_mod_order(bytes);
        Self { scalar, bytes }
    }
    
    /// Create from existing bytes
    pub fn from_bytes(bytes: [u8; PRIVATE_KEY_SIZE]) -> Self {
        let scalar = Scalar::from_bytes_mod_order(bytes);
        Self { scalar, bytes }
    }
    
    /// Get the scalar representation
    pub fn scalar(&self) -> &Scalar {
        &self.scalar
    }
    
    /// Convert to bytes (use carefully - exposes private key)
    pub fn to_bytes(&self) -> [u8; PRIVATE_KEY_SIZE] {
        self.bytes
    }
    
    /// Derive public key from this private key
    pub fn public_key(&self) -> PublicKey {
        let point = &self.scalar * &RISTRETTO_BASEPOINT_POINT;
        PublicKey { point }
    }
}

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PrivateKey([REDACTED])")
    }
}

/// Public key for Curve25519 operations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKey {
    point: RistrettoPoint,
}

impl PublicKey {
    /// Create from RistrettoPoint
    pub fn from_point(point: RistrettoPoint) -> Self {
        Self { point }
    }
    
    /// Create from compressed bytes
    pub fn from_bytes(bytes: [u8; PUBLIC_KEY_SIZE]) -> CryptoResult<Self> {
        let compressed = CompressedRistretto(bytes);
        let point = compressed.decompress()
            .ok_or(CryptoError::InvalidKey("Invalid public key encoding".to_string()))?;
        Ok(Self { point })
    }
    
    /// Get the point representation
    pub fn point(&self) -> &RistrettoPoint {
        &self.point
    }
    
    /// Convert to compressed bytes
    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_SIZE] {
        self.point.compress().0
    }
    
    /// Generate address from public key (20 bytes from hash)
    pub fn to_address(&self) -> [u8; 20] {
        let hash = blake2b_hash(&self.to_bytes());
        let mut address = [0u8; 20];
        address.copy_from_slice(&hash.as_bytes()[..20]);
        address
    }
}

/// Key pair combining private and public keys
#[derive(Debug, Clone)]
pub struct KeyPair {
    private_key: PrivateKey,
    public_key: PublicKey,
}

impl KeyPair {
    /// Generate a new random key pair
    pub fn generate<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        let private_key = PrivateKey::generate(rng);
        let public_key = private_key.public_key();
        Self { private_key, public_key }
    }
    
    /// Create from existing private key
    pub fn from_private_key(private_key: PrivateKey) -> Self {
        let public_key = private_key.public_key();
        Self { private_key, public_key }
    }
    
    /// Get the private key
    pub fn private_key(&self) -> &PrivateKey {
        &self.private_key
    }
    
    /// Get the public key
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }
}

/// Curve25519 key pair implementation as specified
pub type Curve25519KeyPair = KeyPair;