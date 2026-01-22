//! Catalyst Network - P2P networking layer for Catalyst blockchain
//!
//! This crate provides the networking infrastructure for the Catalyst blockchain,
//! including peer discovery, message routing, and libp2p-based transport.

pub mod config;
pub mod error;
pub mod simple;

// Full libp2p implementation (gated). We keep the public API aligned with the
// default (simple TCP) implementation so downstream crates can keep using the
// same `MessageEnvelope` plumbing.
#[cfg(feature = "libp2p-full")]
pub mod service;

// Common re-exports for downstream crates.
pub use config::NetworkConfig;
pub use error::{NetworkError, NetworkResult};

// Default: simple TCP service.
#[cfg(not(feature = "libp2p-full"))]
pub use simple::{NetworkEvent, NetworkService, NetworkStats};

// When enabled: libp2p service.
#[cfg(feature = "libp2p-full")]
pub use service::{NetworkEvent, NetworkService, NetworkStats};

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