//! Minimal Catalyst RPC implementation
//! This is a placeholder implementation to get the build working

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RpcError {
    #[error("RPC error: {0}")]
    Generic(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    pub enabled: bool,
    pub port: u16,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 9933,
        }
    }
}

/// Minimal RPC server
pub struct RpcServer {
    config: RpcConfig,
}

impl RpcServer {
    pub fn new(config: RpcConfig) -> Self {
        Self { config }
    }
    
    pub async fn start(&self) -> Result<(), RpcError> {
        // Placeholder - no actual server
        Ok(())
    }
    
    pub async fn stop(&self) -> Result<(), RpcError> {
        Ok(())
    }
}

// Placeholder types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockInfo {
    pub number: u64,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionInfo {
    pub hash: String,
    pub status: String,
}

// Export everything so other crates can import
pub use RpcError as CatalystRpcError;
pub use RpcServer as CatalystRpcServer;