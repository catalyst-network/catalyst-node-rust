use crate::config::*;
use crate::CatalystConfig;

/// Create development network configuration
pub fn devnet_config() -> CatalystConfig {
    CatalystConfig {
        network: NetworkConfig::devnet(),
        consensus: ConsensusConfig::devnet(),
        crypto: CryptoConfig::devnet(),
        storage: StorageConfig::devnet(),
        service_bus: ServiceBusConfig::devnet(),
        logging: LoggingConfig::devnet(),
        metrics: MetricsConfig::devnet(),
    }
}
