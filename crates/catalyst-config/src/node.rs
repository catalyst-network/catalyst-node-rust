//! Node configuration structures

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::ConfigError;

/// Node role types as defined in the Catalyst protocol
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum NodeRole {
    /// User node - default state, receives and forwards transactions
    User,
    /// Reservist node - signaled intent to perform work
    Reservist,
    /// Worker node - granted worker pass for finite period
    Worker,
    /// Producer node - selected to perform work for specific ledger cycle
    Producer,
    /// Storage node - provides distributed file storage
    Storage,
}

impl Default for NodeRole {
    fn default() -> Self {
        NodeRole::User
    }
}

/// Node configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    /// Node identifier (will be generated if not provided)
    pub node_id: Option<String>,
    /// Node role
    pub role: NodeRole,
    /// Data directory for node storage
    pub data_dir: PathBuf,
    /// Key file path for node identity
    pub key_file: Option<PathBuf>,
    /// Enable validator functionality
    pub enable_validator: bool,
    /// Enable storage provider functionality
    pub enable_storage: bool,
    /// Enable RPC server
    pub enable_rpc: bool,
    /// RPC server bind address
    pub rpc_bind: String,
    /// RPC server port
    pub rpc_port: u16,
    /// Enable metrics collection
    pub enable_metrics: bool,
    /// Metrics bind address
    pub metrics_bind: String,
    /// Metrics port
    pub metrics_port: u16,
    /// Log level
    pub log_level: String,
    /// Log file path (None for stdout)
    pub log_file: Option<PathBuf>,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            node_id: None,
            role: NodeRole::default(),
            data_dir: PathBuf::from("./data"),
            key_file: None,
            enable_validator: false,
            enable_storage: false,
            enable_rpc: true,
            rpc_bind: "127.0.0.1".to_string(),
            rpc_port: 9933,
            enable_metrics: false,
            metrics_bind: "127.0.0.1".to_string(),
            metrics_port: 9615,
            log_level: "info".to_string(),
            log_file: None,
        }
    }
}

impl NodeConfig {
    /// Validate node configuration
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate log level
        match self.log_level.as_str() {
            "error" | "warn" | "info" | "debug" | "trace" => {},
            _ => return Err(ConfigError::Invalid(
                format!("Invalid log level: {}", self.log_level)
            )),
        }

        // Validate RPC port
        if self.enable_rpc && self.rpc_port == 0 {
            return Err(ConfigError::Invalid(
                "RPC port cannot be 0 when RPC is enabled".to_string()
            ));
        }

        // Validate metrics port
        if self.enable_metrics && self.metrics_port == 0 {
            return Err(ConfigError::Invalid(
                "Metrics port cannot be 0 when metrics are enabled".to_string()
            ));
        }

        // Validate data directory exists or can be created
        if !self.data_dir.exists() {
            std::fs::create_dir_all(&self.data_dir)
                .map_err(|e| ConfigError::Invalid(
                    format!("Cannot create data directory: {}", e)
                ))?;
        }

        Ok(())
    }

    /// Get the full RPC address
    pub fn rpc_address(&self) -> String {
        format!("{}:{}", self.rpc_bind, self.rpc_port)
    }

    /// Get the full metrics address
    pub fn metrics_address(&self) -> String {
        format!("{}:{}", self.metrics_bind, self.metrics_port)
    }
}