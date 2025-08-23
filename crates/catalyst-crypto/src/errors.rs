use thiserror::Error;

/// Cryptographic operation errors
#[derive(Error, Debug, Clone, PartialEq)]
pub enum CryptoError {
    #[error("Invalid key format: {0}")]
    InvalidKey(String),

    #[error("Invalid signature format: {0}")]
    InvalidSignature(String),

    #[error("Signature verification failed")]
    SignatureVerificationFailed,

    #[error("Invalid point encoding")]
    InvalidPointEncoding,

    #[error("Invalid scalar encoding")]
    InvalidScalarEncoding,

    #[error("Commitment verification failed")]
    CommitmentVerificationFailed,

    #[error("Invalid commitment format: {0}")]
    InvalidCommitment(String),

    #[error("Aggregation failed: {0}")]
    AggregationFailed(String),

    #[error("Insufficient randomness")]
    InsufficientRandomness,

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

pub type CryptoResult<T> = Result<T, CryptoError>;
