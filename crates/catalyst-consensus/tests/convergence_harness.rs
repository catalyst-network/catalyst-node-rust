use std::time::Duration;

use blake2::{Blake2b512, Digest};
use catalyst_consensus::producer::{Producer, ProducerManager};
use catalyst_consensus::types::{hash_data, ProducerCandidate, ProducerOutput, ProducerQuantity, ProducerVote};
use catalyst_consensus::{CollaborativeConsensus, ConsensusConfig, ProducerId, TransactionEntry};
use catalyst_utils::{MessageEnvelope, MessageType, PublicKey};
use tokio::sync::{mpsc, Mutex};
use tokio::time::sleep;

#[derive(Debug, Clone, Copy)]
struct SimNetConfig {
    /// Maximum per-hop delay applied to each (message, dest) delivery.
    /// This naturally injects reordering across message types and producers.
    max_delay_ms: u64,
}

fn deterministic_delay_ms(
    env: &MessageEnvelope,
    dest_idx: usize,
    cfg: SimNetConfig,
) -> u64 {
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

    if cfg.max_delay_ms == 0 {
        return 0;
    }

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
}

struct SimNet {
    _forwarders: Vec<tokio::task::JoinHandle<()>>,
}

impl SimNet {
    fn wire_nodes(
        nodes: &mut [CollaborativeConsensus],
        cfg: SimNetConfig,
    ) -> Self {
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
                        // Broadcast fanout (including loopback). Dedupe-by-producer-id in the
                        // consensus engine ensures duplicates don't change outcomes.
                        for (dest_idx, tx) in inbound.iter().enumerate() {
                            let env_clone = env.clone();
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

fn test_config(n: usize) -> ConsensusConfig {
    // Keep this tight so `cargo test -p catalyst-consensus` stays fast, but with
    // enough slack for the simulated WAN delays and the buffering logic.
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

fn make_transactions(cycle: u64) -> Vec<TransactionEntry> {
    // Deterministic “pseudo tx” set. Construction sorts deterministically so all nodes
    // should agree even if delivery ordering differs.
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

async fn run_convergence(n: usize, cycles: u64, net_cfg: SimNetConfig) {
    let config = test_config(n);

    let producer_ids: Vec<ProducerId> = (0..n).map(|i| format!("producer_{i}")).collect();

    let mut raw_nodes: Vec<CollaborativeConsensus> = Vec::new();
    for (i, id) in producer_ids.iter().enumerate() {
        let mut node = CollaborativeConsensus::new(config.clone());
        let producer = Producer::new(id.clone(), [i as u8; 32], 0);
        let manager = ProducerManager::new(producer, config.clone());
        node.set_producer_manager(manager);
        raw_nodes.push(node);
    }

    let _net = SimNet::wire_nodes(&mut raw_nodes, net_cfg);

    let nodes: Vec<_> = raw_nodes
        .into_iter()
        .map(|n| std::sync::Arc::new(Mutex::new(n)))
        .collect();

    for cycle in 1..=cycles {
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

        let mut hashes = Vec::new();
        for j in joins {
            let res = j.await.expect("task join failed").expect("cycle failed");
            let lsu = res.expect("producer did not generate LSU");
            let h = hash_data(&lsu).expect("hash_data failed");
            hashes.push(h);
        }

        let first = hashes[0];
        assert!(
            hashes.iter().all(|h| *h == first),
            "cycle {cycle}: nodes diverged: {:?}",
            hashes
        );
    }
}

#[tokio::test]
async fn multi_node_convergence_no_delay() {
    run_convergence(3, 5, SimNetConfig { max_delay_ms: 0 }).await;
}

#[tokio::test]
async fn multi_node_convergence_with_delay_and_reordering() {
    // Delay >0 injects natural reordering across producers + phases.
    run_convergence(5, 5, SimNetConfig { max_delay_ms: 35 }).await;
}

