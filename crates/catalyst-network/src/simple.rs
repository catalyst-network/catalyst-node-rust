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
use catalyst_utils::network::MessageEnvelope;

use futures::{SinkExt, StreamExt};
use libp2p::Multiaddr;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

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
}

impl NetworkService {
    pub async fn new(config: NetworkConfig) -> NetworkResult<Self> {
        config.validate()?;
        Ok(Self {
            config,
            peers: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(RwLock::new(NetworkStats::default())),
            event_tx: Arc::new(RwLock::new(Vec::new())),
            tasks: Arc::new(Mutex::new(Vec::new())),
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

        // Dial bootstrap peers (best-effort).
        for (_peer_id, addr) in &self.config.peer.bootstrap_peers {
            if let Some(socket) = multiaddr_to_socketaddr(addr) {
                let svc = self.clone();
                let handle = tokio::spawn(async move {
                    match TcpStream::connect(socket).await {
                        Ok(stream) => {
                            let _ = svc.emit(NetworkEvent::PeerConnected { addr: socket }).await;
                            svc.spawn_connection(socket, stream).await;
                        }
                        Err(e) => {
                            let _ = svc.emit(NetworkEvent::Error {
                                error: NetworkError::Timeout { duration: std::time::Duration::from_secs(0) },
                            })
                            .await;
                            log_warn!(LogCategory::Network, "Failed to connect to bootstrap {}: {}", socket, e);
                        }
                    }
                });
                self.tasks.lock().await.push(handle);
            }
        }

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
            bincode::serialize(envelope).map_err(|e| NetworkError::SerializationFailed(e.to_string()))?;

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
            while let Some(Ok(bytes)) = stream.next().await {
                let env: MessageEnvelope = match bincode::deserialize(&bytes) {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                {
                    let mut st = stats.write().await;
                    st.messages_received += 1;
                }

                let _ = svc
                    .emit(NetworkEvent::MessageReceived {
                        envelope: env,
                        from: peer_addr,
                    })
                    .await;
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
        }
    }
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

