//! Minimal libp2p-based networking service (feature: `libp2p-full`).
//!
//! Design notes:
//! - The libp2p `Swarm` must be driven by a single async task.
//! - Callers interact via a command channel (publish/dial).
//! - We keep the same high-level `MessageEnvelope` plumbing as the simple TCP service.

use crate::{
    config::NetworkConfig,
    error::{NetworkError, NetworkResult},
    protocol_identify::{
        catalyst_identify_protocol_major_ok, CATALYST_IDENTIFY_PROTOCOL_VERSION,
    },
};

use catalyst_utils::logging::*;
use catalyst_utils::network::{decode_envelope_wire, encode_envelope_wire, EnvelopeWireError, MessageEnvelope, MessageType};

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
    fn allow(&mut self, now: Instant, size: usize, max_msgs: u32, max_bytes: usize) -> bool {
        if now.duration_since(self.window_start) >= Duration::from_secs(1) {
            self.window_start = now;
            self.msgs = 0;
            self.bytes = 0;
        }
        self.msgs = self.msgs.saturating_add(1);
        self.bytes = self.bytes.saturating_add(size);
        self.msgs <= max_msgs && self.bytes <= max_bytes
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
    // Caps concurrent established connections per peer at 1 (regardless of direction). Without
    // this, nothing in the swarm dedupes redial attempts against an already-connected peer, and
    // ordinary reconnect churn (dial retries racing an in-flight connection, ping-timeout
    // disconnects, etc.) silently stacks additional live connections instead of replacing the
    // existing one -- observed live 2026-08-17 as 30-50k duplicate connections per peer pair
    // accumulated over ~4 days, exhausting host memory/CPU and starving the gossipsub send
    // queues this crate otherwise depends on for consensus-critical traffic.
    connection_limits: libp2p::connection_limits::Behaviour,
}

#[derive(Debug)]
enum BehaviourEvent {
    Gossipsub(gossipsub::Event),
    Mdns(mdns::Event),
    Identify(identify::Event),
    Ping(ping::Event),
    ConnectionLimits(std::convert::Infallible),
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
impl From<std::convert::Infallible> for BehaviourEvent {
    fn from(e: std::convert::Infallible) -> Self {
        BehaviourEvent::ConnectionLimits(e)
    }
}

/// Which gossipsub topic (and therefore which independent per-peer send queue) a message
/// goes out on. Consensus messages get their own topic so routine high-volume traffic
/// (tx relay/resync, state sync, etc.) can never head-of-line-block them behind a shared
/// FIFO queue -- confirmed live 2026-08-18/19: a `ProducerQuantity` took 40+ seconds to
/// arrive (well past each phase's ~4s collection window) despite `publish()` reporting
/// success immediately, consistent with queuing behind a burst of routine traffic on the
/// single shared queue this used to be. See `NetworkService::topic_for`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GossipChannel {
    Default,
    Consensus,
}

#[derive(Debug)]
enum Cmd {
    Publish(Vec<u8>, GossipChannel),
    Dial(Multiaddr),
}

/// libp2p NetworkService.
pub struct NetworkService {
    config: NetworkConfig,
    topic: gossipsub::IdentTopic,
    /// Dedicated topic for consensus messages -- see `GossipChannel`.
    topic_consensus: gossipsub::IdentTopic,

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
        //
        // `connection_handler_queue_len` set explicitly (2x the libp2p default of 5000): live
        // production logs showed ~40-50% of cycles never getting a state-root certificate
        // published, traced to "Send Queue full" drops -- every message type (tx relay, LSU CID
        // gossip, state-root attestations, range requests) shares one topic and one per-peer
        // queue with no priority separation, so latency-critical consensus attestations were
        // silently dropped alongside routine tx rebroadcast spam. This buys headroom while the
        // rebroadcast volume itself is also being reduced (see `rebroadcast_persisted_mempool`
        // in catalyst-cli); raising it further without evidence would just trade drops for
        // unbounded queue growth instead of fixing the underlying contention.
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .validation_mode(gossipsub::ValidationMode::Permissive)
            .heartbeat_interval(config.gossip.heartbeat_interval)
            .connection_handler_queue_len(10_000)
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

        // Dedicated topic for consensus messages -- see `GossipChannel` doc comment for why.
        let topic_consensus = gossipsub::IdentTopic::new(format!("{}-consensus", config.gossip.topic_name));
        gossipsub
            .subscribe(&topic_consensus)
            .map_err(|e| NetworkError::ConfigError(e.to_string()))?;

        // mDNS (local discovery)
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), peer_id)
            .map_err(|e| NetworkError::ConfigError(e.to_string()))?;

        // Identify + Ping
        let identify = identify::Behaviour::new(identify::Config::new(
            CATALYST_IDENTIFY_PROTOCOL_VERSION.to_string(),
            id_keys.public(),
        ));
        let ping = ping::Behaviour::new(ping::Config::new());

        // See the doc comment on `Behaviour::connection_limits` for why this exists: caps
        // established connections per peer at 1 so reconnect churn can't silently accumulate
        // duplicate connections.
        let connection_limits = libp2p::connection_limits::Behaviour::new(
            libp2p::connection_limits::ConnectionLimits::default()
                .with_max_established_per_peer(Some(1)),
        );

        let behaviour = Behaviour {
            gossipsub,
            mdns,
            identify,
            ping,
            connection_limits,
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
            topic_consensus,
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
        let topic_consensus = self.topic_consensus.clone();
        let limits = self.config.safety_limits.clone();

        // Bootstrap dial manager (WAN-hardening): retry with backoff+jitter until we meet `min_peers`.
        let bootstrap: Vec<(PeerId, Multiaddr)> = self.config.peer.bootstrap_peers.clone();
        let min_peers = self.config.peer.min_peers;
        let max_attempts = self.config.peer.max_retry_attempts;
        let base_backoff = self.config.peer.retry_backoff;
        let mut bootstrap_tick = tokio::time::interval(self.config.discovery.bootstrap_interval);
        bootstrap_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut dial_backoff: HashMap<PeerId, DialBackoff> = HashMap::new();
        let mut incompatible: HashSet<PeerId> = HashSet::new();

        // TEMPORARY DIAGNOSTIC (2026-08-18, gossip-mesh-stall investigation): mirrors exactly the
        // peer_ids we've called gossipsub.add_explicit_peer/remove_explicit_peer with, since the
        // gossipsub crate doesn't expose a public getter for its internal explicit_peers set.
        // Logged alongside real mesh state (all_mesh_peers/all_peers) and every connection event
        // to check whether a peer can go connected-but-not-explicit/not-meshed without any
        // connection-layer signal of it. Remove once the stall is root-caused.
        let mut explicit_peers_diag: HashSet<PeerId> = HashSet::new();
        let mut mesh_diag_tick = tokio::time::interval(Duration::from_secs(15));
        mesh_diag_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        let handle = tokio::spawn(async move {
            let start = Instant::now();
            let mut budgets: HashMap<PeerId, PeerBudget> = HashMap::new();
            loop {
                tokio::select! {
                    _ = mesh_diag_tick.tick() => {
                        let conns = peer_conns.read().await.clone();
                        let mesh: Vec<PeerId> = swarm.behaviour().gossipsub.all_mesh_peers().copied().collect();
                        let all: Vec<(PeerId, usize)> = swarm.behaviour().gossipsub.all_peers()
                            .map(|(pid, topics)| (*pid, topics.len()))
                            .collect();
                        tracing::info!(
                            "[gossip-mesh-diag] conns={:?} explicit={:?} mesh={:?} all_peers(topic_subs)={:?}",
                            conns, explicit_peers_diag, mesh, all
                        );
                    }
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
                            // Already connected: skip. The connection_limits behaviour would
                            // reject a redundant dial anyway, but checking here avoids wasting a
                            // handshake attempt and resetting this peer's dial_backoff for no
                            // reason (see ConnectionEstablished handler below).
                            if swarm.is_connected(pid) {
                                dial_backoff.remove(pid);
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
                            let backoff = compute_backoff(base_backoff, attempts, limits.dial_backoff_max_ms)
                                .unwrap_or(base_backoff);
                            let jitter = jitter_ms(pid, attempts, limits.dial_jitter_max_ms);
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
                                        if !swarm.is_connected(&peer_id) {
                                            let _ = swarm.dial(addr.clone());
                                        }
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
                                    if message.data.len() > limits.max_gossip_message_bytes {
                                        // TEMPORARY DIAGNOSTIC
                                        tracing::info!(
                                            "[gossip-mesh-diag] DROPPED (size limit) from={} len={} max={}",
                                            propagation_source, message.data.len(), limits.max_gossip_message_bytes
                                        );
                                        continue;
                                    }
                                    let now = Instant::now();
                                    let b = budgets.entry(propagation_source).or_insert(PeerBudget {
                                        window_start: now,
                                        msgs: 0,
                                        bytes: 0,
                                    });
                                    if !b.allow(
                                        now,
                                        message.data.len(),
                                        limits.per_peer_max_msgs_per_sec,
                                        limits.per_peer_max_bytes_per_sec,
                                    ) {
                                        // TEMPORARY DIAGNOSTIC
                                        tracing::info!(
                                            "[gossip-mesh-diag] DROPPED (budget exceeded) from={} len={}",
                                            propagation_source, message.data.len()
                                        );
                                        continue;
                                    }
                                    let env: MessageEnvelope = match decode_envelope_wire(&message.data) {
                                        Ok(e) => e,
                                        Err(EnvelopeWireError::UnsupportedVersion { got, local }) => {
                                            // TEMPORARY DIAGNOSTIC: was log_warn! (dead logger, see
                                            // 7d08036) -- switched to tracing::info! so this is
                                            // actually visible while investigating.
                                            tracing::info!(
                                                "[gossip-mesh-diag] DROPPED (unsupported envelope version) from={} got={} local={}",
                                                propagation_source,
                                                got,
                                                local
                                            );
                                            continue;
                                        }
                                        Err(err) => {
                                            // TEMPORARY DIAGNOSTIC: this decode-failure path had zero
                                            // logging before (silent `continue`).
                                            tracing::info!(
                                                "[gossip-mesh-diag] DROPPED (decode error) from={} len={} err={:?}",
                                                propagation_source, message.data.len(), err
                                            );
                                            continue;
                                        }
                                    };
                                    // TEMPORARY DIAGNOSTIC
                                    tracing::info!(
                                        "[gossip-mesh-diag] RECEIVED type={:?} id={} sender={} from={} len={}",
                                        env.message_type, env.id, env.sender, propagation_source, message.data.len()
                                    );
                                    {
                                        let mut st = stats.write().await;
                                        st.messages_received += 1;
                                        st.connected_peers = peer_conns.read().await.len();
                                    }
                                    let _ = emit(&event_tx, NetworkEvent::MessageReceived { envelope: env, from: propagation_source }).await;
                                }
                            }
                            SwarmEvent::Behaviour(BehaviourEvent::Identify(e)) => {
                                if let identify::Event::Received { peer_id, info, .. } = e {
                                    let pv = info.protocol_version;
                                    if !catalyst_identify_protocol_major_ok(&pv) {
                                        log_warn!(
                                            LogCategory::Network,
                                            "Disconnecting peer {}: incompatible protocol_version={} (supported catalyst major 1; local identify={})",
                                            peer_id,
                                            pv,
                                            CATALYST_IDENTIFY_PROTOCOL_VERSION
                                        );
                                        incompatible.insert(peer_id);
                                        swarm.disconnect_peer_id(peer_id);
                                    }
                                }
                            }
                            SwarmEvent::ConnectionEstablished { peer_id, endpoint, connection_id, .. } => {
                                swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
                                explicit_peers_diag.insert(peer_id); // TEMPORARY DIAGNOSTIC
                                dial_backoff.remove(&peer_id);
                                let resulting_count = {
                                    let mut m = peer_conns.write().await;
                                    let c = m.entry(peer_id).or_insert(0);
                                    *c += 1;
                                    let count = *c;
                                    let mut st = stats.write().await;
                                    st.connected_peers = m.len();
                                    count
                                };
                                // TEMPORARY DIAGNOSTIC
                                tracing::info!(
                                    "[gossip-mesh-diag] ConnectionEstablished peer={} connection_id={:?} endpoint={:?} resulting_conn_count={}",
                                    peer_id, connection_id, endpoint, resulting_count
                                );
                                let addr = endpoint.get_remote_address().clone();
                                let _ = emit(&event_tx, NetworkEvent::PeerConnected { peer_id, address: addr }).await;
                            }
                            SwarmEvent::ConnectionClosed { peer_id, cause, connection_id, .. } => {
                                swarm.behaviour_mut().gossipsub.remove_explicit_peer(&peer_id);
                                explicit_peers_diag.remove(&peer_id); // TEMPORARY DIAGNOSTIC
                                budgets.remove(&peer_id);
                                let resulting_count = {
                                    let mut m = peer_conns.write().await;
                                    let count = if let Some(c) = m.get_mut(&peer_id) {
                                        *c = c.saturating_sub(1);
                                        let count = *c;
                                        if *c == 0 {
                                            m.remove(&peer_id);
                                        }
                                        count
                                    } else {
                                        0
                                    };
                                    let mut st = stats.write().await;
                                    st.connected_peers = m.len();
                                    count
                                };
                                // TEMPORARY DIAGNOSTIC: if resulting_conn_count > 0 here, we just
                                // stripped explicit-peer status from a peer that's still connected
                                // via another connection -- the suspected bug.
                                tracing::info!(
                                    "[gossip-mesh-diag] ConnectionClosed peer={} connection_id={:?} cause={:?} resulting_conn_count={}",
                                    peer_id, connection_id, cause, resulting_count
                                );
                                let _ = emit(&event_tx, NetworkEvent::PeerDisconnected { peer_id, reason: format!("{:?}", cause) }).await;
                            }
                            SwarmEvent::NewListenAddr { address, .. } => {
                                log_info!(LogCategory::Network, "libp2p listening on {} (uptime {:?})", address, start.elapsed());
                            }
                            // TEMPORARY DIAGNOSTIC: catches connection_limits rejecting a
                            // duplicate/racing dial -- expected under max_established_per_peer(1)
                            // when both sides of a peer pair dial each other near-simultaneously.
                            SwarmEvent::OutgoingConnectionError { connection_id, peer_id, error } => {
                                tracing::info!(
                                    "[gossip-mesh-diag] OutgoingConnectionError connection_id={:?} peer={:?} error={:?}",
                                    connection_id, peer_id, error
                                );
                            }
                            SwarmEvent::IncomingConnectionError { connection_id, peer_id, error, .. } => {
                                tracing::info!(
                                    "[gossip-mesh-diag] IncomingConnectionError connection_id={:?} peer={:?} error={:?}",
                                    connection_id, peer_id, error
                                );
                            }
                            _ => {}
                        }

                        // Defensive: keep subscriptions present.
                        let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic);
                        let _ = swarm.behaviour_mut().gossipsub.subscribe(&topic_consensus);
                    }
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            Some(Cmd::Publish(bytes, channel)) => {
                                // TEMPORARY DIAGNOSTIC: this Result was previously discarded
                                // (`let _ = ...publish(...)`) -- any PublishError (e.g.
                                // InsufficientPeers, NoPeersSubscribedToTopic, Duplicate) was
                                // completely silent.
                                let peek = decode_envelope_wire(&bytes).ok();
                                let dest_topic = match channel {
                                    GossipChannel::Consensus => topic_consensus.clone(),
                                    GossipChannel::Default => topic.clone(),
                                };
                                let result = swarm.behaviour_mut().gossipsub.publish(dest_topic, bytes);
                                match &result {
                                    Ok(msg_id) => tracing::info!(
                                        "[gossip-mesh-diag] PUBLISH ok msg_id={:?} type={:?} sender={:?} conns={}",
                                        msg_id,
                                        peek.as_ref().map(|e| e.message_type),
                                        peek.as_ref().map(|e| e.sender.clone()),
                                        peer_conns.read().await.len()
                                    ),
                                    Err(err) => tracing::info!(
                                        "[gossip-mesh-diag] PUBLISH FAILED err={:?} type={:?} sender={:?} conns={}",
                                        err,
                                        peek.as_ref().map(|e| e.message_type),
                                        peek.as_ref().map(|e| e.sender.clone()),
                                        peer_conns.read().await.len()
                                    ),
                                }
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
        let channel = match envelope.message_type {
            MessageType::ProducerQuantity
            | MessageType::ProducerCandidate
            | MessageType::ProducerVote
            | MessageType::ProducerOutput
            | MessageType::ConsensusSync => GossipChannel::Consensus,
            _ => GossipChannel::Default,
        };
        let _ = self.cmd_tx.send(Cmd::Publish(bytes, channel));
        Ok(())
    }

    pub async fn connect_multiaddr(&self, addr: &Multiaddr) -> NetworkResult<()> {
        self.cmd_tx
            .send(Cmd::Dial(addr.clone()))
            .map_err(|_| NetworkError::TransportError("dial channel closed".to_string()))
    }

    /// Inject an envelope as if received from a peer (integration tests / `test-hooks` feature).
    #[cfg(any(test, feature = "test-hooks"))]
    pub async fn inject_test_envelope(&self, envelope: MessageEnvelope) -> NetworkResult<()> {
        let from = PeerId::random();
        emit(
            &self.event_tx,
            NetworkEvent::MessageReceived {
                envelope,
                from,
            },
        )
        .await
    }
}

fn compute_backoff(base: Duration, attempts: u32, max_ms: u64) -> Option<Duration> {
    let pow = attempts.saturating_sub(1).min(10);
    let mult = 1u64.checked_shl(pow)?;
    let ms = base.as_millis().saturating_mul(mult as u128);
    let ms = ms.min(max_ms as u128);
    Some(Duration::from_millis(ms as u64))
}

fn jitter_ms(peer_id: &PeerId, attempts: u32, max_ms: u64) -> u64 {
    if max_ms == 0 {
        return 0;
    }
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    peer_id.hash(&mut h);
    attempts.hash(&mut h);
    let v = h.finish();
    (v % (max_ms + 1)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::PeerId;

    #[test]
    fn peer_budget_enforces_msgs_and_bytes() {
        let now = Instant::now();
        let mut b = PeerBudget {
            window_start: now,
            msgs: 0,
            bytes: 0,
        };

        // Allow exactly 2 messages of 10 bytes each.
        assert!(b.allow(now, 10, 2, 20));
        assert!(b.allow(now, 10, 2, 20));
        // Third message denied by msg cap.
        assert!(!b.allow(now, 1, 2, 20));
    }

    #[test]
    fn backoff_and_jitter_are_bounded() {
        let base = Duration::from_millis(100);
        let b = compute_backoff(base, 100, 1_000).unwrap();
        assert!(b <= Duration::from_millis(1_000));

        let pid = PeerId::random();
        let j = jitter_ms(&pid, 5, 250);
        assert!(j <= 250);
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

