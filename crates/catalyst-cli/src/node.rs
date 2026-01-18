use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use catalyst_core::{NodeId, NodeRole, ResourceProof, WorkerPass};
use catalyst_consensus::{CollaborativeConsensus, ConsensusConfig as ConsensusEngineConfig};
use catalyst_consensus::producer::{Producer, ProducerManager};

use crate::config::NodeConfig;

/// Main Catalyst node implementation.
///
/// Note: This crate currently provides a minimal, compile-safe node wrapper.
/// The full networking/storage/consensus runtime wiring is still under active
/// development across the workspace crates.
pub struct CatalystNode {
    /// Node configuration
    config: NodeConfig,

    /// Unique node identifier
    node_id: NodeId,

    /// Current node role
    role: Arc<RwLock<NodeRole>>,

    /// Background consensus loop handle
    consensus_task: Option<tokio::task::JoinHandle<()>>,

    /// Signal to stop background tasks
    shutdown_tx: Option<tokio::sync::watch::Sender<bool>>,
}

impl CatalystNode {
    /// Create a new Catalyst node.
    pub async fn new(config: NodeConfig) -> Result<Self> {
        info!("Initializing Catalyst node");

        // Generate a node ID (in a full implementation this should be persistent).
        let node_id: NodeId = generate_node_id();
        info!("Node ID: {}", hex_encode(&node_id));

        // Determine initial role.
        let role = if config.validator {
            NodeRole::Worker {
                worker_pass: WorkerPass {
                    node_id,
                    issued_at: current_timestamp(),
                    expires_at: current_timestamp() + 86400,
                    partition_id: None,
                },
                resource_proof: ResourceProof {
                    cpu_score: 1000,
                    memory_mb: 4096,
                    storage_gb: 100,
                    bandwidth_mbps: 100,
                    timestamp: current_timestamp(),
                    signature: vec![],
                },
            }
        } else {
            NodeRole::User
        };

        Ok(Self {
            config,
            node_id,
            role: Arc::new(RwLock::new(role)),
            consensus_task: None,
            shutdown_tx: None,
        })
    }

    /// Start the node.
    pub async fn start(&mut self) -> Result<()> {
        info!("Starting Catalyst node");

        // Ensure local directories exist (data dir, dfs cache, logs dir, etc.)
        self.config.ensure_data_dir()?;

        // For now, run a minimal single-node consensus loop. This makes `catalyst-cli start`
        // actually do useful work while network/RPC/mempool wiring is still in progress.
        //
        // Next milestones:
        // - Replace the empty tx list with a mempool feed.
        // - Use real producer selection (worker pool + seed).
        // - Broadcast/collect consensus messages over the network layer.

        let producer_id = self.config.node.name.clone();
        let public_key = self.node_id;

        let engine_config = ConsensusEngineConfig {
            cycle_duration_ms: (self.config.consensus.cycle_duration_seconds as u64) * 1000,
            construction_phase_ms: (self.config.consensus.phase_timeouts.construction_timeout as u64) * 1000,
            campaigning_phase_ms: (self.config.consensus.phase_timeouts.campaigning_timeout as u64) * 1000,
            voting_phase_ms: (self.config.consensus.phase_timeouts.voting_timeout as u64) * 1000,
            synchronization_phase_ms: (self.config.consensus.phase_timeouts.synchronization_timeout as u64) * 1000,
            freeze_window_ms: (self.config.consensus.freeze_time_seconds as u64) * 1000,
            // Single-node runnable defaults; multi-producer will be wired once networking collection exists.
            min_producers: 1,
            confidence_threshold: 0.6,
        };

        let mut consensus = CollaborativeConsensus::new(engine_config.clone());
        let producer = Producer::new(producer_id.clone(), public_key, 0);
        let manager = ProducerManager::new(producer, engine_config);
        consensus.set_producer_manager(manager);

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
        self.shutdown_tx = Some(shutdown_tx);

        let cycle_ms = self.config.consensus.cycle_duration_seconds as u64 * 1000;
        info!(
            "Consensus loop enabled (single-producer). cycle={}ms producer_id={}",
            cycle_ms, producer_id
        );

        self.consensus_task = Some(tokio::spawn(async move {
            let mut cycle: u64 = 1;
            loop {
                if *shutdown_rx.borrow() {
                    break;
                }

                let selected = vec![producer_id.clone()];
                let transactions = Vec::new();

                match consensus.start_cycle(cycle, selected, transactions).await {
                    Ok(Some(update)) => {
                        info!(
                            "Cycle {} complete: LSU producers_ok={} voters_ok={} tx_entries={}",
                            cycle,
                            update.producer_list.len(),
                            update.vote_list.len(),
                            update.partial_update.transaction_entries.len()
                        );
                    }
                    Ok(None) => {
                        info!("Cycle {} complete: no LSU produced", cycle);
                    }
                    Err(e) => {
                        info!("Cycle {} failed: {}", cycle, e);
                    }
                }

                cycle += 1;

                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_millis(cycle_ms)) => {}
                    _ = shutdown_rx.changed() => {}
                }
            }
        }));

        Ok(())
    }

    /// Stop the node gracefully.
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping Catalyst node");

        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }

        if let Some(handle) = self.consensus_task.take() {
            handle.abort();
        }

        Ok(())
    }
}

fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn generate_node_id() -> NodeId {
    // A lightweight, non-cryptographic node id generator to keep the CLI buildable
    // without pulling in extra deps (important for GVFS/SMB mounts where Cargo.lock
    // updates may fail).
    let mut seed = current_timestamp() as u64;
    seed ^= std::process::id() as u64;
    seed ^= (seed << 13) ^ (seed >> 7) ^ (seed << 17);

    let mut out = [0u8; 32];
    let mut x = seed;
    for chunk in out.chunks_mut(8) {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        chunk.copy_from_slice(&x.to_le_bytes());
    }
    out
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

