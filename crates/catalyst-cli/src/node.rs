use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use catalyst_core::{NodeId, NodeRole, ResourceProof, WorkerPass};
use catalyst_core::protocol::select_producers_for_next_cycle;
use catalyst_consensus::{CollaborativeConsensus, ConsensusConfig as ConsensusEngineConfig};
use catalyst_consensus::producer::{Producer, ProducerManager};
use catalyst_network::{NetworkConfig as P2pConfig, NetworkService as P2pService, Multiaddr};
use catalyst_utils::{
    CatalystDeserialize, CatalystSerialize, MessageType, NetworkMessage, MessageEnvelope, impl_catalyst_serialize,
    utils::current_timestamp_ms,
};
use catalyst_consensus::types::hash_data;

use crate::config::NodeConfig;
use crate::tx::{Mempool, ProtocolTxGossip, TxBatch, TxGossip};
use crate::sync::{LsuCidGossip, LsuGossip};

use catalyst_dfs::ContentId;

use crate::dfs_store::LocalContentStore;

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

    /// Background network task handles
    network_tasks: Vec<tokio::task::JoinHandle<()>>,

    /// Dev/testnet helper: generate and gossip dummy transactions periodically
    generate_txs: bool,
    tx_interval_ms: u64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct NodeStatusMsg {
    producer_id: String,
    node_id: NodeId,
    /// 1 = validator, 0 = non-validator (keeps serialization simple)
    is_validator: u8,
    timestamp: u64,
}

impl_catalyst_serialize!(NodeStatusMsg, producer_id, node_id, is_validator, timestamp);

impl NetworkMessage for NodeStatusMsg {
    fn serialize(&self) -> catalyst_utils::CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(self)
    }

    fn deserialize(data: &[u8]) -> catalyst_utils::CatalystResult<Self> {
        CatalystDeserialize::deserialize(data)
    }

    fn message_type(&self) -> MessageType {
        MessageType::NodeStatus
    }
}

impl CatalystNode {
    /// Create a new Catalyst node.
    pub async fn new(config: NodeConfig, generate_txs: bool, tx_interval_ms: u64) -> Result<Self> {
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
            network_tasks: Vec::new(),
            generate_txs,
            tx_interval_ms: tx_interval_ms.max(50),
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

        // --- Networking (simple TCP transport) ---
        let mut net_cfg = P2pConfig::default();
        net_cfg.peer.listen_addresses = self
            .config
            .network
            .listen_addresses
            .iter()
            // Our simple TCP transport binds sockets directly; binding both 0.0.0.0:port and [::]:port
            // commonly conflicts on Linux (dual-stack). Prefer IPv4 for local testnets.
            .filter(|s| s.starts_with("/ip4/"))
            .filter_map(|s| s.parse::<Multiaddr>().ok())
            .collect();

        // Put keypair in node dir (even if unused by simple transport).
        net_cfg.peer.keypair_path = Some(self.config.storage.data_dir.join("p2p_keypair"));
        net_cfg.peer.bootstrap_peers = Vec::new();

        let network = Arc::new(P2pService::new(net_cfg).await?);
        network.start().await?;

        // Dial bootstrap peers from CLI config
        for peer in &self.config.network.bootstrap_peers {
            if let Ok(ma) = peer.parse::<Multiaddr>() {
                let _ = network.connect_multiaddr(&ma).await;
            }
        }

        // Inbound/outbound envelope channels used by consensus engine.
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<MessageEnvelope>();
        let (in_tx, in_rx) = tokio::sync::mpsc::unbounded_channel::<MessageEnvelope>();
        consensus.set_network_sender(out_tx);
        consensus.set_network_receiver(in_rx);

        // Known validator worker pool (for producer selection).
        #[derive(Clone)]
        struct ValidatorInfo {
            producer_id: String,
            is_validator: bool,
        }
        let known_validators: Arc<tokio::sync::RwLock<std::collections::HashMap<NodeId, ValidatorInfo>>> =
            Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));

        // Transaction mempool (gossiped `TxGossip` messages).
        let mempool: Arc<tokio::sync::RwLock<Mempool>> = Arc::new(tokio::sync::RwLock::new(
            Mempool::new(std::time::Duration::from_secs(60), 2048),
        ));

        // DFS store (shared filesystem CAS). Works across local testnet processes.
        let dfs = if self.config.dfs.enabled {
            Some(LocalContentStore::new(self.config.dfs.cache_dir.clone()))
        } else {
            None
        };

        // Cycle-scoped tx batches (selected by a deterministic "leader" each cycle).
        let tx_batches: Arc<tokio::sync::RwLock<std::collections::HashMap<u64, Vec<catalyst_consensus::types::TransactionEntry>>>> =
            Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));

        // Outbound: envelopes produced by consensus → broadcast to peers.
        {
            let net = network.clone();
            let handle = tokio::spawn(async move {
                while let Some(env) = out_rx.recv().await {
                    let _ = net.broadcast_envelope(&env).await;
                }
            });
            self.network_tasks.push(handle);
        }

        // Periodically broadcast our node status so other nodes can discover producers.
        {
            let net = network.clone();
            let my_id = producer_id.clone();
            let my_node_id = public_key;
            let is_validator = self.config.validator;
            let mut shutdown_rx2 = shutdown_rx.clone();
            let handle = tokio::spawn(async move {
                loop {
                    if *shutdown_rx2.borrow() {
                        break;
                    }

                    let msg = NodeStatusMsg {
                        producer_id: my_id.clone(),
                        node_id: my_node_id,
                is_validator: if is_validator { 1 } else { 0 },
                        timestamp: current_timestamp_ms(),
                    };
                    if let Ok(env) = MessageEnvelope::from_message(&msg, "node".to_string(), None) {
                        let _ = net.broadcast_envelope(&env).await;
                    }

                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {}
                        _ = shutdown_rx2.changed() => {}
                    }
                }
            });
            self.network_tasks.push(handle);
        }

        // Inbound: envelopes received from peers → update validator map / handle tx gossip / handle LSU gossip / forward consensus messages.
        {
            let mut events = network.subscribe_events().await;
            let validators = known_validators.clone();
            let mempool = mempool.clone();
            let tx_batches = tx_batches.clone();
            let dfs = dfs.clone();
            let last_lsu: Arc<tokio::sync::RwLock<std::collections::HashMap<u64, catalyst_consensus::types::LedgerStateUpdate>>> =
                Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));
            let handle = tokio::spawn(async move {
                while let Some(ev) = events.recv().await {
                    if let catalyst_network::NetworkEvent::MessageReceived { envelope, .. } = ev {
                        if envelope.message_type == MessageType::NodeStatus {
                            if let Ok(ns) = envelope.extract_message::<NodeStatusMsg>() {
                                let mut m = validators.write().await;
                                m.insert(
                                    ns.node_id,
                                    ValidatorInfo {
                                        producer_id: ns.producer_id,
                                        is_validator: ns.is_validator != 0,
                                    },
                                );
                            }
                        } else if envelope.message_type == MessageType::Transaction {
                            // Prefer protocol-shaped txs; fall back to legacy TxGossip.
                            let now_secs = current_timestamp_ms() / 1000;
                            if let Ok(tx) = envelope.extract_message::<ProtocolTxGossip>() {
                                let mut mp = mempool.write().await;
                                let _ = mp.insert_protocol(tx, now_secs);
                            } else if let Ok(tx) = envelope.extract_message::<TxGossip>() {
                                let mut mp = mempool.write().await;
                                let _ = mp.insert(tx);
                            }
                        } else if envelope.message_type == MessageType::TransactionBatch {
                            if let Ok(batch) = envelope.extract_message::<TxBatch>() {
                                // Verify hash before accepting.
                                if let Ok(h) = catalyst_consensus::types::hash_data(&batch.entries) {
                                    if h == batch.batch_hash {
                                        tx_batches.write().await.insert(batch.cycle, batch.entries);
                                    }
                                }
                            }
                        } else if envelope.message_type == MessageType::ConsensusSync {
                            // Prefer CID-based LSU sync; fallback to full LSU gossip.
                            if let Ok(ref_msg) = envelope.extract_message::<LsuCidGossip>() {
                                if let Some(dfs) = &dfs {
                                    if let Ok(_cid) = ContentId::from_string(&ref_msg.cid) {
                                        if let Ok(bytes) = dfs.get(&ref_msg.cid).await {
                                            if let Ok(lsu) =
                                                catalyst_consensus::types::LedgerStateUpdate::deserialize(&bytes)
                                            {
                                                if let Ok(h) = catalyst_consensus::types::hash_data(&lsu) {
                                                    if h == ref_msg.lsu_hash {
                                                        last_lsu.write().await.insert(ref_msg.cycle, lsu);
                                                        info!(
                                                            "Synced LSU via CID cycle={} cid={}",
                                                            ref_msg.cycle, ref_msg.cid
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            } else if let Ok(lsu_msg) = envelope.extract_message::<LsuGossip>() {
                                if let Ok(h) = catalyst_consensus::types::hash_data(&lsu_msg.lsu) {
                                    if h == lsu_msg.lsu_hash {
                                        last_lsu.write().await.insert(lsu_msg.cycle, lsu_msg.lsu);
                                    }
                                }
                            }
                        } else {
                            let _ = in_tx.send(envelope);
                        }
                    }
                }
            });
            self.network_tasks.push(handle);
        }

        // Outbound: optional dummy tx generator (dev/testnet helper).
        if self.generate_txs {
            let net = network.clone();
            let my_node_id = public_key;
            let interval_ms = self.tx_interval_ms;
            let mempool = mempool.clone();
            let mut shutdown_rx2 = shutdown_rx.clone();
            let handle = tokio::spawn(async move {
                let mut counter: u64 = 0;
                loop {
                    if *shutdown_rx2.borrow() {
                        break;
                    }
                    let now = current_timestamp_ms();
                    counter = counter.wrapping_add(1);

                    // Protocol-shaped 2-entry transfer (lock_time = now_secs, immediate).
                    let now_secs = now / 1000;
                    let mut recv_pk = [0u8; 32];
                    recv_pk[0..8].copy_from_slice(&counter.to_le_bytes());

                    let tx = catalyst_core::protocol::Transaction {
                        core: catalyst_core::protocol::TransactionCore {
                            tx_type: catalyst_core::protocol::TransactionType::NonConfidentialTransfer,
                            entries: vec![
                                catalyst_core::protocol::TransactionEntry {
                                    public_key: my_node_id,
                                    amount: catalyst_core::protocol::EntryAmount::NonConfidential(-1),
                                },
                                catalyst_core::protocol::TransactionEntry {
                                    public_key: recv_pk,
                                    amount: catalyst_core::protocol::EntryAmount::NonConfidential(1),
                                },
                            ],
                            lock_time: now_secs as u32,
                            fees: 0,
                            data: Vec::new(),
                        },
                        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
                        timestamp: now,
                    };

                    if let Ok(msg) = ProtocolTxGossip::new(tx, now) {
                        // Ensure the local node sees its own generated txs (broadcast does not loop back).
                        {
                            let mut mp = mempool.write().await;
                            let _ = mp.insert_protocol(msg.clone(), now_secs);
                        }
                        if let Ok(env) = MessageEnvelope::from_message(&msg, "txgen".to_string(), None) {
                            let _ = net.broadcast_envelope(&env).await;
                        }
                    }

                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_millis(interval_ms)) => {}
                        _ = shutdown_rx2.changed() => {}
                    }
                }
            });
            self.network_tasks.push(handle);
        }

        let cycle_ms = self.config.consensus.cycle_duration_seconds as u64 * 1000;
        info!(
            "Consensus loop enabled. cycle={}ms producer_id={} validator={}",
            cycle_ms, producer_id, self.config.validator
        );

        // Only validator nodes should run consensus cycles. Non-validator nodes still participate
        // in networking and can be upgraded later to observer mode.
        if self.config.validator {
            let self_is_validator = self.config.validator;
            let min_producer_count = self.config.consensus.min_producer_count as usize;
            let max_entries_per_cycle = self.config.consensus.max_transactions_per_block as usize;
            self.consensus_task = Some(tokio::spawn(async move {
                // Seed for deterministic producer selection (paper uses previous LSU merkle root).
                // We approximate with the hash of the most recent LSU we produced/observed.
                let mut prev_seed: [u8; 32] = [0u8; 32];

                // Epoch-aligned cycle schedule so nodes start cycles together.
                let now_ms = current_timestamp_ms();
                let rem = now_ms % cycle_ms;
                let wait_ms = if rem == 0 { 0 } else { cycle_ms - rem };
                if wait_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;
                }

                let mut ticker = tokio::time::interval(std::time::Duration::from_millis(cycle_ms));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    tokio::select! {
                        _ = ticker.tick() => {},
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() { break; }
                            continue;
                        }
                    }

                    if *shutdown_rx.borrow() {
                        break;
                    }

                    // A stable cycle number based on wall-clock epoch time.
                    let cycle = current_timestamp_ms() / cycle_ms;

                    // Deterministic producer selection (protocol function) based on:
                    // - worker pool: discovered validators (node ids)
                    // - seed: prev cycle LSU hash (approx)
                    // - producer_count: config min_producer_count (bounded by pool size)
                    let (worker_pool, id_map) = {
                        let m = known_validators.read().await;
                        let mut pool: Vec<NodeId> = Vec::new();
                        let mut map: std::collections::HashMap<NodeId, String> = std::collections::HashMap::new();

                        // Always include self in the worker pool.
                        if self_is_validator {
                            pool.push(public_key);
                            map.insert(public_key, producer_id.clone());
                        }

                        for (node_id, info) in m.iter() {
                            if info.is_validator {
                                pool.push(*node_id);
                                map.insert(*node_id, info.producer_id.clone());
                            }
                        }

                        pool.sort();
                        pool.dedup();
                        (pool, map)
                    };

                    let producer_count = std::cmp::max(1, min_producer_count);
                    let selected_worker_ids = select_producers_for_next_cycle(&worker_pool, &prev_seed, producer_count);
                    let mut selected: Vec<String> = selected_worker_ids
                        .iter()
                        .filter_map(|id| id_map.get(id).cloned())
                        .collect();
                    selected.sort();
                    selected.dedup();

                    info!("Cycle {} selected_producers={:?}", cycle, selected);

                    // Temporary: deterministically pick a "batch leader" so all producers use
                    // the same tx entries list for Construction.
                    let leader = selected.first().cloned().unwrap_or_else(|| producer_id.clone());
                    let transactions = if producer_id == leader {
                        let entries = {
                            let mut mp = mempool.write().await;
                            mp.freeze_and_drain_entries(max_entries_per_cycle)
                        };
                        info!("Cycle {} tx batch leader={} entries={}", cycle, leader, entries.len());
                        if let Ok(batch) = TxBatch::new(cycle, entries.clone()) {
                            if let Ok(env) = MessageEnvelope::from_message(&batch, "txbatch".to_string(), None) {
                                let _ = network.broadcast_envelope(&env).await;
                            }
                        }
                        entries
                    } else {
                        // Wait briefly for the batch to arrive.
                        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(3000);
                        loop {
                            if std::time::Instant::now() >= deadline {
                                info!("Cycle {} tx batch follower={} timeout waiting for leader={}", cycle, producer_id, leader);
                                break Vec::new();
                            }
                            if let Some(entries) = tx_batches.write().await.remove(&cycle) {
                                info!("Cycle {} tx batch follower={} got entries={}", cycle, producer_id, entries.len());
                                break entries;
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        }
                    };

                    match consensus.start_cycle(cycle, selected, transactions).await {
                        Ok(Some(update)) => {
                            if let Ok(h) = hash_data(&update) {
                                prev_seed = h;
                            }
                            // DFS-backed LSU sync:
                            // - store LSU bytes in DFS (local content addressing)
                            // - gossip CID + expected LSU hash
                            // fallback: full LSU gossip if DFS is disabled/unavailable.
                            if let Some(dfs) = &dfs {
                                if let Ok(lsu_hash) = catalyst_consensus::types::hash_data(&update) {
                                    if let Ok(bytes) = update.serialize() {
                                        let cid_str = dfs.put(bytes).await.ok();
                                        if let Some(cid) = cid_str {
                                            let msg = LsuCidGossip { cycle, lsu_hash, cid };
                                            if let Ok(env) = MessageEnvelope::from_message(&msg, "lsu_cid".to_string(), None) {
                                                let _ = network.broadcast_envelope(&env).await;
                                            }
                                            info!("Stored LSU in DFS and broadcast CID cycle={} cid={}", cycle, msg.cid);
                                        } else if let Ok(msg) = LsuGossip::new(update.clone()) {
                                            if let Ok(env) = MessageEnvelope::from_message(&msg, "lsu".to_string(), None) {
                                                let _ = network.broadcast_envelope(&env).await;
                                            }
                                        }
                                    }
                                }
                            } else if let Ok(msg) = LsuGossip::new(update.clone()) {
                                if let Ok(env) = MessageEnvelope::from_message(&msg, "lsu".to_string(), None) {
                                    let _ = network.broadcast_envelope(&env).await;
                                }
                            }
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
                }
            }));
        }

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

        for h in self.network_tasks.drain(..) {
            h.abort();
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

