//! A minimal TCP-based networking implementation used as a stepping stone while the
//! full libp2p stack is being updated.
//!
//! This service is intentionally simple:
//! - Listens on the configured `/ip4/.../tcp/...` multiaddr(s)
//! - Dials bootstrap `/ip4/.../tcp/...` peers (no peer-id required)
//! - Broadcasts `catalyst_utils::network::MessageEnvelope` frames to all connected peers
//!
//! It is sufficient to exchange consensus messages across processes for local testnets.

use crate::config::NetworkConfig;
use crate::error::{NetworkError, NetworkResult};

use catalyst_utils::logging::*;
use catalyst_utils::network::{
    decode_envelope_wire, encode_envelope_wire, EnvelopeWireError, MessageEnvelope, RoutingInfo,
};

use futures::{SinkExt, StreamExt};
use libp2p::Multiaddr;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

// --- Safety limits (anti-DoS) ---
// These are conservative defaults for public testnets.
const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024; // 8 MiB hard cap per frame
const PER_CONN_MAX_MSGS_PER_SEC: u32 = 200;
const PER_CONN_MAX_BYTES_PER_SEC: usize = 8 * 1024 * 1024; // 8 MiB/s per connection

#[derive(Debug, Clone)]
struct ConnBudget {
    window_start: std::time::Instant,
    msgs: u32,
    bytes: usize,
}

impl ConnBudget {
    fn allow(&mut self, now: std::time::Instant, size: usize) -> bool {
        if now.duration_since(self.window_start) >= std::time::Duration::from_secs(1) {
            self.window_start = now;
            self.msgs = 0;
            self.bytes = 0;
        }
        self.msgs = self.msgs.saturating_add(1);
        self.bytes = self.bytes.saturating_add(size);
        self.msgs <= PER_CONN_MAX_MSGS_PER_SEC && self.bytes <= PER_CONN_MAX_BYTES_PER_SEC
    }
}

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    PeerConnected { addr: SocketAddr },
    PeerDisconnected { addr: SocketAddr },
    MessageReceived { envelope: MessageEnvelope, from: SocketAddr },
    Error { error: NetworkError },
}

#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    pub connected_peers: usize,
    pub messages_sent: u64,
    pub messages_received: u64,
}

/// Minimal network service.
pub struct NetworkService {
    config: NetworkConfig,
    peers: Arc<Mutex<HashMap<SocketAddr, mpsc::UnboundedSender<Vec<u8>>>>>,
    stats: Arc<RwLock<NetworkStats>>,
    event_tx: Arc<RwLock<Vec<mpsc::UnboundedSender<NetworkEvent>>>>,
    tasks: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    local_id: String,
    seen: Arc<Mutex<HashMap<String, std::time::Instant>>>,
}

impl NetworkService {
    pub async fn new(config: NetworkConfig) -> NetworkResult<Self> {
        config.validate()?;
        let local_id = config
            .peer
            .listen_addresses
            .get(0)
            .map(|a| a.to_string())
            .unwrap_or_else(|| "unknown".to_string());
        Ok(Self {
            config,
            peers: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(RwLock::new(NetworkStats::default())),
            event_tx: Arc::new(RwLock::new(Vec::new())),
            tasks: Arc::new(Mutex::new(Vec::new())),
            local_id,
            seen: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn start(&self) -> NetworkResult<()> {
        // Start listeners
        for addr in &self.config.peer.listen_addresses {
            let socket = multiaddr_to_socketaddr(addr)
                .ok_or_else(|| NetworkError::ConfigError(format!("Unsupported listen address: {}", addr)))?;
            let listener = TcpListener::bind(socket)
                .await
                .map_err(|e| NetworkError::TransportError(format!("Failed to bind {}: {}", socket, e)))?;

            let svc = self.clone();
            let handle = tokio::spawn(async move {
                log_info!(LogCategory::Network, "Listening on {}", socket);
                loop {
                    match listener.accept().await {
                        Ok((stream, peer_addr)) => {
                            let _ = svc.emit(NetworkEvent::PeerConnected { addr: peer_addr }).await;
                            svc.spawn_connection(peer_addr, stream).await;
                        }
                        Err(e) => {
                            let _ = svc.emit(NetworkEvent::Error {
                                error: NetworkError::TransportError(format!("accept error: {}", e)),
                            })
                            .await;
                        }
                    }
                }
            });
            self.tasks.lock().await.push(handle);
        }

        // Dial bootstrap peers with retry/backoff/jitter until we meet `min_peers`.
        let svc = self.clone();
        let handle = tokio::spawn(async move {
            svc.bootstrap_dial_loop().await;
        });
        self.tasks.lock().await.push(handle);

        Ok(())
    }

    /// Dial a peer using a plain multiaddr (e.g. `/ip4/127.0.0.1/tcp/30333`).
    pub async fn connect_multiaddr(&self, addr: &Multiaddr) -> NetworkResult<()> {
        let socket = multiaddr_to_socketaddr(addr)
            .ok_or_else(|| NetworkError::ConfigError(format!("Unsupported peer address: {}", addr)))?;

        match TcpStream::connect(socket).await {
            Ok(stream) => {
                let _ = self.emit(NetworkEvent::PeerConnected { addr: socket }).await;
                self.spawn_connection(socket, stream).await;
                Ok(())
            }
            Err(e) => Err(NetworkError::TransportError(format!("Failed to connect to {}: {}", socket, e))),
        }
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
        st.connected_peers = self.peers.lock().await.len();
        st
    }

    /// Broadcast a message envelope to all connected peers.
    pub async fn broadcast_envelope(&self, envelope: &MessageEnvelope) -> NetworkResult<()> {
        let bytes =
            encode_envelope_wire(envelope).map_err(|e| NetworkError::SerializationFailed(e.to_string()))?;

        let peers = self.peers.lock().await;
        for (_addr, tx) in peers.iter() {
            let _ = tx.send(bytes.clone());
        }

        let mut stats = self.stats.write().await;
        stats.messages_sent += 1;
        stats.connected_peers = peers.len();
        Ok(())
    }

    async fn spawn_connection(&self, peer_addr: SocketAddr, stream: TcpStream) {
        let peers = self.peers.clone();
        let stats = self.stats.clone();
        let svc = self.clone();
        let local_id = self.local_id.clone();
        let seen = self.seen.clone();
        let dedup_window = self.config.gossip.duplicate_detection_window;

        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        {
            let mut map = peers.lock().await;
            map.insert(peer_addr, out_tx);
            let mut st = stats.write().await;
            st.connected_peers = map.len();
        }

        let framed = Framed::new(stream, LengthDelimitedCodec::new());
        let (mut sink, mut stream) = framed.split();

        let writer = tokio::spawn(async move {
            while let Some(bytes) = out_rx.recv().await {
                if sink.send(bytes.into()).await.is_err() {
                    break;
                }
            }
        });

        let reader = tokio::spawn(async move {
            let mut budget = ConnBudget {
                window_start: std::time::Instant::now(),
                msgs: 0,
                bytes: 0,
            };
            while let Some(Ok(bytes)) = stream.next().await {
                if bytes.len() > MAX_FRAME_BYTES {
                    continue;
                }
                let now = std::time::Instant::now();
                if !budget.allow(now, bytes.len()) {
                    continue;
                }
                let env: MessageEnvelope = match decode_envelope_wire(&bytes) {
                    Ok(e) => e,
                    Err(EnvelopeWireError::UnsupportedVersion { got, local }) => {
                        log_warn!(
                            LogCategory::Network,
                            "Dropping frame from {} due to unsupported envelope version (got={} local={})",
                            peer_addr,
                            got,
                            local
                        );
                        continue;
                    }
                    Err(_) => continue,
                };

                // Dedup by envelope id within a sliding window.
                {
                    let mut s = seen.lock().await;
                    s.retain(|_, t| now.duration_since(*t) <= dedup_window);
                    if s.contains_key(&env.id) {
                        continue;
                    }
                    s.insert(env.id.clone(), now);
                }

                {
                    let mut st = stats.write().await;
                    st.messages_received += 1;
                }

                let _ = svc
                    .emit(NetworkEvent::MessageReceived {
                        envelope: env.clone(),
                        from: peer_addr,
                    })
                    .await;

                // Multi-hop rebroadcast: forward broadcast envelopes to all peers except sender
                // with hop/loop limits.
                if env.target.is_none() && should_forward(&env, &local_id) {
                    if let Some(fwd) = forwarded(env, &local_id) {
                        let bytes = match encode_envelope_wire(&fwd) {
                            Ok(b) => b,
                            Err(_) => continue,
                        };
                        let peers = peers.lock().await;
                        for (addr, tx) in peers.iter() {
                            if *addr == peer_addr {
                                continue;
                            }
                            let _ = tx.send(bytes.clone());
                        }
                    }
                }
            }

            {
                let mut map = peers.lock().await;
                map.remove(&peer_addr);
                let mut st = stats.write().await;
                st.connected_peers = map.len();
            }
            let _ = svc.emit(NetworkEvent::PeerDisconnected { addr: peer_addr }).await;
        });

        self.tasks.lock().await.push(writer);
        self.tasks.lock().await.push(reader);
    }

    async fn bootstrap_dial_loop(&self) {
        let bootstrap: Vec<SocketAddr> = self
            .config
            .peer
            .bootstrap_peers
            .iter()
            .filter_map(|(_peer_id, addr)| multiaddr_to_socketaddr(addr))
            .collect();

        if bootstrap.is_empty() {
            return;
        }

        let mut backoff: HashMap<SocketAddr, (u32, std::time::Instant)> = HashMap::new();
        let mut incompatible: HashSet<SocketAddr> = HashSet::new();

        let base = self.config.peer.retry_backoff;
        let max_attempts = self.config.peer.max_retry_attempts;
        let mut tick = tokio::time::interval(self.config.discovery.bootstrap_interval);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tick.tick().await;

            let connected = self.peers.lock().await.len();
            if connected >= self.config.peer.min_peers {
                continue;
            }

            for addr in &bootstrap {
                if incompatible.contains(addr) {
                    continue;
                }

                let now = std::time::Instant::now();
                let (attempts, next_at) = backoff.get(addr).cloned().unwrap_or((0, now));
                if now < next_at {
                    continue;
                }
                if max_attempts > 0 && attempts >= max_attempts {
                    continue;
                }

                let socket = *addr;
                let svc = self.clone();
                let attempts_next = attempts.saturating_add(1);
                let delay = compute_backoff(base, attempts_next).unwrap_or(base);
                let jitter = jitter_ms(socket, attempts_next);
                backoff.insert(
                    *addr,
                    (
                        attempts_next,
                        now + delay + std::time::Duration::from_millis(jitter),
                    ),
                );

                let handle = tokio::spawn(async move {
                    match TcpStream::connect(socket).await {
                        Ok(stream) => {
                            let _ = svc.emit(NetworkEvent::PeerConnected { addr: socket }).await;
                            svc.spawn_connection(socket, stream).await;
                        }
                        Err(e) => {
                            log_warn!(LogCategory::Network, "bootstrap dial failed {}: {}", socket, e);
                        }
                    }
                });
                self.tasks.lock().await.push(handle);
            }
        }
    }

    async fn emit(&self, event: NetworkEvent) -> NetworkResult<()> {
        let senders = self.event_tx.read().await;
        for tx in senders.iter() {
            let _ = tx.send(event.clone());
        }
        Ok(())
    }
}

impl Clone for NetworkService {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            peers: self.peers.clone(),
            stats: self.stats.clone(),
            event_tx: self.event_tx.clone(),
            tasks: self.tasks.clone(),
            local_id: self.local_id.clone(),
            seen: self.seen.clone(),
        }
    }
}

fn should_forward(env: &MessageEnvelope, local_id: &str) -> bool {
    if env.is_expired() {
        return false;
    }
    if let Some(r) = &env.routing_info {
        if r.hop_count >= r.max_hops {
            return false;
        }
        if r.visited_nodes.iter().any(|v| v == local_id) {
            return false;
        }
    }
    true
}

fn forwarded(mut env: MessageEnvelope, local_id: &str) -> Option<MessageEnvelope> {
    if env.is_expired() {
        return None;
    }
    let mut r = env.routing_info.take().unwrap_or_else(|| RoutingInfo::new(10));
    if r.hop_count >= r.max_hops {
        return None;
    }
    if r.visited_nodes.iter().any(|v| v == local_id) {
        return None;
    }
    r.visited_nodes.push(local_id.to_string());
    r.hop_count = r.hop_count.saturating_add(1);
    env.routing_info = Some(r);
    Some(env)
}

fn compute_backoff(base: std::time::Duration, attempts: u32) -> Option<std::time::Duration> {
    // base * 2^(attempts-1), clamped to 60s
    let pow = attempts.saturating_sub(1).min(10); // 2^10 = 1024x
    let mult = 1u64.checked_shl(pow)?;
    let ms = base.as_millis().saturating_mul(mult as u128);
    let ms = ms.min(60_000);
    Some(std::time::Duration::from_millis(ms as u64))
}

fn jitter_ms(addr: SocketAddr, attempts: u32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    addr.hash(&mut h);
    attempts.hash(&mut h);
    let v = h.finish();
    (v % 250) as u64
}

fn multiaddr_to_socketaddr(addr: &Multiaddr) -> Option<SocketAddr> {
    // Support: /ip4/x.x.x.x/tcp/port (and /ip6/.../tcp/port)
    let mut ip: Option<IpAddr> = None;
    let mut port: Option<u16> = None;
    for p in addr.iter() {
        match p {
            libp2p::multiaddr::Protocol::Ip4(v4) => ip = Some(IpAddr::V4(v4)),
            libp2p::multiaddr::Protocol::Ip6(v6) => ip = Some(IpAddr::V6(v6)),
            libp2p::multiaddr::Protocol::Tcp(p) => port = Some(p),
            _ => {}
        }
    }
    match (ip, port) {
        (Some(ip), Some(port)) => Some(SocketAddr::new(ip, port)),
        _ => None,
    }
}

