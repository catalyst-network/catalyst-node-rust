//! Catalyst Network Configuration Management
//!
//! This crate provides configuration loading, validation, and hot-reloading
//! capabilities for all Catalyst Network components.

pub mod config;
pub mod error;
pub mod hot_reload;
pub mod loader;
pub mod networks;
pub mod utils;

// Re-exports for convenience
pub use config::*;
// Re-export main types
pub use error::{ConfigError, ConfigResult};
pub use hot_reload::*;
pub use loader::*;
// Re-export network configurations
pub use networks::{devnet_config, mainnet_config, testnet_config, NetworkType};
pub use utils::ConfigUtils;
