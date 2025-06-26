use thiserror::Error;
use catalyst_utils::CatalystError;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("TOML parsing error: {0}")]
    Toml(#[from] toml::de::Error),
    
    #[error("Invalid format: {0}")]
    InvalidFormat(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("Environment error: {0}")]
    EnvironmentError(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("File not found: {0}")]
    FileNotFound(String),
    
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
    
    #[error("Invalid network: {0}")]
    InvalidNetwork(String),
    
    #[error("Catalyst error: {0}")]
    Catalyst(#[from] CatalystError),
}

pub type ConfigResult<T> = Result<T, ConfigError>;

impl From<ConfigError> for CatalystError {
    fn from(err: ConfigError) -> Self {
        CatalystError::Config(err.to_string())
    }
}