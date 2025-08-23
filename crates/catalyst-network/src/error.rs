//! Network-specific error types

use catalyst_utils::CatalystError;
use libp2p::PeerId;
use std::time::Duration;
use thiserror::Error;

/// Network-specific error types
#[derive(Error, Clone, Debug)]
pub enum NetworkError {
    #[error("Connection failed to peer {peer_id}: {reason}")]
    ConnectionFailed { peer_id: PeerId, reason: String },

    #[error("Peer {peer_id} disconnected unexpectedly")]
    PeerDisconnected { peer_id: PeerId },

    #[error("Message serialization failed: {0}")]
    SerializationFailed(String),

    #[error("Message deserialization failed: {0}")]
    DeserializationFailed(String),

    #[error("Invalid message format: {0}")]
    InvalidMessage(String),

    #[error("Bandwidth limit exceeded for peer {peer_id}")]
    BandwidthExceeded { peer_id: PeerId },

    #[error("Rate limit exceeded: {limit} requests per {duration:?}")]
    RateLimitExceeded { limit: u32, duration: Duration },

    #[error("Peer {peer_id} reputation too low: {reputation}")]
    LowReputation { peer_id: PeerId, reputation: f64 },

    #[error("Network timeout after {duration:?}")]
    Timeout { duration: Duration },

    #[error("Gossip propagation failed: {0}")]
    GossipFailed(String),

    #[error("Discovery failed: {0}")]
    DiscoveryFailed(String),

    #[error("Security validation failed: {0}")]
    SecurityFailed(String),

    #[error("Network configuration error: {0}")]
    ConfigError(String),

    #[error("Swarm error: {0}")]
    SwarmError(String),

    #[error("Transport error: {0}")]
    TransportError(String),

    #[error("Protocol error: {0}")]
    ProtocolError(String),

    #[error("Network is not initialized")]
    NotInitialized,

    #[error("Network is shutting down")]
    ShuttingDown,

    #[error("Maximum peers reached: {max_peers}")]
    MaxPeersReached { max_peers: usize },

    #[error("Peer {peer_id} not found")]
    PeerNotFound { peer_id: PeerId },

    #[error("Invalid peer address: {address}")]
    InvalidAddress { address: String },

    #[error("Handshake failed with peer {peer_id}: {reason}")]
    HandshakeFailed { peer_id: PeerId, reason: String },

    #[error("Message routing failed: {0}")]
    RoutingFailed(String),

    #[error("Network resource exhausted: {resource}")]
    ResourceExhausted { resource: String },
}

/// Result type for network operations
pub type NetworkResult<T> = Result<T, NetworkError>;

/// Convert NetworkError to CatalystError
impl From<NetworkError> for CatalystError {
    fn from(err: NetworkError) -> Self {
        match err {
            NetworkError::ConnectionFailed { peer_id, reason } => {
                CatalystError::Network(format!("Connection to {} failed: {}", peer_id, reason))
            }
            NetworkError::Timeout { duration } => CatalystError::Timeout {
                duration_ms: duration.as_millis() as u64,
                operation: "network operation".to_string(),
            },
            NetworkError::SerializationFailed(msg) => {
                CatalystError::Serialization(format!("Network serialization: {}", msg))
            }
            NetworkError::ConfigError(msg) => {
                CatalystError::Config(format!("Network config: {}", msg))
            }
            _ => CatalystError::Network(err.to_string()),
        }
    }
}

/// Convert libp2p errors to NetworkError
impl From<libp2p::swarm::DialError> for NetworkError {
    fn from(err: libp2p::swarm::DialError) -> Self {
        NetworkError::TransportError(format!("Dial error: {}", err))
    }
}

impl From<libp2p::TransportError<std::io::Error>> for NetworkError {
    fn from(err: libp2p::TransportError<std::io::Error>) -> Self {
        NetworkError::TransportError(format!("Transport error: {}", err))
    }
}

/// Helper macros for creating network errors
#[macro_export]
macro_rules! connection_error {
    ($peer_id:expr, $($arg:tt)*) => {
        NetworkError::ConnectionFailed {
            peer_id: $peer_id,
            reason: format!($($arg)*),
        }
    };
}

#[macro_export]
macro_rules! gossip_error {
    ($($arg:tt)*) => {
        NetworkError::GossipFailed(format!($($arg)*))
    };
}

#[macro_export]
macro_rules! discovery_error {
    ($($arg:tt)*) => {
        NetworkError::DiscoveryFailed(format!($($arg)*))
    };
}

#[macro_export]
macro_rules! security_error {
    ($($arg:tt)*) => {
        NetworkError::SecurityFailed(format!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyst_utils::CatalystError;

    #[test]
    fn test_error_conversion() {
        let peer_id = PeerId::random();
        let network_err = NetworkError::ConnectionFailed {
            peer_id,
            reason: "timeout".to_string(),
        };

        let catalyst_err: CatalystError = network_err.into();
        assert!(matches!(catalyst_err, CatalystError::Network(_)));
    }

    #[test]
    fn test_error_macros() {
        let peer_id = PeerId::random();
        let err = connection_error!(peer_id, "Failed to connect: {}", "timeout");
        assert!(matches!(err, NetworkError::ConnectionFailed { .. }));

        let err = gossip_error!("Propagation failed");
        assert!(matches!(err, NetworkError::GossipFailed(_)));
    }

    #[test]
    fn test_timeout_error() {
        let duration = Duration::from_secs(30);
        let err = NetworkError::Timeout { duration };
        let catalyst_err: CatalystError = err.into();

        if let CatalystError::Timeout { duration_ms, .. } = catalyst_err {
            assert_eq!(duration_ms, 30000);
        } else {
            panic!("Expected timeout error");
        }
    }
}
