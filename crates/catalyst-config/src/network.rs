//! Network configuration structures

use serde::{Deserialize, Serialize};
use crate::ConfigError;

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Listening addresses for P2P network
    pub listen_addresses: Vec<String>,
    /// Bootstrap peer addresses
    pub bootstrap_peers: Vec<String>,
    /// Maximum number of connected peers
    pub max_connections: usize,
    /// Enable mDNS peer discovery
    pub enable_mdns: bool,
    /// Enable Kademlia DHT
    pub enable_kademlia: bool,
    /// Network protocol configuration
    pub protocol: ProtocolConfig,
    /// Gossip configuration
    pub gossip: GossipConfig,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/0".to_string(),
                "/ip6/::/tcp/0".to_string(),
            ],
            bootstrap_peers: vec![],
            max_connections: 100,
            enable_mdns: true,
            enable_kademlia: true,
            protocol: ProtocolConfig::default(),
            gossip: GossipConfig::default(),
        }
    }
}

/// Protocol-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    /// Protocol version
    pub version: String,
    /// Connection timeout in seconds
    pub connection_timeout: u64,
    /// Keep-alive interval in seconds
    pub keep_alive_interval: u64,
    /// Maximum message size in bytes
    pub max_message_size: usize,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            version: "1.0.0".to_string(),
            connection_timeout: 30,
            keep_alive_interval: 60,
            max_message_size: 1024 * 1024, // 1MB
        }
    }
}

/// Gossip protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipConfig {
    /// Topics to subscribe to
    pub topics: Vec<String>,
    /// Message propagation delay in milliseconds
    pub propagation_delay: u64,
    /// Heartbeat interval in seconds
    pub heartbeat_interval: u64,
    /// Mesh maintenance interval in seconds
    pub mesh_interval: u64,
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            topics: vec![
                "catalyst/transactions".to_string(),
                "catalyst/consensus".to_string(),
                "catalyst/blocks".to_string(),
            ],
            propagation_delay: 100,
            heartbeat_interval: 1,
            mesh_interval: 5,
        }
    }
}

impl NetworkConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.listen_addresses.is_empty() {
            return Err(ConfigError::Invalid(
                "At least one listen address must be specified".to_string()
            ));
        }

        if self.max_connections == 0 {
            return Err(ConfigError::Invalid(
                "Max connections cannot be zero".to_string()
            ));
        }

        self.protocol.validate()?;
        self.gossip.validate()?;

        Ok(())
    }
}

impl ProtocolConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.connection_timeout == 0 {
            return Err(ConfigError::Invalid(
                "Connection timeout cannot be zero".to_string()
            ));
        }

        if self.max_message_size == 0 {
            return Err(ConfigError::Invalid(
                "Max message size cannot be zero".to_string()
            ));
        }

        Ok(())
    }
}

impl GossipConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.topics.is_empty() {
            return Err(ConfigError::Invalid(
                "At least one gossip topic must be specified".to_string()
            ));
        }

        Ok(())
    }
}