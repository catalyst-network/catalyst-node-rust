use std::env;
use std::str::FromStr;
use catalyst_utils::logging::LogLevel as UtilsLogLevel;
use crate::config::logging::LogLevel;
use crate::config::logging::LogFormat;
use crate::networks::NetworkType;
use crate::{CatalystConfig, ConfigError, ConfigResult};

/// Environment variable-based configuration loader
pub struct EnvLoader;

impl EnvLoader {
    /// Load configuration from environment variables
    pub fn load_from_env() -> ConfigResult<CatalystConfig> {
        // Start with default network type from env or default to devnet
        let network_type = Self::get_network_type()?;
        let mut config = CatalystConfig::new_for_network(network_type);
        
        // Override with environment variables
        Self::apply_network_overrides(&mut config)?;
        Self::apply_consensus_overrides(&mut config)?;
        Self::apply_crypto_overrides(&mut config)?;
        Self::apply_storage_overrides(&mut config)?;
        Self::apply_service_bus_overrides(&mut config)?;
        Self::apply_logging_overrides(&mut config)?;
        Self::apply_metrics_overrides(&mut config)?;
        
        config.validate()?;
        Ok(config)
    }
    
    /// Get network type from environment
    fn get_network_type() -> ConfigResult<NetworkType> {
        match env::var("CATALYST_NETWORK") {
            Ok(network_str) => {
                NetworkType::from_str(&network_str)
                    .map_err(|e| ConfigError::EnvironmentError(format!("Invalid CATALYST_NETWORK: {}", e)))
            }
            Err(_) => Ok(NetworkType::Devnet), // Default to devnet
        }
    }
    
    /// Apply network configuration overrides from environment
    fn apply_network_overrides(config: &mut CatalystConfig) -> ConfigResult<()> {
        if let Ok(port) = env::var("CATALYST_NETWORK_P2P_PORT") {
            config.network.p2p_port = port.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_NETWORK_P2P_PORT".to_string()))?;
        }
        
        if let Ok(port) = env::var("CATALYST_NETWORK_API_PORT") {
            config.network.api_port = port.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_NETWORK_API_PORT".to_string()))?;
        }
        
        if let Ok(max_peers) = env::var("CATALYST_NETWORK_MAX_PEERS") {
            config.network.max_peers = max_peers.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_NETWORK_MAX_PEERS".to_string()))?;
        }
        
        if let Ok(min_peers) = env::var("CATALYST_NETWORK_MIN_PEERS") {
            config.network.min_peers = min_peers.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_NETWORK_MIN_PEERS".to_string()))?;
        }
        
        if let Ok(timeout) = env::var("CATALYST_NETWORK_CONNECTION_TIMEOUT_MS") {
            config.network.connection_timeout_ms = timeout.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_NETWORK_CONNECTION_TIMEOUT_MS".to_string()))?;
        }
        
        if let Ok(ipv6) = env::var("CATALYST_NETWORK_IPV6_ENABLED") {
            config.network.ipv6_enabled = ipv6.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_NETWORK_IPV6_ENABLED".to_string()))?;
        }
        
        // Bootstrap peers (comma-separated)
        if let Ok(peers) = env::var("CATALYST_NETWORK_BOOTSTRAP_PEERS") {
            config.network.bootstrap_peers = peers
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        
        Ok(())
    }
    
    /// Apply consensus configuration overrides from environment
    fn apply_consensus_overrides(config: &mut CatalystConfig) -> ConfigResult<()> {
        if let Ok(cycle_duration) = env::var("CATALYST_CONSENSUS_CYCLE_DURATION_MS") {
            config.consensus.cycle_duration_ms = cycle_duration.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_CONSENSUS_CYCLE_DURATION_MS".to_string()))?;
        }
        
        if let Ok(construction_phase) = env::var("CATALYST_CONSENSUS_CONSTRUCTION_PHASE_MS") {
            config.consensus.construction_phase_ms = construction_phase.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_CONSENSUS_CONSTRUCTION_PHASE_MS".to_string()))?;
        }
        
        if let Ok(campaigning_phase) = env::var("CATALYST_CONSENSUS_CAMPAIGNING_PHASE_MS") {
            config.consensus.campaigning_phase_ms = campaigning_phase.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_CONSENSUS_CAMPAIGNING_PHASE_MS".to_string()))?;
        }
        
        if let Ok(voting_phase) = env::var("CATALYST_CONSENSUS_VOTING_PHASE_MS") {
            config.consensus.voting_phase_ms = voting_phase.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_CONSENSUS_VOTING_PHASE_MS".to_string()))?;
        }
        
        if let Ok(sync_phase) = env::var("CATALYST_CONSENSUS_SYNCHRONIZATION_PHASE_MS") {
            config.consensus.synchronization_phase_ms = sync_phase.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_CONSENSUS_SYNCHRONIZATION_PHASE_MS".to_string()))?;
        }
        
        if let Ok(producer_count) = env::var("CATALYST_CONSENSUS_PRODUCER_COUNT") {
            config.consensus.producer_count = producer_count.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_CONSENSUS_PRODUCER_COUNT".to_string()))?;
        }
        
        if let Ok(threshold) = env::var("CATALYST_CONSENSUS_SUPERMAJORITY_THRESHOLD") {
            config.consensus.supermajority_threshold = threshold.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_CONSENSUS_SUPERMAJORITY_THRESHOLD".to_string()))?;
        }
        
        Ok(())
    }
    
    /// Apply crypto configuration overrides from environment
    fn apply_crypto_overrides(config: &mut CatalystConfig) -> ConfigResult<()> {
        if let Ok(iterations) = env::var("CATALYST_CRYPTO_KDF_ITERATIONS") {
            config.crypto.key_derivation.iterations = iterations.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_CRYPTO_KDF_ITERATIONS".to_string()))?;
        }
        
        if let Ok(enabled) = env::var("CATALYST_CRYPTO_SIGNATURE_AGGREGATION_ENABLED") {
            config.crypto.signature_aggregation.enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_CRYPTO_SIGNATURE_AGGREGATION_ENABLED".to_string()))?;
        }
        
        if let Ok(max_sigs) = env::var("CATALYST_CRYPTO_MAX_SIGNATURES_PER_AGGREGATION") {
            config.crypto.signature_aggregation.max_signatures_per_aggregation = max_sigs.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_CRYPTO_MAX_SIGNATURES_PER_AGGREGATION".to_string()))?;
        }
        
        if let Ok(enabled) = env::var("CATALYST_CRYPTO_CONFIDENTIAL_TRANSACTIONS_ENABLED") {
            config.crypto.confidential_transactions.enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_CRYPTO_CONFIDENTIAL_TRANSACTIONS_ENABLED".to_string()))?;
        }
        
        if let Ok(enabled) = env::var("CATALYST_CRYPTO_CONSTANT_TIME_OPERATIONS") {
            config.crypto.security.constant_time_operations = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_CRYPTO_CONSTANT_TIME_OPERATIONS".to_string()))?;
        }
        
        Ok(())
    }
    
    /// Apply storage configuration overrides from environment
    fn apply_storage_overrides(config: &mut CatalystConfig) -> ConfigResult<()> {
        if let Ok(data_dir) = env::var("CATALYST_STORAGE_DATA_DIRECTORY") {
            config.storage.database.data_directory = data_dir.into();
        }
        
        if let Ok(max_size) = env::var("CATALYST_STORAGE_MAX_DB_SIZE_BYTES") {
            let size: u64 = max_size.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_STORAGE_MAX_DB_SIZE_BYTES".to_string()))?;
            config.storage.database.max_db_size_bytes = Some(size);
        }
        
        if let Ok(buffer_size) = env::var("CATALYST_STORAGE_WRITE_BUFFER_SIZE") {
            config.storage.database.write_buffer_size = buffer_size.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_STORAGE_WRITE_BUFFER_SIZE".to_string()))?;
        }
        
        if let Ok(cache_size) = env::var("CATALYST_STORAGE_BLOCK_CACHE_SIZE") {
            config.storage.database.block_cache_size = cache_size.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_STORAGE_BLOCK_CACHE_SIZE".to_string()))?;
        }
        
        if let Ok(threads) = env::var("CATALYST_STORAGE_BACKGROUND_THREADS") {
            config.storage.database.background_threads = threads.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_STORAGE_BACKGROUND_THREADS".to_string()))?;
        }
        
        if let Ok(enabled) = env::var("CATALYST_STORAGE_COMPRESSION_ENABLED") {
            config.storage.database.compression_enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_STORAGE_COMPRESSION_ENABLED".to_string()))?;
        }
        
        if let Ok(enabled) = env::var("CATALYST_STORAGE_DFS_ENABLED") {
            config.storage.dfs.enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_STORAGE_DFS_ENABLED".to_string()))?;
        }
        
        if let Ok(replication) = env::var("CATALYST_STORAGE_DFS_REPLICATION_FACTOR") {
            config.storage.dfs.replication_factor = replication.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_STORAGE_DFS_REPLICATION_FACTOR".to_string()))?;
        }
        
        Ok(())
    }
    
    /// Apply service bus configuration overrides from environment
    fn apply_service_bus_overrides(config: &mut CatalystConfig) -> ConfigResult<()> {
        if let Ok(enabled) = env::var("CATALYST_SERVICE_BUS_ENABLED") {
            config.service_bus.enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_SERVICE_BUS_ENABLED".to_string()))?;
        }
        
        if let Ok(port) = env::var("CATALYST_SERVICE_BUS_WEBSOCKET_PORT") {
            config.service_bus.websocket.port = port.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_SERVICE_BUS_WEBSOCKET_PORT".to_string()))?;
        }
        
        if let Ok(port) = env::var("CATALYST_SERVICE_BUS_REST_API_PORT") {
            config.service_bus.rest_api.port = port.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_SERVICE_BUS_REST_API_PORT".to_string()))?;
        }
        
        if let Ok(max_connections) = env::var("CATALYST_SERVICE_BUS_MAX_CONNECTIONS") {
            config.service_bus.websocket.max_connections = max_connections.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_SERVICE_BUS_MAX_CONNECTIONS".to_string()))?;
        }
        
        if let Ok(buffer_size) = env::var("CATALYST_SERVICE_BUS_EVENT_BUFFER_SIZE") {
            config.service_bus.event_processing.buffer.size = buffer_size.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_SERVICE_BUS_EVENT_BUFFER_SIZE".to_string()))?;
        }
        
        if let Ok(enabled) = env::var("CATALYST_SERVICE_BUS_AUTH_ENABLED") {
            config.service_bus.auth.enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_SERVICE_BUS_AUTH_ENABLED".to_string()))?;
        }
        
        if let Ok(enabled) = env::var("CATALYST_SERVICE_BUS_RATE_LIMITING_ENABLED") {
            config.service_bus.rate_limiting.enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_SERVICE_BUS_RATE_LIMITING_ENABLED".to_string()))?;
        }
        
        Ok(())
    }
    
    /// Apply logging configuration overrides from environment
    fn apply_logging_overrides(config: &mut CatalystConfig) -> ConfigResult<()> {
        if let Ok(level_str) = env::var("CATALYST_LOG_LEVEL") {
            config.logging.level = match level_str.to_lowercase().as_str() {
                "trace" => LogLevel::Trace,
                "debug" => LogLevel::Debug,
                "info" => LogLevel::Info,
                "warn" | "warning" => LogLevel::Warn,
                "error" => LogLevel::Error,
                "critical" => LogLevel::Critical,
                "off" => LogLevel::Off,
                _ => return Err(ConfigError::EnvironmentError(format!("Invalid CATALYST_LOG_LEVEL: {}", level_str))),
            };
        }
        
        if let Ok(format_str) = env::var("CATALYST_LOG_FORMAT") {
            config.logging.format = match format_str.to_lowercase().as_str() {
                "text" => LogFormat::Text,
                "json" => LogFormat::Json,
                "compact" => LogFormat::Compact,
                _ => return Err(ConfigError::EnvironmentError(format!("Invalid CATALYST_LOG_FORMAT: {}", format_str))),
            };
        }
        
        if let Ok(enabled) = env::var("CATALYST_LOG_STRUCTURED_ENABLED") {
            config.logging.structured.enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_LOG_STRUCTURED_ENABLED".to_string()))?;
        }
        
        if let Ok(enabled) = env::var("CATALYST_LOG_ROTATION_ENABLED") {
            config.logging.rotation.enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_LOG_ROTATION_ENABLED".to_string()))?;
        }
        
        if let Ok(max_size) = env::var("CATALYST_LOG_MAX_FILE_SIZE_BYTES") {
            config.logging.rotation.max_file_size_bytes = max_size.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_LOG_MAX_FILE_SIZE_BYTES".to_string()))?;
        }
        
        if let Ok(enabled) = env::var("CATALYST_LOG_ASYNC_ENABLED") {
            config.logging.performance.async_logging = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_LOG_ASYNC_ENABLED".to_string()))?;
        }
        
        Ok(())
    }
    
    /// Apply metrics configuration overrides from environment
    fn apply_metrics_overrides(config: &mut CatalystConfig) -> ConfigResult<()> {
        if let Ok(enabled) = env::var("CATALYST_METRICS_ENABLED") {
            config.metrics.enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_METRICS_ENABLED".to_string()))?;
        }
        
        if let Ok(port) = env::var("CATALYST_METRICS_PORT") {
            config.metrics.server.port = port.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_METRICS_PORT".to_string()))?;
        }
        
        if let Ok(bind_addr) = env::var("CATALYST_METRICS_BIND_ADDRESS") {
            config.metrics.server.bind_address = bind_addr;
        }
        
        if let Ok(interval) = env::var("CATALYST_METRICS_COLLECTION_INTERVAL_SECONDS") {
            config.metrics.collection.collection_interval_seconds = interval.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_METRICS_COLLECTION_INTERVAL_SECONDS".to_string()))?;
        }
        
        if let Ok(enabled) = env::var("CATALYST_METRICS_HIGH_FREQUENCY_ENABLED") {
            config.metrics.collection.high_frequency_enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_METRICS_HIGH_FREQUENCY_ENABLED".to_string()))?;
        }
        
        if let Ok(enabled) = env::var("CATALYST_METRICS_EXPORT_ENABLED") {
            config.metrics.export.enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_METRICS_EXPORT_ENABLED".to_string()))?;
        }
        
        if let Ok(enabled) = env::var("CATALYST_METRICS_ALERTING_ENABLED") {
            config.metrics.alerting.enabled = enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_METRICS_ALERTING_ENABLED".to_string()))?;
        }
        
        if let Ok(auth_enabled) = env::var("CATALYST_METRICS_AUTH_ENABLED") {
            config.metrics.server.auth_enabled = auth_enabled.parse()
                .map_err(|_| ConfigError::EnvironmentError("Invalid CATALYST_METRICS_AUTH_ENABLED".to_string()))?;
        }
        
        if let Ok(auth_token) = env::var("CATALYST_METRICS_AUTH_TOKEN") {
            config.metrics.server.auth_token = Some(auth_token);
        }
        
        Ok(())
    }
    
    /// Get a typed environment variable value
    pub fn get_env_var<T: FromStr>(key: &str) -> ConfigResult<Option<T>> {
        match env::var(key) {
            Ok(value) => {
                value.parse()
                    .map(Some)
                    .map_err(|_| ConfigError::EnvironmentError(format!("Invalid value for {}: {}", key, value)))
            }
            Err(env::VarError::NotPresent) => Ok(None),
            Err(env::VarError::NotUnicode(_)) => {
                Err(ConfigError::EnvironmentError(format!("Non-Unicode value for {}", key)))
            }
        }
    }
    
    /// Get a required environment variable
    pub fn get_required_env_var<T: FromStr>(key: &str) -> ConfigResult<T> {
        let value = env::var(key)
            .map_err(|_| ConfigError::EnvironmentError(format!("Required environment variable {} not found", key)))?;
        
        value.parse()
            .map_err(|_| ConfigError::EnvironmentError(format!("Invalid value for {}: {}", key, value)))
    }
    
    /// Check if running in a specific environment (development, testing, production)
    pub fn get_environment() -> String {
        env::var("CATALYST_ENVIRONMENT")
            .or_else(|_| env::var("ENVIRONMENT"))
            .or_else(|_| env::var("ENV"))
            .unwrap_or_else(|_| "development".to_string())
            .to_lowercase()
    }
    
    /// Get the node ID from environment
    pub fn get_node_id() -> ConfigResult<Option<String>> {
        match env::var("CATALYST_NODE_ID") {
            Ok(node_id) => {
                if node_id.is_empty() {
                    Err(ConfigError::EnvironmentError("CATALYST_NODE_ID cannot be empty".to_string()))
                } else {
                    Ok(Some(node_id))
                }
            }
            Err(_) => Ok(None),
        }
    }
    
    /// List all Catalyst-related environment variables
    pub fn list_catalyst_env_vars() -> Vec<(String, String)> {
        env::vars()
            .filter(|(key, _)| key.starts_with("CATALYST_"))
            .collect()
    }
}