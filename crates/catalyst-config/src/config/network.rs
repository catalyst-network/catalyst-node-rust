use crate::error::{ConfigError, ConfigResult};
use serde::{Deserialize, Serialize};

/// Network configuration for P2P communication and API endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Network name identifier
    pub name: String,

    /// P2P networking port
    pub p2p_port: u16,

    /// REST API port
    pub api_port: u16,

    /// Maximum number of concurrent peer connections
    pub max_peers: usize,

    /// Minimum number of peers to maintain
    pub min_peers: usize,

    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,

    /// Heartbeat interval in milliseconds
    pub heartbeat_interval_ms: u64,

    /// Maximum message size in bytes
    pub max_message_size: usize,

    /// Enable IPv6 support
    pub ipv6_enabled: bool,

    /// Bootstrap peer addresses
    pub bootstrap_peers: Vec<String>,

    /// Network discovery settings
    pub discovery: DiscoveryConfig,

    /// Bandwidth limiting settings
    pub bandwidth: BandwidthConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Enable peer discovery
    pub enabled: bool,

    /// Discovery interval in milliseconds
    pub discovery_interval_ms: u64,

    /// Maximum peers to discover per interval
    pub max_discovery_peers: usize,

    /// Discovery timeout in milliseconds
    pub discovery_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthConfig {
    /// Enable bandwidth limiting
    pub enabled: bool,

    /// Maximum upload bandwidth in bytes per second
    pub max_upload_bps: Option<u64>,

    /// Maximum download bandwidth in bytes per second
    pub max_download_bps: Option<u64>,

    /// Burst allowance in bytes
    pub burst_allowance: u64,
}

impl NetworkConfig {
    /// Create default development network configuration
    pub fn devnet() -> Self {
        Self {
            name: "devnet".to_string(),
            p2p_port: 7000,
            api_port: 8080,
            max_peers: 20,
            min_peers: 3,
            connection_timeout_ms: 5000,
            heartbeat_interval_ms: 30000,
            max_message_size: 1024 * 1024, // 1MB
            ipv6_enabled: true,
            bootstrap_peers: vec!["127.0.0.1:7001".to_string(), "127.0.0.1:7002".to_string()],
            discovery: DiscoveryConfig {
                enabled: true,
                discovery_interval_ms: 10000,
                max_discovery_peers: 5,
                discovery_timeout_ms: 3000,
            },
            bandwidth: BandwidthConfig {
                enabled: false,
                max_upload_bps: None,
                max_download_bps: None,
                burst_allowance: 1024 * 1024, // 1MB
            },
        }
    }

    /// Create default test network configuration
    pub fn testnet() -> Self {
        Self {
            name: "testnet".to_string(),
            p2p_port: 7000,
            api_port: 8080,
            max_peers: 100,
            min_peers: 10,
            connection_timeout_ms: 10000,
            heartbeat_interval_ms: 30000,
            max_message_size: 2 * 1024 * 1024, // 2MB
            ipv6_enabled: true,
            bootstrap_peers: vec![
                "testnet-seed1.catalyst.network:7000".to_string(),
                "testnet-seed2.catalyst.network:7000".to_string(),
            ],
            discovery: DiscoveryConfig {
                enabled: true,
                discovery_interval_ms: 15000,
                max_discovery_peers: 10,
                discovery_timeout_ms: 5000,
            },
            bandwidth: BandwidthConfig {
                enabled: true,
                max_upload_bps: Some(10 * 1024 * 1024), // 10 MB/s
                max_download_bps: Some(50 * 1024 * 1024), // 50 MB/s
                burst_allowance: 5 * 1024 * 1024,       // 5MB
            },
        }
    }

    /// Create default main network configuration
    pub fn mainnet() -> Self {
        Self {
            name: "mainnet".to_string(),
            p2p_port: 7000,
            api_port: 8080,
            max_peers: 200,
            min_peers: 20,
            connection_timeout_ms: 15000,
            heartbeat_interval_ms: 30000,
            max_message_size: 4 * 1024 * 1024, // 4MB
            ipv6_enabled: true,
            bootstrap_peers: vec![
                "seed1.catalyst.network:7000".to_string(),
                "seed2.catalyst.network:7000".to_string(),
                "seed3.catalyst.network:7000".to_string(),
            ],
            discovery: DiscoveryConfig {
                enabled: true,
                discovery_interval_ms: 30000,
                max_discovery_peers: 20,
                discovery_timeout_ms: 10000,
            },
            bandwidth: BandwidthConfig {
                enabled: true,
                max_upload_bps: Some(100 * 1024 * 1024), // 100 MB/s
                max_download_bps: Some(500 * 1024 * 1024), // 500 MB/s
                burst_allowance: 10 * 1024 * 1024,       // 10MB
            },
        }
    }

    /// Validate network configuration
    pub fn validate(&self) -> ConfigResult<()> {
        if self.name.is_empty() {
            return Err(ConfigError::ValidationFailed(
                "Network name cannot be empty".to_string(),
            ));
        }

        if self.p2p_port == 0 {
            return Err(ConfigError::ValidationFailed(
                "P2P port cannot be 0".to_string(),
            ));
        }

        if self.api_port == 0 {
            return Err(ConfigError::ValidationFailed(
                "API port cannot be 0".to_string(),
            ));
        }

        if self.p2p_port == self.api_port {
            return Err(ConfigError::ValidationFailed(
                "P2P and API ports cannot be the same".to_string(),
            ));
        }

        if self.max_peers == 0 {
            return Err(ConfigError::ValidationFailed(
                "Max peers must be greater than 0".to_string(),
            ));
        }

        if self.min_peers > self.max_peers {
            return Err(ConfigError::ValidationFailed(
                "Min peers cannot exceed max peers".to_string(),
            ));
        }

        if self.connection_timeout_ms == 0 {
            return Err(ConfigError::ValidationFailed(
                "Connection timeout must be greater than 0".to_string(),
            ));
        }

        if self.max_message_size == 0 {
            return Err(ConfigError::ValidationFailed(
                "Max message size must be greater than 0".to_string(),
            ));
        }

        // Validate bootstrap peers
        for peer in &self.bootstrap_peers {
            if peer.is_empty() {
                return Err(ConfigError::ValidationFailed(
                    "Bootstrap peer address cannot be empty".to_string(),
                ));
            }
            // Could add more sophisticated address validation here
        }

        Ok(())
    }
}
