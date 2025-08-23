use crate::config::*;
use crate::CatalystConfig;

/// Create main network configuration
pub fn mainnet_config() -> CatalystConfig {
    CatalystConfig {
        network: NetworkConfig::mainnet(),
        consensus: ConsensusConfig::mainnet(),
        crypto: CryptoConfig::mainnet(),
        storage: StorageConfig::mainnet(),
        service_bus: ServiceBusConfig::mainnet(),
        logging: LoggingConfig::mainnet(),
        metrics: MetricsConfig::mainnet(),
    }
}
