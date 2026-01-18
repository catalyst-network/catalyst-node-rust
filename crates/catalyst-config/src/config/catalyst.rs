use crate::networks::NetworkType;
use serde::{Deserialize, Serialize};
use catalyst_utils::CatalystResult;

use super::*;

/// Main configuration structure for Catalyst Network
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalystConfig {
    /// Network-specific settings
    pub network: NetworkConfig,
    
    /// Consensus algorithm parameters
    pub consensus: ConsensusConfig,
    
    /// Cryptographic settings
    pub crypto: CryptoConfig,
    
    /// Storage configuration
    pub storage: StorageConfig,
    
    /// Service bus settings
    pub service_bus: ServiceBusConfig,
    
    /// Logging configuration
    pub logging: LoggingConfig,
    
    /// Metrics collection settings
    pub metrics: MetricsConfig,
}

impl CatalystConfig {
    /// Create a new configuration with defaults for the specified network
    pub fn new_for_network(network_type: crate::NetworkType) -> Self {
        match network_type {
            crate::NetworkType::Devnet => crate::networks::devnet_config(),
            crate::NetworkType::Testnet => crate::networks::testnet_config(),
            crate::NetworkType::Mainnet => crate::networks::mainnet_config(),
        }
    }
    
    /// Validate the configuration for consistency
    pub fn validate(&self) -> CatalystResult<()> {
        self.network.validate()?;
        self.consensus.validate()?;
        self.crypto.validate()?;
        self.storage.validate()?;
        self.service_bus.validate()?;
        self.logging.validate()?;
        self.metrics.validate()?;
        
        // Cross-validation between sections
        self.validate_cross_dependencies()?;
        
        Ok(())
    }
    
    /// Validate cross-dependencies between configuration sections
    fn validate_cross_dependencies(&self) -> CatalystResult<()> {
        // Example: Ensure consensus cycle fits within network timeouts
        // Add more cross-validations as needed
        Ok(())
    }
}

impl Default for CatalystConfig {
    fn default() -> Self {
        Self::new_for_network(crate::NetworkType::Devnet)
    }
}