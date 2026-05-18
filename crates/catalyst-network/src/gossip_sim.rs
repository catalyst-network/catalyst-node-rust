//! In-process multi-node gossip mesh for integration tests (selective per-dest drops, delay).
//!
//! Mirrors the fanout model of libp2p gossipsub at the **envelope** layer: each node
//! `publish`es to a shared forwarder that delivers copies to peer inboxes. Used for checklist
//! §7.2 (`ProtocolTxBatch` / `TransactionBatch` loss) without standing up a full swarm.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use catalyst_utils::network::MessageEnvelope;
use catalyst_utils::MessageType;
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::error::NetworkResult;

/// Per-hop delivery tuning (same semantics as `catalyst_consensus::convergence_test_support::SimNetConfig`).
#[derive(Debug, Clone, Copy, Default)]
pub struct GossipSimConfig {
    pub max_delay_ms: u64,
    pub slow_dest: Option<usize>,
    pub slow_extra_ms: u64,
    /// Do not deliver copies of `skip_message_type` to destination index `skip_dest_idx`.
    pub skip_dest_message: Option<(usize, MessageType)>,
}

fn deterministic_delay_ms(env: &MessageEnvelope, dest_idx: usize, cfg: GossipSimConfig) -> u64 {
    let base = if cfg.max_delay_ms == 0 {
        0u64
    } else {
        let mut hasher = DefaultHasher::new();
        env.message_type.to_string().hash(&mut hasher);
        dest_idx.hash(&mut hasher);
        hasher.finish() % (cfg.max_delay_ms + 1)
    };
    base.saturating_add(if cfg.slow_dest == Some(dest_idx) {
        cfg.slow_extra_ms
    } else {
        0
    })
}

/// Whether `env` should be delivered to simulated destination `dest_idx` under `cfg`.
pub fn gossip_sim_should_deliver(cfg: &GossipSimConfig, dest_idx: usize, env: &MessageEnvelope) -> bool {
    !matches!(
        cfg.skip_dest_message,
        Some((skip_dest, skip_mt)) if skip_dest == dest_idx && skip_mt == env.message_type
    )
}

#[derive(Debug, Clone)]
pub enum GossipSimEvent {
    MessageReceived {
        envelope: MessageEnvelope,
        from_index: usize,
    },
}

/// One simulated validator's gossip endpoint.
pub struct GossipSimPeer {
    index: usize,
    mesh_tx: mpsc::UnboundedSender<(usize, MessageEnvelope)>,
    event_rx: MutexOption<mpsc::UnboundedReceiver<GossipSimEvent>>,
}

struct MutexOption<T>(std::sync::Mutex<Option<T>>);

impl<T> MutexOption<T> {
    fn take(&self) -> Option<T> {
        self.0.lock().ok().and_then(|mut g| g.take())
    }
}

impl GossipSimPeer {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn take_events(&self) -> Option<mpsc::UnboundedReceiver<GossipSimEvent>> {
        self.event_rx.take()
    }

    /// Fan out `envelope` to all peers (respecting [`GossipSimConfig`] on the mesh).
    pub async fn publish(&self, envelope: &MessageEnvelope) -> NetworkResult<()> {
        let _ = self
            .mesh_tx
            .send((self.index, envelope.clone()))
            .map_err(|_| crate::error::NetworkError::TransportError("gossip sim mesh closed".into()));
        Ok(())
    }
}

/// Keeps forwarder tasks alive for the lifetime of the mesh.
pub struct GossipSimMesh {
    _forwarders: Vec<tokio::task::JoinHandle<()>>,
    peers: Vec<GossipSimPeer>,
}

impl GossipSimMesh {
    /// Wire `n` peers with shared fanout and return handles (index `0..n`).
    pub fn new(n: usize, cfg: GossipSimConfig) -> Self {
        let mut inboxes: Vec<mpsc::UnboundedSender<GossipSimEvent>> = Vec::with_capacity(n);
        let mut event_receivers: Vec<mpsc::UnboundedReceiver<GossipSimEvent>> = Vec::with_capacity(n);

        for _ in 0..n {
            let (tx, rx) = mpsc::unbounded_channel();
            inboxes.push(tx);
            event_receivers.push(rx);
        }

        let (mesh_tx, mut mesh_rx) = mpsc::unbounded_channel::<(usize, MessageEnvelope)>();

        let forwarder = tokio::spawn(async move {
            while let Some((src, env)) = mesh_rx.recv().await {
                for (dest_idx, inbox) in inboxes.iter().enumerate() {
                    if dest_idx == src {
                        continue;
                    }
                    if !gossip_sim_should_deliver(&cfg, dest_idx, &env) {
                        continue;
                    }
                    let env_clone = env.clone();
                    let inbox = inbox.clone();
                    let delay_ms = deterministic_delay_ms(&env_clone, dest_idx, cfg);
                    tokio::spawn(async move {
                        if delay_ms > 0 {
                            sleep(Duration::from_millis(delay_ms)).await;
                        }
                        let _ = inbox.send(GossipSimEvent::MessageReceived {
                            envelope: env_clone,
                            from_index: src,
                        });
                    });
                }
            }
        });

        let peers: Vec<GossipSimPeer> = event_receivers
            .into_iter()
            .enumerate()
            .map(|(index, rx)| GossipSimPeer {
                index,
                mesh_tx: mesh_tx.clone(),
                event_rx: MutexOption(std::sync::Mutex::new(Some(rx))),
            })
            .collect();

        Self {
            _forwarders: vec![forwarder],
            peers,
        }
    }

    pub fn peers(&self) -> &[GossipSimPeer] {
        &self.peers
    }

    pub fn peer(&self, index: usize) -> &GossipSimPeer {
        &self.peers[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyst_utils::MessageEnvelope;

    fn env(mt: MessageType) -> MessageEnvelope {
        MessageEnvelope::new(mt, "s".into(), None, vec![])
    }

    #[tokio::test]
    async fn drops_transaction_batch_only_for_configured_dest() {
        let mesh = GossipSimMesh::new(
            3,
            GossipSimConfig {
                skip_dest_message: Some((2, MessageType::TransactionBatch)),
                ..GossipSimConfig::default()
            },
        );
        let mut r0 = mesh.peer(0).take_events().unwrap();
        let mut r1 = mesh.peer(1).take_events().unwrap();
        let mut r2 = mesh.peer(2).take_events().unwrap();

        mesh.peer(0)
            .publish(&env(MessageType::TransactionBatch))
            .await
            .unwrap();
        mesh.peer(0)
            .publish(&env(MessageType::TransactionRequest))
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        assert!(r1.try_recv().is_ok());
        assert!(r2.try_recv().is_err());
        let got_commit = r2.try_recv().is_ok();
        assert!(got_commit, "other message types must still be delivered");
        let _ = r0.try_recv();
    }
}
