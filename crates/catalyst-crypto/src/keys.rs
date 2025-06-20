use ed25519_dalek::{SigningKey, VerifyingKey, Signature, Signer, Verifier};
use crate::{CryptoError, Bytes32, Bytes64};

#[derive(Debug, Clone)]
pub struct PrivateKey {
    inner: SigningKey,
}

// Custom serialization for PrivateKey
impl serde::Serialize for PrivateKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bytes = self.inner.to_bytes();
        bytes.to_vec().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("Invalid private key length"));
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&bytes);
        let inner = SigningKey::from_bytes(&key_bytes);
        Ok(Self { inner })
    }
}

impl PrivateKey {
    pub fn generate() -> Self {
        let inner = SigningKey::generate(&mut rand::thread_rng());
        Self { inner }
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKey);
        }
        
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(bytes);
        
        let inner = SigningKey::from_bytes(&key_bytes);
        Ok(Self { inner })
    }

    pub fn to_bytes(&self) -> Bytes32 {
        self.inner.to_bytes()
    }

    pub fn public_key(&self) -> PublicKey {
        PublicKey {
            inner: self.inner.verifying_key(),
        }
    }

    pub fn sign(&self, message: &[u8]) -> CryptoSignature {
        CryptoSignature {
            inner: self.inner.sign(message),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublicKey {
    inner: VerifyingKey,
}

// Custom serialization for PublicKey
impl serde::Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bytes = self.inner.to_bytes();
        bytes.to_vec().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        if bytes.len() != 32 {
            return Err(serde::de::Error::custom("Invalid public key length"));
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&bytes);
        match VerifyingKey::from_bytes(&key_bytes) {
            Ok(inner) => Ok(Self { inner }),
            Err(_) => Err(serde::de::Error::custom("Invalid public key bytes")),
        }
    }
}

impl PublicKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != 32 {
            return Err(CryptoError::InvalidKey);
        }
        
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(bytes);
        
        match VerifyingKey::from_bytes(&key_bytes) {
            Ok(inner) => Ok(Self { inner }),
            Err(_) => Err(CryptoError::InvalidKey),
        }
    }

    pub fn to_bytes(&self) -> Bytes32 {
        self.inner.to_bytes()
    }

    pub fn verify(&self, message: &[u8], signature: &CryptoSignature) -> Result<(), CryptoError> {
        self.inner
            .verify(message, &signature.inner)
            .map_err(|_| CryptoError::SignatureVerificationFailed)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CryptoSignature {
    inner: Signature,
}

// Custom serialization for CryptoSignature
impl serde::Serialize for CryptoSignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let bytes = self.inner.to_bytes();
        bytes.to_vec().serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for CryptoSignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom("Invalid signature length"));
        }
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(&bytes);
        let inner = Signature::from_bytes(&sig_bytes);
        Ok(Self { inner })
    }
}

impl CryptoSignature {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CryptoError> {
        if bytes.len() != 64 {
            return Err(CryptoError::InvalidSignature);
        }
        
        let mut sig_bytes = [0u8; 64];
        sig_bytes.copy_from_slice(bytes);
        
        let inner = Signature::from_bytes(&sig_bytes);
        Ok(Self { inner })
    }

    pub fn to_bytes(&self) -> Bytes64 {
        self.inner.to_bytes()
    }
}

// Key pair for convenience
#[derive(Debug, Clone)]
pub struct KeyPair {
    pub private_key: PrivateKey,
    pub public_key: PublicKey,
}

impl KeyPair {
    pub fn generate() -> Self {
        let private_key = PrivateKey::generate();
        let public_key = private_key.public_key();
        
        Self {
            private_key,
            public_key,
        }
    }
    
    pub fn sign(&self, message: &[u8]) -> CryptoSignature {
        self.private_key.sign(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_generation() {
        let keypair = KeyPair::generate();
        let message = b"test message";
        
        let signature = keypair.sign(message);
        assert!(keypair.public_key.verify(message, &signature).is_ok());
    }

    #[test]
    fn test_key_serialization() {
        let keypair = KeyPair::generate();
        
        let private_bytes = keypair.private_key.to_bytes();
        let public_bytes = keypair.public_key.to_bytes();
        
        let restored_private = PrivateKey::from_bytes(&private_bytes).unwrap();
        let restored_public = PublicKey::from_bytes(&public_bytes).unwrap();
        
        assert_eq!(keypair.public_key.to_bytes(), restored_public.to_bytes());
        assert_eq!(restored_private.public_key().to_bytes(), restored_public.to_bytes());
    }
}