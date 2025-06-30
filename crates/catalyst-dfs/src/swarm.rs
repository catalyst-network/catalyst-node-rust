//! P2P networking layer for DFS using libp2p
//! 
//! Handles peer discovery, content routing, and block exchange

use crate::{ContentId, DfsError, ProviderId};
use libp2p::{
    gossipsub::{self, Gossipsub, GossipsubEvent, MessageAuthenticity, ValidationMode},
    identify::{self, Identify, IdentifyEvent},
    kad::{self, Kademlia, KademliaEvent, QueryResult, Record},
    noise,
    request_response::{self, RequestResponse, RequestResponseEvent},
    swarm::{SwarmBuilder, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, Transport,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// DFS message types for peer communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DfsMessage {
    /// Request content by CID
    GetContent { cid: ContentId },
    /// Provide content data
    ContentData { cid: ContentId, data: Vec<u8> },
    /// Announce that we provide content
    ProvideContent { cid: ContentId },
    /// Request list of providers for content
    FindProviders { cid: ContentId },
    /// List of providers for content
    Providers { cid: ContentId, providers: Vec<ProviderId> },
    /// Content not found
    NotFound { cid: ContentId },
}

/// Network events for the DFS system
#[derive(Debug)]
pub enum NetworkEvent {
    /// New peer connected
    PeerConnected(PeerId),
    /// Peer disconnected
    PeerDisconnected(PeerId),
    /// Content request received
    ContentRequest { from: PeerId, cid: ContentId },
    /// Content response received
    ContentResponse { from: PeerId, cid: ContentId, data: Vec<u8> },
    /// Provider announcement received
    ProviderAnnouncement { from: PeerId, cid: ContentId },
    /// Content not found response
    ContentNotFound { from: PeerId, cid: ContentId },
}

/// Configuration for the DFS swarm
#[derive(Debug, Clone)]
pub struct SwarmConfig {
    /// Local peer ID keypair
    pub keypair: libp2p::identity::Keypair,
    /// Listen addresses
    pub listen_addresses: Vec<Multiaddr>,
    /// Bootstrap nodes
    pub bootstrap_nodes: Vec<(PeerId, Multiaddr)>,
    /// Enable DHT for content discovery
    pub enable_dht: bool,
    /// Maximum number of connections
    pub max_connections: usize,
}

impl Default for SwarmConfig {
    fn default() -> Self {
        Self {
            keypair: libp2p::identity::Keypair::generate_ed25519(),
            listen_addresses: vec!["/ip4/0.0.0.0/tcp/0".parse().unwrap()],
            bootstrap_nodes: Vec::new(),
            enable_dht: true,
            max_connections: 100,
        }
    }
}

/// DFS network swarm for P2P content sharing
pub struct DfsSwarm {
    swarm: Swarm<DfsBehaviour>,
    event_sender: mpsc::UnboundedSender<NetworkEvent>,
    pending_requests: HashMap<request_response::RequestId, oneshot::Sender<DfsMessage>>,
    provided_content: HashSet<ContentId>,
}

/// Combined behaviour for DFS networking
#[derive(libp2p::swarm::NetworkBehaviour)]
pub struct DfsBehaviour {
    /// Request-response for direct content exchange
    request_response: RequestResponse<DfsCodec>,
    /// Kademlia DHT for content discovery
    kademlia: Kademlia<kad::store::MemoryStore>,
    /// Gossipsub for efficient content announcements
    gossipsub: Gossipsub,
    /// Identify protocol for peer information
    identify: Identify,
}

/// Codec for DFS message serialization
#[derive(Debug, Clone)]
pub struct DfsCodec;

impl request_response::Codec for DfsCodec {
    type Protocol = String;
    type Request = DfsMessage;
    type Response = DfsMessage;

    async fn read_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Request>
    where
        T: tokio::io::AsyncRead + Unpin + Send,
    {
        use tokio::io::AsyncReadExt;
        
        let mut length_bytes = [0u8; 4];
        io.read_exact(&mut length_bytes).await?;
        let length = u32::from_be_bytes(length_bytes) as usize;
        
        let mut buffer = vec![0u8; length];
        io.read_exact(&mut buffer).await?;
        
        serde_json::from_slice(&buffer)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Response>
    where
        T: tokio::io::AsyncRead + Unpin + Send,
    {
        self.read_request("", io).await
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> std::io::Result<()>
    where
        T: tokio::io::AsyncWrite + Unpin + Send,
    {
        use tokio::io::AsyncWriteExt;
        
        let data = serde_json::to_vec(&req)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        
        let length = data.len() as u32;
        io.write_all(&length.to_be_bytes()).await?;
        io.write_all(&data).await?;
        
        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> std::io::Result<()>
    where
        T: tokio::io::AsyncWrite + Unpin + Send,
    {
        self.write_request("", io, res).await
    }
}

impl DfsSwarm {
    /// Create a new DFS swarm
    pub async fn new(
        config: SwarmConfig,
        event_sender: mpsc::UnboundedSender<NetworkEvent>,
    ) -> Result<Self, DfsError> {
        let local_peer_id = PeerId::from(config.keypair.public());
        log::info!("Local peer id: {}", local_peer_id);

        // Create transport
        let transport = tcp::async_io::Transport::default()
            .upgrade(libp2p::core::upgrade::Version::V1)
            .authenticate(noise::Config::new(&config.keypair).unwrap())
            .multiplex(yamux::Config::default())
            .boxed();

        // Setup request-response behaviour
        let request_response = RequestResponse::new(
            DfsCodec,
            [(String::from("/catalyst/dfs/1.0.0"), vec![])],
            request_response::Config::default(),
        );

        // Setup Kademlia DHT
        let mut kademlia = if config.enable_dht {
            let store = kad::store::MemoryStore::new(local_peer_id);
            let mut kad = Kademlia::new(local_peer_id, store);
            
            // Add bootstrap nodes
            for (peer_id, addr) in &config.bootstrap_nodes {
                kad.add_address(peer_id, addr.clone());
            }
            
            kad
        } else {
            let store = kad::store::MemoryStore::new(local_peer_id);
            Kademlia::new(local_peer_id, store)
        };

        // Setup Gossipsub
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(10))
            .validation_mode(ValidationMode::Strict)
            .build()
            .map_err(|e| DfsError::Network(format!("Gossipsub config error: {}", e)))?;

        let mut gossipsub = Gossipsub::new(
            MessageAuthenticity::Signed(config.keypair.clone()),
            gossipsub_config,
        ).map_err(|e| DfsError::Network(format!("Gossipsub creation error: {}", e)))?;

        // Subscribe to DFS topics
        let content_topic = gossipsub::IdentTopic::new("catalyst-dfs-content");
        gossipsub.subscribe(&content_topic)
            .map_err(|e| DfsError::Network(format!("Gossipsub subscribe error: {}", e)))?;

        // Setup Identify
        let identify = Identify::new(identify::Config::new(
            "/catalyst/dfs/1.0.0".to_string(),
            config.keypair.public(),
        ));

        // Create behaviour
        let behaviour = DfsBehaviour {
            request_response,
            kademlia,
            gossipsub,
            identify,
        };

        // Create swarm
        let mut swarm = SwarmBuilder::with_async_std_executor(transport, behaviour, local_peer_id)
            .build();

        // Start listening
        for addr in &config.listen_addresses {
            match swarm.listen_on(addr.clone()) {
                Ok(_) => log::info!("Listening on {}", addr),
                Err(e) => log::warn!("Failed to listen on {}: {}", addr, e),
            }
        }

        Ok(Self {
            swarm,
            event_sender,
            pending_requests: HashMap::new(),
            provided_content: HashSet::new(),
        })
    }

    /// Get the local peer ID
    pub fn local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }

    /// Add a known address for a peer
    pub fn add_address(&mut self, peer_id: PeerId, address: Multiaddr) {
        self.swarm.behaviour_mut().kademlia.add_address(&peer_id, address);
    }

    /// Bootstrap the DHT
    pub fn bootstrap(&mut self) -> Result<(), DfsError> {
        if let Err(e) = self.swarm.behaviour_mut().kademlia.bootstrap() {
            return Err(DfsError::Network(format!("Bootstrap failed: {:?}", e)));
        }
        Ok(())
    }

    /// Request content from the network
    pub async fn request_content(&mut self, cid: ContentId) -> Result<Vec<u8>, DfsError> {
        // First try to find providers
        let providers = self.find_providers(&cid).await?;
        
        if providers.is_empty() {
            return Err(DfsError::NotFound(format!("No providers found for {}", cid.to_string())));
        }

        // Try to request content from each provider
        for provider in providers {
            if let Ok(peer_id) = provider.0.parse::<PeerId>() {
                let message = DfsMessage::GetContent { cid: cid.clone() };
                
                let (tx, rx) = oneshot::channel();
                let request_id = self.swarm.behaviour_mut().request_response.send_request(&peer_id, message);
                self.pending_requests.insert(request_id, tx);
                
                // Wait for response with timeout
                match tokio::time::timeout(Duration::from_secs(30), rx).await {
                    Ok(Ok(DfsMessage::ContentData { data, .. })) => return Ok(data),
                    Ok(Ok(DfsMessage::NotFound { .. })) => continue,
                    _ => continue,
                }
            }
        }

        Err(DfsError::NotFound(format!("Content not available from any provider: {}", cid.to_string())))
    }

    /// Announce that we provide content
    pub fn provide_content(&mut self, cid: ContentId) {
        // Store in local set
        self.provided_content.insert(cid.clone());

        // Announce via DHT
        let key = kad::RecordKey::new(&cid.to_string());
        let record = Record::new(key, self.local_peer_id().to_bytes());
        
        if let Err(e) = self.swarm.behaviour_mut().kademlia.put_record(record, kad::Quorum::One) {
            log::warn!("Failed to announce content in DHT: {:?}", e);
        }

        // Announce via gossipsub
        let message = DfsMessage::ProvideContent { cid };
        if let Ok(data) = serde_json::to_vec(&message) {
            let topic = gossipsub::IdentTopic::new("catalyst-dfs-content");
            if let Err(e) = self.swarm.behaviour_mut().gossipsub.publish(topic, data) {
                log::warn!("Failed to publish content announcement: {:?}", e);
            }
        }
    }

    /// Find providers for content
    pub async fn find_providers(&mut self, cid: &ContentId) -> Result<Vec<ProviderId>, DfsError> {
        let key = kad::RecordKey::new(&cid.to_string());
        let query_id = self.swarm.behaviour_mut().kademlia.get_providers(key);
        
        // Wait for providers with timeout
        let timeout = Duration::from_secs(10);
        let start = std::time::Instant::now();
        
        while start.elapsed() < timeout {
            match self.swarm.select_next_some().await {
                SwarmEvent::Behaviour(DfsBehaviourEvent::Kademlia(KademliaEvent::OutboundQueryProgressed {
                    id,
                    result: QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { providers, .. })),
                    ..
                })) if id == query_id => {
                    return Ok(providers.into_iter()
                        .map(|peer_id| ProviderId(peer_id.to_string()))
                        .collect());
                }
                SwarmEvent::Behaviour(DfsBehaviourEvent::Kademlia(KademliaEvent::OutboundQueryProgressed {
                    id,
                    result: QueryResult::GetProviders(Ok(kad::GetProvidersOk::FinishedWithNoAdditionalRecord { .. })),
                    ..
                })) if id == query_id => {
                    break;
                }
                _ => {
                    // Continue processing other events
                    tokio::task::yield_now().await;
                }
            }
        }

        Ok(Vec::new())
    }

    /// Process swarm events
    pub async fn next_event(&mut self) -> Option<NetworkEvent> {
        loop {
            match self.swarm.select_next_some().await {
                SwarmEvent::NewListenAddr { address, .. } => {
                    log::info!("Listening on {}", address);
                }
                
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    log::debug!("Connected to {}", peer_id);
                    let _ = self.event_sender.send(NetworkEvent::PeerConnected(peer_id));
                }
                
                SwarmEvent::ConnectionClosed { peer_id, .. } => {
                    log::debug!("Disconnected from {}", peer_id);
                    let _ = self.event_sender.send(NetworkEvent::PeerDisconnected(peer_id));
                }

                SwarmEvent::Behaviour(event) => {
                    if let Some(network_event) = self.handle_behaviour_event(event).await {
                        return Some(network_event);
                    }
                }

                SwarmEvent::IncomingConnection { .. } => {}
                SwarmEvent::IncomingConnectionError { .. } => {}
                SwarmEvent::OutgoingConnectionError { .. } => {}
                SwarmEvent::BannedPeer { .. } => {}
                SwarmEvent::ExpiredListenAddr { .. } => {}
                SwarmEvent::ListenerClosed { .. } => {}
                SwarmEvent::ListenerError { .. } => {}
                SwarmEvent::Dialing { .. } => {}
            }
        }
    }

    /// Handle behaviour-specific events
    async fn handle_behaviour_event(&mut self, event: DfsBehaviourEvent) -> Option<NetworkEvent> {
        match event {
            // Request-Response events
            DfsBehaviourEvent::RequestResponse(RequestResponseEvent::Message { peer, message }) => {
                match message {
                    request_response::Message::Request { request, channel, .. } => {
                        self.handle_request(peer, request, channel).await
                    }
                    request_response::Message::Response { request_id, response } => {
                        self.handle_response(request_id, response).await
                    }
                }
            }

            // Kademlia events
            DfsBehaviourEvent::Kademlia(KademliaEvent::OutboundQueryProgressed { result, .. }) => {
                match result {
                    QueryResult::GetProviders(Ok(kad::GetProvidersOk::FoundProviders { providers, .. })) => {
                        log::debug!("Found {} providers", providers.len());
                    }
                    _ => {}
                }
                None
            }

            // Gossipsub events
            DfsBehaviourEvent::Gossipsub(GossipsubEvent::Message { 
                propagation_source: peer_id,
                message,
                ..
            }) => {
                if let Ok(dfs_message) = serde_json::from_slice::<DfsMessage>(&message.data) {
                    self.handle_gossip_message(peer_id, dfs_message).await
                } else {
                    None
                }
            }

            // Identify events
            DfsBehaviourEvent::Identify(IdentifyEvent::Received { peer_id, info }) => {
                log::debug!("Identified peer {}: {}", peer_id, info.protocol_version);
                // Add peer addresses to Kademlia
                for addr in info.listen_addrs {
                    self.swarm.behaviour_mut().kademlia.add_address(&peer_id, addr);
                }
                None
            }

            _ => None,
        }
    }

    /// Handle incoming requests
    async fn handle_request(
        &mut self,
        peer: PeerId,
        request: DfsMessage,
        channel: request_response::ResponseChannel<DfsMessage>,
    ) -> Option<NetworkEvent> {
        match request {
            DfsMessage::GetContent { cid } => {
                let _ = self.event_sender.send(NetworkEvent::ContentRequest {
                    from: peer,
                    cid: cid.clone(),
                });

                // Check if we have the content
                if self.provided_content.contains(&cid) {
                    // In a real implementation, we would retrieve the actual content
                    // For now, we'll send a placeholder response
                    let response = DfsMessage::NotFound { cid };
                    let _ = self.swarm.behaviour_mut().request_response.send_response(channel, response);
                } else {
                    let response = DfsMessage::NotFound { cid };
                    let _ = self.swarm.behaviour_mut().request_response.send_response(channel, response);
                }

                None
            }

            DfsMessage::FindProviders { cid } => {
                // Return known providers from our DHT
                let providers = Vec::new(); // Simplified
                let response = DfsMessage::Providers { cid, providers };
                let _ = self.swarm.behaviour_mut().request_response.send_response(channel, response);
                None
            }

            _ => {
                log::warn!("Unexpected request type from {}: {:?}", peer, request);
                None
            }
        }
    }

    /// Handle responses to our requests
    async fn handle_response(
        &mut self,
        request_id: request_response::RequestId,
        response: DfsMessage,
    ) -> Option<NetworkEvent> {
        if let Some(sender) = self.pending_requests.remove(&request_id) {
            let _ = sender.send(response);
        }
        None
    }

    /// Handle gossipsub messages
    async fn handle_gossip_message(
        &mut self,
        peer_id: PeerId,
        message: DfsMessage,
    ) -> Option<NetworkEvent> {
        match message {
            DfsMessage::ProvideContent { cid } => {
                Some(NetworkEvent::ProviderAnnouncement {
                    from: peer_id,
                    cid,
                })
            }
            _ => None,
        }
    }

    /// Send content data to a requesting peer
    pub fn send_content(&mut self, peer_id: PeerId, cid: ContentId, data: Vec<u8>) {
        let message = DfsMessage::ContentData { cid, data };
        let _request_id = self.swarm.behaviour_mut().request_response.send_request(&peer_id, message);
    }

    /// Get current network statistics
    pub fn network_stats(&self) -> NetworkStats {
        let connected_peers = self.swarm.connected_peers().count();
        let provided_content_count = self.provided_content.len();

        NetworkStats {
            connected_peers,
            provided_content_count,
            pending_requests: self.pending_requests.len(),
        }
    }
}

/// Network statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub connected_peers: usize,
    pub provided_content_count: usize,
    pub pending_requests: usize,
}