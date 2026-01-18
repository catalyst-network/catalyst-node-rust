use crate::{CryptoError, CryptoResult, Hash256, blake2b_hash, PrivateKey, PublicKey};
use curve25519_dalek::{
    scalar::Scalar,
    ristretto::{RistrettoPoint, CompressedRistretto},
    constants::RISTRETTO_BASEPOINT_POINT,
};
use rand::{RngCore, CryptoRng};
use serde::{Deserialize, Serialize};

pub const SIGNATURE_SIZE: usize = 64;

// Helper macro for array references
macro_rules! array_ref {
    ($arr:expr, $offset:expr, $len:expr) => {{
        {
            #[inline]
            unsafe fn as_array<T>(slice: &[T]) -> &[T; $len] {
                &*(slice.as_ptr() as *const [_; $len])
            }
            let offset = $offset;
            let slice = &$arr[offset..offset + $len];
            unsafe { as_array(slice) }
        }
    }};
}

/// Basic Schnorr signature
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    pub r: RistrettoPoint,
    pub s: Scalar,
}

impl Signature {
    /// Create signature from components
    pub fn new(r: RistrettoPoint, s: Scalar) -> Self {
        Self { r, s }
    }
    
    /// Serialize signature to bytes
    pub fn to_bytes(&self) -> [u8; SIGNATURE_SIZE] {
        let mut bytes = [0u8; SIGNATURE_SIZE];
        bytes[..32].copy_from_slice(&self.r.compress().0);
        bytes[32..].copy_from_slice(self.s.as_bytes()); // Fixed: removed extra &
        bytes
    }
    
    /// Deserialize signature from bytes
    pub fn from_bytes(bytes: [u8; SIGNATURE_SIZE]) -> CryptoResult<Self> {
        let r_compressed = CompressedRistretto(*array_ref![bytes, 0, 32]);
        let r = r_compressed.decompress()
            .ok_or(CryptoError::InvalidSignature("Invalid R point".to_string()))?;
        
        let s_bytes = *array_ref![bytes, 32, 32];
        // Fixed: Handle CtOption properly
        let s = Scalar::from_canonical_bytes(s_bytes)
            .into_option()
            .ok_or(CryptoError::InvalidSignature("Invalid S scalar".to_string()))?;
        
        Ok(Self { r, s })
    }
}

/// MuSig aggregated signature implementation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MuSigSignature {
    pub aggregated_signature: Signature,
    pub aggregated_public_key: PublicKey,
    pub participant_count: usize,
}

impl MuSigSignature {
    /// Create new MuSig signature
    pub fn new(signature: Signature, public_key: PublicKey, participant_count: usize) -> Self {
        Self {
            aggregated_signature: signature,
            aggregated_public_key: public_key,
            participant_count,
        }
    }
    
    /// Verify the aggregated signature
    pub fn verify(&self, message: &[u8]) -> CryptoResult<bool> {
        let scheme = SignatureScheme::new();
        scheme.verify_aggregated(message, self)
    }
}

/// Signature scheme implementation for Catalyst Network
pub struct SignatureScheme;

impl SignatureScheme {
    pub fn new() -> Self {
        Self
    }
    
    /// Sign a message with a single key
    pub fn sign<R: RngCore + CryptoRng>(
        &self,
        rng: &mut R,
        private_key: &PrivateKey,
        message: &[u8],
    ) -> CryptoResult<Signature> {
        // Generate random nonce
        let mut nonce_bytes = [0u8; 32];
        rng.fill_bytes(&mut nonce_bytes);
        let k = Scalar::from_bytes_mod_order(nonce_bytes);
        
        // R = k * G
        let r = &k * &RISTRETTO_BASEPOINT_POINT;
        
        // Calculate challenge: e = H(R || P || m)
        let public_key = private_key.public_key();
        let challenge_data = [
            &r.compress().0[..],
            &public_key.to_bytes()[..],
            message,
        ].concat();
        let challenge_hash = blake2b_hash(&challenge_data);
        let e = Scalar::from_bytes_mod_order(*challenge_hash.as_bytes());
        
        // s = k + e * x
        let s = k + e * private_key.scalar();
        
        Ok(Signature { r, s })
    }
    
    /// Verify a single signature
    pub fn verify(
        &self,
        message: &[u8],
        signature: &Signature,
        public_key: &PublicKey,
    ) -> CryptoResult<bool> {
        // Calculate challenge: e = H(R || P || m)
        let challenge_data = [
            &signature.r.compress().0[..],
            &public_key.to_bytes()[..],
            message,
        ].concat();
        let challenge_hash = blake2b_hash(&challenge_data);
        let e = Scalar::from_bytes_mod_order(*challenge_hash.as_bytes());
        
        // Verify: s * G = R + e * P
        let left = &signature.s * &RISTRETTO_BASEPOINT_POINT;
        let right = signature.r + e * public_key.point();
        
        Ok(left == right)
    }
    
    /// Create MuSig aggregated signature from partial signatures
    pub fn aggregate_signatures(
        &self,
        _message: &[u8],
        partial_signatures: &[(Signature, PublicKey)],
    ) -> CryptoResult<MuSigSignature> {
        if partial_signatures.is_empty() {
            return Err(CryptoError::AggregationFailed("No signatures provided".to_string()));
        }
        
        // Aggregate public keys with MuSig coefficient
        let public_keys: Vec<&PublicKey> = partial_signatures.iter().map(|(_, pk)| pk).collect();
        let aggregated_pubkey = self.aggregate_public_keys(&public_keys)?;
        
        // Calculate MuSig coefficients for each participant
        let mut coefficients = Vec::new();
        let l = self.compute_public_key_list_hash(&public_keys)?;
        
        for pubkey in &public_keys {
            // Fixed: Use Vec to concatenate properly
            let mut coeff_data = Vec::new();
            coeff_data.extend_from_slice(l.as_bytes());
            coeff_data.extend_from_slice(&pubkey.to_bytes());
            let coeff_hash = blake2b_hash(&coeff_data);
            let coeff = Scalar::from_bytes_mod_order(*coeff_hash.as_bytes());
            coefficients.push(coeff);
        }
        
        // Aggregate R values and s values
        let mut aggregated_r = RistrettoPoint::default();
        let mut aggregated_s = Scalar::from(0u64); // Fixed: Use Scalar::from(0) instead of zero()
        
        for (i, (sig, _)) in partial_signatures.iter().enumerate() {
            aggregated_r += sig.r;
            aggregated_s += coefficients[i] * sig.s;
        }
        
        let aggregated_signature = Signature {
            r: aggregated_r,
            s: aggregated_s,
        };
        
        Ok(MuSigSignature::new(
            aggregated_signature,
            aggregated_pubkey,
            partial_signatures.len(),
        ))
    }
    
    /// Verify aggregated MuSig signature
    pub fn verify_aggregated(
        &self,
        message: &[u8],
        musig_signature: &MuSigSignature,
    ) -> CryptoResult<bool> {
        // Calculate challenge for aggregated signature
        let challenge_data = [
            &musig_signature.aggregated_signature.r.compress().0[..],
            &musig_signature.aggregated_public_key.to_bytes()[..],
            message,
        ].concat();
        let challenge_hash = blake2b_hash(&challenge_data);
        let e = Scalar::from_bytes_mod_order(*challenge_hash.as_bytes());
        
        // Verify: s * G = R + e * P_agg
        let left = &musig_signature.aggregated_signature.s * &RISTRETTO_BASEPOINT_POINT;
        let right = musig_signature.aggregated_signature.r + 
                   e * musig_signature.aggregated_public_key.point();
        
        Ok(left == right)
    }
    
    /// Aggregate public keys with MuSig coefficients
    fn aggregate_public_keys(&self, public_keys: &[&PublicKey]) -> CryptoResult<PublicKey> {
        if public_keys.is_empty() {
            return Err(CryptoError::AggregationFailed("No public keys provided".to_string()));
        }
        
        let l = self.compute_public_key_list_hash(public_keys)?;
        let mut aggregated_point = RistrettoPoint::default();
        
        for pubkey in public_keys {
            // Fixed: Use Vec to concatenate properly
            let mut coeff_data = Vec::new();
            coeff_data.extend_from_slice(l.as_bytes());
            coeff_data.extend_from_slice(&pubkey.to_bytes());
            let coeff_hash = blake2b_hash(&coeff_data);
            let coeff = Scalar::from_bytes_mod_order(*coeff_hash.as_bytes());
            
            aggregated_point += coeff * pubkey.point();
        }
        
        Ok(PublicKey::from_point(aggregated_point))
    }
    
    /// Compute hash of public key list for MuSig
    fn compute_public_key_list_hash(&self, public_keys: &[&PublicKey]) -> CryptoResult<Hash256> {
        let mut concatenated = Vec::new();
        for pubkey in public_keys {
            concatenated.extend_from_slice(&pubkey.to_bytes());
        }
        Ok(blake2b_hash(&concatenated))
    }
}

impl Default for SignatureScheme {
    fn default() -> Self {
        Self::new()
    }
}