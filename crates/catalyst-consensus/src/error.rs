use catalyst_utils::CatalystError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConsensusError {
    #[error("Insufficient data collected: got {got}, required {required}")]
    InsufficientData { got: usize, required: usize },
    
    #[error("No majority found: highest count {highest} below threshold {threshold}{details}")]
    NoMajority { highest: usize, threshold: usize, details: String },
    
    #[error("Phase timeout: {phase} phase exceeded {duration_ms}ms")]
    PhaseTimeout { phase: String, duration_ms: u64 },
    
    #[error("Invalid producer: {producer_id} not in selected set")]
    InvalidProducer { producer_id: String },
    
    #[error("Consensus failed: {reason}")]
    ConsensusFailed { reason: String },
    
    #[error("Invalid partial ledger state update: {reason}")]
    InvalidPartialUpdate { reason: String },
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<ConsensusError> for CatalystError {
    fn from(err: ConsensusError) -> Self {
        CatalystError::Consensus {
            phase: "unknown".to_string(),
            message: err.to_string(),
        }
    }
}