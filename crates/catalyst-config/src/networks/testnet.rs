use crate::config::*;
use crate::CatalystConfig;

/// Create test network configuration
pub fn testnet_config() -> CatalystConfig {
    CatalystConfig {
        network: NetworkConfig::testnet(),
        consensus: ConsensusConfig::testnet(),
        crypto: CryptoConfig::testnet(),
        storage: StorageConfig::testnet(),
        service_bus: ServiceBusConfig::testnet(),
        logging: LoggingConfig::testnet(),
        metrics: MetricsConfig::testnet(),
    }
}
