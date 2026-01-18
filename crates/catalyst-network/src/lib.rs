//! Catalyst Network - P2P networking layer for Catalyst blockchain
//!
//! This crate provides the networking infrastructure for the Catalyst blockchain,
//! including peer discovery, message routing, and libp2p-based transport.

pub mod config;
pub mod error;
pub mod simple;

// Full libp2p implementation is present but not yet API/trait-aligned with the rest of the workspace.
// Gate it behind a feature so we can keep the workspace buildable while we wire consensus end-to-end.
#[cfg(feature = "libp2p-full")]
pub mod bandwidth;
#[cfg(feature = "libp2p-full")]
pub mod discovery;
#[cfg(feature = "libp2p-full")]
pub mod gossip;
#[cfg(feature = "libp2p-full")]
pub mod messaging;
#[cfg(feature = "libp2p-full")]
pub mod metrics;
#[cfg(feature = "libp2p-full")]
pub mod monitoring;
#[cfg(feature = "libp2p-full")]
pub mod reputation;
#[cfg(feature = "libp2p-full")]
pub mod security;
#[cfg(feature = "libp2p-full")]
pub mod service;
#[cfg(feature = "libp2p-full")]
pub mod swarm;

// Common re-exports for downstream crates.
pub use config::NetworkConfig;
pub use error::{NetworkError, NetworkResult};
pub use simple::{NetworkEvent, NetworkService, NetworkStats};

// Convenience alias used across the workspace.
pub type Network = NetworkService;

// Re-export multiaddr primitives commonly needed by callers (e.g. CLI).
pub use libp2p::Multiaddr;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_service_creation() {
        let config = NetworkConfig::test_config();
        let service = NetworkService::new(config).await;
        assert!(service.is_ok());
    }
}