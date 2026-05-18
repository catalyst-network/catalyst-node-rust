//! TCP `NetworkService` test mesh: real listeners + optional selective delivery via inject.
//!
//! Used for checklist §7.3: proves wire-encoded envelopes flow across TCP peers and supports
//! batch-drop scenarios (linear relay topology or inject filter).

use std::sync::Arc;
use std::time::Duration;

use catalyst_utils::MessageType;
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::config::NetworkConfig;
use crate::error::NetworkResult;
use crate::gossip_sim::{gossip_sim_should_deliver, GossipSimConfig};
use crate::{NetworkEvent, NetworkService};

fn link_settle_ms() -> u64 {
    if cfg!(feature = "libp2p-full") {
        800
    } else {
        200
    }
}

fn mesh_settle_ms() -> u64 {
    if cfg!(feature = "libp2p-full") {
        1_200
    } else {
        250
    }
}

/// Ephemeral base port for parallel-safe tests (`127.0.0.1:0` bind).
pub fn ephemeral_base_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    listener.local_addr().expect("local addr").port()
}

fn test_config_port(base_port: u16) -> NetworkConfig {
    let mut cfg = NetworkConfig::test_config();
    let p = base_port;
    cfg.peer.listen_addresses = vec![
        format!("/ip4/127.0.0.1/tcp/{}", p).parse().expect("multiaddr"),
    ];
    cfg.peer.bootstrap_peers.clear();
    cfg.peer.min_peers = 0;
    cfg
}

/// Keeps mesh tasks and network services alive.
pub struct TcpTestMesh {
    pub services: Vec<Arc<NetworkService>>,
    _tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl TcpTestMesh {
    /// Three nodes on `base_port`, `base_port+1`, `base_port+2` in a line: 0—1—2.
    pub async fn line_topology_three(base_port: u16) -> NetworkResult<Self> {
        let mut services = Vec::new();
        for i in 0..3u16 {
            let cfg = test_config_port(base_port.saturating_add(i));
            let svc = Arc::new(NetworkService::new(cfg).await?);
            svc.start().await?;
            services.push(svc);
        }
        sleep(Duration::from_millis(link_settle_ms())).await;
        services[1]
            .connect_multiaddr(
                &format!("/ip4/127.0.0.1/tcp/{}", base_port)
                    .parse()
                    .expect("addr"),
            )
            .await?;
        services[2]
            .connect_multiaddr(
                &format!("/ip4/127.0.0.1/tcp/{}", base_port.saturating_add(1))
                    .parse()
                    .expect("addr"),
            )
            .await?;
        sleep(Duration::from_millis(link_settle_ms())).await;
        Ok(Self {
            services,
            _tasks: Vec::new(),
        })
    }

    /// Line 0—1—2 plus coordinators on nodes 0 and 1 that re-inject with [`GossipSimConfig`] skip rules.
    pub async fn line_with_selective_inject(
        base_port: u16,
        cfg: GossipSimConfig,
    ) -> NetworkResult<Self> {
        let mesh = Self::line_topology_three(base_port).await?;
        let mut tasks = Vec::new();
        for src_idx in 0..2 {
            let peers: Vec<Arc<NetworkService>> = mesh.services.clone();
            let mut rx = mesh.services[src_idx].subscribe_events().await;
            let cfg_copy = cfg;
            tasks.push(tokio::spawn(async move {
                while let Some(ev) = rx.recv().await {
                    let NetworkEvent::MessageReceived { envelope, .. } = ev else {
                        continue;
                    };
                    for (dest_idx, dest) in peers.iter().enumerate() {
                        if dest_idx == src_idx {
                            continue;
                        }
                        if !gossip_sim_should_deliver(&cfg_copy, dest_idx, &envelope) {
                            continue;
                        }
                        let _ = dest.inject_test_envelope(envelope.clone()).await;
                    }
                }
            }));
        }
        Ok(Self {
            services: mesh.services,
            _tasks: tasks,
        })
    }

    /// Full mesh + coordinator that re-injects with [`GossipSimConfig`] skip rules (batch-drop).
    pub async fn full_mesh_with_selective_inject(
        n: usize,
        base_port: u16,
        cfg: GossipSimConfig,
    ) -> NetworkResult<Self> {
        let mut services = Vec::new();
        for i in 0..n {
            let cfg_net = test_config_port(base_port.saturating_add(i as u16));
            let svc = Arc::new(NetworkService::new(cfg_net).await?);
            svc.start().await?;
            services.push(svc);
        }
        sleep(Duration::from_millis(link_settle_ms())).await;
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    continue;
                }
                let _ = services[i].connect_multiaddr(
                    &format!("/ip4/127.0.0.1/tcp/{}", base_port.saturating_add(j as u16))
                        .parse()
                        .expect("addr"),
                );
            }
        }
        sleep(Duration::from_millis(mesh_settle_ms())).await;

        let mut tasks = Vec::new();
        for (src_idx, svc) in services.iter().enumerate() {
            let peers: Vec<Arc<NetworkService>> = services.clone();
            let mut rx = svc.subscribe_events().await;
            let cfg_copy = cfg;
            tasks.push(tokio::spawn(async move {
                while let Some(ev) = rx.recv().await {
                    let NetworkEvent::MessageReceived { envelope, .. } = ev else {
                        continue;
                    };
                    for (dest_idx, dest) in peers.iter().enumerate() {
                        if dest_idx == src_idx {
                            continue;
                        }
                        if !gossip_sim_should_deliver(&cfg_copy, dest_idx, &envelope) {
                            continue;
                        }
                        let _ = dest.inject_test_envelope(envelope.clone()).await;
                    }
                }
            }));
        }

        Ok(Self {
            services,
            _tasks: tasks,
        })
    }

    pub fn peer(&self, index: usize) -> Arc<NetworkService> {
        Arc::clone(&self.services[index])
    }
}

/// Wait until `receiver` sees `message_type` or timeout.
pub async fn wait_for_message_type(
    rx: &mut mpsc::UnboundedReceiver<NetworkEvent>,
    message_type: MessageType,
    timeout_ms: u64,
) -> bool {
    let deadline = catalyst_utils::utils::current_timestamp_ms().saturating_add(timeout_ms);
    while catalyst_utils::utils::current_timestamp_ms() < deadline {
        let remaining = deadline.saturating_sub(catalyst_utils::utils::current_timestamp_ms());
        let wait = Duration::from_millis(remaining.min(50));
        if let Ok(Some(ev)) = tokio::time::timeout(wait, rx.recv()).await {
            if let NetworkEvent::MessageReceived { envelope, .. } = ev {
                if envelope.message_type == message_type {
                    return true;
                }
            }
        }
    }
    false
}
