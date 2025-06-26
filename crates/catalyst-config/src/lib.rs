//! Catalyst Network Configuration Management
//! 
//! This crate provides configuration loading, validation, and hot-reloading
//! capabilities for all Catalyst Network components.

pub mod config;
pub mod loader;
pub mod networks;
pub mod hot_reload;
pub mod error;
pub mod utils;

// Re-exports for convenience
pub use config::*;
pub use loader::*;
pub use hot_reload::*;
pub use utils::ConfigUtils;

// Re-export main types
pub use error::{ConfigError, ConfigResult};

// Re-export network configurations
pub use networks::{NetworkType, devnet_config, testnet_config, mainnet_config};
