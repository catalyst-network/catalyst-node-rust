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
use catalyst_utils::network::{decode_envelope_wire, encode_envelope_wire, EnvelopeWireError, MessageEnvelope};

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
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, Mutex, RwLock};

// --- Safety limits (anti-DoS) ---
// These are conservative defaults for public testnets.
const MAX_GOSSIP_MESSAGE_BYTES: usize = 8 * 1024 * 1024; // 8 MiB hard cap per message
const PER_PEER_MAX_MSGS_PER_SEC: u32 = 200;
const PER_PEER_MAX_BYTES_PER_SEC: usize = 8 * 1024 * 1024; // 8 MiB/s per peer
const IDENTIFY_PROTOCOL_VERSION: &str = "catalyst/1";

#[derive(Debug, Clone)]
struct DialBackoff {
    attempts: u32,
    next_at: Instant,
}

impl DialBackoff {
    fn can_attempt(&self, now: Instant) -> bool {
        now >= self.next_at
    }
}

#[derive(Debug, Clone)]
struct PeerBudget {
    window_start: Instant,
    msgs: u32,
    bytes: usize,
}

impl PeerBudget {
    fn allow(&mut self, now: Instant, size: usize) -> bool {
        if now.duration_since(self.window_start) >= Duration::from_secs(1) {
            self.window_start = now;
            self.msgs = 0;
            self.bytes = 0;
        }
        self.msgs = self.msgs.saturating_add(1);
        self.bytes = self.bytes.saturating_add(size);
        self.msgs <= PER_PEER_MAX_MSGS_PER_SEC && self.bytes <= PER_PEER_MAX_BYTES_PER_SEC
    }
}

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
    /// Connection counts per peer id (multiple connections per peer can exist).
    peer_conns: Arc<RwLock<HashMap<PeerId, usize>>>,

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
            .heartbeat_interval(config.gossip.heartbeat_interval)
            .build()
            .map_err(|e| NetworkError::ConfigError(e.to_string()))?;

        let mut gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(id_keys.clone()),
            gossipsub_config,
        )
        .map_err(|e| NetworkError::ConfigError(e.to_string()))?;

        let topic = gossipsub::IdentTopic::new(config.gossip.topic_name.clone());
        gossipsub
            .subscribe(&topic)
            .map_err(|e| NetworkError::ConfigError(e.to_string()))?;

        // mDNS (local discovery)
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
            .map_err(|e| NetworkError::ConfigError(e.to_string()))?;

        // Identify + Ping
        let identify = identify::Behaviour::new(identify::Config::new(
            IDENTIFY_PROTOCOL_VERSION.to_string(),
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
            peer_conns: Arc::new(RwLock::new(HashMap::new())),
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
        let peer_conns = self.peer_conns.clone();
        let topic = self.topic.clone();

        // Bootstrap dial manager (WAN-hardening): retry with backoff+jitter until we meet `min_peers`.
        let bootstrap: Vec<(PeerId, Multiaddr)> = self.config.peer.bootstrap_peers.clone();
        let min_peers = self.config.peer.min_peers;
        let max_attempts = self.config.peer.max_retry_attempts;
        let base_backoff = self.config.peer.retry_backoff;
        let mut bootstrap_tick = tokio::time::interval(self.config.discovery.bootstrap_interval);
        bootstrap_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut dial_backoff: HashMap<PeerId, DialBackoff> = HashMap::new();
        let mut incompatible: HashSet<PeerId> = HashSet::new();

        let handle = tokio::spawn(async move {
            let start = Instant::now();
            let mut budgets: HashMap<PeerId, PeerBudget> = HashMap::new();
            loop {
                tokio::select! {
                    _ = bootstrap_tick.tick() => {
                        let connected = peer_conns.read().await.len();
                        if connected >= min_peers {
                            continue;
                        }
                        let now = Instant::now();
                        for (pid, addr) in &bootstrap {
                            if incompatible.contains(pid) {
                                continue;
                            }
                            if let Some(st) = dial_backoff.get(pid) {
                                if !st.can_attempt(now) {
                                    continue;
                                }
                                if max_attempts > 0 && st.attempts >= max_attempts {
                                    continue;
                                }
                            }

                            // Schedule next attempt before dialing to avoid tight loops.
                            let attempts = dial_backoff.get(pid).map(|s| s.attempts).unwrap_or(0) + 1;
                            let backoff = compute_backoff(base_backoff, attempts).unwrap_or(base_backoff);
                            let jitter = jitter_ms(pid, attempts);
                            dial_backoff.insert(*pid, DialBackoff {
                                attempts,
                                next_at: now + backoff + Duration::from_millis(jitter),
                            });

                            swarm.behaviour_mut().gossipsub.add_explicit_peer(pid);
                            let _ = swarm.dial(addr.clone());
                        }
                    }
                    ev = swarm.select_next_some() => {
                        match ev {
                            SwarmEvent::Behaviour(BehaviourEvent::Mdns(e)) => match e {
                                mdns::Event::Discovered(list) => {
                                    for (peer_id, addr) in list {
                                        // Discovery hint: dial and (optionally) add as explicit gossip peer.
                                        swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                        let _ = swarm.dial(addr.clone());
                                        let _ = emit(&event_tx, NetworkEvent::PeerConnected { peer_id, address: addr }).await;
                                    }
                                }
                                mdns::Event::Expired(list) => {
                                    for (peer_id, _addr) in list {
                                        swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                                        let _ = emit(&event_tx, NetworkEvent::PeerDisconnected { peer_id, reason: "mdns expired".to_string() }).await;
                                    }
                                }
                            },
                            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(e)) => {
                                if let gossipsub::Event::Message { message, propagation_source, .. } = e {
                                    if message.data.len() > MAX_GOSSIP_MESSAGE_BYTES {
                                        continue;
                                    }
                                    let now = Instant::now();
                                    let b = budgets.entry(propagation_source).or_insert(PeerBudget {
                                        window_start: now,
                                        msgs: 0,
                                        bytes: 0,
                                    });
                                    if !b.allow(now, message.data.len()) {
                                        continue;
                                    }
                                    let env: MessageEnvelope = match decode_envelope_wire(&message.data) {
                                        Ok(e) => e,
                                        Err(EnvelopeWireError::UnsupportedVersion { got, local }) => {
                                            log_warn!(
                                                LogCategory::Network,
                                                "Dropping message from {} due to unsupported envelope version (got={} local={})",
                                                propagation_source,
                                                got,
                                                local
                                            );
                                            continue;
                                        }
                                        Err(_) => continue,
                                    };
                                    {
                                        let mut st = stats.write().await;
                                        st.messages_received += 1;
                                        st.connected_peers = peer_conns.read().await.len();
                                    }
                                    let _ = emit(&event_tx, NetworkEvent::MessageReceived { envelope: env, from: propagation_source }).await;
                                }
                            }
                            SwarmEvent::Behaviour(BehaviourEvent::Identify(e)) => {
                                if let identify::Event::Received { peer_id, info } = e {
                                    let pv = info.protocol_version;
                                    if pv != IDENTIFY_PROTOCOL_VERSION {
                                        log_warn!(
                                            LogCategory::Network,
                                            "Disconnecting peer {}: incompatible protocol_version={} (local={})",
                                            peer_id,
                                            pv,
                                            IDENTIFY_PROTOCOL_VERSION
                                        );
                                        incompatible.insert(peer_id);
                                        swarm.disconnect_peer_id(peer_id);
                                    }
                                }
                            }
                            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                                swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                dial_backoff.remove(&peer_id);
                                {
                                    let mut m = peer_conns.write().await;
                                    *m.entry(peer_id).or_insert(0) += 1;
                                    let mut st = stats.write().await;
                                    st.connected_peers = m.len();
                                }
                                let addr = endpoint.get_remote_address().clone();
                                let _ = emit(&event_tx, NetworkEvent::PeerConnected { peer_id, address: addr }).await;
                            }
                            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                                swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                                budgets.remove(&peer_id);
                                {
                                    let mut m = peer_conns.write().await;
                                    if let Some(c) = m.get_mut(&peer_id) {
                                        *c = c.saturating_sub(1);
                                        if *c == 0 {
                                            m.remove(&peer_id);
                                        }
                                    }
                                    let mut st = stats.write().await;
                                    st.connected_peers = m.len();
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
                                st.connected_peers = peer_conns.read().await.len();
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
        // Stats are best-effort; ensure peer count reflects live connections even when idle.
        let mut st = self.stats.read().await.clone();
        st.connected_peers = self.peer_conns.read().await.len();
        st
    }

    pub async fn broadcast_envelope(&self, envelope: &MessageEnvelope) -> NetworkResult<()> {
        let bytes = encode_envelope_wire(envelope)
            .map_err(|e| NetworkError::SerializationFailed(e.to_string()))?;
        let _ = self.cmd_tx.send(Cmd::Publish(bytes));
        Ok(())
    }

    pub async fn connect_multiaddr(&self, addr: &Multiaddr) -> NetworkResult<()> {
        self.cmd_tx
            .send(Cmd::Dial(addr.clone()))
            .map_err(|_| NetworkError::TransportError("dial channel closed".to_string()))
    }
}

fn compute_backoff(base: Duration, attempts: u32) -> Option<Duration> {
    let pow = attempts.saturating_sub(1).min(10);
    let mult = 1u64.checked_shl(pow)?;
    let ms = base.as_millis().saturating_mul(mult as u128);
    let ms = ms.min(60_000);
    Some(Duration::from_millis(ms as u64))
}

fn jitter_ms(peer_id: &PeerId, attempts: u32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    peer_id.hash(&mut h);
    attempts.hash(&mut h);
    let v = h.finish();
    (v % 250) as u64
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

