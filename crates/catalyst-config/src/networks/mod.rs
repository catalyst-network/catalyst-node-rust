//! Network-specific configuration presets

pub mod devnet;
pub mod mainnet;
pub mod testnet;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkType {
    Devnet,
    Testnet,
    Mainnet,
}

impl std::str::FromStr for NetworkType {
    type Err = crate::error::ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "devnet" | "dev" => Ok(NetworkType::Devnet),
            "testnet" | "test" => Ok(NetworkType::Testnet),
            "mainnet" | "main" => Ok(NetworkType::Mainnet),
            _ => Err(crate::error::ConfigError::InvalidNetwork(s.to_string())),
        }
    }
}

// Re-export network configurations
pub use devnet::devnet_config;
pub use mainnet::mainnet_config;
pub use testnet::testnet_config;
