//! libp2p swarm management and configuration

use crate::{
    config::NetworkConfig,
    error::{NetworkError, NetworkResult},
    messaging::NetworkMessage,
};

use catalyst_utils::{
    logging::*,
    metrics::*,
};

use libp2p::{
    PeerId, Multiaddr,
    identity::Keypair,
    swarm::{SwarmBuilder, SwarmEvent, Swarm, NetworkBehaviour},
    transport::{Transport, tcp::TcpTransport, websocket::WsConfig},
    noise::{NoiseConfig, X25519Spec, Keypair as NoiseKeypair},
    yamux::YamuxConfig,
    gossipsub::{self, Gossipsub, GossipsubEvent, IdentTopic},
    kad::{Kademlia, KademliaEvent, store::MemoryStore},
    mdns::{Mdns, MdnsEvent},
    identify::{Identify, IdentifyEvent},
    ping::{Ping, PingEvent},
    autonat::{Autonat, AutonatEvent},
    relay::{Relay, RelayEvent},
    dcutr::{Dcutr, DcutrEvent},
};

use futures::StreamExt;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
    task::{Context, Poll},
};
use tokio::time::Instant;

/// Custom network behaviour combining all protocols
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "CatalystBehaviourEvent")]
pub struct CatalystBehaviour {
    /// Gossipsub for message propagation
    pub gossipsub: Gossipsub,
    
    /// Kademlia DHT for peer discovery
    pub kademlia: Kademlia<MemoryStore>,
    
    /// mDNS for local peer discovery
    pub mdns: Mdns,
    
    /// Identify protocol for peer information
    pub identify: Identify,
    
    /// Ping for connection health
    pub ping: Ping,
    
    /// AutoNAT for external address discovery
    pub autonat: Autonat,
    
    /// Relay for NAT traversal
    pub relay: Relay,
    
    /// DCUtR for hole punching
    pub dcutr: Dcutr,
}

/// Combined events from all protocols
#[derive(Debug)]
pub enum CatalystBehaviourEvent {
    Gossipsub(GossipsubEvent),
    Kademlia(KademliaEvent),
    Mdns(MdnsEvent),
    Identify(IdentifyEvent),
    Ping(PingEvent),
    Autonat(AutonatEvent),
    Relay(RelayEvent),
    Dcutr(DcutrEvent),
}

impl From<GossipsubEvent> for CatalystBehaviourEvent {
    fn from(event: GossipsubEvent) -> Self {
        CatalystBehaviourEvent::Gossipsub(event)
    }
}

impl From<KademliaEvent> for CatalystBehaviourEvent {
    fn from(event: KademliaEvent) -> Self {
        CatalystBehaviourEvent::Kademlia(event)
    }
}

impl From<MdnsEvent> for CatalystBehaviourEvent {
    fn from(event: MdnsEvent) -> Self {
        CatalystBehaviourEvent::Mdns(event)
    }
}

impl From<IdentifyEvent> for CatalystBehaviourEvent {
    fn from(event: IdentifyEvent) -> Self {
        CatalystBehaviourEvent::Identify(event)
    }
}

impl From<PingEvent> for CatalystBehaviourEvent {
    fn from(event: PingEvent) -> Self {
        CatalystBehaviourEvent::Ping(event)
    }
}

impl From<AutonatEvent> for CatalystBehaviourEvent {
    fn from(event: AutonatEvent) -> Self {
        CatalystBehaviourEvent::Autonat(event)
    }
}

impl From<RelayEvent> for CatalystBehaviourEvent {
    fn from(event: RelayEvent) -> Self {
        CatalystBehaviourEvent::Relay(event)
    }
}

impl From<DcutrEvent> for CatalystBehaviourEvent {
    fn from(event: DcutrEvent) -> Self {
        CatalystBehaviourEvent::Dcutr(event)
    }
}

/// Catalyst Network Swarm wrapper
pub struct CatalystSwarm {
    /// The underlying libp2p swarm
    swarm: Swarm<CatalystBehaviour>,
    
    /// Connected peers
    connected_peers: HashSet<PeerId>,
    
    /// Peer connection times
    connection_times: HashMap<PeerId, Instant>,
    
    /// Message queue for outgoing messages
    message_queue: Vec<(NetworkMessage, Option<PeerId>)>,
    
    /// Gossip topic for Catalyst messages
    catalyst_topic: IdentTopic,
    
    /// Network statistics
    stats: SwarmStats,
}

/// Swarm statistics
#[derive(Debug, Default)]
pub struct SwarmStats {
    pub connections_established: u64,
    pub connections_failed: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub uptime: Duration,
    pub start_time: Option<Instant>,
}

impl CatalystSwarm {
    /// Create new Catalyst swarm
    pub async fn new(config: &NetworkConfig) -> NetworkResult<Self> {
        log_info!(LogCategory::Network, "Creating Catalyst swarm");
        
        // Load or generate keypair
        let keypair = Self::load_or_generate_keypair(config)?;
        let local_peer_id = PeerId::from(keypair.public());
        
        log_info!(LogCategory::Network, "Local peer ID: {}", local_peer_id);
        
        // Create transport
        let transport = Self::create_transport(&keypair, config).await?;
        
        // Create network behaviour
        let behaviour = Self::create_behaviour(&keypair, config).await?;
        
        // Build swarm
        let swarm = SwarmBuilder::with_existing_identity(keypair)
            .with_tokio()
            .with_other_transport(|_| transport)?
            .with_behaviour(|_| behaviour)?
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(Duration::from_secs(60))
                    .with_connection_limits(
                        libp2p::swarm::ConnectionLimits::default()
                            .with_max_pending_incoming(Some(config.transport.max_connections_per_ip))
                            .with_max_pending_outgoing(Some(config.transport.max_connections_per_ip))
                            .with_max_established_incoming(Some(config.peer.max_peers))
                            .with_max_established_outgoing(Some(config.peer.max_peers))
                            .with_max_established_per_peer(Some(1))
                    )
            })
            .build();
        
        // Create gossip topic
        let catalyst_topic = IdentTopic::new(&config.gossip.topic_name);
        
        let mut catalyst_swarm = Self {
            swarm,
            connected_peers: HashSet::new(),
            connection_times: HashMap::new(),
            message_queue: Vec::new(),
            catalyst_topic,
            stats: SwarmStats {
                start_time: Some(Instant::now()),
                ..Default::default()
            },
        };
        
        // Subscribe to gossip topic
        catalyst_swarm.swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&catalyst_swarm.catalyst_topic.clone())
            .map_err(|e| NetworkError::GossipFailed(format!("Failed to subscribe to topic: {}", e)))?;
        
        // Start listening on configured addresses
        for addr in &config.peer.listen_addresses {
            catalyst_swarm.swarm.listen_on(addr.clone())
                .map_err(|e| NetworkError::TransportError(format!("Failed to listen on {}: {}", addr, e)))?;
            
            log_info!(LogCategory::Network, "Listening on {}", addr);
        }
        
        // Add bootstrap peers
        for (peer_id, addr) in &config.peer.bootstrap_peers {
            catalyst_swarm.swarm
                .behaviour_mut()
                .kademlia
                .add_address(peer_id, addr.clone());
            
            log_info!(LogCategory::Network, "Added bootstrap peer {} at {}", peer_id, addr);
        }
        
        log_info!(LogCategory::Network, "Catalyst swarm created successfully");
        Ok(catalyst_swarm)
    }
    
    /// Load keypair from file or generate new one
    fn load_or_generate_keypair(config: &NetworkConfig) -> NetworkResult<Keypair> {
        if let Some(keypair_path) = &config.peer.keypair_path {
            if keypair_path.exists() {
                log_info!(LogCategory::Network, "Loading keypair from {:?}", keypair_path);
                
                let keypair_bytes = std::fs::read(keypair_path)
                    .map_err(|e| NetworkError::ConfigError(format!("Failed to read keypair: {}", e)))?;
                
                let keypair = Keypair::from_protobuf_encoding(&keypair_bytes)
                    .map_err(|e| NetworkError::ConfigError(format!("Failed to decode keypair: {}", e)))?;
                
                return Ok(keypair);
            }
        }
        
        log_info!(LogCategory::Network, "Generating new keypair");
        let keypair = Keypair::generate_ed25519();
        
        // Save keypair if path is specified
        if let Some(keypair_path) = &config.peer.keypair_path {
            let keypair_bytes = keypair.to_protobuf_encoding()
                .map_err(|e| NetworkError::ConfigError(format!("Failed to encode keypair: {}", e)))?;
            
            std::fs::write(keypair_path, keypair_bytes)
                .map_err(|e| NetworkError::ConfigError(format!("Failed to save keypair: {}", e)))?;
            
            log_info!(LogCategory::Network, "Saved keypair to {:?}", keypair_path);
        }
        
        Ok(keypair)
    }
    
    /// Create transport stack
    async fn create_transport(
        keypair: &Keypair,
        config: &NetworkConfig,
    ) -> NetworkResult<impl Transport<Output = (PeerId, libp2p::core::muxing::StreamMuxerBox)> + Clone> {
        
        // Create base TCP transport
        let tcp_transport = TcpTransport::new(libp2p::tcp::Config::default());
        
        // Add WebSocket support if enabled
        let transport = if config.transport.enable_websocket {
            tcp_transport.or_transport(
                TcpTransport::new(libp2p::tcp::Config::default())
                    .and_then(|socket, _| WsConfig::new(socket))
            )
        } else {
            tcp_transport.boxed()
        };
        
        // Add Noise encryption
        let noise_config = NoiseConfig::new(
            NoiseKeypair::<X25519Spec>::new()
                .into_authentic(keypair)
                .map_err(|e| NetworkError::SecurityFailed(format!("Noise config failed: {}", e)))?
        );
        
        let transport = transport
            .upgrade(libp2p::core::upgrade::Version::V1)
            .authenticate(noise_config)
            .multiplex(YamuxConfig::default())
            .boxed();
        
        Ok(transport)
    }
    
    /// Create network behaviour
    async fn create_behaviour(
        keypair: &Keypair,
        config: &NetworkConfig,
    ) -> NetworkResult<CatalystBehaviour> {
        let local_peer_id = PeerId::from(keypair.public());
        
        // Configure Gossipsub
        let gossipsub_config = gossipsub::GossipsubConfigBuilder::default()
            .heartbeat_interval(config.gossip.heartbeat_interval)
            .validation_mode(gossipsub::ValidationMode::Strict)
            .message_id_fn(|message| {
                use std::hash::{Hash, Hasher};
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                message.data.hash(&mut hasher);
                gossipsub::MessageId::from(hasher.finish().to_string())
            })
            .max_transmit_size(config.security.max_message_size)
            .duplicate_cache_time(config.gossip.duplicate_detection_window)
            .build()
            .map_err(|e| NetworkError::GossipFailed(format!("Gossipsub config error: {}", e)))?;
        
        let gossipsub = Gossipsub::new(
            if config.gossip.enable_signing {
                gossipsub::MessageAuthenticity::Signed(keypair.clone())
            } else {
                gossipsub::MessageAuthenticity::Anonymous
            },
            gossipsub_config,
        )
        .map_err(|e| NetworkError::GossipFailed(format!("Failed to create Gossipsub: {}", e)))?;
        
        // Configure Kademlia DHT
        let kad_store = MemoryStore::new(local_peer_id);
        let mut kademlia = Kademlia::new(local_peer_id, kad_store);
        kademlia.set_mode(Some(libp2p::kad::Mode::Server));
        
        // Configure mDNS
        let mdns = Mdns::new(libp2p::mdns::Config::default())
            .await
            .map_err(|e| NetworkError::DiscoveryFailed(format!("mDNS creation failed: {}", e)))?;
        
        // Configure Identify
        let identify = Identify::new(
            libp2p::identify::Config::new(
                "/catalyst/1.0.0".to_string(),
                keypair.public(),
            )
            .with_interval(Duration::from_secs(60))
        );
        
        // Configure Ping
        let ping = Ping::new(
            libp2p::ping::Config::new()
                .with_interval(Duration::from_secs(30))
                .with_timeout(Duration::from_secs(10))
                .with_max_failures(3.try_into().unwrap())
        );
        
        // Configure AutoNAT
        let autonat = Autonat::new(
            local_peer_id,
            libp2p::autonat::Config::default()
        );
        
        // Configure Relay
        let relay = Relay::new(
            local_peer_id,
            libp2p::relay::Config::default()
        );
        
        // Configure DCUtR
        let dcutr = Dcutr::new(local_peer_id);
        
        Ok(CatalystBehaviour {
            gossipsub,
            kademlia,
            mdns,
            identify,
            ping,
            autonat,
            relay,
            dcutr,
        })
    }
    
    /// Connect to a peer
    pub async fn connect(&mut self, peer_id: PeerId, address: Multiaddr) -> NetworkResult<()> {
        log_info!(LogCategory::Network, "Attempting to connect to {} at {}", peer_id, address);
        
        // Add address to Kademlia
        self.swarm.behaviour_mut().kademlia.add_address(&peer_id, address.clone());
        
        // Initiate connection
        self.swarm.dial(address.clone())
            .map_err(|e| NetworkError::ConnectionFailed {
                peer_id,
                reason: format!("Dial failed: {}", e),
            })?;
        
        log_debug!(LogCategory::Network, "Dial initiated for peer {}", peer_id);
        Ok(())
    }
    
    /// Disconnect from a peer
    pub async fn disconnect(&mut self, peer_id: PeerId) -> NetworkResult<()> {
        log_info!(LogCategory::Network, "Disconnecting from peer {}", peer_id);
        
        // Close connection
        if self.swarm.is_connected(&peer_id) {
            self.swarm.disconnect_peer_id(peer_id)
                .map_err(|e| NetworkError::ProtocolError(format!("Disconnect failed: {}", e)))?;
            
            // Remove from connected peers
            self.connected_peers.remove(&peer_id);
            self.connection_times.remove(&peer_id);
            
            log_info!(LogCategory::Network, "Disconnected from peer {}", peer_id);
        } else {
            log_warn!(LogCategory::Network, "Peer {} was not connected", peer_id);
        }
        
        Ok(())
    }
    
    /// Send message via gossipsub
    pub async fn send_message(&mut self, message: NetworkMessage) -> NetworkResult<()> {
        log_debug!(
            LogCategory::Network,
            "Publishing message type {:?} to gossip topic",
            message.message_type()
        );
        
        // Serialize message
        let message_bytes = catalyst_utils::serialization::CatalystSerialize::serialize(&message)
            .map_err(|e| NetworkError::SerializationFailed(e.to_string()))?;
        
        // Publish to gossip topic
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(self.catalyst_topic.clone(), message_bytes)
            .map_err(|e| NetworkError::GossipFailed(format!("Publish failed: {}", e)))?;
        
        // Update statistics
        self.stats.messages_sent += 1;
        self.stats.bytes_sent += message.transport.size_bytes as u64;
        
        increment_counter!("network_gossip_messages_sent_total", 1);
        increment_counter!("network_gossip_bytes_sent_total", message.transport.size_bytes as f64);
        
        Ok(())
    }
    
    /// Poll for next swarm event
    pub async fn poll_next(&mut self) -> Option<SwarmEvent<CatalystBehaviourEvent, std::io::Error>> {
        futures::poll!(self.swarm.poll_next_unpin(&mut std::task::Context::from_waker(
            futures::task::noop_waker_ref()
        )))
        .map(|event| {
            self.handle_swarm_event(&event);
            event
        })
    }
    
    /// Handle swarm events and update internal state
    fn handle_swarm_event(&mut self, event: &SwarmEvent<CatalystBehaviourEvent, std::io::Error>) {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                self.connected_peers.insert(*peer_id);
                self.connection_times.insert(*peer_id, Instant::now());
                self.stats.connections_established += 1;
                
                log_info!(LogCategory::Network, "Connection established with {}", peer_id);
            }
            
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                self.connected_peers.remove(peer_id);
                self.connection_times.remove(peer_id);
                
                log_info!(LogCategory::Network, "Connection closed with {}", peer_id);
            }
            
            SwarmEvent::Behaviour(CatalystBehaviourEvent::Gossipsub(gossip_event)) => {
                self.handle_gossip_event(gossip_event);
            }
            
            SwarmEvent::Behaviour(CatalystBehaviourEvent::Kademlia(kad_event)) => {
                self.handle_kademlia_event(kad_event);
            }
            
            SwarmEvent::Behaviour(CatalystBehaviourEvent::Mdns(mdns_event)) => {
                self.handle_mdns_event(mdns_event);
            }
            
            SwarmEvent::Behaviour(CatalystBehaviourEvent::Identify(identify_event)) => {
                self.handle_identify_event(identify_event);
            }
            
            SwarmEvent::Behaviour(CatalystBehaviourEvent::Ping(ping_event)) => {
                self.handle_ping_event(ping_event);
            }
            
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                self.stats.connections_failed += 1;
                log_warn!(
                    LogCategory::Network,
                    "Outgoing connection failed to {:?}: {}",
                    peer_id,
                    error
                );
            }
            
            SwarmEvent::IncomingConnectionError { error, .. } => {
                self.stats.connections_failed += 1;
                log_warn!(LogCategory::Network, "Incoming connection failed: {}", error);
            }
            
            _ => {}
        }
    }
    
    /// Handle gossipsub events
    fn handle_gossip_event(&mut self, event: &GossipsubEvent) {
        match event {
            GossipsubEvent::Message {
                propagation_source,
                message_id,
                message,
            } => {
                log_debug!(
                    LogCategory::Network,
                    "Received gossip message {} from {}",
                    message_id,
                    propagation_source
                );
                
                self.stats.messages_received += 1;
                self.stats.bytes_received += message.data.len() as u64;
                
                increment_counter!("network_gossip_messages_received_total", 1);
                increment_counter!("network_gossip_bytes_received_total", message.data.len() as f64);
                
                // TODO: Parse and route message
            }
            
            GossipsubEvent::Subscribed { peer_id, topic } => {
                log_debug!(
                    LogCategory::Network,
                    "Peer {} subscribed to topic {}",
                    peer_id,
                    topic
                );
            }
            
            GossipsubEvent::Unsubscribed { peer_id, topic } => {
                log_debug!(
                    LogCategory::Network,
                    "Peer {} unsubscribed from topic {}",
                    peer_id,
                    topic
                );
            }
            
            GossipsubEvent::GraftReceived { peer_id, topic } => {
                log_debug!(
                    LogCategory::Network,
                    "Received graft from {} for topic {}",
                    peer_id,
                    topic
                );
            }
            
            GossipsubEvent::PruneReceived { peer_id, topic } => {
                log_debug!(
                    LogCategory::Network,
                    "Received prune from {} for topic {}",
                    peer_id,
                    topic
                );
            }
        }
    }
    
    /// Handle Kademlia events
    fn handle_kademlia_event(&mut self, event: &KademliaEvent) {
        match event {
            KademliaEvent::OutboundQueryProgressed { result, .. } => {
                log_debug!(LogCategory::Network, "Kademlia query progressed: {:?}", result);
            }
            
            KademliaEvent::RoutingUpdated { peer, .. } => {
                log_debug!(LogCategory::Network, "Kademlia routing updated for peer {}", peer);
            }
            
            KademliaEvent::UnroutablePeer { peer } => {
                log_debug!(LogCategory::Network, "Peer {} is unroutable", peer);
            }
            
            KademliaEvent::RoutablePeer { peer, address } => {
                log_debug!(LogCategory::Network, "Found routable peer {} at {}", peer, address);
            }
            
            KademliaEvent::PendingRoutablePeer { peer, address } => {
                log_debug!(LogCategory::Network, "Pending routable peer {} at {}", peer, address);
            }
        }
    }
    
    /// Handle mDNS events
    fn handle_mdns_event(&mut self, event: &MdnsEvent) {
        match event {
            MdnsEvent::Discovered(list) => {
                for (peer_id, multiaddr) in list {
                    log_info!(
                        LogCategory::Network,
                        "mDNS discovered peer {} at {}",
                        peer_id,
                        multiaddr
                    );
                    
                    // Add to Kademlia
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(peer_id, multiaddr.clone());
                }
                
                increment_counter!("network_mdns_discoveries_total", list.len() as f64);
            }
            
            MdnsEvent::Expired(list) => {
                for (peer_id, multiaddr) in list {
                    log_debug!(
                        LogCategory::Network,
                        "mDNS expired peer {} at {}",
                        peer_id,
                        multiaddr
                    );
                }
            }
        }
    }
    
    /// Handle identify events
    fn handle_identify_event(&mut self, event: &IdentifyEvent) {
        match event {
            IdentifyEvent::Received { peer_id, info } => {
                log_debug!(
                    LogCategory::Network,
                    "Identified peer {}: protocol={}, agent={}",
                    peer_id,
                    info.protocol_version,
                    info.agent_version
                );
                
                // Add addresses to Kademlia
                for addr in &info.listen_addrs {
                    self.swarm
                        .behaviour_mut()
                        .kademlia
                        .add_address(peer_id, addr.clone());
                }
            }
            
            IdentifyEvent::Sent { peer_id } => {
                log_debug!(LogCategory::Network, "Sent identify info to {}", peer_id);
            }
            
            IdentifyEvent::Pushed { peer_id } => {
                log_debug!(LogCategory::Network, "Pushed identify info to {}", peer_id);
            }
            
            IdentifyEvent::Error { peer_id, error } => {
                log_warn!(
                    LogCategory::Network,
                    "Identify error with {}: {}",
                    peer_id,
                    error
                );
            }
        }
    }
    
    /// Handle ping events
    fn handle_ping_event(&mut self, event: &PingEvent) {
        match event {
            PingEvent { peer, result } => {
                match result {
                    Ok(duration) => {
                        log_debug!(
                            LogCategory::Network,
                            "Ping to {} successful: {:?}",
                            peer,
                            duration
                        );
                        
                        observe_histogram!("network_ping_duration_seconds", duration.as_secs_f64());
                    }
                    Err(failure) => {
                        log_warn!(
                            LogCategory::Network,
                            "Ping to {} failed: {:?}",
                            peer,
                            failure
                        );
                        
                        increment_counter!("network_ping_failures_total", 1);
                    }
                }
            }
        }
    }
    
    /// Get connected peers
    pub fn connected_peers(&self) -> &HashSet<PeerId> {
        &self.connected_peers
    }
    
    /// Check if peer is connected
    pub fn is_connected(&self, peer_id: &PeerId) -> bool {
        self.connected_peers.contains(peer_id)
    }
    
    /// Get connection duration for a peer
    pub fn connection_duration(&self, peer_id: &PeerId) -> Option<Duration> {
        self.connection_times.get(peer_id).map(|start| start.elapsed())
    }
    
    /// Get swarm statistics
    pub fn stats(&self) -> &SwarmStats {
        &self.stats
    }
    
    /// Get local peer ID
    pub fn local_peer_id(&self) -> &PeerId {
        self.swarm.local_peer_id()
    }
    
    /// Get listening addresses
    pub fn listeners(&self) -> impl Iterator<Item = &Multiaddr> {
        self.swarm.listeners()
    }
    
    /// Get external addresses
    pub fn external_addresses(&self) -> impl Iterator<Item = &Multiaddr> {
        self.swarm.external_addresses()
    }
    
    /// Bootstrap the DHT
    pub async fn bootstrap(&mut self) -> NetworkResult<()> {
        log_info!(LogCategory::Network, "Starting DHT bootstrap");
        
        self.swarm
            .behaviour_mut()
            .kademlia
            .bootstrap()
            .map_err(|e| NetworkError::DiscoveryFailed(format!("Bootstrap failed: {}", e)))?;
        
        Ok(())
    }
    
    /// Add address for a peer
    pub fn add_peer_address(&mut self, peer_id: PeerId, address: Multiaddr) {
        self.swarm
            .behaviour_mut()
            .kademlia
            .add_address(&peer_id, address);
    }
    
    /// Remove peer from routing table
    pub fn remove_peer(&mut self, peer_id: &PeerId) {
        self.swarm
            .behaviour_mut()
            .kademlia
            .remove_peer(peer_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NetworkConfig;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_swarm_creation() {
        let config = NetworkConfig::default();
        let swarm = CatalystSwarm::new(&config).await.unwrap();
        
        assert!(!swarm.connected_peers().is_empty() == false); // Should start with no peers
        assert!(swarm.local_peer_id() != &PeerId::random()); // Should have valid peer ID
    }
    
    #[tokio::test]
    async fn test_keypair_generation() {
        let temp_dir = TempDir::new().unwrap();
        let keypair_path = temp_dir.path().join("test_keypair");
        
        let mut config = NetworkConfig::default();
        config.peer.keypair_path = Some(keypair_path.clone());
        
        // First creation should generate keypair
        let swarm1 = CatalystSwarm::new(&config).await.unwrap();
        let peer_id1 = *swarm1.local_peer_id();
        
        // Second creation should load same keypair
        let swarm2 = CatalystSwarm::new(&config).await.unwrap();
        let peer_id2 = *swarm2.local_peer_id();
        
        assert_eq!(peer_id1, peer_id2);
        assert!(keypair_path.exists());
    }
    
    #[tokio::test]
    async fn test_message_serialization() {
        use catalyst_utils::network::PingMessage;
        use crate::messaging::{NetworkMessage, RoutingStrategy};
        
        let ping = PingMessage {
            timestamp: catalyst_utils::utils::current_timestamp(),
            data: vec![1, 2, 3],
        };
        
        let message = NetworkMessage::new(
            &ping,
            "sender".to_string(),
            None,
            RoutingStrategy::Broadcast,
        ).unwrap();
        
        let serialized = catalyst_utils::serialization::CatalystSerialize::serialize(&message).unwrap();
        assert!(!serialized.is_empty());
    }
}