use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;

// Import the correct types from catalyst-core
use catalyst_core::{
    Transaction, NodeId, BlockHash, 
    NetworkMessage,
};
use libp2p::PeerId;

// Module declarations - comment out missing modules for now
// pub mod gossip;     // TODO: Create for Phase 1
// pub mod peer;       // TODO: Create for Phase 1  
// pub mod protocol;   // TODO: Create for Phase 1
// pub mod swarm;      // TODO: Create for Phase 1
// pub mod discovery;  // TODO: Create for Phase 1

// Re-exports - comment out for now
// pub use gossip::*;
// pub use peer::*;
// pub use protocol::*;
// pub use swarm::*;
// pub use discovery::*;

/// Peer information structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    pub id: NodeId,
    pub peer_id_string: String, // Store PeerId as string for serialization
    pub addresses: Vec<String>,
    pub protocols: Vec<String>,
    pub connection_status: ConnectionStatus,
    pub last_seen: u64,
}

impl PeerInfo {
    pub fn new(id: NodeId, peer_id: PeerId, addresses: Vec<String>) -> Self {
        Self {
            id,
            peer_id_string: peer_id.to_string(),
            addresses,
            protocols: Vec::new(),
            connection_status: ConnectionStatus::Disconnected,
            last_seen: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
    
    /// Get the PeerId from the stored string
    pub fn peer_id(&self) -> Result<PeerId, String> {
        self.peer_id_string.parse()
            .map_err(|e| format!("Failed to parse PeerId: {}", e))
    }
}

/// Connection status for peers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    Connecting,
    Disconnected,
    Failed(String),
}

/// Network message types for internal communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InternalNetworkMessage {
    /// Broadcast message to all peers
    Broadcast {
        message: NetworkMessage,
        exclude_peers: Vec<NodeId>,
    },
    /// Send message to specific peer
    SendToPeer {
        peer_id: NodeId,
        message: NetworkMessage,
    },
    /// Request data from network
    DataRequest(DataRequest),
    /// Response to data request
    DataResponse(DataResponse),
    /// Peer management
    PeerInfo(PeerInfo),
}

/// Data request types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataRequest {
    /// Request specific block
    Block(BlockHash),
    /// Request transaction
    Transaction([u8; 32]), // Transaction hash
    /// Request peer list
    PeerList,
    /// Request consensus state
    ConsensusState,
}

/// Data response types  
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataResponse {
    /// Block data
    Block(catalyst_core::types::Block),
    /// Transaction data
    Transaction(Transaction),
    /// List of known peers
    PeerList(Vec<PeerInfo>),
    /// Current consensus state
    ConsensusState(ConsensusStateInfo),
    /// Data not found
    NotFound,
    /// Error occurred
    Error(String),
}

/// Consensus state information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusStateInfo {
    pub current_height: u64,
    pub current_round: u32,
    pub leader: Option<NodeId>,
    pub phase: ConsensusPhase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusPhase {
    Construction,
    Campaigning,
    Voting,
    Synchronization,
}

/// Main network interface trait
#[async_trait]
pub trait Network: Send + Sync {
    /// Start the network service
    async fn start(&mut self) -> Result<(), NetworkError>;
    
    /// Stop the network service
    async fn stop(&mut self) -> Result<(), NetworkError>;
    
    /// Broadcast a message to all connected peers
    async fn broadcast(&self, message: NetworkMessage) -> Result<(), NetworkError>;
    
    /// Send a message to a specific peer
    async fn send_to_peer(&self, peer_id: &NodeId, message: NetworkMessage) -> Result<(), NetworkError>;
    
    /// Get list of connected peers
    async fn get_peers(&self) -> Result<Vec<PeerInfo>, NetworkError>;
    
    /// Subscribe to network events
    async fn subscribe_events(&self) -> Result<NetworkEventReceiver, NetworkError>;
    
    /// Add a new peer address for connection
    async fn add_peer_address(&mut self, peer_id: NodeId, address: String) -> Result<(), NetworkError>;
    
    /// Remove a peer
    async fn remove_peer(&mut self, peer_id: &NodeId) -> Result<(), NetworkError>;
}

/// Network event types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkEvent {
    /// Peer connected to the network
    PeerConnected(PeerInfo),
    /// Peer disconnected from the network
    PeerDisconnected(NodeId),
    /// Message received from a peer
    MessageReceived {
        from: NodeId,
        message: NetworkMessage,
    },
    /// Network error occurred
    NetworkError(NetworkError),
}

/// Network error types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkError {
    /// Connection failed
    ConnectionFailed(String),
    /// Peer not found
    PeerNotFound(NodeId),
    /// Message send failed
    SendFailed(String),
    /// Network is not started
    NotStarted,
    /// Configuration error
    ConfigError(String),
    /// Protocol error
    ProtocolError(String),
    /// Timeout occurred
    Timeout,
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            NetworkError::PeerNotFound(peer_id) => write!(f, "Peer not found: {:?}", peer_id),
            NetworkError::SendFailed(msg) => write!(f, "Send failed: {}", msg),
            NetworkError::NotStarted => write!(f, "Network not started"),
            NetworkError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            NetworkError::ProtocolError(msg) => write!(f, "Protocol error: {}", msg),
            NetworkError::Timeout => write!(f, "Network timeout"),
        }
    }
}

impl std::error::Error for NetworkError {}

/// Type alias for network event receiver
pub type NetworkEventReceiver = mpsc::Receiver<NetworkEvent>;

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Local peer ID
    pub local_peer_id: NodeId,
    /// Listen addresses
    pub listen_addresses: Vec<String>,
    /// Bootstrap peers
    pub bootstrap_peers: Vec<String>,
    /// Maximum number of connections
    pub max_connections: usize,
    /// Connection timeout in milliseconds
    pub connection_timeout_ms: u64,
    /// Keep-alive interval in seconds
    pub keep_alive_interval_secs: u64,
    /// Enable mDNS discovery
    pub enable_mdns: bool,
    /// Enable Kademlia DHT
    pub enable_kademlia: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            local_peer_id: [0u8; 32], // Will be generated
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/0".to_string()],
            bootstrap_peers: Vec::new(),
            max_connections: 50,
            connection_timeout_ms: 10_000,
            keep_alive_interval_secs: 30,
            enable_mdns: true,
            enable_kademlia: true,
        }
    }
}

/// Simple in-memory network implementation for testing
pub struct MockNetwork {
    config: NetworkConfig,
    peers: HashMap<NodeId, PeerInfo>,
    #[allow(dead_code)]
    event_sender: Option<mpsc::Sender<NetworkEvent>>,
    is_started: bool,
}

impl MockNetwork {
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            config,
            peers: HashMap::new(),
            event_sender: None,
            is_started: false,
        }
    }
}

#[async_trait]
impl Network for MockNetwork {
    async fn start(&mut self) -> Result<(), NetworkError> {
        if self.is_started {
            return Err(NetworkError::ConfigError("Network already started".to_string()));
        }
        
        self.is_started = true;
        tracing::info!("Mock network started with peer ID: {:?}", self.config.local_peer_id);
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), NetworkError> {
        if !self.is_started {
            return Err(NetworkError::NotStarted);
        }
        
        self.is_started = false;
        self.peers.clear();
        tracing::info!("Mock network stopped");
        Ok(())
    }

    async fn broadcast(&self, message: NetworkMessage) -> Result<(), NetworkError> {
        if !self.is_started {
            return Err(NetworkError::NotStarted);
        }
        
        tracing::debug!("Broadcasting message to {} peers", self.peers.len());
        
        // In a real implementation, this would send to all connected peers
        for peer_id in self.peers.keys() {
            self.send_to_peer(peer_id, message.clone()).await?;
        }
        
        Ok(())
    }

    async fn send_to_peer(&self, peer_id: &NodeId, _message: NetworkMessage) -> Result<(), NetworkError> {
        if !self.is_started {
            return Err(NetworkError::NotStarted);
        }
        
        if !self.peers.contains_key(peer_id) {
            return Err(NetworkError::PeerNotFound(*peer_id));
        }
        
        tracing::debug!("Sending message to peer: {:?}", peer_id);
        
        // In a real implementation, this would send the message over the network
        // For mock, we just log it
        
        Ok(())
    }

    async fn get_peers(&self) -> Result<Vec<PeerInfo>, NetworkError> {
        if !self.is_started {
            return Err(NetworkError::NotStarted);
        }
        
        Ok(self.peers.values().cloned().collect())
    }

    async fn subscribe_events(&self) -> Result<NetworkEventReceiver, NetworkError> {
        let (_sender, receiver) = mpsc::channel(1000);
        // In a real implementation, store the sender to send events
        Ok(receiver)
    }

    async fn add_peer_address(&mut self, peer_id: NodeId, address: String) -> Result<(), NetworkError> {
        if !self.is_started {
            return Err(NetworkError::NotStarted);
        }
        
        // Create a mock PeerId - in real implementation this would be derived properly
        let libp2p_peer_id = PeerId::random();
        
        let peer_info = PeerInfo::new(peer_id, libp2p_peer_id, vec![address]);
        self.peers.insert(peer_id, peer_info);
        
        tracing::info!("Added peer: {:?}", peer_id);
        Ok(())
    }

    async fn remove_peer(&mut self, peer_id: &NodeId) -> Result<(), NetworkError> {
        if !self.is_started {
            return Err(NetworkError::NotStarted);
        }
        
        if self.peers.remove(peer_id).is_some() {
            tracing::info!("Removed peer: {:?}", peer_id);
            Ok(())
        } else {
            Err(NetworkError::PeerNotFound(*peer_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_network() {
        let config = NetworkConfig::default();
        let mut network = MockNetwork::new(config);
        
        // Test start
        assert!(network.start().await.is_ok());
        assert!(network.is_started);
        
        // Test add peer
        let peer_id = [1u8; 32];
        let address = "/ip4/127.0.0.1/tcp/8000".to_string();
        assert!(network.add_peer_address(peer_id, address).await.is_ok());
        
        // Test get peers
        let peers = network.get_peers().await.unwrap();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].id, peer_id);
        
        // Test remove peer
        assert!(network.remove_peer(&peer_id).await.is_ok());
        let peers = network.get_peers().await.unwrap();
        assert_eq!(peers.len(), 0);
        
        // Test stop
        assert!(network.stop().await.is_ok());
        assert!(!network.is_started);
    }

    #[test]
    fn test_peer_info() {
        let node_id = [1u8; 32];
        let peer_id = PeerId::random();
        let addresses = vec!["addr1".to_string(), "addr2".to_string()];
        
        let peer_info = PeerInfo::new(node_id, peer_id, addresses.clone());
        
        assert_eq!(peer_info.id, node_id);
        assert_eq!(peer_info.peer_id(), Ok(peer_id));
        assert_eq!(peer_info.addresses, addresses);
        assert!(matches!(peer_info.connection_status, ConnectionStatus::Disconnected));
    }
}