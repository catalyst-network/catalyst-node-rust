//! In-process multi-node consensus simulation (deterministic txs + delayed fanout).
//!
//! Used by `catalyst-consensus` integration tests and `catalyst-cli` storage apply checks.
//! This is **not** a wire protocol or public node API.

use std::time::Duration;

use blake2::{Blake2b512, Digest};
use tokio::sync::{mpsc, Mutex};
use tokio::time::sleep;

use catalyst_utils::{MessageEnvelope, MessageType, PublicKey};

use crate::producer::{Producer, ProducerManager};
use crate::types::{
    hash_data, LedgerStateUpdate, ProducerCandidate, ProducerOutput, ProducerQuantity, ProducerVote,
};
use crate::{CollaborativeConsensus, ConsensusConfig, ProducerId, TransactionEntry};

/// Maximum per-hop delay applied to each (message, dest) delivery.
#[derive(Debug, Clone, Copy)]
pub struct SimNetConfig {
    pub max_delay_ms: u64,
    /// If set, extra delay added only for deliveries to this destination index (slow / straggler link).
    pub slow_dest: Option<usize>,
    pub slow_extra_ms: u64,
    /// Test hook: do not deliver copies of `skip_message_type` to destination index `skip_dest_idx`
    /// (simulates selective gossip loss, e.g. tx batches to one follower).
    pub skip_dest_message: Option<(usize, MessageType)>,
}

impl Default for SimNetConfig {
    fn default() -> Self {
        Self {
            max_delay_ms: 0,
            slow_dest: None,
            slow_extra_ms: 0,
            skip_dest_message: None,
        }
    }
}

fn deterministic_delay_ms(env: &MessageEnvelope, dest_idx: usize, cfg: SimNetConfig) -> u64 {
    let (cycle, producer_id) = match env.message_type {
        MessageType::ProducerQuantity => env
            .extract_message::<ProducerQuantity>()
            .map(|m| (m.cycle_number, m.producer_id))
            .unwrap_or((0, String::new())),
        MessageType::ProducerCandidate => env
            .extract_message::<ProducerCandidate>()
            .map(|m| (m.cycle_number, m.producer_id))
            .unwrap_or((0, String::new())),
        MessageType::ProducerVote => env
            .extract_message::<ProducerVote>()
            .map(|m| (m.cycle_number, m.producer_id))
            .unwrap_or((0, String::new())),
        MessageType::ProducerOutput => env
            .extract_message::<ProducerOutput>()
            .map(|m| (m.cycle_number, m.producer_id))
            .unwrap_or((0, String::new())),
        _ => (0, String::new()),
    };

    let base = if cfg.max_delay_ms == 0 {
        0u64
    } else {
        let mut hasher = Blake2b512::new();
        hasher.update(env.message_type.to_string().as_bytes());
        hasher.update(&cycle.to_be_bytes());
        hasher.update(producer_id.as_bytes());
        hasher.update(&(dest_idx as u64).to_be_bytes());
        let out = hasher.finalize();
        let mut b = [0u8; 8];
        b.copy_from_slice(&out[..8]);
        let v = u64::from_be_bytes(b);
        v % (cfg.max_delay_ms + 1)
    };

    let extra = if cfg.slow_dest == Some(dest_idx) {
        cfg.slow_extra_ms
    } else {
        0
    };

    base.saturating_add(extra)
}

/// Whether `env` should be delivered to simulated destination `dest_idx` under `cfg`.
pub fn sim_net_should_deliver(cfg: &SimNetConfig, dest_idx: usize, env: &MessageEnvelope) -> bool {
    !matches!(
        cfg.skip_dest_message,
        Some((skip_dest, skip_mt)) if skip_dest == dest_idx && skip_mt == env.message_type
    )
}

/// Keeps forwarding tasks alive for the lifetime of the simulated network.
pub struct SimNet {
    _forwarders: Vec<tokio::task::JoinHandle<()>>,
}

impl SimNet {
    pub fn wire_nodes(nodes: &mut [CollaborativeConsensus], cfg: SimNetConfig) -> Self {
        let mut inbound_senders: Vec<mpsc::UnboundedSender<MessageEnvelope>> = Vec::new();
        let mut outbound_receivers: Vec<mpsc::UnboundedReceiver<MessageEnvelope>> = Vec::new();

        for node in nodes.iter_mut() {
            let (out_tx, out_rx) = mpsc::unbounded_channel::<MessageEnvelope>();
            node.set_network_sender(out_tx);
            outbound_receivers.push(out_rx);

            let (in_tx, in_rx) = mpsc::unbounded_channel::<MessageEnvelope>();
            node.set_network_receiver(in_rx);
            inbound_senders.push(in_tx);
        }

        let forwarders = outbound_receivers
            .into_iter()
            .map(|mut out_rx| {
                let inbound = inbound_senders.clone();
                tokio::spawn(async move {
                    while let Some(env) = out_rx.recv().await {
                        for (dest_idx, tx) in inbound.iter().enumerate() {
                            let env_clone = env.clone();
                            if !sim_net_should_deliver(&cfg, dest_idx, &env_clone) {
                                continue;
                            }
                            let tx_clone = tx.clone();
                            let delay_ms = deterministic_delay_ms(&env_clone, dest_idx, cfg);
                            tokio::spawn(async move {
                                if delay_ms > 0 {
                                    sleep(Duration::from_millis(delay_ms)).await;
                                }
                                let _ = tx_clone.send(env_clone);
                            });
                        }
                    }
                })
            })
            .collect();

        Self {
            _forwarders: forwarders,
        }
    }
}

pub fn test_config(n: usize) -> ConsensusConfig {
    let phase_ms = 180;
    ConsensusConfig {
        cycle_duration_ms: (phase_ms * 4) as u64,
        construction_phase_ms: phase_ms,
        campaigning_phase_ms: phase_ms,
        voting_phase_ms: phase_ms,
        synchronization_phase_ms: phase_ms,
        freeze_window_ms: 0,
        min_producers: n,
        confidence_threshold: 0.67,
    }
}

pub fn make_transactions(cycle: u64) -> Vec<TransactionEntry> {
    (0..5)
        .map(|i| {
            let pk: PublicKey = {
                let mut b = [0u8; 32];
                b[0] = (cycle as u8).wrapping_add(i as u8);
                b[31] = (i as u8).wrapping_mul(7);
                b
            };
            let mut sig = vec![0u8; 64];
            sig[0] = (cycle as u8).wrapping_mul(3).wrapping_add(i as u8);
            sig[1] = (i as u8).wrapping_add(11);
            sig[63] = (cycle as u8) ^ (i as u8);
            TransactionEntry {
                public_key: pk,
                amount: (i as i64) - 2,
                signature: sig,
            }
        })
        .collect()
}

fn build_nodes(n: usize, config: ConsensusConfig) -> (Vec<ProducerId>, Vec<CollaborativeConsensus>) {
    let producer_ids: Vec<ProducerId> = (0..n).map(|i| format!("producer_{i}")).collect();

    let mut raw_nodes: Vec<CollaborativeConsensus> = Vec::new();
    for (i, id) in producer_ids.iter().enumerate() {
        let mut node = CollaborativeConsensus::new(config.clone());
        let producer = Producer::new(id.clone(), [i as u8; 32], 0);
        let manager = ProducerManager::new(producer, config.clone());
        node.set_producer_manager(manager);
        raw_nodes.push(node);
    }
    (producer_ids, raw_nodes)
}

/// Run `cycles` wall-clock rounds; each round asserts identical LSU hashes across all `n` nodes.
pub async fn run_convergence(n: usize, cycles: u64, net_cfg: SimNetConfig) {
    run_convergence_with_txs(n, cycles, net_cfg, |_, cycle| make_transactions(cycle)).await;
}

/// Like [`run_convergence`], but each node may receive a different construction tx list.
///
/// Used to show that heterogeneous producer inputs **diverge** LSU hashes unless the
/// node-layer tx-batch coordinator supplies a canonical set (checklist §1 / §7.2).
pub async fn run_convergence_with_txs<F>(n: usize, cycles: u64, net_cfg: SimNetConfig, tx_for_node: F)
where
    F: Fn(usize, u64) -> Vec<TransactionEntry> + Send + Sync,
{
    let hashes = run_convergence_collect_hashes(n, cycles, net_cfg, tx_for_node).await;
    for (cycle, round) in hashes.iter().enumerate() {
        let cycle = (cycle as u64) + 1;
        let first = round[0];
        assert!(
            round.iter().all(|h| *h == first),
            "cycle {cycle}: nodes diverged: {:?}",
            round
        );
    }
}

/// Per-cycle LSU hashes for each node (one inner vec per cycle, length `n`).
pub async fn run_convergence_collect_hashes<F>(
    n: usize,
    cycles: u64,
    net_cfg: SimNetConfig,
    tx_for_node: F,
) -> Vec<Vec<[u8; 32]>>
where
    F: Fn(usize, u64) -> Vec<TransactionEntry> + Send + Sync,
{
    let config = test_config(n);
    let (producer_ids, mut raw_nodes) = build_nodes(n, config);
    let _net = SimNet::wire_nodes(&mut raw_nodes, net_cfg);

    let nodes: Vec<_> = raw_nodes
        .into_iter()
        .map(|n| std::sync::Arc::new(Mutex::new(n)))
        .collect();

    let mut all_cycles = Vec::new();
    for cycle in 1..=cycles {
        let selected = producer_ids.clone();

        let mut joins = Vec::new();
        for (node_idx, node) in nodes.iter().cloned().enumerate() {
            let selected = selected.clone();
            let txs = tx_for_node(node_idx, cycle);
            joins.push(tokio::spawn(async move {
                node.lock().await.start_cycle(cycle, selected, txs).await
            }));
        }

        let mut hashes = Vec::new();
        for j in joins {
            let res = j.await.expect("task join failed").expect("cycle failed");
            let lsu = res.expect("producer did not generate LSU");
            let h = hash_data(&lsu).expect("hash_data failed");
            hashes.push(h);
        }
        all_cycles.push(hashes);
    }
    all_cycles
}

/// One convergence round; returns a representative LSU (all nodes agree on the same hash).
pub async fn produce_single_cycle_converged_lsu(
    n: usize,
    cycle: u64,
    net_cfg: SimNetConfig,
) -> LedgerStateUpdate {
    let config = test_config(n);
    let (producer_ids, mut raw_nodes) = build_nodes(n, config);
    let _net = SimNet::wire_nodes(&mut raw_nodes, net_cfg);

    let nodes: Vec<_> = raw_nodes
        .into_iter()
        .map(|n| std::sync::Arc::new(Mutex::new(n)))
        .collect();

    let selected = producer_ids.clone();
    let txs = make_transactions(cycle);

    let mut joins = Vec::new();
    for node in nodes.iter().cloned() {
        let selected = selected.clone();
        let txs = txs.clone();
        joins.push(tokio::spawn(async move {
            node.lock().await.start_cycle(cycle, selected, txs).await
        }));
    }

    let mut lsus: Vec<LedgerStateUpdate> = Vec::new();
    for j in joins {
        let res = j.await.expect("task join failed").expect("cycle failed");
        lsus.push(res.expect("producer did not generate LSU"));
    }

    let h0 = hash_data(&lsus[0]).expect("hash_data failed");
    for (i, lsu) in lsus.iter().enumerate().skip(1) {
        let hi = hash_data(lsu).expect("hash_data failed");
        assert_eq!(h0, hi, "node {i} diverged from node 0");
    }
    lsus.into_iter().next().expect("lsus non-empty")
}

#[cfg(test)]
mod sim_net_should_deliver_tests {
    use catalyst_utils::{MessageEnvelope, MessageType};

    use super::{sim_net_should_deliver, SimNetConfig};

    fn env(mt: MessageType) -> MessageEnvelope {
        MessageEnvelope::new(mt, "s".into(), None, vec![])
    }

    #[test]
    fn delivers_when_no_skip_configured() {
        let cfg = SimNetConfig::default();
        assert!(sim_net_should_deliver(&cfg, 1, &env(MessageType::TransactionBatch)));
    }

    #[test]
    fn skips_only_matching_destination_and_message_type() {
        let cfg = SimNetConfig {
            skip_dest_message: Some((2, MessageType::TransactionBatch)),
            ..SimNetConfig::default()
        };
        assert!(!sim_net_should_deliver(&cfg, 2, &env(MessageType::TransactionBatch)));
        assert!(sim_net_should_deliver(&cfg, 1, &env(MessageType::TransactionBatch)));
        assert!(sim_net_should_deliver(&cfg, 2, &env(MessageType::ProducerVote)));
    }
}
