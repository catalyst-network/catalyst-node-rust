//! Minimal libp2p-based networking service (feature: `libp2p-full`).
//!
//! Design notes:
//! - The libp2p `Swarm` must be driven by a single async task.
//! - Callers interact via a command channel (publish/dial).
//! - We keep the same high-level `MessageEnvelope` plumbing as the simple TCP service.

use crate::{
    config::NetworkConfig,
    error::{NetworkError, NetworkResult},
};

use catalyst_utils::logging::*;
use catalyst_utils::network::MessageEnvelope;

use futures::StreamExt;
use libp2p::{
    core::upgrade,
    gossipsub,
    identify,
    identity,
    mdns,
    noise,
    ping,
    swarm::SwarmEvent,
    tcp,
    yamux,
    Multiaddr, PeerId, Swarm, Transport,
};
use std::{
    collections::HashSet,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, Mutex, RwLock};

/// Network events for external subscribers.
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    PeerConnected { peer_id: PeerId, address: Multiaddr },
    PeerDisconnected { peer_id: PeerId, reason: String },
    MessageReceived { envelope: MessageEnvelope, from: PeerId },
    Error { error: NetworkError },
}

/// Minimal network stats used by CLI/RPC.
#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    pub connected_peers: usize,
    pub messages_sent: u64,
    pub messages_received: u64,
}

#[derive(libp2p::swarm::NetworkBehaviour)]
#[behaviour(to_swarm = "BehaviourEvent", event_process = false)]
struct Behaviour {
    gossipsub: gossipsub::Behaviour,
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
}

#[derive(Debug)]
enum BehaviourEvent {
    Gossipsub(gossipsub::Event),
    Mdns(mdns::Event),
    Identify(identify::Event),
    Ping(ping::Event),
}

impl From<gossipsub::Event> for BehaviourEvent {
    fn from(e: gossipsub::Event) -> Self {
        BehaviourEvent::Gossipsub(e)
    }
}
impl From<mdns::Event> for BehaviourEvent {
    fn from(e: mdns::Event) -> Self {
        BehaviourEvent::Mdns(e)
    }
}
impl From<identify::Event> for BehaviourEvent {
    fn from(e: identify::Event) -> Self {
        BehaviourEvent::Identify(e)
    }
}
impl From<ping::Event> for BehaviourEvent {
    fn from(e: ping::Event) -> Self {
        BehaviourEvent::Ping(e)
    }
}

#[derive(Debug)]
enum Cmd {
    Publish(Vec<u8>),
    Dial(Multiaddr),
}

/// libp2p NetworkService.
pub struct NetworkService {
    config: NetworkConfig,
    topic: gossipsub::IdentTopic,

    event_tx: Arc<RwLock<Vec<mpsc::UnboundedSender<NetworkEvent>>>>,
    stats: Arc<RwLock<NetworkStats>>,
    peers: Arc<RwLock<HashSet<PeerId>>>,

    cmd_tx: mpsc::UnboundedSender<Cmd>,
    cmd_rx: Mutex<Option<mpsc::UnboundedReceiver<Cmd>>>,
    swarm: Mutex<Option<Swarm<Behaviour>>>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl NetworkService {
    pub async fn new(config: NetworkConfig) -> NetworkResult<Self> {
        config.validate()?;

        // Identity (ed25519)
        let id_keys: identity::Keypair = if let Some(kp) = config.peer.keypair.clone() {
            kp
        } else if let Some(path) = &config.peer.keypair_path {
            load_or_generate_keypair(path.as_path())?
        } else {
            identity::Keypair::generate_ed25519()
        };
        let peer_id = PeerId::from(id_keys.public());

        // Transport (tokio TCP + noise + yamux)
        let transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
            .upgrade(upgrade::Version::V1)
            .authenticate(
                noise::Config::new(&id_keys).map_err(|e| NetworkError::ConfigError(e.to_string()))?,
            )
            .multiplex(yamux::Config::default())
            .timeout(config.peer.connection_timeout)
            .boxed();

        // Gossipsub
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .validation_mode(gossipsub::ValidationMode::Permissive)
            .heartbeat_interval(Duration::from_secs(1))
            .build()
            .map_err(|e| NetworkError::ConfigError(e.to_string()))?;

        let mut gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(id_keys.clone()),
            gossipsub_config,
        )
        .map_err(|e| NetworkError::ConfigError(e.to_string()))?;

        let topic = gossipsub::IdentTopic::new("catalyst/envelope/1");
        gossipsub
            .subscribe(&topic)
            .map_err(|e| NetworkError::ConfigError(e.to_string()))?;

        // mDNS (local discovery)
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
            .map_err(|e| NetworkError::ConfigError(e.to_string()))?;

        // Identify + Ping
        let identify = identify::Behaviour::new(identify::Config::new(
            "catalyst/1.0".to_string(),
            id_keys.public(),
        ));
        let ping = ping::Behaviour::new(ping::Config::new());

        let behaviour = Behaviour {
            gossipsub,
            mdns,
            identify,
            ping,
        };

        let mut swarm = Swarm::new(
            transport,
            behaviour,
            peer_id,
            libp2p::swarm::Config::with_tokio_executor(),
        );

        // Listen
        for addr in &config.peer.listen_addresses {
            swarm
                .listen_on(addr.clone())
                .map_err(|e| NetworkError::TransportError(e.to_string()))?;
        }

        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        Ok(Self {
            config,
            topic,
            event_tx: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(NetworkStats::default())),
            peers: Arc::new(RwLock::new(HashSet::new())),
            cmd_tx,
            cmd_rx: Mutex::new(Some(cmd_rx)),
            swarm: Mutex::new(Some(swarm)),
            tasks: Mutex::new(Vec::new()),
        })
    }

    pub async fn start(&self) -> NetworkResult<()> {
        let mut swarm = self
            .swarm
            .lock()
            .await
            .take()
            .ok_or_else(|| NetworkError::ConfigError("NetworkService::start called twice".to_string()))?;

        let mut cmd_rx = self
            .cmd_rx
            .lock()
            .await
            .take()
            .ok_or_else(|| NetworkError::ConfigError("cmd_rx already taken".to_string()))?;

        let event_tx = self.event_tx.clone();
        let stats = self.stats.clone();
        let peers = self.peers.clone();
        let topic = self.topic.clone();

        // Bootstrap dials (best-effort).
        let bootstrap_addrs: Vec<Multiaddr> = self
            .config
            .peer
            .bootstrap_peers
            .iter()
            .map(|(_pid, addr)| addr.clone())
            .collect();
        for addr in bootstrap_addrs {
            let _ = swarm.dial(addr);
        }

        let handle = tokio::spawn(async move {
            let start = Instant::now();
            loop {
                tokio::select! {
                    ev = swarm.select_next_some() => {
                        match ev {
                            SwarmEvent::Behaviour(BehaviourEvent::Mdns(e)) => match e {
                                mdns::Event::Discovered(list) => {
                                    for (peer_id, addr) in list {
                                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                        let _ = swarm.dial(addr.clone());
                                        peers.write().await.insert(peer_id);
                                        let _ = emit(&event_tx, NetworkEvent::PeerConnected { peer_id, address: addr }).await;
                                    }
                                    // Update stats on peer set change.
                                    {
                                        let mut st = stats.write().await;
                                        st.connected_peers = peers.read().await.len();
                                    }
                                }
                                mdns::Event::Expired(list) => {
                                    for (peer_id, _addr) in list {
                                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                                        peers.write().await.remove(&peer_id);
                                        let _ = emit(&event_tx, NetworkEvent::PeerDisconnected { peer_id, reason: "mdns expired".to_string() }).await;
                                    }
                                    // Update stats on peer set change.
                                    {
                                        let mut st = stats.write().await;
                                        st.connected_peers = peers.read().await.len();
                                    }
                                }
                            },
                            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(e)) => {
                                if let gossipsub::Event::Message { message, propagation_source, .. } = e {
                                    let env: MessageEnvelope = match bincode::deserialize(&message.data) {
                                        Ok(e) => e,
                                        Err(_) => continue,
                                    };
                                    {
                                        let mut st = stats.write().await;
                                        st.messages_received += 1;
                                        st.connected_peers = peers.read().await.len();
                                    }
                                    let _ = emit(&event_tx, NetworkEvent::MessageReceived { envelope: env, from: propagation_source }).await;
                                }
                            }
                            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                                swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                peers.write().await.insert(peer_id);
                                {
                                    let mut st = stats.write().await;
                                    st.connected_peers = peers.read().await.len();
                                }
                                let addr = endpoint.get_remote_address().clone();
                                let _ = emit(&event_tx, NetworkEvent::PeerConnected { peer_id, address: addr }).await;
                            }
                            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                                swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                                peers.write().await.remove(&peer_id);
                                {
                                    let mut st = stats.write().await;
                                    st.connected_peers = peers.read().await.len();
                                }
                                let _ = emit(&event_tx, NetworkEvent::PeerDisconnected { peer_id, reason: format!("{:?}", cause) }).await;
                            }
                            SwarmEvent::NewListenAddr { address, .. } => {
                                log_info!(LogCategory::Network, "libp2p listening on {} (uptime {:?})", address, start.elapsed());
                            }
                            _ => {}
                        }

                        // Defensive: keep subscription present.
                        let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic);
                    }
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(Cmd::Publish(bytes)) => {
                                let _ = swarm.behaviour_mut().gossipsub.publish(topic.clone(), bytes);
                                let mut st = stats.write().await;
                                st.messages_sent += 1;
                                st.connected_peers = peers.read().await.len();
                            }
                            Some(Cmd::Dial(addr)) => {
                                let _ = swarm.dial(addr);
                            }
                            None => break,
                        }
                    }
                }
            }
        });

        self.tasks.lock().await.push(handle);
        Ok(())
    }

    pub async fn stop(&self) -> NetworkResult<()> {
        let mut tasks = self.tasks.lock().await;
        for t in tasks.drain(..) {
            t.abort();
        }
        Ok(())
    }

    pub async fn subscribe_events(&self) -> mpsc::UnboundedReceiver<NetworkEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.event_tx.write().await.push(tx);
        rx
    }

    pub async fn get_stats(&self) -> NetworkStats {
        self.stats.read().await.clone()
    }

    pub async fn broadcast_envelope(&self, envelope: &MessageEnvelope) -> NetworkResult<()> {
        let bytes = bincode::serialize(envelope).map_err(|e| NetworkError::SerializationFailed(e.to_string()))?;
        let _ = self.cmd_tx.send(Cmd::Publish(bytes));
        Ok(())
    }

    pub async fn connect_multiaddr(&self, addr: &Multiaddr) -> NetworkResult<()> {
        self.cmd_tx
            .send(Cmd::Dial(addr.clone()))
            .map_err(|_| NetworkError::TransportError("dial channel closed".to_string()))
    }
}

fn load_or_generate_keypair(path: &Path) -> NetworkResult<identity::Keypair> {
    if let Ok(bytes) = std::fs::read(path) {
        if let Ok(kp) = identity::Keypair::from_protobuf_encoding(&bytes) {
            return Ok(kp);
        }
    }

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let kp = identity::Keypair::generate_ed25519();
    let bytes = kp
        .to_protobuf_encoding()
        .map_err(|e| NetworkError::ConfigError(e.to_string()))?;
    std::fs::write(path, bytes).map_err(|e| NetworkError::ConfigError(e.to_string()))?;
    Ok(kp)
}

async fn emit(
    txs: &Arc<RwLock<Vec<mpsc::UnboundedSender<NetworkEvent>>>>,
    ev: NetworkEvent,
) -> NetworkResult<()> {
    for tx in txs.read().await.iter() {
        let _ = tx.send(ev.clone());
    }
    Ok(())
}

