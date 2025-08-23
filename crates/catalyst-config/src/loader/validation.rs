use crate::{CatalystConfig, ConfigError, ConfigResult};

/// Configuration validation utilities
pub struct ConfigValidator;

impl ConfigValidator {
    pub fn new() -> Self {
        Self
    }

    /// Perform comprehensive validation of a configuration
    pub fn validate_comprehensive(config: &CatalystConfig) -> ConfigResult<()> {
        // Basic validation first
        config.validate().map_err(|e| ConfigError::Catalyst(e))?;

        // Additional cross-section validations
        Self::validate_port_conflicts(config)?;
        Self::validate_resource_allocations(config)?;
        Self::validate_timing_constraints(config)?;
        Self::validate_security_requirements(config)?;
        Self::validate_performance_settings(config)?;

        Ok(())
    }

    pub fn validate(&self, config: &CatalystConfig) -> ConfigResult<()> {
        config.validate().map_err(|e| ConfigError::Catalyst(e))
    }

    /// Check for port conflicts across different services
    fn validate_port_conflicts(config: &CatalystConfig) -> ConfigResult<()> {
        let mut used_ports = std::collections::HashSet::new();

        // Network ports
        if !used_ports.insert(config.network.p2p_port) {
            return Err(ConfigError::ValidationFailed(format!(
                "Port conflict: {} is used by multiple services",
                config.network.p2p_port
            )));
        }

        if !used_ports.insert(config.network.api_port) {
            return Err(ConfigError::ValidationFailed(format!(
                "Port conflict: {} is used by multiple services",
                config.network.api_port
            )));
        }

        // Service bus ports
        if config.service_bus.enabled {
            if !used_ports.insert(config.service_bus.websocket.port) {
                return Err(ConfigError::ValidationFailed(format!(
                    "Port conflict: {} is used by multiple services",
                    config.service_bus.websocket.port
                )));
            }

            if !used_ports.insert(config.service_bus.rest_api.port) {
                return Err(ConfigError::ValidationFailed(format!(
                    "Port conflict: {} is used by multiple services",
                    config.service_bus.rest_api.port
                )));
            }
        }

        // Metrics port
        if config.metrics.enabled {
            if !used_ports.insert(config.metrics.server.port) {
                return Err(ConfigError::ValidationFailed(format!(
                    "Port conflict: {} is used by multiple services",
                    config.metrics.server.port
                )));
            }
        }

        Ok(())
    }

    /// Validate resource allocation consistency
    fn validate_resource_allocations(config: &CatalystConfig) -> ConfigResult<()> {
        // Check memory allocations don't exceed reasonable limits
        let total_cache_memory = config.storage.database.block_cache_size as u64
            + config.storage.cache.max_cache_size_bytes
            + config
                .metrics
                .performance
                .memory_management
                .max_memory_bytes;

        // Warn if total cache usage exceeds 2GB (reasonable for most systems)
        if total_cache_memory > 2 * 1024 * 1024 * 1024 {
            // This is a warning, not an error, so we just log it
            // In a real implementation, you might want to use the logging system
        }

        // Check thread allocations
        let total_threads = config.storage.database.background_threads
            + config.logging.performance.async_worker_threads
            + config.metrics.performance.concurrency.worker_threads;

        if total_threads > 100 {
            return Err(ConfigError::ValidationFailed(format!(
                "Total thread allocation ({}) exceeds reasonable limit",
                total_threads
            )));
        }

        Ok(())
    }

    /// Validate timing constraints and relationships
    fn validate_timing_constraints(config: &CatalystConfig) -> ConfigResult<()> {
        // Consensus timing validation
        let total_phase_time = config.consensus.construction_phase_ms
            + config.consensus.campaigning_phase_ms
            + config.consensus.voting_phase_ms
            + config.consensus.synchronization_phase_ms;

        if total_phase_time != config.consensus.cycle_duration_ms {
            return Err(ConfigError::ValidationFailed(format!(
                "Consensus phase times ({}) don't sum to cycle duration ({})",
                total_phase_time, config.consensus.cycle_duration_ms
            )));
        }

        // Network timeouts should be reasonable relative to consensus timing
        if config.network.connection_timeout_ms > config.consensus.cycle_duration_ms / 2 {
            return Err(ConfigError::ValidationFailed(
                "Network connection timeout is too large relative to consensus cycle duration"
                    .to_string(),
            ));
        }

        // Service bus timeouts
        if config.service_bus.enabled {
            if config.service_bus.websocket.connection_timeout_ms
                > config.consensus.cycle_duration_ms
            {
                return Err(ConfigError::ValidationFailed(
                    "WebSocket connection timeout exceeds consensus cycle duration".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Validate security requirements
    fn validate_security_requirements(config: &CatalystConfig) -> ConfigResult<()> {
        // Production security checks
        if config.network.name == "mainnet" {
            // Ensure strong crypto settings for mainnet
            if !config.crypto.security.constant_time_operations {
                return Err(ConfigError::ValidationFailed(
                    "Constant time operations must be enabled for mainnet".to_string(),
                ));
            }

            if !config.crypto.security.secure_memory_cleanup {
                return Err(ConfigError::ValidationFailed(
                    "Secure memory cleanup must be enabled for mainnet".to_string(),
                ));
            }

            // Check key derivation iterations
            if config.crypto.key_derivation.iterations < 10000 {
                return Err(ConfigError::ValidationFailed(
                    "Key derivation iterations too low for mainnet (minimum 10000)".to_string(),
                ));
            }

            // Ensure storage encryption for mainnet
            if !config.storage.security.encryption_at_rest {
                return Err(ConfigError::ValidationFailed(
                    "Storage encryption at rest must be enabled for mainnet".to_string(),
                ));
            }

            // Service bus security
            if config.service_bus.enabled && !config.service_bus.auth.enabled {
                return Err(ConfigError::ValidationFailed(
                    "Service bus authentication must be enabled for mainnet".to_string(),
                ));
            }

            // Metrics security
            if config.metrics.enabled && !config.metrics.server.auth_enabled {
                return Err(ConfigError::ValidationFailed(
                    "Metrics server authentication must be enabled for mainnet".to_string(),
                ));
            }
        }

        // JWT secret validation
        if config.service_bus.enabled && config.service_bus.auth.enabled {
            if config.service_bus.auth.jwt.secret_key.len() < 32 {
                return Err(ConfigError::ValidationFailed(
                    "JWT secret key must be at least 32 characters".to_string(),
                ));
            }

            if config
                .service_bus
                .auth
                .jwt
                .secret_key
                .contains("CHANGE_THIS")
                || config
                    .service_bus
                    .auth
                    .jwt
                    .secret_key
                    .contains("dev_secret")
            {
                return Err(ConfigError::ValidationFailed(
                    "JWT secret key must be changed from default value".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Validate performance settings
    fn validate_performance_settings(config: &CatalystConfig) -> ConfigResult<()> {
        // Consensus performance validation
        if config.consensus.producer_count > config.consensus.worker_pool.max_worker_pool_size {
            return Err(ConfigError::ValidationFailed(
                "Producer count cannot exceed worker pool size".to_string(),
            ));
        }

        // Network performance validation
        if config.network.max_peers < config.network.min_peers {
            return Err(ConfigError::ValidationFailed(
                "Max peers cannot be less than min peers".to_string(),
            ));
        }

        // Service bus performance validation
        if config.service_bus.enabled {
            let buffer_size = config.service_bus.event_processing.buffer.size;
            let batch_size = config.service_bus.event_processing.buffer.max_batch_size;

            if batch_size > buffer_size {
                return Err(ConfigError::ValidationFailed(
                    "Event processing batch size cannot exceed buffer size".to_string(),
                ));
            }
        }

        // Storage performance validation
        if config.storage.cache.enabled {
            let total_cache_layers = config.storage.cache.layers.l1_size_bytes
                + config.storage.cache.layers.l2_size_bytes
                + config.storage.cache.layers.l3_size_bytes;

            if total_cache_layers > config.storage.cache.max_cache_size_bytes {
                return Err(ConfigError::ValidationFailed(
                    "Sum of cache layer sizes exceeds total cache size".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Validate configuration for a specific network type
    pub fn validate_for_network(
        config: &CatalystConfig,
        expected_network: &str,
    ) -> ConfigResult<()> {
        if config.network.name != expected_network {
            return Err(ConfigError::ValidationFailed(format!(
                "Expected network '{}' but found '{}'",
                expected_network, config.network.name
            )));
        }

        // Network-specific validations
        match expected_network {
            "devnet" => Self::validate_devnet_specific(config)?,
            "testnet" => Self::validate_testnet_specific(config)?,
            "mainnet" => Self::validate_mainnet_specific(config)?,
            _ => {
                return Err(ConfigError::ValidationFailed(format!(
                    "Unknown network type: {}",
                    expected_network
                )))
            }
        }

        Ok(())
    }

    fn validate_devnet_specific(config: &CatalystConfig) -> ConfigResult<()> {
        // Devnet should have fast consensus cycles
        if config.consensus.cycle_duration_ms > 30000 {
            return Err(ConfigError::ValidationFailed(
                "Devnet consensus cycles should be fast (≤30s) for development".to_string(),
            ));
        }

        // Devnet should have small producer pools
        if config.consensus.producer_count > 20 {
            return Err(ConfigError::ValidationFailed(
                "Devnet should have small producer pools (≤20) for development".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_testnet_specific(config: &CatalystConfig) -> ConfigResult<()> {
        // Testnet should have moderate settings
        if config.consensus.cycle_duration_ms < 15000 || config.consensus.cycle_duration_ms > 60000
        {
            return Err(ConfigError::ValidationFailed(
                "Testnet consensus cycles should be moderate (15-60s)".to_string(),
            ));
        }

        if config.consensus.producer_count > 200 {
            return Err(ConfigError::ValidationFailed(
                "Testnet should have moderate producer pools (≤200)".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_mainnet_specific(config: &CatalystConfig) -> ConfigResult<()> {
        // Mainnet should have production-ready settings
        if config.consensus.cycle_duration_ms < 30000 {
            return Err(ConfigError::ValidationFailed(
                "Mainnet consensus cycles should be stable (≥30s) for production".to_string(),
            ));
        }

        if config.consensus.statistical_confidence < 0.999 {
            return Err(ConfigError::ValidationFailed(
                "Mainnet requires high statistical confidence (≥99.9%)".to_string(),
            ));
        }

        // Mainnet should have robust storage settings
        if let Some(max_size) = config.storage.database.max_db_size_bytes {
            if max_size < 10 * 1024 * 1024 * 1024 {
                // 10GB
                return Err(ConfigError::ValidationFailed(
                    "Mainnet database size should be at least 10GB".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Generate a configuration report
    pub fn generate_report(config: &CatalystConfig) -> String {
        let mut report = String::new();

        report.push_str(&format!("Catalyst Network Configuration Report\n"));
        report.push_str(&format!("=====================================\n\n"));

        report.push_str(&format!("Network: {}\n", config.network.name));
        report.push_str(&format!("P2P Port: {}\n", config.network.p2p_port));
        report.push_str(&format!("API Port: {}\n", config.network.api_port));
        report.push_str(&format!("Max Peers: {}\n\n", config.network.max_peers));

        report.push_str(&format!("Consensus:\n"));
        report.push_str(&format!(
            "  Cycle Duration: {}ms\n",
            config.consensus.cycle_duration_ms
        ));
        report.push_str(&format!(
            "  Producer Count: {}\n",
            config.consensus.producer_count
        ));
        report.push_str(&format!(
            "  Supermajority Threshold: {:.2}%\n\n",
            config.consensus.supermajority_threshold * 100.0
        ));

        report.push_str(&format!("Storage:\n"));
        report.push_str(&format!(
            "  Database Type: {:?}\n",
            config.storage.database.db_type
        ));
        report.push_str(&format!(
            "  Data Directory: {:?}\n",
            config.storage.database.data_directory
        ));
        report.push_str(&format!(
            "  DFS Enabled: {}\n\n",
            config.storage.dfs.enabled
        ));

        if config.service_bus.enabled {
            report.push_str(&format!("Service Bus:\n"));
            report.push_str(&format!(
                "  WebSocket Port: {}\n",
                config.service_bus.websocket.port
            ));
            report.push_str(&format!(
                "  REST API Port: {}\n",
                config.service_bus.rest_api.port
            ));
            report.push_str(&format!(
                "  Auth Enabled: {}\n\n",
                config.service_bus.auth.enabled
            ));
        }

        if config.metrics.enabled {
            report.push_str(&format!("Metrics:\n"));
            report.push_str(&format!("  Server Port: {}\n", config.metrics.server.port));
            report.push_str(&format!(
                "  Collection Interval: {}s\n",
                config.metrics.collection.collection_interval_seconds
            ));
            report.push_str(&format!(
                "  Export Enabled: {}\n\n",
                config.metrics.export.enabled
            ));
        }

        report.push_str(&format!("Security:\n"));
        report.push_str(&format!(
            "  Encryption at Rest: {}\n",
            config.storage.security.encryption_at_rest
        ));
        report.push_str(&format!(
            "  Constant Time Crypto: {}\n",
            config.crypto.security.constant_time_operations
        ));
        report.push_str(&format!(
            "  Service Bus Auth: {}\n",
            if config.service_bus.enabled {
                config.service_bus.auth.enabled.to_string()
            } else {
                "N/A".to_string()
            }
        ));

        report
    }
}
