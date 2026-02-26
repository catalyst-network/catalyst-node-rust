use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tracing::{info, warn};

use catalyst_core::{NodeId, NodeRole, ResourceProof, WorkerPass};
use catalyst_core::protocol::select_producers_for_next_cycle;
use catalyst_consensus::{CollaborativeConsensus, ConsensusConfig as ConsensusEngineConfig};
use catalyst_consensus::producer::{Producer, ProducerManager};
use catalyst_network::{NetworkConfig as P2pConfig, NetworkService as P2pService, Multiaddr};
use catalyst_utils::{
    CatalystDeserialize, CatalystSerialize, MessageType, MessageEnvelope,
    utils::current_timestamp_ms,
    impl_catalyst_serialize,
};
use catalyst_consensus::types::hash_data;
use catalyst_utils::state::StateManager;

use crate::config::NodeConfig;
use crate::tx::{Mempool, ProtocolTxBatch, ProtocolTxGossip, TxBatch, TxGossip};
use crate::sync::{
    FileRequestMsg, FileResponseMsg, KeyProofChange, LsuCidGossip, LsuGossip, LsuRangeRequest,
    LsuRangeResponse, StateProofBundle,
};
use crate::evm::{decode_evm_marker, encode_evm_marker, EvmTxKind, EvmTxPayload};
use crate::evm_revm::{execute_call_and_persist, execute_deploy_and_persist};

use catalyst_dfs::ContentId;

use crate::dfs_store::LocalContentStore;

use catalyst_storage::{StorageConfig as StorageConfigLib, StorageManager};

use alloy_primitives::Address as EvmAddress;

#[derive(Debug, Clone)]
struct DialState {
    next_at: Instant,
    backoff: std::time::Duration,
}

fn jitter_ms(max_ms: u64) -> u64 {
    if max_ms == 0 {
        return 0;
    }
    // Good enough jitter; avoids synchronizing dials across nodes.
    let r: u64 = rand::random();
    r % (max_ms + 1)
}

async fn local_applied_state_root(store: &catalyst_storage::StorageManager) -> [u8; 32] {
    // Prefer the engine-provided cached root (restored on open), and fall back to the persisted
    // metadata key used by older codepaths.
    if let Some(r) = store.get_state_root() {
        return r;
    }
    // Best-effort: older DBs may only have the metadata key.
    // NOTE: this is sync code; failures should not panic.
    store
        .get_metadata("consensus:last_applied_state_root")
        .await
        .ok()
        .flatten()
        .and_then(|b| if b.len() == 32 {
            let mut r = [0u8; 32];
            r.copy_from_slice(&b[..32]);
            Some(r)
        } else {
            None
        })
        .unwrap_or([0u8; 32])
}

async fn local_applied_cycle(store: &catalyst_storage::StorageManager) -> u64 {
    let Some(b) = store
        .get_metadata("consensus:last_applied_cycle")
        .await
        .ok()
        .flatten()
    else {
        return 0;
    };
    if b.len() != 8 {
        return 0;
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&b[..8]);
    u64::from_le_bytes(arr)
}

async fn meta_32(store: &catalyst_storage::StorageManager, key: &str) -> Option<[u8; 32]> {
    let b = store.get_metadata(key).await.ok().flatten()?;
    if b.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&b[..32]);
    Some(out)
}

async fn meta_string(store: &catalyst_storage::StorageManager, key: &str) -> Option<String> {
    let b = store.get_metadata(key).await.ok().flatten()?;
    let s = String::from_utf8_lossy(&b).to_string();
    if s.is_empty() { None } else { Some(s) }
}

async fn persist_lsu_history(
    store: &catalyst_storage::StorageManager,
    cycle: u64,
    lsu_bytes: &[u8],
    lsu_hash: &[u8; 32],
    cid: &str,
    state_root: &[u8; 32],
) {
    // Per-cycle history used by RPC/indexers. Best-effort.
    let _ = store
        .set_metadata(&format!("consensus:lsu:{}", cycle), lsu_bytes)
        .await;
    let _ = store
        .set_metadata(&format!("consensus:lsu_hash:{}", cycle), lsu_hash)
        .await;
    let _ = store
        .set_metadata(&cycle_by_lsu_hash_key(lsu_hash), &cycle.to_le_bytes())
        .await;
    if !cid.is_empty() {
        let _ = store
            .set_metadata(&format!("consensus:lsu_cid:{}", cycle), cid.as_bytes())
            .await;
    }
    let _ = store
        .set_metadata(&format!("consensus:lsu_state_root:{}", cycle), state_root)
        .await;
}

#[derive(Debug, Clone)]
struct RelayCacheCfg {
    max_entries: usize,
    target_entries: usize,
    retention_ms: u64,
}

impl Default for RelayCacheCfg {
    fn default() -> Self {
        Self {
            max_entries: 5000,
            target_entries: 4000,
            retention_ms: 10 * 60 * 1000,
        }
    }
}

#[derive(Debug)]
struct RelayCache {
    cfg: RelayCacheCfg,
    seen: std::collections::HashMap<String, u64>,
}

impl RelayCache {
    fn new(cfg: RelayCacheCfg) -> Self {
        Self {
            cfg,
            seen: std::collections::HashMap::new(),
        }
    }

    fn should_relay(&mut self, env: &MessageEnvelope, now_ms: u64) -> bool {
        if env.is_expired() {
            return false;
        }
        // Only relay broadcasts.
        if env.target.is_some() {
            return false;
        }
        if self.seen.contains_key(&env.id) {
            return false;
        }

        self.seen.insert(env.id.clone(), now_ms);

        // Prune old ids (best-effort).
        let keep_after = now_ms.saturating_sub(self.cfg.retention_ms);
        self.seen.retain(|_, ts| *ts >= keep_after);

        // Cap size to prevent unbounded growth under attack.
        if self.seen.len() > self.cfg.max_entries {
            let mut v: Vec<(String, u64)> = self
                .seen
                .iter()
                .map(|(k, ts)| (k.clone(), *ts))
                .collect();
            v.sort_by_key(|(_, ts)| *ts);
            let drop_n = v.len().saturating_sub(self.cfg.target_entries);
            for (k, _) in v.into_iter().take(drop_n) {
                self.seen.remove(&k);
            }
        }

        true
    }
}

const DEFAULT_EVM_GAS_LIMIT: u64 = 8_000_000;

// Dev/testnet faucet key (32-byte scalar); pubkey is derived and is a valid compressed Ristretto point.
const FAUCET_PRIVATE_KEY_BYTES: [u8; 32] = [0xFA; 32];

const META_PROTOCOL_CHAIN_ID: &str = "protocol:chain_id";
const META_PROTOCOL_NETWORK_ID: &str = "protocol:network_id";
const META_PROTOCOL_GENESIS_HASH: &str = "protocol:genesis_hash";

async fn resolve_dns_seeds_to_bootstrap_multiaddrs(seeds: &[String], default_port: u16) -> Vec<Multiaddr> {
    if seeds.is_empty() {
        return Vec::new();
    }

    let resolver: hickory_resolver::TokioResolver = match hickory_resolver::TokioResolver::builder_tokio() {
        Ok(b) => b.build(),
        Err(e) => {
            warn!("dns_seeds: failed to init resolver from system conf: {e}");
            return Vec::new();
        }
    };

    let mut out: Vec<Multiaddr> = Vec::new();

    for seed in seeds {
        let name = seed.trim().trim_end_matches('.').to_string();
        if name.is_empty() {
            continue;
        }

        // Preferred: TXT records containing multiaddrs or ip[:port] tokens.
        let mut found_any = false;
        if let Ok(lookup) = resolver.txt_lookup(name.clone()).await {
            for txt in lookup.iter() {
                for chunk in txt.txt_data().iter() {
                    let s = String::from_utf8_lossy(chunk).to_string();
                    for token in s.split(|c: char| c == ',' || c.is_whitespace() || c == ';').map(|t| t.trim()).filter(|t| !t.is_empty()) {
                        let addrs = parse_seed_token_to_multiaddrs(token, default_port);
                        if !addrs.is_empty() {
                            found_any = true;
                            out.extend(addrs);
                        }
                    }
                }
            }
        }

        // Fallback: resolve A/AAAA and convert to `/ip{4,6}/.../tcp/<port>`.
        if !found_any {
            if let Ok(ips) = resolver.lookup_ip(name.clone()).await {
                for ip in ips.iter() {
                    if let Ok(ma) = ip_to_multiaddr(ip, default_port) {
                        out.push(ma);
                    }
                }
            }
        }
    }

    out.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
    out.dedup_by(|a, b| a.to_string() == b.to_string());
    out
}

fn parse_seed_token_to_multiaddrs(token: &str, default_port: u16) -> Vec<Multiaddr> {
    let t = token.trim();
    if t.is_empty() {
        return Vec::new();
    }

    // Multiaddr directly.
    if t.starts_with('/') {
        if let Ok(ma) = t.parse::<Multiaddr>() {
            return vec![ma];
        }
        return Vec::new();
    }

    // SocketAddr (e.g. "45.32.177.248:30333" or "[2606:...]:30333").
    if let Ok(sa) = t.parse::<std::net::SocketAddr>() {
        if let Ok(ma) = ip_to_multiaddr(sa.ip(), sa.port()) {
            return vec![ma];
        }
        return Vec::new();
    }

    // Bare IP (e.g. "45.32.177.248" or "2606:...").
    if let Ok(ip) = t.parse::<std::net::IpAddr>() {
        if let Ok(ma) = ip_to_multiaddr(ip, default_port) {
            return vec![ma];
        }
        return Vec::new();
    }

    Vec::new()
}

fn ip_to_multiaddr(ip: std::net::IpAddr, port: u16) -> Result<Multiaddr, ()> {
    let s = match ip {
        std::net::IpAddr::V4(v4) => format!("/ip4/{}/tcp/{}", v4, port),
        std::net::IpAddr::V6(v6) => format!("/ip6/{}/tcp/{}", v6, port),
    };
    s.parse::<Multiaddr>().map_err(|_| ())
}

fn balance_key_for_pubkey(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut k = b"bal:".to_vec();
    k.extend_from_slice(pubkey);
    k
}

fn worker_key_for_pubkey(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut k = b"workers:".to_vec();
    k.extend_from_slice(pubkey);
    k
}

fn is_worker_reg_marker(sig: &[u8]) -> bool {
    sig.starts_with(b"WRKREG1")
}

fn is_evm_marker(sig: &[u8]) -> bool {
    sig.starts_with(b"EVM1")
}

fn lsu_contains_evm(lsu: &catalyst_consensus::types::LedgerStateUpdate) -> bool {
    lsu.partial_update
        .transaction_entries
        .iter()
        .any(|e| is_evm_marker(&e.signature))
}

fn pubkey_to_evm_addr20(pk: &[u8; 32]) -> [u8; 20] {
    let mut out = [0u8; 20];
    out.copy_from_slice(&pk[12..32]);
    out
}

fn evm_code_key(addr20: &[u8; 20]) -> Vec<u8> {
    let mut k = b"evm:code:".to_vec();
    k.extend_from_slice(addr20);
    k
}

fn evm_last_return_key(addr20: &[u8; 20]) -> Vec<u8> {
    let mut k = b"evm:last_return:".to_vec();
    k.extend_from_slice(addr20);
    k
}

fn encode_i64(v: i64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn encode_u64(v: u64) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn touched_keys_for_lsu(lsu: &catalyst_consensus::types::LedgerStateUpdate) -> Vec<Vec<u8>> {
    use std::collections::BTreeSet;
    let mut keys: BTreeSet<Vec<u8>> = BTreeSet::new();

    // Balance keys (all entries, excluding worker reg / EVM markers)
    for e in &lsu.partial_update.transaction_entries {
        if is_worker_reg_marker(&e.signature) {
            keys.insert(worker_key_for_pubkey(&e.public_key));
        } else if is_evm_marker(&e.signature) {
            if let Some((payload, _sig64)) = decode_evm_marker(&e.signature) {
                match payload.kind {
                    EvmTxKind::Deploy { .. } => {
                        let from20 = pubkey_to_evm_addr20(&e.public_key);
                        let from_addr = EvmAddress::from_slice(&from20);
                        let evm_nonce = payload.nonce.saturating_sub(1);
                        let created = catalyst_runtime_evm::utils::calculate_ethereum_create_address(&from_addr, evm_nonce);
                        let mut addr20 = [0u8; 20];
                        addr20.copy_from_slice(created.as_slice());
                        keys.insert(evm_code_key(&addr20));
                    }
                    EvmTxKind::Call { to, .. } => {
                        keys.insert(evm_last_return_key(&to));
                    }
                }
            }
        } else {
            keys.insert(balance_key_for_pubkey(&e.public_key));
        }
    }

    // Nonce keys: one per "tx" boundary (signature group)
    use std::collections::BTreeMap;
    let mut by_sig: BTreeMap<&[u8], Vec<&catalyst_consensus::types::TransactionEntry>> = BTreeMap::new();
    for e in &lsu.partial_update.transaction_entries {
        by_sig.entry(&e.signature).or_default().push(e);
    }
    for (_sig, entries) in by_sig {
        let mut sender: Option<[u8; 32]> = None;
        for e in &entries {
            if e.amount < 0 {
                sender = Some(e.public_key);
                break;
            }
        }
        if sender.is_none() {
            if let Some(e0) = entries.iter().find(|e| is_worker_reg_marker(&e.signature)) {
                sender = Some(e0.public_key);
            }
        }
        if sender.is_none() {
            if let Some(e0) = entries.iter().find(|e| is_evm_marker(&e.signature)) {
                sender = Some(e0.public_key);
            }
        }
        if let Some(pk) = sender {
            keys.insert(nonce_key_for_pubkey(&pk));
        }
    }

    keys.into_iter().collect()
}

fn verify_state_transition_bundle(
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    bundle: &StateProofBundle,
) -> bool {
    // Verify Merkle proofs for old/new roots, and verify that the new values match applying LSU deltas.
    if bundle.cycle != lsu.cycle_number {
        return false;
    }
    if bundle.changes.is_empty() {
        return false;
    }

    use std::collections::BTreeMap;
    let mut old: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
    let mut new: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();

    for c in &bundle.changes {
        if !catalyst_storage::merkle::verify_proof(&bundle.prev_state_root, &c.old_proof) {
            return false;
        }
        if !catalyst_storage::merkle::verify_proof(&bundle.new_state_root, &c.new_proof) {
            return false;
        }
        old.insert(c.key.clone(), c.old_value.clone());
        new.insert(c.key.clone(), c.new_value.clone());
    }

    // Apply LSU deltas to old map and compare with new map.
    let mut expected: BTreeMap<Vec<u8>, Vec<u8>> = old.clone();

    for e in &lsu.partial_update.transaction_entries {
        if is_worker_reg_marker(&e.signature) {
            let k = worker_key_for_pubkey(&e.public_key);
            expected.insert(k, vec![1u8]);
            continue;
        }
        if is_evm_marker(&e.signature) {
            // NOTE: EVM execution touches dynamic storage keys; proof bundles are disabled when EVM txs exist.
            if let Some((payload, _sig64)) = decode_evm_marker(&e.signature) {
                match payload.kind {
                    EvmTxKind::Deploy { .. } => {
                        let from20 = pubkey_to_evm_addr20(&e.public_key);
                        let from_addr = EvmAddress::from_slice(&from20);
                        let evm_nonce = payload.nonce.saturating_sub(1);
                        let created = catalyst_runtime_evm::utils::calculate_ethereum_create_address(&from_addr, evm_nonce);
                        let mut addr20 = [0u8; 20];
                        addr20.copy_from_slice(created.as_slice());
                        expected.insert(evm_code_key(&addr20), Vec::new());
                    }
                    EvmTxKind::Call { to, .. } => {
                        expected.insert(evm_last_return_key(&to), Vec::new());
                    }
                }
            }
            continue;
        }

        let k = balance_key_for_pubkey(&e.public_key);
        let cur = expected.get(&k).and_then(|b| decode_i64(b)).unwrap_or(0);
        let next = cur.saturating_add(e.amount);
        expected.insert(k, encode_i64(next));
    }

    // Nonce increments per tx boundary.
    use std::collections::BTreeMap as BT;
    let mut by_sig: BT<Vec<u8>, Vec<&catalyst_consensus::types::TransactionEntry>> = BT::new();
    for e in &lsu.partial_update.transaction_entries {
        by_sig.entry(e.signature.clone()).or_default().push(e);
    }
    for (_sig, entries) in by_sig {
        let mut sender: Option<[u8; 32]> = None;
        for e in &entries {
            if e.amount < 0 {
                sender = Some(e.public_key);
                break;
            }
        }
        if sender.is_none() {
            if let Some(e0) = entries.iter().find(|e| is_worker_reg_marker(&e.signature)) {
                sender = Some(e0.public_key);
            }
        }
        if sender.is_none() {
            if let Some(e0) = entries.iter().find(|e| is_evm_marker(&e.signature)) {
                sender = Some(e0.public_key);
            }
        }
        if let Some(pk) = sender {
            let k = nonce_key_for_pubkey(&pk);
            let cur = expected.get(&k).and_then(|b| decode_u64(b)).unwrap_or(0);
            expected.insert(k, encode_u64(cur.saturating_add(1)));
        }
    }

    // Ensure all expected keys match bundle new values (bundle might contain extra keys; that's fine).
    for (k, vexp) in expected {
        if let Some(vnew) = new.get(&k) {
            if vnew != &vexp {
                return false;
            }
        }
    }
    true
}

async fn apply_lsu_to_storage_without_root_check(
    store: &StorageManager,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    new_root: [u8; 32],
) -> Result<()> {
    #[derive(Clone, Debug, Default)]
    struct ApplyOutcome {
        success: bool,
        gas_used: Option<u64>,
        return_data: Option<Vec<u8>>,
        error: Option<String>,
    }

    let mut outcome_by_sig: std::collections::HashMap<Vec<u8>, ApplyOutcome> = std::collections::HashMap::new();
    let mut executed_sigs: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();

    let chain_id = load_chain_id_u64(store).await;

    // Apply balance / worker / EVM updates.
    for e in &lsu.partial_update.transaction_entries {
        if is_worker_reg_marker(&e.signature) {
            let k = worker_key_for_pubkey(&e.public_key);
            let _ = store.set_state(&k, vec![1u8]).await;
            outcome_by_sig.entry(e.signature.clone()).or_insert(ApplyOutcome {
                success: true,
                ..Default::default()
            });
            continue;
        }
        if is_evm_marker(&e.signature) {
            if !executed_sigs.contains(&e.signature) {
                executed_sigs.insert(e.signature.clone());
                if let Some((payload, _sig64)) = decode_evm_marker(&e.signature) {
                    let from20 = pubkey_to_evm_addr20(&e.public_key);
                    let from = EvmAddress::from_slice(&from20);
                    let gas_limit = DEFAULT_EVM_GAS_LIMIT.max(21_000);
                    match payload.kind {
                        EvmTxKind::Deploy { bytecode } => {
                            match execute_deploy_and_persist(store, from, chain_id, payload.nonce, bytecode, gas_limit).await {
                                Ok((_created, info, _persisted)) => {
                                    outcome_by_sig.insert(
                                        e.signature.clone(),
                                        ApplyOutcome {
                                            success: info.success,
                                            gas_used: Some(info.gas_used),
                                            return_data: Some(info.return_data.to_vec()),
                                            error: info.error.clone(),
                                        },
                                    );
                                }
                                Err(err) => {
                                    outcome_by_sig.insert(
                                        e.signature.clone(),
                                        ApplyOutcome {
                                            success: false,
                                            error: Some(err.to_string()),
                                            ..Default::default()
                                        },
                                    );
                                }
                            }
                        }
                        EvmTxKind::Call { to, input } => {
                            let to_addr = EvmAddress::from_slice(&to);
                            match execute_call_and_persist(store, from, to_addr, chain_id, payload.nonce, input, gas_limit).await {
                                Ok((info, _persisted)) => {
                                    outcome_by_sig.insert(
                                        e.signature.clone(),
                                        ApplyOutcome {
                                            success: info.success,
                                            gas_used: Some(info.gas_used),
                                            return_data: Some(info.return_data.to_vec()),
                                            error: info.error.clone(),
                                        },
                                    );
                                }
                                Err(err) => {
                                    outcome_by_sig.insert(
                                        e.signature.clone(),
                                        ApplyOutcome {
                                            success: false,
                                            error: Some(err.to_string()),
                                            ..Default::default()
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
            continue;
        }
        let bal = get_balance_i64(store, &e.public_key).await;
        let next = bal.saturating_add(e.amount);
        let _ = set_balance_i64(store, &e.public_key, next).await;

        outcome_by_sig.entry(e.signature.clone()).or_insert(ApplyOutcome {
            success: true,
            ..Default::default()
        });
    }

    // Nonce increments.
    use std::collections::BTreeMap;
    let mut by_sig: BTreeMap<Vec<u8>, Vec<&catalyst_consensus::types::TransactionEntry>> = BTreeMap::new();
    for e in &lsu.partial_update.transaction_entries {
        by_sig.entry(e.signature.clone()).or_default().push(e);
    }
    for (_sig, entries) in by_sig {
        let mut sender: Option<[u8; 32]> = None;
        for e in &entries {
            if e.amount < 0 {
                sender = Some(e.public_key);
                break;
            }
        }
        if sender.is_none() {
            if let Some(e0) = entries.iter().find(|e| is_worker_reg_marker(&e.signature)) {
                sender = Some(e0.public_key);
            }
        }
        if sender.is_none() {
            if let Some(e0) = entries.iter().find(|e| is_evm_marker(&e.signature)) {
                sender = Some(e0.public_key);
            }
        }
        if let Some(pk) = sender {
            let cur = get_nonce_u64(store, &pk).await;
            let _ = set_nonce_u64(store, &pk, cur.saturating_add(1)).await;
        }
    }

    // Persist head metadata (trusted).
    store.set_state_root_cache(new_root);
    let _ = store
        .set_metadata("consensus:last_applied_cycle", &lsu.cycle_number.to_le_bytes())
        .await;
    let _ = store
        .set_metadata("consensus:last_applied_state_root", &new_root)
        .await;
    if let Ok(h) = catalyst_consensus::types::hash_data(lsu) {
        let _ = store
            .set_metadata("consensus:last_applied_lsu_hash", &h)
            .await;
    }

    // Update per-tx receipts for this cycle (best-effort): mark Applied + attach outcomes.
    let cycle = lsu.cycle_number;
    let lsu_hash = catalyst_consensus::types::hash_data(lsu).unwrap_or([0u8; 32]);
    let txids = load_cycle_txids(store, cycle).await;
    for txid in txids {
        let key = tx_meta_key(&txid);
        let existing = store.get_metadata(&key).await.ok().flatten();
        let mut meta = existing
            .as_deref()
            .and_then(|b| bincode::deserialize::<catalyst_core::protocol::TxMeta>(b).ok())
            .unwrap_or_else(|| catalyst_core::protocol::TxMeta {
                tx_id: txid,
                status: catalyst_core::protocol::TxStatus::Pending,
                received_at_ms: current_timestamp_ms(),
                sender: None,
                nonce: 0,
                fees: 0,
                selected_cycle: Some(cycle),
                applied_cycle: None,
                applied_lsu_hash: None,
                applied_state_root: None,
                applied_success: None,
                applied_error: None,
                evm_gas_used: None,
                evm_return: None,
            });
        meta.status = catalyst_core::protocol::TxStatus::Applied;
        meta.applied_cycle = Some(cycle);
        meta.applied_lsu_hash = Some(lsu_hash);
        meta.applied_state_root = Some(new_root);

        if let Some(bytes) = store.get_metadata(&tx_raw_key(&txid)).await.ok().flatten() {
            if let Ok(tx) = bincode::deserialize::<catalyst_core::protocol::Transaction>(&bytes) {
                let sig = tx_boundary_signature(&tx);
                if let Some(o) = outcome_by_sig.get(&sig) {
                    meta.applied_success = Some(o.success);
                    meta.applied_error = o.error.clone();
                    meta.evm_gas_used = o.gas_used;
                    meta.evm_return = o.return_data.clone();
                } else {
                    meta.applied_success = Some(true);
                }
            } else {
                meta.applied_success = Some(true);
            }
        } else {
            meta.applied_success = Some(true);
        }

        if let Ok(mbytes) = bincode::serialize(&meta) {
            let _ = store.set_metadata(&key, &mbytes).await;
        }
    }

    // Prune persisted mempool against updated nonces.
    prune_persisted_mempool(store).await;

    // Opportunistic disk-bounding: prune old historical metadata (if enabled).
    crate::pruning::maybe_prune_history(store).await;

    // Flush without recomputing.
    for cf_name in store.engine().cf_names() {
        let _ = store.engine().flush_cf(&cf_name);
    }
    Ok(())
}
fn nonce_key_for_pubkey(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut k = b"nonce:".to_vec();
    k.extend_from_slice(pubkey);
    k
}

fn decode_i64(bytes: &[u8]) -> Option<i64> {
    if bytes.len() != 8 {
        return None;
    }
    let mut b = [0u8; 8];
    b.copy_from_slice(bytes);
    Some(i64::from_le_bytes(b))
}

fn decode_u64(bytes: &[u8]) -> Option<u64> {
    if bytes.len() != 8 {
        return None;
    }
    let mut b = [0u8; 8];
    b.copy_from_slice(bytes);
    Some(u64::from_le_bytes(b))
}

fn parse_hex_32(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

fn faucet_genesis_params(cfg: &crate::config::ProtocolConfig) -> anyhow::Result<([u8; 32], i64)> {
    use crate::config::FaucetMode;

    match cfg.faucet_mode {
        FaucetMode::Deterministic => {
            anyhow::ensure!(
                cfg.allow_deterministic_faucet,
                "deterministic faucet is disabled by config (set protocol.allow_deterministic_faucet=true to allow it)"
            );
            let pk = catalyst_crypto::PrivateKey::from_bytes(FAUCET_PRIVATE_KEY_BYTES)
                .public_key()
                .to_bytes();
            anyhow::ensure!(cfg.faucet_balance > 0, "protocol.faucet_balance must be > 0");
            Ok((pk, cfg.faucet_balance))
        }
        FaucetMode::Configured => {
            let Some(s) = cfg.faucet_pubkey_hex.as_ref() else {
                anyhow::bail!(
                    "protocol.faucet_pubkey_hex must be set when protocol.faucet_mode=configured"
                );
            };
            let pk = parse_hex_32(s).ok_or_else(|| {
                anyhow::anyhow!("protocol.faucet_pubkey_hex must be 32 bytes hex (64 chars)")
            })?;
            anyhow::ensure!(cfg.faucet_balance > 0, "protocol.faucet_balance must be > 0");
            Ok((pk, cfg.faucet_balance))
        }
        FaucetMode::Disabled => Ok(([0u8; 32], 0)),
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct GenesisDescriptor {
    mode: String,
    chain_id: u64,
    network_id: String,
    faucet_pubkey: [u8; 32],
    faucet_balance: i64,
}

impl_catalyst_serialize!(GenesisDescriptor, mode, chain_id, network_id, faucet_pubkey, faucet_balance);

async fn load_chain_id_u64(store: &StorageManager) -> u64 {
    store
        .get_metadata(META_PROTOCOL_CHAIN_ID)
        .await
        .ok()
        .flatten()
        .and_then(|b| {
            if b.len() != 8 {
                return None;
            }
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&b);
            Some(u64::from_le_bytes(arr))
        })
        .unwrap_or(31337)
}

async fn ensure_chain_identity_and_genesis(
    store: &StorageManager,
    cfg: &crate::config::ProtocolConfig,
) -> anyhow::Result<()> {
    // Chain id
    let existing_chain_id = store.get_metadata(META_PROTOCOL_CHAIN_ID).await.ok().flatten();
    if existing_chain_id.is_none() {
        let _ = store
            .set_metadata(META_PROTOCOL_CHAIN_ID, &cfg.chain_id.to_le_bytes())
            .await;
    }

    // Network id
    let existing_net = store.get_metadata(META_PROTOCOL_NETWORK_ID).await.ok().flatten();
    if existing_net.is_none() {
        let _ = store
            .set_metadata(META_PROTOCOL_NETWORK_ID, cfg.network_id.as_bytes())
            .await;
    }

    // Genesis hash: only apply genesis state on a fresh DB.
    let existing_genesis = store.get_metadata(META_PROTOCOL_GENESIS_HASH).await.ok().flatten();
    if existing_genesis.is_some() {
        return Ok(());
    }

    let already = store
        .get_metadata("consensus:last_applied_cycle")
        .await
        .ok()
        .flatten()
        .and_then(|b| {
            if b.len() != 8 {
                return None;
            }
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&b);
            Some(u64::from_le_bytes(arr))
        })
        .unwrap_or(0);

    let (faucet_pk, faucet_balance) = faucet_genesis_params(cfg)?;

    if already == 0 {
        // Fresh DB: apply a minimal genesis state (optional faucet funding).
        if faucet_balance > 0 && faucet_pk != [0u8; 32] {
            let existing_balance = store
                .get_state(&balance_key_for_pubkey(&faucet_pk))
                .await
                .ok()
                .flatten();
            if existing_balance.is_none() {
                let _ = set_balance_i64(store, &faucet_pk, faucet_balance).await;
                let _ = set_nonce_u64(store, &faucet_pk, 0).await;
            }
        }
        let _ = store.commit().await;

        let g = GenesisDescriptor {
            mode: "fresh".to_string(),
            chain_id: cfg.chain_id,
            network_id: cfg.network_id.clone(),
            faucet_pubkey: faucet_pk,
            faucet_balance,
        };
        let gh = hash_data(&g).unwrap_or([0u8; 32]);
        let _ = store.set_metadata(META_PROTOCOL_GENESIS_HASH, &gh).await;
        info!(
            "Genesis initialized chain_id={} network_id={} genesis_hash=0x{} faucet_pk={} faucet_balance={}",
            cfg.chain_id,
            cfg.network_id,
            hex_encode(&gh),
            hex_encode(&faucet_pk),
            faucet_balance
        );
    } else {
        // Legacy DB: do not mutate balances/consensus head; just stamp an identity hash.
        let g = GenesisDescriptor {
            mode: "legacy".to_string(),
            chain_id: cfg.chain_id,
            network_id: cfg.network_id.clone(),
            faucet_pubkey: faucet_pk,
            faucet_balance,
        };
        let gh = hash_data(&g).unwrap_or([0u8; 32]);
        let _ = store.set_metadata(META_PROTOCOL_GENESIS_HASH, &gh).await;
        info!(
            "Stamped legacy genesis identity chain_id={} network_id={} genesis_hash=0x{} (no state reset)",
            cfg.chain_id,
            cfg.network_id,
            hex_encode(&gh),
        );
    }

    Ok(())
}

async fn load_genesis_hash_32(store: &StorageManager) -> [u8; 32] {
    let Some(bytes) = store
        .get_metadata(META_PROTOCOL_GENESIS_HASH)
        .await
        .ok()
        .flatten()
    else {
        return [0u8; 32];
    };
    if bytes.len() != 32 {
        return [0u8; 32];
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

async fn verify_protocol_tx_signature_with_domain(store: &StorageManager, tx: &catalyst_core::protocol::Transaction) -> bool {
    let chain_id = load_chain_id_u64(store).await;
    let genesis_hash = load_genesis_hash_32(store).await;
    catalyst_crypto::verify_tx_signature_with_domain(tx, chain_id, genesis_hash).is_ok()
}

async fn get_balance_i64(store: &StorageManager, pubkey: &[u8; 32]) -> i64 {
    let k = balance_key_for_pubkey(pubkey);
    store
        .get_state(&k)
        .await
        .ok()
        .flatten()
        .and_then(|b| decode_i64(&b))
        .unwrap_or(0)
}

async fn set_balance_i64(store: &StorageManager, pubkey: &[u8; 32], v: i64) -> Result<()> {
    let k = balance_key_for_pubkey(pubkey);
    store.set_state(&k, v.to_le_bytes().to_vec()).await?;
    Ok(())
}

async fn get_nonce_u64(store: &StorageManager, pubkey: &[u8; 32]) -> u64 {
    let k = nonce_key_for_pubkey(pubkey);
    store
        .get_state(&k)
        .await
        .ok()
        .flatten()
        .and_then(|b| decode_u64(&b))
        .unwrap_or(0)
}

async fn set_nonce_u64(store: &StorageManager, pubkey: &[u8; 32], v: u64) -> Result<()> {
    let k = nonce_key_for_pubkey(pubkey);
    store.set_state(&k, v.to_le_bytes().to_vec()).await?;
    Ok(())
}

fn tx_is_sane(entries: &[catalyst_consensus::types::TransactionEntry]) -> bool {
    // Minimal sanity for our dev tx model:
    // - non-empty
    // - net sum is <= 0 (net-negative is interpreted as burned fees)
    //
    // NOTE: We intentionally allow `sum < 0` to support fee debits represented as a
    // dedicated negative entry within the transaction boundary.
    let sum: i64 = entries.iter().map(|e| e.amount).sum();
    !entries.is_empty() && sum <= 0
}

async fn tx_is_funded(store: &StorageManager, entries: &[catalyst_consensus::types::TransactionEntry]) -> bool {
    // Sufficient-funds check:
    // compute the net delta per pubkey and ensure no account would go negative.
    use std::collections::HashMap;
    let mut deltas: HashMap<[u8; 32], i64> = HashMap::new();
    for e in entries {
        *deltas.entry(e.public_key).or_insert(0) = deltas
            .get(&e.public_key)
            .copied()
            .unwrap_or(0)
            .saturating_add(e.amount);
    }
    for (pk, d) in deltas {
        if d < 0 {
            let bal = get_balance_i64(store, &pk).await;
            if bal.saturating_add(d) < 0 {
                return false;
            }
        }
    }
    true
}

async fn tx_nonce_expected_next(
    store: &StorageManager,
    sender: &[u8; 32],
    pending_max_nonce: Option<u64>,
) -> u64 {
    let cur = get_nonce_u64(store, sender).await;
    let max_seen = pending_max_nonce.map(|n| n.max(cur)).unwrap_or(cur);
    max_seen.saturating_add(1)
}

fn load_workers_from_state(store: &StorageManager) -> Vec<[u8; 32]> {
    let mut out: Vec<[u8; 32]> = Vec::new();
    if let Ok(iter) = store.engine().iterator("accounts") {
        for item in iter {
            if let Ok((k, v)) = item {
                if !k.starts_with(b"workers:") {
                    continue;
                }
                if v.as_ref() != [1u8] {
                    continue;
                }
                if k.len() != b"workers:".len() + 32 {
                    continue;
                }
                let mut pk = [0u8; 32];
                pk.copy_from_slice(&k[b"workers:".len()..]);
                out.push(pk);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

fn tx_sender_pubkey(tx: &catalyst_core::protocol::Transaction) -> Option<[u8; 32]> {
    match tx.core.tx_type {
        catalyst_core::protocol::TransactionType::WorkerRegistration => tx.core.entries.get(0).map(|e| e.public_key),
        catalyst_core::protocol::TransactionType::SmartContract => tx.core.entries.get(0).map(|e| e.public_key),
        _ => {
            let mut sender: Option<[u8; 32]> = None;
            for e in &tx.core.entries {
                if let catalyst_core::protocol::EntryAmount::NonConfidential(v) = e.amount {
                    if v < 0 {
                        match sender {
                            None => sender = Some(e.public_key),
                            Some(pk) if pk == e.public_key => {}
                            Some(_) => return None,
                        }
                    }
                }
            }
            sender
        }
    }
}

fn tx_boundary_signature(tx: &catalyst_core::protocol::Transaction) -> Vec<u8> {
    match tx.core.tx_type {
        catalyst_core::protocol::TransactionType::WorkerRegistration => {
            let mut sig = b"WRKREG1".to_vec();
            sig.extend_from_slice(&tx.signature.0);
            sig
        }
        catalyst_core::protocol::TransactionType::SmartContract => {
            let kind = bincode::deserialize::<EvmTxKind>(&tx.core.data)
                .unwrap_or(EvmTxKind::Call { to: [0u8; 20], input: Vec::new() });
            let payload = crate::evm::EvmTxPayload { nonce: tx.core.nonce, kind };
            crate::evm::encode_evm_marker(&payload, &tx.signature.0)
                .unwrap_or_else(|_| tx.signature.0.clone())
        }
        _ => tx.signature.0.clone(),
    }
}

fn tx_to_consensus_entries(tx: &catalyst_core::protocol::Transaction) -> Vec<catalyst_consensus::types::TransactionEntry> {
    if tx.core.tx_type == catalyst_core::protocol::TransactionType::WorkerRegistration {
        let pk = tx.core.entries.get(0).map(|e| e.public_key).unwrap_or([0u8; 32]);
        let mut sig = b"WRKREG1".to_vec();
        sig.extend_from_slice(&tx.signature.0);
        return vec![catalyst_consensus::types::TransactionEntry {
            public_key: pk,
            amount: 0,
            signature: sig,
        }];
    }

    if tx.core.tx_type == catalyst_core::protocol::TransactionType::SmartContract {
        let pk = tx.core.entries.get(0).map(|e| e.public_key).unwrap_or([0u8; 32]);
        let kind = bincode::deserialize::<EvmTxKind>(&tx.core.data)
            .unwrap_or(EvmTxKind::Call { to: [0u8; 20], input: Vec::new() });
        let payload = EvmTxPayload {
            nonce: tx.core.nonce,
            kind,
        };
        let marker = encode_evm_marker(&payload, &tx.signature.0).unwrap_or_else(|_| tx.signature.0.clone());
        return vec![catalyst_consensus::types::TransactionEntry {
            public_key: pk,
            amount: 0,
            signature: marker,
        }];
    }

    let sig = tx.signature.0.clone();
    let mut out = Vec::new();
    for e in &tx.core.entries {
        let amount = match &e.amount {
            catalyst_core::protocol::EntryAmount::NonConfidential(v) => *v,
            catalyst_core::protocol::EntryAmount::Confidential { .. } => continue,
        };
        out.push(catalyst_consensus::types::TransactionEntry {
            public_key: e.public_key,
            amount,
            signature: sig.clone(),
        });
    }
    out
}

async fn validate_and_select_protocol_txs_for_construction(
    store: &StorageManager,
    txs: Vec<catalyst_core::protocol::Transaction>,
    max_entries: usize,
) -> Vec<catalyst_core::protocol::Transaction> {
    use std::collections::HashMap;

    // Local simulation state.
    let mut balances: HashMap<[u8; 32], i64> = HashMap::new();
    let mut nonces: HashMap<[u8; 32], u64> = HashMap::new();

    let mut out: Vec<catalyst_core::protocol::Transaction> = Vec::new();
    let mut used_entries = 0usize;
    let now_secs = current_timestamp_ms() / 1000;

    // Deterministic iteration order: sort by bincode(tx) hash (same as tx_id derivation).
    let mut txs = txs;
    txs.sort_by(|a, b| {
        let ha = bincode::serialize(a).ok().and_then(|bytes| catalyst_consensus::types::hash_data(&bytes).ok()).unwrap_or([0u8;32]);
        let hb = bincode::serialize(b).ok().and_then(|bytes| catalyst_consensus::types::hash_data(&bytes).ok()).unwrap_or([0u8;32]);
        ha.cmp(&hb)
    });

    for tx in txs {
        // Basic format checks
        if tx.validate_basic().is_err() {
            continue;
        }
        // Fee floor check
        let min_fee = catalyst_core::protocol::min_fee(&tx);
        if tx.core.fees < min_fee {
            continue;
        }
        // Lock_time gate (same as mempool)
        if tx.core.lock_time as u64 > now_secs {
            continue;
        }
        // Signature check (already done in mempool, but re-check at formation time).
        // Must support v1 domain-separated signatures (wallet/CLI default).
        if !verify_protocol_tx_signature_with_domain(store, &tx).await {
            continue;
        }

        let Some(sender) = tx_sender_pubkey(&tx) else { continue };

        // Load sender nonce if needed.
        if !nonces.contains_key(&sender) {
            let n = get_nonce_u64(store, &sender).await;
            nonces.insert(sender, n);
        }
        let expected = nonces.get(&sender).copied().unwrap_or(0).saturating_add(1);
        if tx.core.nonce != expected {
            continue;
        }

        // Preload balances for all pubkeys in this tx.
        for e in &tx.core.entries {
            balances.entry(e.public_key).or_insert_with(|| 0);
        }
        for pk in balances.keys().cloned().collect::<Vec<_>>() {
            if balances.get(&pk).copied().unwrap_or(0) == 0 {
                let b = get_balance_i64(store, &pk).await;
                balances.insert(pk, b);
            }
        }

        // Apply deltas; ensure no negative.
        let mut ok = true;
        let mut deltas: Vec<([u8; 32], i64)> = Vec::new();
        for e in &tx.core.entries {
            let v = match &e.amount {
                catalyst_core::protocol::EntryAmount::NonConfidential(v) => *v,
                catalyst_core::protocol::EntryAmount::Confidential { .. } => { ok = false; break; }
            };
            deltas.push((e.public_key, v));
        }
        if !ok {
            continue;
        }
        // Fee debit is burned (no sink credit yet).
        if let Ok(fee_i64) = i64::try_from(tx.core.fees) {
            if fee_i64 > 0 {
                deltas.push((sender, -fee_i64));
            }
        } else {
            // Fee doesn't fit into ledger delta representation
            continue;
        }

        for (pk, d) in &deltas {
            let cur = *balances.get(pk).unwrap_or(&0);
            let next = cur.saturating_add(*d);
            if next < 0 {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }

        // Entry budget check
        let entry_count = tx_to_consensus_entries(&tx).len();
        if used_entries.saturating_add(entry_count) > max_entries {
            continue;
        }

        // Commit simulation
        for (pk, d) in deltas {
            let cur = *balances.get(&pk).unwrap_or(&0);
            balances.insert(pk, cur.saturating_add(d));
        }
        nonces.insert(sender, tx.core.nonce);
        used_entries += entry_count;
        out.push(tx);
    }

    out
}

fn mempool_txid(tx: &catalyst_core::protocol::Transaction) -> Option<[u8; 32]> {
    catalyst_core::protocol::tx_id_v2(tx).ok()
}

fn mempool_tx_key(txid: &[u8; 32]) -> String {
    format!("mempool:tx:{}", hex_encode(txid))
}

fn tx_raw_key(txid: &[u8; 32]) -> String {
    format!("tx:raw:{}", hex_encode(txid))
}

fn tx_meta_key(txid: &[u8; 32]) -> String {
    format!("tx:meta:{}", hex_encode(txid))
}

fn cycle_txids_key(cycle: u64) -> String {
    format!("tx:cycle:{}:txids", cycle)
}

fn cycle_by_lsu_hash_key(lsu_hash: &[u8; 32]) -> String {
    format!("consensus:cycle_by_lsu_hash:{}", hex_encode(lsu_hash))
}

async fn load_mempool_txids(store: &StorageManager) -> Vec<[u8; 32]> {
    let Some(bytes) = store.get_metadata("mempool:txids").await.ok().flatten() else {
        return Vec::new();
    };
    bincode::deserialize::<Vec<[u8; 32]>>(&bytes).unwrap_or_default()
}

async fn save_mempool_txids(store: &StorageManager, mut ids: Vec<[u8; 32]>) {
    ids.sort();
    ids.dedup();
    if let Ok(bytes) = bincode::serialize(&ids) {
        let _ = store.set_metadata("mempool:txids", &bytes).await;
    }
}

async fn persist_mempool_tx(store: &StorageManager, tx: &catalyst_core::protocol::Transaction) {
    let Some(txid) = mempool_txid(tx) else { return };
    let Ok(bytes) = bincode::serialize(tx) else { return };

    // 1) Persist in the mempool namespace (pruned later).
    let _ = store.set_metadata(&mempool_tx_key(&txid), &bytes).await;
    // 2) Persist in the tx history namespace (NOT pruned).
    let _ = store.set_metadata(&tx_raw_key(&txid), &bytes).await;

    // 3) Persist tx meta for receipt-like RPCs (best-effort, idempotent).
    //    If meta already exists with a later status, do not regress it.
    let now_ms = current_timestamp_ms();
    let existing = store.get_metadata(&tx_meta_key(&txid)).await.ok().flatten();
    let mut meta = existing
        .as_deref()
        .and_then(|b| bincode::deserialize::<catalyst_core::protocol::TxMeta>(b).ok())
        .unwrap_or_else(|| {
            catalyst_core::protocol::TxMeta {
                tx_id: txid,
                status: catalyst_core::protocol::TxStatus::Pending,
                received_at_ms: now_ms,
                sender: tx_sender_pubkey(tx),
                nonce: tx.core.nonce,
                fees: tx.core.fees,
                selected_cycle: None,
                applied_cycle: None,
                applied_lsu_hash: None,
                applied_state_root: None,
                applied_success: None,
                applied_error: None,
                evm_gas_used: None,
                evm_return: None,
            }
        });
    // Never regress terminal-ish states
    if matches!(meta.status, catalyst_core::protocol::TxStatus::Applied) {
        // keep
    } else {
        meta.status = catalyst_core::protocol::TxStatus::Pending;
        meta.received_at_ms = meta.received_at_ms.max(now_ms);
        meta.sender = meta.sender.or_else(|| tx_sender_pubkey(tx));
        meta.nonce = meta.nonce.max(tx.core.nonce);
        meta.fees = meta.fees.max(tx.core.fees);
    }
    if let Ok(mbytes) = bincode::serialize(&meta) {
        let _ = store.set_metadata(&tx_meta_key(&txid), &mbytes).await;
    }

    let mut ids = load_mempool_txids(store).await;
    if !ids.iter().any(|x| x == &txid) {
        ids.push(txid);
        save_mempool_txids(store, ids).await;
    }
}

async fn delete_persisted_mempool_tx(store: &StorageManager, txid: &[u8; 32]) {
    let key_bytes = catalyst_utils::state::state_keys::metadata_key(&mempool_tx_key(txid));
    let _ = store.engine().delete("metadata", &key_bytes);
    let mut ids = load_mempool_txids(store).await;
    ids.retain(|x| x != txid);
    save_mempool_txids(store, ids).await;
}

async fn persist_cycle_txids(store: &StorageManager, cycle: u64, mut txids: Vec<[u8; 32]>) {
    txids.sort();
    txids.dedup();
    if let Ok(bytes) = bincode::serialize(&txids) {
        let _ = store.set_metadata(&cycle_txids_key(cycle), &bytes).await;
    }
}

async fn load_cycle_txids(store: &StorageManager, cycle: u64) -> Vec<[u8; 32]> {
    let Some(bytes) = store.get_metadata(&cycle_txids_key(cycle)).await.ok().flatten() else {
        return Vec::new();
    };
    bincode::deserialize::<Vec<[u8; 32]>>(&bytes).unwrap_or_default()
}

async fn update_tx_meta_status_selected(store: &StorageManager, tx: &catalyst_core::protocol::Transaction, cycle: u64) {
    let Some(txid) = mempool_txid(tx) else { return };
    let key = tx_meta_key(&txid);
    let existing = store.get_metadata(&key).await.ok().flatten();
    let mut meta = existing
        .as_deref()
        .and_then(|b| bincode::deserialize::<catalyst_core::protocol::TxMeta>(b).ok())
        .unwrap_or_else(|| catalyst_core::protocol::TxMeta {
            tx_id: txid,
            status: catalyst_core::protocol::TxStatus::Pending,
            received_at_ms: current_timestamp_ms(),
            sender: tx_sender_pubkey(tx),
            nonce: tx.core.nonce,
            fees: tx.core.fees,
            selected_cycle: None,
            applied_cycle: None,
            applied_lsu_hash: None,
            applied_state_root: None,
            applied_success: None,
            applied_error: None,
            evm_gas_used: None,
            evm_return: None,
        });
    if !matches!(meta.status, catalyst_core::protocol::TxStatus::Applied) {
        meta.status = catalyst_core::protocol::TxStatus::Selected;
        meta.selected_cycle = Some(cycle);
    }
    if let Ok(mbytes) = bincode::serialize(&meta) {
        let _ = store.set_metadata(&key, &mbytes).await;
    }
}

async fn prune_persisted_mempool(store: &StorageManager) {
    let ids = load_mempool_txids(store).await;
    for txid in ids.clone() {
        let key = mempool_tx_key(&txid);
        let Some(bytes) = store.get_metadata(&key).await.ok().flatten() else {
            delete_persisted_mempool_tx(store, &txid).await;
            continue;
        };
        let Ok(tx) = bincode::deserialize::<catalyst_core::protocol::Transaction>(&bytes) else {
            delete_persisted_mempool_tx(store, &txid).await;
            continue;
        };

        // Drop invalid or already-applied txs.
        if tx.validate_basic().is_err() || !verify_protocol_tx_signature_with_domain(store, &tx).await {
            delete_persisted_mempool_tx(store, &txid).await;
            continue;
        }
        let Some(sender) = tx_sender_pubkey(&tx) else {
            delete_persisted_mempool_tx(store, &txid).await;
            continue;
        };
        let committed = get_nonce_u64(store, &sender).await;
        if tx.core.nonce <= committed {
            delete_persisted_mempool_tx(store, &txid).await;
            continue;
        }
    }
}

async fn rehydrate_mempool_from_storage(store: &StorageManager, mempool: &tokio::sync::RwLock<Mempool>) {
    let mut ids = load_mempool_txids(store).await;
    ids.sort();
    ids.dedup();

    let now_ms = current_timestamp_ms();
    let now_secs = now_ms / 1000;

    for txid in ids {
        let key = mempool_tx_key(&txid);
        let Some(bytes) = store.get_metadata(&key).await.ok().flatten() else {
            continue;
        };
        let Ok(tx) = bincode::deserialize::<catalyst_core::protocol::Transaction>(&bytes) else {
            continue;
        };

        // Basic gate: must still validate/signature-check.
        if tx.validate_basic().is_err() || !verify_protocol_tx_signature_with_domain(store, &tx).await {
            delete_persisted_mempool_tx(store, &txid).await;
            continue;
        }
        if tx.core.lock_time as u64 > now_secs {
            continue;
        }

        // Enforce sequential nonce at load time.
        let Some(sender_pk) = tx_sender_pubkey(&tx) else {
            delete_persisted_mempool_tx(store, &txid).await;
            continue;
        };
        let pending_max = {
            let mp = mempool.read().await;
            mp.max_nonce_for_sender(&sender_pk)
        };
        let expected = tx_nonce_expected_next(store, &sender_pk, pending_max).await;
        if tx.core.nonce != expected {
            // If nonce already applied, prune; otherwise keep in DB but don't load yet.
            let committed = get_nonce_u64(store, &sender_pk).await;
            if tx.core.nonce <= committed {
                delete_persisted_mempool_tx(store, &txid).await;
            }
            continue;
        }

        if let Ok(msg) = ProtocolTxGossip::new(tx, now_ms) {
            let mut mp = mempool.write().await;
            let _ = mp.insert_protocol(msg, now_secs);
        }
    }
}

async fn rebroadcast_persisted_mempool(
    store: &StorageManager,
    network: &P2pService,
) {
    // Broadcast in deterministic txid order.
    let mut ids = load_mempool_txids(store).await;
    ids.sort();
    ids.dedup();

    let now_ms = current_timestamp_ms();
    let now_secs = now_ms / 1000;

    for txid in ids {
        let key = mempool_tx_key(&txid);
        let Some(bytes) = store.get_metadata(&key).await.ok().flatten() else {
            continue;
        };
        let Ok(tx) = bincode::deserialize::<catalyst_core::protocol::Transaction>(&bytes) else {
            continue;
        };
        // Only broadcast txs that still look valid (cheap checks).
        if tx.validate_basic().is_err() || !verify_protocol_tx_signature_with_domain(store, &tx).await {
            continue;
        }
        if tx.core.lock_time as u64 > now_secs {
            continue;
        }
        if let Some(sender) = tx_sender_pubkey(&tx) {
            let committed = get_nonce_u64(store, &sender).await;
            if tx.core.nonce <= committed {
                continue;
            }
        }
        if let Ok(msg) = ProtocolTxGossip::new(tx, now_ms) {
            if let Ok(env) = MessageEnvelope::from_message(&msg, "rebroadcast".to_string(), None) {
                let _ = network.broadcast_envelope(&env).await;
            }
        }
    }
}

async fn apply_lsu_to_storage(
    store: &StorageManager,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
) -> Result<[u8; 32]> {
    // Idempotence: only apply if this LSU advances the applied head.
    let already = store
        .get_metadata("consensus:last_applied_cycle")
        .await
        .ok()
        .flatten()
        .and_then(|b| {
            if b.len() != 8 {
                return None;
            }
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&b);
            Some(u64::from_le_bytes(arr))
        })
        .unwrap_or(0);

    if lsu.cycle_number <= already {
        // Return current state root (best-effort).
        let root = store.get_state_root().unwrap_or([0u8; 32]);
        return Ok(root);
    }

    #[derive(Clone, Debug, Default)]
    struct ApplyOutcome {
        success: bool,
        gas_used: Option<u64>,
        return_data: Option<Vec<u8>>,
        error: Option<String>,
    }

    let mut outcome_by_sig: std::collections::HashMap<Vec<u8>, ApplyOutcome> = std::collections::HashMap::new();
    let mut executed_sigs: std::collections::HashSet<Vec<u8>> = std::collections::HashSet::new();

    let chain_id = load_chain_id_u64(store).await;

    // Apply balance deltas from the LSU's ordered transaction entries.
    // Also apply worker registry markers into state under `workers:<pubkey>`.
    for e in &lsu.partial_update.transaction_entries {
        if is_evm_marker(&e.signature) {
            info!(
                "EVM tx marker detected in LSU cycle={} pk={} sig_len={}",
                lsu.cycle_number,
                hex_encode(&e.public_key),
                e.signature.len()
            );
        }
        if is_worker_reg_marker(&e.signature) {
            // Register worker (idempotent).
            let k = worker_key_for_pubkey(&e.public_key);
            let _ = store.set_state(&k, vec![1u8]).await;
            outcome_by_sig.entry(e.signature.clone()).or_insert(ApplyOutcome {
                success: true,
                ..Default::default()
            });
            continue;
        }
        if is_evm_marker(&e.signature) {
            // Execute EVM once per tx boundary (same signature bytes).
            if !executed_sigs.contains(&e.signature) {
                executed_sigs.insert(e.signature.clone());
                if let Some((payload, _sig64)) = decode_evm_marker(&e.signature) {
                    let from20 = pubkey_to_evm_addr20(&e.public_key);
                    let from = EvmAddress::from_slice(&from20);
                    let gas_limit = DEFAULT_EVM_GAS_LIMIT.max(21_000);
                    match payload.kind {
                        EvmTxKind::Deploy { bytecode } => {
                            match execute_deploy_and_persist(store, from, chain_id, payload.nonce, bytecode, gas_limit).await {
                                Ok((created, info, _persisted)) => {
                                    info!("EVM deploy applied addr=0x{} ok={} gas_used={}", hex::encode(created.as_slice()), info.success, info.gas_used);
                                    outcome_by_sig.insert(
                                        e.signature.clone(),
                                        ApplyOutcome {
                                            success: info.success,
                                            gas_used: Some(info.gas_used),
                                            return_data: Some(info.return_data.to_vec()),
                                            error: info.error.clone(),
                                        },
                                    );
                                }
                                Err(err) => {
                                    tracing::warn!("EVM deploy failed: {err}");
                                    outcome_by_sig.insert(
                                        e.signature.clone(),
                                        ApplyOutcome {
                                            success: false,
                                            error: Some(err.to_string()),
                                            ..Default::default()
                                        },
                                    );
                                }
                            }
                        }
                        EvmTxKind::Call { to, input } => {
                            let to_addr = EvmAddress::from_slice(&to);
                            match execute_call_and_persist(store, from, to_addr, chain_id, payload.nonce, input, gas_limit).await {
                                Ok((info, _persisted)) => {
                                    info!(
                                        "EVM call applied to=0x{} ok={} gas_used={} ret_len={}",
                                        hex::encode(to_addr.as_slice()),
                                        info.success,
                                        info.gas_used,
                                        info.return_data.len()
                                    );
                                    outcome_by_sig.insert(
                                        e.signature.clone(),
                                        ApplyOutcome {
                                            success: info.success,
                                            gas_used: Some(info.gas_used),
                                            return_data: Some(info.return_data.to_vec()),
                                            error: info.error.clone(),
                                        },
                                    );
                                }
                                Err(err) => {
                                    tracing::warn!("EVM call failed: {err}");
                                    outcome_by_sig.insert(
                                        e.signature.clone(),
                                        ApplyOutcome {
                                            success: false,
                                            error: Some(err.to_string()),
                                            ..Default::default()
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }
            continue;
        }
        let k = balance_key_for_pubkey(&e.public_key);
        let cur = store
            .get_state(&k)
            .await
            .ok()
            .flatten()
            .and_then(|b| decode_i64(&b))
            .unwrap_or(0);

        let next = cur.saturating_add(e.amount);
        store.set_state(&k, next.to_le_bytes().to_vec()).await?;

        // Default: if it made it into an applied LSU, treat as successful (best-effort).
        outcome_by_sig.entry(e.signature.clone()).or_insert(ApplyOutcome {
            success: true,
            ..Default::default()
        });
    }

    // Update nonces: group by signature bytes (our current "tx boundary" marker).
    use std::collections::BTreeMap;
    let mut by_sig: BTreeMap<Vec<u8>, Vec<&catalyst_consensus::types::TransactionEntry>> = BTreeMap::new();
    for e in &lsu.partial_update.transaction_entries {
        by_sig.entry(e.signature.clone()).or_default().push(e);
    }
    for (_sig, entries) in by_sig {
        // Sender selection:
        // - transfers: single pubkey with negative amount
        // - worker registration: first marker entry pubkey
        let mut sender: Option<[u8; 32]> = None;
        for e in &entries {
            if e.amount < 0 {
                match sender {
                    None => sender = Some(e.public_key),
                    Some(pk) if pk == e.public_key => {}
                    Some(_) => {
                        sender = None;
                        break;
                    }
                }
            }
        }
        if sender.is_none() {
            // WorkerRegistration marker tx: pick the marker entry pubkey.
            if let Some(e0) = entries.iter().find(|e| is_worker_reg_marker(&e.signature)) {
                sender = Some(e0.public_key);
            }
        }
        if sender.is_none() {
            if let Some(e0) = entries.iter().find(|e| is_evm_marker(&e.signature)) {
                sender = Some(e0.public_key);
            }
        }
        if let Some(pk) = sender {
            let cur = get_nonce_u64(store, &pk).await;
            let _ = set_nonce_u64(store, &pk, cur.saturating_add(1)).await;
        }
    }

    // Flush + compute a state root that commits the applied balances.
    let state_root = store.commit().await?;
    let lsu_hash = hash_data(lsu).unwrap_or([0u8; 32]);

    // Update per-tx receipts for this cycle (best-effort): mark Applied + attach execution outcomes.
    let cycle = lsu.cycle_number;
    let txids = load_cycle_txids(store, cycle).await;
    for txid in txids {
        let key = tx_meta_key(&txid);
        let existing = store.get_metadata(&key).await.ok().flatten();
        let mut meta = existing
            .as_deref()
            .and_then(|b| bincode::deserialize::<catalyst_core::protocol::TxMeta>(b).ok())
            .unwrap_or_else(|| catalyst_core::protocol::TxMeta {
                tx_id: txid,
                status: catalyst_core::protocol::TxStatus::Pending,
                received_at_ms: current_timestamp_ms(),
                sender: None,
                nonce: 0,
                fees: 0,
                selected_cycle: Some(cycle),
                applied_cycle: None,
                applied_lsu_hash: None,
                applied_state_root: None,
            applied_success: None,
            applied_error: None,
            evm_gas_used: None,
            evm_return: None,
            });
        meta.status = catalyst_core::protocol::TxStatus::Applied;
        meta.applied_cycle = Some(cycle);
        meta.applied_lsu_hash = Some(lsu_hash);
        meta.applied_state_root = Some(state_root);

        // Attach per-tx outcome using the tx boundary signature bytes.
        if let Some(bytes) = store.get_metadata(&tx_raw_key(&txid)).await.ok().flatten() {
            if let Ok(tx) = bincode::deserialize::<catalyst_core::protocol::Transaction>(&bytes) {
                let sig = tx_boundary_signature(&tx);
                if let Some(o) = outcome_by_sig.get(&sig) {
                    meta.applied_success = Some(o.success);
                    meta.applied_error = o.error.clone();
                    meta.evm_gas_used = o.gas_used;
                    meta.evm_return = o.return_data.clone();
                } else {
                    meta.applied_success = Some(true);
                }
            } else {
                meta.applied_success = Some(true);
            }
        } else {
            meta.applied_success = Some(true);
        }

        if let Ok(mbytes) = bincode::serialize(&meta) {
            let _ = store.set_metadata(&key, &mbytes).await;
        }
    }

    // Persist the applied head.
    let _ = store
        .set_metadata("consensus:last_applied_cycle", &lsu.cycle_number.to_le_bytes())
        .await;
    let _ = store
        .set_metadata("consensus:last_applied_lsu_hash", &lsu_hash)
        .await;
    let _ = store
        .set_metadata("consensus:last_applied_state_root", &state_root)
        .await;

    info!(
        "Applied LSU to storage cycle={} state_root={}",
        lsu.cycle_number,
        hex_encode(&state_root)
    );

    // Prune persisted mempool against updated nonces.
    prune_persisted_mempool(store).await;

    // Opportunistic disk-bounding: prune old historical metadata (if enabled).
    crate::pruning::maybe_prune_history(store).await;

    Ok(state_root)
}

async fn apply_lsu_with_root_check(
    store: &StorageManager,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    expected_state_root: [u8; 32],
) -> Result<bool> {
    let snap = format!("verify_lsu_{}_{}", lsu.cycle_number, current_timestamp_ms());
    let _ = store
        .create_snapshot(&snap)
        .await
        .map_err(|e| anyhow::anyhow!("snapshot create failed: {e}"))?;

    let got = apply_lsu_to_storage(store, lsu).await?;
    if got == expected_state_root {
        return Ok(true);
    }

    // Revert
    let _ = store
        .load_snapshot(&snap)
        .await
        .map_err(|e| anyhow::anyhow!("snapshot revert failed: {e}"))?;
    Ok(false)
}

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

    /// Optional JSON-RPC server handle (when enabled)
    rpc_handle: Option<jsonrpsee::server::ServerHandle>,
}

impl CatalystNode {
    /// Create a new Catalyst node.
    pub async fn new(config: NodeConfig, generate_txs: bool, tx_interval_ms: u64) -> Result<Self> {
        info!("Initializing Catalyst node");

        // Node ID is the public key derived from the node identity key file.
        let node_sk = crate::identity::load_or_generate_private_key(
            &config.node.private_key_file,
            config.node.auto_generate_identity,
        )?;
        let node_id: NodeId = crate::identity::public_key_bytes(&node_sk);
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
            rpc_handle: None,
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

        // Producer ID used in consensus messages. Use a stable, protocol-shaped identifier:
        // hex(public key bytes), so all nodes can deterministically refer to each other.
        let public_key = self.node_id;
        let producer_id = hex_encode(&public_key);

        let engine_config = ConsensusEngineConfig {
            cycle_duration_ms: (self.config.consensus.cycle_duration_seconds as u64) * 1000,
            construction_phase_ms: (self.config.consensus.phase_timeouts.construction_timeout as u64) * 1000,
            campaigning_phase_ms: (self.config.consensus.phase_timeouts.campaigning_timeout as u64) * 1000,
            voting_phase_ms: (self.config.consensus.phase_timeouts.voting_timeout as u64) * 1000,
            synchronization_phase_ms: (self.config.consensus.phase_timeouts.synchronization_timeout as u64) * 1000,
            freeze_window_ms: (self.config.consensus.freeze_time_seconds as u64) * 1000,
            // Minimum producers required for consensus (used for majority thresholding).
            min_producers: self.config.consensus.min_producer_count as usize,
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
        net_cfg.peer.max_peers = self.config.network.max_peers as usize;
        net_cfg.peer.min_peers = self.config.network.min_peers as usize;

        // Wire safety limits from node config.
        net_cfg.safety_limits.max_gossip_message_bytes = self.config.network.safety_limits.max_gossip_message_bytes;
        net_cfg.safety_limits.per_peer_max_msgs_per_sec = self.config.network.safety_limits.per_peer_max_msgs_per_sec;
        net_cfg.safety_limits.per_peer_max_bytes_per_sec = self.config.network.safety_limits.per_peer_max_bytes_per_sec;
        net_cfg.safety_limits.max_tcp_frame_bytes = self.config.network.safety_limits.max_tcp_frame_bytes;
        net_cfg.safety_limits.per_conn_max_msgs_per_sec = self.config.network.safety_limits.per_conn_max_msgs_per_sec;
        net_cfg.safety_limits.per_conn_max_bytes_per_sec = self.config.network.safety_limits.per_conn_max_bytes_per_sec;
        net_cfg.safety_limits.max_hops = self.config.network.safety_limits.max_hops;
        net_cfg.safety_limits.dedup_cache_max_entries = self.config.network.safety_limits.dedup_cache_max_entries;
        net_cfg.safety_limits.dial_jitter_max_ms = self.config.network.safety_limits.dial_jitter_max_ms;
        net_cfg.safety_limits.dial_backoff_max_ms = self.config.network.safety_limits.dial_backoff_max_ms;
        net_cfg.peer.bootstrap_peers = Vec::new();

        let network = Arc::new(P2pService::new(net_cfg).await?);
        network.start().await?;

        // Dial bootstrap peers from CLI config + optional DNS seeds.
        let peers_static = self.config.network.bootstrap_peers.clone();
        let seeds = self.config.network.dns_seeds.clone();

        let peers_from_dns: Vec<String> = resolve_dns_seeds_to_bootstrap_multiaddrs(&seeds, 30333)
            .await
            .into_iter()
            .map(|m| m.to_string())
            .collect();

        let mut all_peers: Vec<String> = Vec::new();
        all_peers.extend(peers_static.clone());
        all_peers.extend(peers_from_dns.clone());
        all_peers.sort();
        all_peers.dedup();

        if !seeds.is_empty() {
            info!(
                "dns_seeds={} resolved_bootstrap_peers={}",
                seeds.len(),
                peers_from_dns.len()
            );
        }

        for peer in &all_peers {
            if let Ok(ma) = peer.parse::<Multiaddr>() {
                let _ = network.connect_multiaddr(&ma).await;
            }
        }

        // Keep trying to connect to bootstrap peers. Use per-peer exponential backoff with jitter
        // so dead peers don't cause tight retry loops.
        if !all_peers.is_empty() || !seeds.is_empty() {
            let net = network.clone();
            let mut shutdown_rx2 = shutdown_rx.clone();
            let handle = tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_millis(500));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                let mut last_resolve = Instant::now() - std::time::Duration::from_secs(3600);
                let mut peers_cached: Vec<String> = peers_from_dns.clone();

                let mut dial: std::collections::HashMap<String, DialState> = std::collections::HashMap::new();

                let success_retry = std::time::Duration::from_secs(30);
                let backoff_min = std::time::Duration::from_secs(1);
                let backoff_max = std::time::Duration::from_secs(60);

                loop {
                    tokio::select! {
                        _ = tick.tick() => {},
                        _ = shutdown_rx2.changed() => {},
                    }
                    if *shutdown_rx2.borrow() {
                        break;
                    }

                    // Refresh DNS seeds periodically so IPs can rotate without restarts.
                    if !seeds.is_empty() && last_resolve.elapsed() >= std::time::Duration::from_secs(60) {
                        let mut out: Vec<String> = resolve_dns_seeds_to_bootstrap_multiaddrs(&seeds, 30333)
                            .await
                            .into_iter()
                            .map(|m| m.to_string())
                            .collect();
                        out.sort();
                        out.dedup();
                        peers_cached = out;
                        last_resolve = Instant::now();
                    }

                    // Current desired peer set (static + dns).
                    let mut peers: Vec<String> = Vec::new();
                    peers.extend(peers_static.iter().cloned());
                    peers.extend(peers_cached.iter().cloned());
                    peers.sort();
                    peers.dedup();

                    let now = Instant::now();

                    // Ensure dial state exists for all peers.
                    for p in &peers {
                        dial.entry(p.clone()).or_insert(DialState {
                            next_at: now,
                            backoff: backoff_min,
                        });
                    }

                    // Limit dial attempts per tick to avoid long stalls if peer list grows.
                    let mut attempts_left = 4usize;
                    for p in peers {
                        if attempts_left == 0 {
                            break;
                        }
                        let Some(st) = dial.get_mut(&p) else { continue };
                        if now < st.next_at {
                            continue;
                        }
                        attempts_left = attempts_left.saturating_sub(1);

                        if let Ok(ma) = p.parse::<Multiaddr>() {
                            match net.connect_multiaddr(&ma).await {
                                Ok(_) => {
                                    st.backoff = success_retry;
                                    st.next_at = now + success_retry + std::time::Duration::from_millis(jitter_ms(500));
                                }
                                Err(_) => {
                                    let next = (st.backoff.as_secs().saturating_mul(2)).max(backoff_min.as_secs());
                                    st.backoff = std::time::Duration::from_secs(next).min(backoff_max);
                                    st.next_at = now + st.backoff + std::time::Duration::from_millis(jitter_ms(250));
                                }
                            }
                        } else {
                            // Bad peer string; backoff it.
                            st.backoff = backoff_max;
                            st.next_at = now + backoff_max;
                        }
                    }
                }
            });
            self.network_tasks.push(handle);
        }

        // Inbound/outbound envelope channels used by consensus engine.
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<MessageEnvelope>();
        let (in_tx, in_rx) = tokio::sync::mpsc::unbounded_channel::<MessageEnvelope>();
        consensus.set_network_sender(out_tx);
        consensus.set_network_receiver(in_rx);

        // Deterministic validator worker pool (protocol input).
        let validator_worker_ids_hex = self.config.consensus.validator_worker_ids.clone();

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

        // Persistent storage (RocksDB) for consensus chain state.
        // Even when "storage provisioning" is disabled, validators still benefit from persisting
        // the latest LSU so restarts keep the same seed / last known chain state.
        let storage = if self.config.validator || self.config.storage.enabled {
            let mut cfg = StorageConfigLib::default();
            cfg.data_dir = self.config.storage.data_dir.clone();
            cfg.max_open_files = self.config.storage.max_open_files;
            cfg.write_buffer_size = (self.config.storage.write_buffer_size_mb as usize) * 1024 * 1024;
            cfg.block_cache_size = (self.config.storage.cache_size_mb as usize) * 1024 * 1024;
            cfg.compression_enabled = self.config.storage.compression_enabled;
            Some(Arc::new(
                StorageManager::new(cfg)
                    .await
                    .map_err(|e| anyhow::anyhow!("storage init failed: {e}"))?,
            ))
        } else {
            None
        };

        // Ensure chain identity + genesis are initialized (one-time, idempotent).
        if let Some(store) = &storage {
            ensure_chain_identity_and_genesis(store.as_ref(), &self.config.protocol)
                .await
                .map_err(|e| anyhow::anyhow!("genesis/identity initialization failed: {e}"))?;
        }

        // Persist storage pruning knobs into DB metadata so apply paths can enforce bounded disk
        // growth without needing to thread config through every callsite.
        if let Some(store) = &storage {
            let enabled: u8 = if self.config.storage.history_prune_enabled { 1 } else { 0 };
            let _ = store
                .set_metadata("storage:history_prune_enabled", &[enabled])
                .await;
            let _ = store
                .set_metadata(
                    "storage:history_keep_cycles",
                    &self.config.storage.history_keep_cycles.to_le_bytes(),
                )
                .await;
            let _ = store
                .set_metadata(
                    "storage:history_prune_interval_seconds",
                    &self.config
                        .storage
                        .history_prune_interval_seconds
                        .to_le_bytes(),
                )
                .await;
            let _ = store
                .set_metadata(
                    "storage:history_prune_batch_cycles",
                    &self.config.storage.history_prune_batch_cycles.to_le_bytes(),
                )
                .await;
        }

        // Auto-register as a worker (on-chain) for validator nodes.
        if self.config.validator {
            if let Some(store) = &storage {
                let node_sk = crate::identity::load_or_generate_private_key(
                    &self.config.node.private_key_file,
                    self.config.node.auto_generate_identity,
                )?;
                let node_pk = crate::identity::public_key_bytes(&node_sk);
                let wk = worker_key_for_pubkey(&node_pk);
                let already = store.get_state(&wk).await.ok().flatten().is_some();
                if !already {
                    // Build a WorkerRegistration tx and inject it via the same gossip path.
                    let committed_nonce = get_nonce_u64(store.as_ref(), &node_pk).await;
                    let now_ms = current_timestamp_ms();
                    let now_secs = (now_ms / 1000) as u32;
                    let mut tx = catalyst_core::protocol::Transaction {
                        core: catalyst_core::protocol::TransactionCore {
                            tx_type: catalyst_core::protocol::TransactionType::WorkerRegistration,
                            entries: vec![catalyst_core::protocol::TransactionEntry {
                                public_key: node_pk,
                                amount: catalyst_core::protocol::EntryAmount::NonConfidential(0),
                            }],
                            nonce: committed_nonce.saturating_add(1),
                            lock_time: now_secs,
                            fees: 0,
                            data: Vec::new(),
                        },
                        signature_scheme: catalyst_core::protocol::sig_scheme::SCHNORR_V1,
                        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
                        sender_pubkey: None,
                        timestamp: now_ms,
                    };
                    tx.core.fees = catalyst_core::protocol::min_fee(&tx);

                    let chain_id = load_chain_id_u64(store.as_ref()).await;
                    let genesis_hash = load_genesis_hash_32(store.as_ref()).await;
                    let payload = tx
                        .signing_payload_v2(chain_id, genesis_hash)
                        .or_else(|_| tx.signing_payload_v1(chain_id, genesis_hash))
                        .or_else(|_| tx.signing_payload())
                        .map_err(anyhow::Error::msg)?;
                    let scheme = catalyst_crypto::signatures::SignatureScheme::new();
                    let mut rng = rand::rngs::OsRng;
                    let sig: catalyst_crypto::signatures::Signature = scheme.sign(&mut rng, &node_sk, &payload)?;
                    tx.signature = catalyst_core::protocol::AggregatedSignature(sig.to_bytes().to_vec());

                    if let Ok(msg) = ProtocolTxGossip::new(tx, now_ms) {
                        // Insert locally (mempool) so leader can include it.
                        {
                            let mut mp = mempool.write().await;
                            let _ = mp.insert_protocol(msg.clone(), now_ms / 1000);
                        }
                        persist_mempool_tx(store.as_ref(), &msg.tx).await;
                        if let Ok(env) = MessageEnvelope::from_message(&msg, "autoreg".to_string(), None) {
                            let _ = network.broadcast_envelope(&env).await;
                        }
                        info!("Auto-submitted WorkerRegistration for {}", hex_encode(&node_pk));
                    }
                }
            }
        }

        // Rehydrate mempool from persisted txs (best-effort).
        if let Some(store) = &storage {
            rehydrate_mempool_from_storage(store.as_ref(), &mempool).await;
        }

        // --- RPC (HTTP JSON-RPC) ---
        if self.config.rpc.enabled {
            if let Some(store) = storage.clone() {
                // Channel for RPC-submitted transactions.
                let (rpc_tx, mut rpc_rx) =
                    tokio::sync::mpsc::unbounded_channel::<catalyst_core::protocol::Transaction>();

                let bind: std::net::SocketAddr = format!("{}:{}", self.config.rpc.address, self.config.rpc.port)
                    .parse()
                    .map_err(|e| anyhow::anyhow!("invalid rpc bind addr: {e}"))?;
                let handle = catalyst_rpc::start_rpc_http(
                    bind,
                    store.clone(),
                    Some(network.clone()),
                    Some(rpc_tx),
                    self.config.rpc.rate_limit,
                    self.config.rpc.rate_limit,
                )
                    .await
                    .map_err(|e| anyhow::anyhow!("rpc start failed: {e}"))?;
                info!("RPC server listening on http://{}", bind);
                self.rpc_handle = Some(handle);

                // Spawn receiver task: RPC -> mempool + gossip.
                let mempool = mempool.clone();
                let net = network.clone();
                let storage = store.clone();
                let handle = tokio::spawn(async move {
                    while let Some(tx) = rpc_rx.recv().await {
                        let now = current_timestamp_ms();
                        let now_secs = now / 1000;
                        if let Ok(msg) = ProtocolTxGossip::new(tx, now) {
                            if !verify_protocol_tx_signature_with_domain(storage.as_ref(), &msg.tx).await {
                                continue;
                            }
                            // Nonce check (single-sender only).
                            let Some(sender_pk) = msg.sender_pubkey() else { continue };
                            let pending_max = {
                                let mp = mempool.read().await;
                                mp.max_nonce_for_sender(&sender_pk)
                            };
                            let expected =
                                tx_nonce_expected_next(storage.as_ref(), &sender_pk, pending_max).await;
                            if msg.tx.core.nonce != expected {
                                continue;
                            }
                            let entries = msg.to_consensus_entries();
                            if !tx_is_sane(&entries) {
                                continue;
                            }
                            if !tx_is_funded(storage.as_ref(), &entries).await {
                                continue;
                            }
                            {
                                let mut mp = mempool.write().await;
                                let _ = mp.insert_protocol(msg.clone(), now_secs);
                            }
                            // Persist after acceptance.
                            persist_mempool_tx(storage.as_ref(), &msg.tx).await;
                            if let Ok(env) = MessageEnvelope::from_message(&msg, "rpc".to_string(), None) {
                                let _ = net.broadcast_envelope(&env).await;
                            }
                        }
                    }
                });
                self.network_tasks.push(handle);
            } else {
                info!("RPC enabled but storage not initialized; RPC not started");
            }
        }

        // Cycle-scoped tx batches (selected by a deterministic "leader" each cycle).
        let tx_batches: Arc<tokio::sync::RwLock<std::collections::HashMap<u64, Vec<catalyst_consensus::types::TransactionEntry>>>> =
            Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));
        let max_entries_per_cycle_cfg = self.config.consensus.max_transactions_per_block as usize;

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

        // NodeStatus discovery removed: producer/validator set is derived deterministically
        // from `config.consensus.validator_worker_ids`.

        // Periodically rebroadcast persisted mempool txs (deterministic order).
        if let Some(store) = storage.clone() {
            let net = network.clone();
            let mut shutdown_rx2 = shutdown_rx.clone();
            let handle = tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(20));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                // initial burst shortly after startup
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                loop {
                    if *shutdown_rx2.borrow() {
                        break;
                    }
                    rebroadcast_persisted_mempool(store.as_ref(), net.as_ref()).await;
                    tokio::select! {
                        _ = tick.tick() => {}
                        _ = shutdown_rx2.changed() => {}
                    }
                }
            });
            self.network_tasks.push(handle);
        }

        // Inbound: envelopes received from peers → update validator map / handle tx gossip / handle LSU gossip / forward consensus messages.
        {
            let mut events = network.subscribe_events().await;
            let net = network.clone();
            let mempool = mempool.clone();
            let tx_batches = tx_batches.clone();
            let max_entries_per_cycle_cfg = max_entries_per_cycle_cfg;
            let dfs = dfs.clone();
            let storage = storage.clone();
            let producer_id_self = producer_id.clone();
            #[derive(Clone)]
            struct PendingLsuFetch {
                cycle: u64,
                lsu_hash: [u8; 32],
                prev_state_root: [u8; 32],
                expected_state_root: [u8; 32],
                proof_cid: String,
                attempts: u32,
                next_retry_at_ms: u64,
            }
            let pending_lsu_fetch: Arc<
                tokio::sync::RwLock<std::collections::HashMap<String, PendingLsuFetch>>,
            > = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));
            let pending_lsu_fetch_retry = pending_lsu_fetch.clone();
            let pending_lsu_fetch_inbound = pending_lsu_fetch.clone();
            let net_retry = network.clone();
            let dfs_retry = dfs.clone();
            let producer_id_retry = producer_id_self.clone();
            let mut shutdown_rx_retry = shutdown_rx.clone();

            // Retry/backoff loop: if we don't have a CID locally, periodically request it over P2P.
            // This makes sync resilient to packet loss / startup ordering.
            let retry_handle = tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_millis(250));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

                loop {
                    tokio::select! {
                        _ = tick.tick() => {},
                        _ = shutdown_rx_retry.changed() => {
                            if *shutdown_rx_retry.borrow() { break; }
                            continue;
                        }
                    }
                    if *shutdown_rx_retry.borrow() {
                        break;
                    }

                    // Avoid holding the lock across awaits.
                    let now = current_timestamp_ms();
                    let due: Vec<String> = {
                        let m = pending_lsu_fetch_retry.read().await;
                        m.iter()
                            .filter_map(|(cid, info)| {
                                if now >= info.next_retry_at_ms && info.attempts < 20 {
                                    Some(cid.clone())
                                } else {
                                    None
                                }
                            })
                            .collect()
                    };

                    for cid in due {
                        // If it's already in local DFS now, stop retrying.
                        if let Some(dfs) = &dfs_retry {
                            if dfs.has(&cid).await {
                                pending_lsu_fetch_retry.write().await.remove(&cid);
                                continue;
                            }
                        }

                        let delay_ms: u64;
                        {
                            let mut m = pending_lsu_fetch_retry.write().await;
                            let Some(info) = m.get_mut(&cid) else { continue };
                            info.attempts = info.attempts.saturating_add(1);
                            // exponential backoff up to 5s
                            delay_ms = (500u64.saturating_mul(2u64.saturating_pow(info.attempts.saturating_sub(1))))
                                .min(5000);
                            info.next_retry_at_ms = now.saturating_add(delay_ms);
                        }

                        let req = FileRequestMsg {
                            requester: producer_id_retry.clone(),
                            cid: cid.clone(),
                        };
                        if let Ok(env) = MessageEnvelope::from_message(&req, "file_req_retry".to_string(), None) {
                            let _ = net_retry.broadcast_envelope(&env).await;
                        }
                    }

                    // Drop items that have exceeded max attempts.
                    let mut m = pending_lsu_fetch_retry.write().await;
                    m.retain(|_cid, info| info.attempts < 20);
                }
            });
            self.network_tasks.push(retry_handle);

            let last_lsu: Arc<tokio::sync::RwLock<std::collections::HashMap<u64, catalyst_consensus::types::LedgerStateUpdate>>> =
                Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));
            let relay_cfg = RelayCacheCfg {
                max_entries: self.config.network.relay_cache.max_entries,
                target_entries: self.config.network.relay_cache.target_entries,
                retention_ms: self.config.network.relay_cache.retention_seconds.saturating_mul(1000),
            };
            let relay_cache: Arc<tokio::sync::Mutex<RelayCache>> =
                Arc::new(tokio::sync::Mutex::new(RelayCache::new(relay_cfg)));
            #[derive(Debug, Default)]
            struct CatchupState {
                observed_head_cycle: u64,
                last_req_ms: u64,
            }
            let catchup: Arc<tokio::sync::Mutex<CatchupState>> =
                Arc::new(tokio::sync::Mutex::new(CatchupState::default()));
            // Background: repair "holes" in per-cycle LSU history for RPC/indexers.
            // This can happen if a node applied forward but missed persisting some cycles.
            let net_gap = net.clone();
            let storage_gap = storage.clone();
            let catchup_gap = catchup.clone();
            let producer_id_gap = producer_id_self.clone();
            let mut shutdown_rx_gap = shutdown_rx.clone();
            let gap_handle = tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(10));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        _ = tick.tick() => {},
                        _ = shutdown_rx_gap.changed() => {},
                    }
                    if *shutdown_rx_gap.borrow() {
                        break;
                    }
                    let Some(store) = &storage_gap else { continue };

                    let head = local_applied_cycle(store.as_ref()).await;
                    if head == 0 {
                        continue;
                    }

                    // Scan a recent window behind head; request backfill starting at the newest missing cycle.
                    let window: u64 = 2048;
                    let mut missing: Option<u64> = None;
                    let start_scan = head.saturating_sub(window);
                    for cycle in (start_scan..=head).rev() {
                        if store
                            .get_metadata(&format!("consensus:lsu:{}", cycle))
                            .await
                            .ok()
                            .flatten()
                            .is_none()
                        {
                            missing = Some(cycle);
                            break;
                        }
                    }
                    let Some(miss) = missing else { continue };

                    let now_ms = current_timestamp_ms();
                    let mut c = catchup_gap.lock().await;
                    let should = now_ms.saturating_sub(c.last_req_ms) > 2000;
                    if !should {
                        continue;
                    }
                    c.observed_head_cycle = c.observed_head_cycle.max(head);

                    let remaining = head.saturating_sub(miss).saturating_add(1);
                    let count = remaining.min(256) as u32;
                    let req = LsuRangeRequest {
                        requester: producer_id_gap.clone(),
                        start_cycle: miss,
                        count,
                    };
                    if let Ok(env) = MessageEnvelope::from_message(&req, "lsu_range_req_gap".to_string(), None) {
                        let _ = net_gap.broadcast_envelope(&env).await;
                        c.last_req_ms = now_ms;
                    }
                }
            });
            let handle = tokio::spawn(async move {
                while let Some(ev) = events.recv().await {
                    if let catalyst_network::NetworkEvent::MessageReceived { envelope, .. } = ev {
                        let now_ms = current_timestamp_ms();
                        if envelope.is_expired() {
                            continue;
                        }
                        if envelope.message_type == MessageType::Transaction {
                            // Prefer protocol-shaped txs; fall back to legacy TxGossip.
                            let now_secs = now_ms / 1000;
                            if let Ok(tx) = envelope.extract_message::<ProtocolTxGossip>() {
                                // Nonce check (single-sender only, requires storage).
                                if let Some(store) = &storage {
                                    if !verify_protocol_tx_signature_with_domain(store.as_ref(), &tx.tx).await {
                                        continue;
                                    }
                                    let Some(sender_pk) = tx.sender_pubkey() else { continue };
                                    let pending_max = {
                                        let mp = mempool.read().await;
                                        mp.max_nonce_for_sender(&sender_pk)
                                    };
                                    let expected =
                                        tx_nonce_expected_next(store.as_ref(), &sender_pk, pending_max).await;
                                    if tx.tx.core.nonce != expected {
                                        continue;
                                    }
                                }
                                // Enforce basic sufficient-funds using current storage.
                                if let Some(store) = &storage {
                                    let entries = tx.to_consensus_entries();
                                    if tx_is_sane(&entries) && tx_is_funded(store.as_ref(), &entries).await {
                                        let mut mp = mempool.write().await;
                                        if mp.insert_protocol(tx.clone(), now_secs) {
                                            persist_mempool_tx(store.as_ref(), &tx.tx).await;
                                            // Help multi-hop propagation on WAN: relay when first seen.
                                            let do_relay = {
                                                let mut rc = relay_cache.lock().await;
                                                rc.should_relay(&envelope, now_ms)
                                            };
                                            if do_relay {
                                                let _ = net.broadcast_envelope(&envelope).await;
                                            }
                                        }
                                    }
                                } else {
                                    let mut mp = mempool.write().await;
                                    if mp.insert_protocol(tx, now_secs) {
                                        let do_relay = {
                                            let mut rc = relay_cache.lock().await;
                                            rc.should_relay(&envelope, now_ms)
                                        };
                                        if do_relay {
                                            let _ = net.broadcast_envelope(&envelope).await;
                                        }
                                    }
                                }
                            } else if let Ok(tx) = envelope.extract_message::<TxGossip>() {
                                let mut mp = mempool.write().await;
                                if mp.insert(tx) {
                                    let do_relay = {
                                        let mut rc = relay_cache.lock().await;
                                        rc.should_relay(&envelope, now_ms)
                                    };
                                    if do_relay {
                                        let _ = net.broadcast_envelope(&envelope).await;
                                    }
                                }
                            }
                        } else if envelope.message_type == MessageType::TransactionBatch {
                            // Prefer protocol tx batches (full signed txs), fallback to legacy entry batch.
                            let mut accepted = false;
                            if let Ok(batch) = envelope.extract_message::<ProtocolTxBatch>() {
                                if batch.verify_hash().unwrap_or(false) {
                                    if let Some(store) = &storage {
                                        let valid =
                                            validate_and_select_protocol_txs_for_construction(store.as_ref(), batch.txs, max_entries_per_cycle_cfg).await;
                                        let mut entries: Vec<catalyst_consensus::types::TransactionEntry> = Vec::new();
                                        for tx in &valid {
                                            entries.extend(tx_to_consensus_entries(tx));
                                        }
                                        // Persist cycle txids + mark Selected.
                                        let txids: Vec<[u8; 32]> = valid.iter().filter_map(|t| mempool_txid(t)).collect();
                                        persist_cycle_txids(store.as_ref(), batch.cycle, txids).await;
                                        for tx in &valid {
                                            update_tx_meta_status_selected(store.as_ref(), tx, batch.cycle).await;
                                        }
                                        tx_batches.write().await.insert(batch.cycle, entries);
                                        accepted = true;
                                    }
                                }
                            } else if let Ok(batch) = envelope.extract_message::<TxBatch>() {
                                // Verify hash before accepting.
                                if let Ok(h) = catalyst_consensus::types::hash_data(&batch.entries) {
                                    if h == batch.batch_hash {
                                        tx_batches.write().await.insert(batch.cycle, batch.entries);
                                        accepted = true;
                                    }
                                }
                            }
                            if accepted {
                                let do_relay = {
                                    let mut rc = relay_cache.lock().await;
                                    rc.should_relay(&envelope, now_ms)
                                };
                                if do_relay {
                                    let _ = net.broadcast_envelope(&envelope).await;
                                }
                            }
                        } else if envelope.message_type == MessageType::ConsensusSync {
                            // Prefer CID-based LSU sync; fallback to full LSU gossip.
                            if let Ok(ref_msg) = envelope.extract_message::<LsuCidGossip>() {
                                // Relay LSU references even if we can't fetch/apply locally.
                                let do_relay = {
                                    let mut rc = relay_cache.lock().await;
                                    rc.should_relay(&envelope, now_ms)
                                };
                                if do_relay {
                                    let _ = net.broadcast_envelope(&envelope).await;
                                }
                                // Track observed head and request backfill if we're behind.
                                if let Some(store) = &storage {
                                    let mut c = catchup.lock().await;
                                    c.observed_head_cycle = c.observed_head_cycle.max(ref_msg.cycle);
                                    let local_cycle = local_applied_cycle(store.as_ref()).await;
                                    if local_cycle.saturating_add(1) < c.observed_head_cycle {
                                        let should = now_ms.saturating_sub(c.last_req_ms) > 2000;
                                        if should {
                                            let start = local_cycle.saturating_add(1);
                                            let remaining = c.observed_head_cycle.saturating_sub(start).saturating_add(1);
                                            let count = remaining.min(256) as u32;
                                            let req = LsuRangeRequest {
                                                requester: producer_id_self.clone(),
                                                start_cycle: start,
                                                count,
                                            };
                                            if let Ok(env) = MessageEnvelope::from_message(&req, "lsu_range_req".to_string(), None) {
                                                let _ = net.broadcast_envelope(&env).await;
                                                c.last_req_ms = now_ms;
                                            }
                                        }
                                    }
                                }
                                if let Some(dfs) = &dfs {
                                    if let Ok(_cid) = ContentId::from_string(&ref_msg.cid) {
                                        if let Ok(bytes) = dfs.get(&ref_msg.cid).await {
                                            if let Ok(lsu) =
                                                catalyst_consensus::types::LedgerStateUpdate::deserialize(&bytes)
                                            {
                                                if let Ok(h) = catalyst_consensus::types::hash_data(&lsu) {
                                                    if h == ref_msg.lsu_hash {
                                                        if let Some(store) = &storage {
                                                            // Always persist per-cycle LSU history for RPC/indexers,
                                                            // even if we cannot apply (e.g. filling historical gaps).
                                                            persist_lsu_history(
                                                                store.as_ref(),
                                                                ref_msg.cycle,
                                                                &bytes,
                                                                &ref_msg.lsu_hash,
                                                                &ref_msg.cid,
                                                                &ref_msg.state_root,
                                                            )
                                                            .await;

                                                            // Ensure we're applying onto the expected previous root.
                                                            //
                                                            // IMPORTANT: do not apply onto an "unknown" root. The only
                                                            // safe bypass is the explicit bootstrap case where both
                                                            // local and expected prev roots are zero.
                                                            let local_prev = local_applied_state_root(store.as_ref()).await;
                                                            if local_prev != ref_msg.prev_state_root {
                                                                if local_prev == [0u8; 32] && ref_msg.prev_state_root == [0u8; 32] {
                                                                    // bootstrap ok
                                                                } else {
                                                                    tracing::warn!(
                                                                        "Skipping LSU cycle={} cid={} (prev_root mismatch local={} expected={})",
                                                                        ref_msg.cycle,
                                                                        ref_msg.cid,
                                                                        hex_encode(&local_prev),
                                                                        hex_encode(&ref_msg.prev_state_root)
                                                                    );
                                                                    // Request backfill starting from local+1 to observed head.
                                                                    let mut c = catchup.lock().await;
                                                                    c.observed_head_cycle = c.observed_head_cycle.max(ref_msg.cycle);
                                                                    let local_cycle = local_applied_cycle(store.as_ref()).await;
                                                                    if local_cycle.saturating_add(1) < c.observed_head_cycle
                                                                        && now_ms.saturating_sub(c.last_req_ms) > 2000
                                                                    {
                                                                        let start = local_cycle.saturating_add(1);
                                                                        let remaining = c.observed_head_cycle.saturating_sub(start).saturating_add(1);
                                                                        let count = remaining.min(256) as u32;
                                                                        let req = LsuRangeRequest {
                                                                            requester: producer_id_self.clone(),
                                                                            start_cycle: start,
                                                                            count,
                                                                        };
                                                                        if let Ok(env) = MessageEnvelope::from_message(&req, "lsu_range_req".to_string(), None) {
                                                                            let _ = net.broadcast_envelope(&env).await;
                                                                            c.last_req_ms = now_ms;
                                                                        }
                                                                    }
                                                                    continue;
                                                                }
                                                            }

                                                            // Proof-driven path: if we have a proof bundle CID locally, verify it and apply without recomputing root.
                                                            let mut applied = false;
                                                            if !ref_msg.proof_cid.is_empty() {
                                                                if let Ok(pb) = dfs.get(&ref_msg.proof_cid).await {
                                                                    if let Ok(bundle) = bincode::deserialize::<StateProofBundle>(&pb) {
                                                                        if bundle.prev_state_root == ref_msg.prev_state_root
                                                                            && bundle.new_state_root == ref_msg.state_root
                                                                            && bundle.lsu_hash == ref_msg.lsu_hash
                                                                            && verify_state_transition_bundle(&lsu, &bundle)
                                                                        {
                                                                            if apply_lsu_to_storage_without_root_check(
                                                                                store.as_ref(),
                                                                                &lsu,
                                                                                bundle.new_state_root,
                                                                            )
                                                                            .await
                                                                            .is_ok()
                                                                            {
                                                                                applied = true;
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }

                                                            // Fallback: apply-then-check via snapshot/root recompute.
                                                            if !applied {
                                                                match apply_lsu_with_root_check(
                                                                    store.as_ref(),
                                                                    &lsu,
                                                                    ref_msg.state_root,
                                                                )
                                                                .await
                                                                {
                                                                    Ok(true) => applied = true,
                                                                    Ok(false) => {
                                                                        tracing::warn!(
                                                                            "Rejected LSU via CID cycle={} (state_root mismatch) cid={}",
                                                                            ref_msg.cycle,
                                                                            ref_msg.cid
                                                                        );
                                                                        continue;
                                                                    }
                                                                    Err(e) => {
                                                                        tracing::warn!(
                                                                            "Failed to verify/apply LSU via CID cycle={} err={}",
                                                                            ref_msg.cycle,
                                                                            e
                                                                        );
                                                                        continue;
                                                                    }
                                                                }
                                                            }
                                                        }

                                                        last_lsu.write().await.insert(ref_msg.cycle, lsu);
                                                        info!(
                                                            "Synced LSU via CID cycle={} cid={}",
                                                            ref_msg.cycle, ref_msg.cid
                                                        );

                                                        // Persist latest observed LSU.
                                                        if let Some(store) = &storage {
                                                            // Per-cycle history
                                                            let _ = store
                                                                .set_metadata(
                                                                    &format!("consensus:lsu:{}", ref_msg.cycle),
                                                                    &bytes,
                                                                )
                                                                .await;
                                                            let _ = store
                                                                .set_metadata(
                                                                    &format!("consensus:lsu_hash:{}", ref_msg.cycle),
                                                                    &ref_msg.lsu_hash,
                                                                )
                                                                .await;
                                                            let _ = store
                                                                .set_metadata(
                                                                    &cycle_by_lsu_hash_key(&ref_msg.lsu_hash),
                                                                    &ref_msg.cycle.to_le_bytes(),
                                                                )
                                                                .await;
                                                            let _ = store
                                                                .set_metadata(
                                                                    &format!("consensus:lsu_cid:{}", ref_msg.cycle),
                                                                    ref_msg.cid.as_bytes(),
                                                                )
                                                                .await;
                                                            let _ = store
                                                                .set_metadata(
                                                                    &format!("consensus:lsu_state_root:{}", ref_msg.cycle),
                                                                    &ref_msg.state_root,
                                                                )
                                                                .await;

                                                            let _ = store
                                                                .set_metadata("consensus:last_lsu", &bytes)
                                                                .await;
                                                            let _ = store
                                                                .set_metadata("consensus:last_lsu_hash", &ref_msg.lsu_hash)
                                                                .await;
                                                            let _ = store
                                                                .set_metadata("consensus:last_lsu_cycle", &ref_msg.cycle.to_le_bytes())
                                                                .await;
                                                            let _ = store
                                                                .set_metadata("consensus:last_lsu_cid", ref_msg.cid.as_bytes())
                                                                .await;
                                                            let _ = store
                                                                .set_metadata("consensus:last_lsu_state_root", &ref_msg.state_root)
                                                                .await;
                                                        }
                                                    }
                                                }
                                            }
                                        } else {
                                            // Not found locally: request bytes over P2P using FileRequest/FileResponse.
                                            pending_lsu_fetch_inbound.write().await.insert(
                                                ref_msg.cid.clone(),
                                                PendingLsuFetch {
                                                    cycle: ref_msg.cycle,
                                                    lsu_hash: ref_msg.lsu_hash,
                                                    prev_state_root: ref_msg.prev_state_root,
                                                    expected_state_root: ref_msg.state_root,
                                                    proof_cid: ref_msg.proof_cid.clone(),
                                                    attempts: 0,
                                                    next_retry_at_ms: 0,
                                                },
                                            );
                                            let req = FileRequestMsg {
                                                requester: producer_id_self.clone(),
                                                cid: ref_msg.cid.clone(),
                                            };
                                            if let Ok(env) =
                                                MessageEnvelope::from_message(&req, "file_req".to_string(), None)
                                            {
                                                let _ = net.broadcast_envelope(&env).await;
                                            }
                                        }
                                    }
                                }
                            } else if let Ok(lsu_msg) = envelope.extract_message::<LsuGossip>() {
                                if let Ok(h) = catalyst_consensus::types::hash_data(&lsu_msg.lsu) {
                                    if h == lsu_msg.lsu_hash {
                                        let lsu = lsu_msg.lsu;
                                        last_lsu.write().await.insert(lsu_msg.cycle, lsu.clone());

                                        if let Some(store) = &storage {
                                            let _ = apply_lsu_to_storage(store.as_ref(), &lsu).await;

                                            if let Ok(bytes) = lsu.serialize() {
                                                // Per-cycle history
                                                let _ = store
                                                    .set_metadata(
                                                        &format!("consensus:lsu:{}", lsu_msg.cycle),
                                                        &bytes,
                                                    )
                                                    .await;
                                                let _ = store
                                                    .set_metadata(
                                                        &format!("consensus:lsu_hash:{}", lsu_msg.cycle),
                                                        &lsu_msg.lsu_hash,
                                                    )
                                                    .await;
                                                let _ = store
                                                    .set_metadata(
                                                        &cycle_by_lsu_hash_key(&lsu_msg.lsu_hash),
                                                        &lsu_msg.cycle.to_le_bytes(),
                                                    )
                                                    .await;

                                                let _ = store
                                                    .set_metadata("consensus:last_lsu", &bytes)
                                                    .await;
                                                let _ = store
                                                    .set_metadata("consensus:last_lsu_hash", &lsu_msg.lsu_hash)
                                                    .await;
                                                let _ = store
                                                    .set_metadata("consensus:last_lsu_cycle", &lsu_msg.cycle.to_le_bytes())
                                                    .await;
                                            }
                                        }
                                        // Relay only after hash check.
                                        let do_relay = {
                                            let mut rc = relay_cache.lock().await;
                                            rc.should_relay(&envelope, now_ms)
                                        };
                                        if do_relay {
                                            let _ = net.broadcast_envelope(&envelope).await;
                                        }
                                    }
                                }
                            }
                        } else if envelope.message_type == MessageType::StateRequest {
                            if let Ok(req) = envelope.extract_message::<LsuRangeRequest>() {
                                if req.requester == producer_id_self {
                                    continue;
                                }
                                let Some(store) = &storage else {
                                    continue;
                                };
                                let max = req.count.min(256) as u64;
                                let mut refs: Vec<LsuCidGossip> = Vec::new();
                                for i in 0..max {
                                    let cycle = req.start_cycle.saturating_add(i);
                                    let Some(cid) = meta_string(store.as_ref(), &format!("consensus:lsu_cid:{}", cycle)).await else {
                                        break;
                                    };
                                    let Some(lsu_hash) = meta_32(store.as_ref(), &format!("consensus:lsu_hash:{}", cycle)).await else {
                                        break;
                                    };
                                    let Some(state_root) = meta_32(store.as_ref(), &format!("consensus:lsu_state_root:{}", cycle)).await else {
                                        break;
                                    };
                                    let prev_state_root = if cycle == 0 {
                                        [0u8; 32]
                                    } else {
                                        meta_32(store.as_ref(), &format!("consensus:lsu_state_root:{}", cycle.saturating_sub(1)))
                                            .await
                                            .unwrap_or([0u8; 32])
                                    };
                                    refs.push(LsuCidGossip {
                                        cycle,
                                        lsu_hash,
                                        cid,
                                        prev_state_root,
                                        state_root,
                                        proof_cid: String::new(),
                                    });
                                }
                                if refs.is_empty() {
                                    continue;
                                }
                                let resp = LsuRangeResponse {
                                    requester: req.requester,
                                    refs,
                                };
                                if let Ok(env) = MessageEnvelope::from_message(&resp, "lsu_range_resp".to_string(), None) {
                                    let _ = net.broadcast_envelope(&env).await;
                                }
                            }
                        } else if envelope.message_type == MessageType::StateResponse {
                            if let Ok(resp) = envelope.extract_message::<LsuRangeResponse>() {
                                if resp.requester != producer_id_self {
                                    continue;
                                }
                                // Update observed head based on response.
                                if let Some(max_cycle) = resp.refs.iter().map(|r| r.cycle).max() {
                                    let mut c = catchup.lock().await;
                                    c.observed_head_cycle = c.observed_head_cycle.max(max_cycle);
                                }
                                // For each reference, if we don't have bytes locally, request via FileRequest.
                                for r in resp.refs {
                                    if let Some(store) = &storage {
                                        // If we already have the LSU bytes in DB, skip requesting.
                                        if store
                                            .get_metadata(&format!("consensus:lsu:{}", r.cycle))
                                            .await
                                            .ok()
                                            .flatten()
                                            .is_some()
                                        {
                                            continue;
                                        }
                                    }
                                    pending_lsu_fetch_inbound.write().await.insert(
                                        r.cid.clone(),
                                        PendingLsuFetch {
                                            cycle: r.cycle,
                                            lsu_hash: r.lsu_hash,
                                            prev_state_root: r.prev_state_root,
                                            expected_state_root: r.state_root,
                                            proof_cid: r.proof_cid.clone(),
                                            attempts: 0,
                                            next_retry_at_ms: 0,
                                        },
                                    );
                                    let req = FileRequestMsg {
                                        requester: producer_id_self.clone(),
                                        cid: r.cid.clone(),
                                    };
                                    if let Ok(env) = MessageEnvelope::from_message(&req, "file_req_backfill".to_string(), None) {
                                        let _ = net.broadcast_envelope(&env).await;
                                    }
                                }
                            }
                        } else if envelope.message_type == MessageType::FileRequest {
                            // Multi-hop relay is required for WAN topologies where requester isn't directly connected.
                            let do_relay = {
                                let mut rc = relay_cache.lock().await;
                                rc.should_relay(&envelope, now_ms)
                            };
                            if do_relay {
                                let _ = net.broadcast_envelope(&envelope).await;
                            }
                            if let Ok(req) = envelope.extract_message::<FileRequestMsg>() {
                                if let Some(dfs) = &dfs {
                                    if let Ok(bytes) = dfs.get(&req.cid).await {
                                        let resp = FileResponseMsg {
                                            requester: req.requester,
                                            cid: req.cid,
                                            bytes,
                                        };
                                        if let Ok(env) = MessageEnvelope::from_message(
                                            &resp,
                                            "file_resp".to_string(),
                                            None,
                                        ) {
                                            let _ = net.broadcast_envelope(&env).await;
                                        }
                                    }
                                }
                            }
                        } else if envelope.message_type == MessageType::FileResponse {
                            // Relay so the requester can receive even across multiple hops.
                            let do_relay = {
                                let mut rc = relay_cache.lock().await;
                                rc.should_relay(&envelope, now_ms)
                            };
                            if do_relay {
                                let _ = net.broadcast_envelope(&envelope).await;
                            }
                            if let Ok(resp) = envelope.extract_message::<FileResponseMsg>() {
                                if resp.requester != producer_id_self {
                                    continue;
                                }
                                let Some(info) = pending_lsu_fetch_inbound.write().await.remove(&resp.cid) else {
                                    continue;
                                };

                                if let Some(dfs) = &dfs {
                                    let _ = dfs.put(resp.bytes.clone()).await;
                                }

                                if let Ok(lsu) =
                                    catalyst_consensus::types::LedgerStateUpdate::deserialize(&resp.bytes)
                                {
                                    if let Ok(h) = catalyst_consensus::types::hash_data(&lsu) {
                                        if h != info.lsu_hash {
                                            continue;
                                        }
                                        if let Some(store) = &storage {
                                                // Always persist per-cycle LSU history for RPC/indexers (gap repair),
                                                // regardless of whether we can apply it to current state.
                                                persist_lsu_history(
                                                    store.as_ref(),
                                                    info.cycle,
                                                    &resp.bytes,
                                                    &info.lsu_hash,
                                                    &resp.cid,
                                                    &info.expected_state_root,
                                                )
                                                .await;

                                            // Ensure we're applying onto the expected previous root.
                                            //
                                            // IMPORTANT: do not apply onto an "unknown" root. The only
                                            // safe bypass is the explicit bootstrap case where both
                                            // local and expected prev roots are zero.
                                            let local_prev = local_applied_state_root(store.as_ref()).await;
                                            if local_prev != info.prev_state_root {
                                                if local_prev == [0u8; 32] && info.prev_state_root == [0u8; 32] {
                                                    // bootstrap ok
                                                } else {
                                                        // History-only backfill: keep persisted per-cycle metadata but
                                                        // do not attempt to apply onto a different root.
                                                        info!(
                                                            "Backfilled LSU bytes (history only) cycle={} cid={}",
                                                            info.cycle, resp.cid
                                                        );
                                                    continue;
                                                }
                                            }

                                            // Proof-driven path if proof bundle is present locally.
                                            let mut ok = false;
                                            if let Some(dfs) = &dfs {
                                                if !info.proof_cid.is_empty() {
                                                    if let Ok(pb) = dfs.get(&info.proof_cid).await {
                                                    if let Ok(bundle) = bincode::deserialize::<StateProofBundle>(&pb) {
                                                        if bundle.prev_state_root == info.prev_state_root
                                                            && bundle.new_state_root == info.expected_state_root
                                                            && bundle.lsu_hash == info.lsu_hash
                                                            && verify_state_transition_bundle(&lsu, &bundle)
                                                        {
                                                            ok = apply_lsu_to_storage_without_root_check(
                                                                store.as_ref(),
                                                                &lsu,
                                                                bundle.new_state_root,
                                                            )
                                                            .await
                                                            .is_ok();
                                                        }
                                                    }
                                                    }
                                                }
                                            }

                                            // Fallback to apply-then-check.
                                            if !ok {
                                                ok = apply_lsu_with_root_check(
                                                    store.as_ref(),
                                                    &lsu,
                                                    info.expected_state_root,
                                                )
                                                .await
                                                .unwrap_or(false);
                                            }
                                            if ok {
                                                last_lsu.write().await.insert(info.cycle, lsu);
                                                info!(
                                                    "Synced LSU via P2P FileResponse cycle={} cid={}",
                                                    info.cycle, resp.cid
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        } else {
                            // Forward to consensus engine, and also relay broadcast envelopes to support multi-hop WAN gossip.
                            // (Consensus messages don't have local validity checks at this layer.)
                            let env = envelope;
                            let _ = in_tx.send(env.clone());
                            let do_relay = {
                                let mut rc = relay_cache.lock().await;
                                rc.should_relay(&env, now_ms)
                            };
                            if do_relay {
                                let _ = net.broadcast_envelope(&env).await;
                            }
                        }
                    }
                }
            });
            self.network_tasks.push(handle);
            self.network_tasks.push(gap_handle);
        }

        // Outbound: optional dummy tx generator (dev/local helper).
        if self.generate_txs {
            let allow_dummy = self.config.protocol.faucet_mode == crate::config::FaucetMode::Deterministic
                && self.config.protocol.allow_deterministic_faucet;
            let net = network.clone();
            let my_node_id = public_key;
            let interval_ms = self.tx_interval_ms;
            let mempool = mempool.clone();
            let storage = storage.clone();
            let mut shutdown_rx2 = shutdown_rx.clone();
            let handle = tokio::spawn(async move {
                if !allow_dummy {
                    warn!(
                        "generate_txs is enabled but deterministic faucet is disabled/configured; skipping dummy tx generation"
                    );
                    return;
                }
                let faucet_sk = catalyst_crypto::PrivateKey::from_bytes(FAUCET_PRIVATE_KEY_BYTES);
                let faucet_pk = faucet_sk.public_key().to_bytes();
                let mut counter: u64 = 0;
                loop {
                    if *shutdown_rx2.borrow() {
                        break;
                    }
                    let now = current_timestamp_ms();
                    counter = counter.wrapping_add(1);

                    // Protocol-shaped 2-entry transfer (lock_time = now_secs, immediate).
                    //
                    // Dev/testnet behavior: send from faucet -> this node, so querying your node's
                    // pubkey shows a *non-decreasing* balance and avoids confusing negative values.
                    let now_secs = now / 1000;
                    let recv_pk = my_node_id;

                    // Stop generating if faucet is out of funds (basic non-negative enforcement).
                    if let Some(store) = &storage {
                        let faucet_bal = get_balance_i64(store.as_ref(), &faucet_pk).await;
                        if faucet_bal <= 0 {
                            tokio::select! {
                                _ = tokio::time::sleep(std::time::Duration::from_millis(interval_ms)) => {}
                                _ = shutdown_rx2.changed() => {}
                            }
                            continue;
                        }
                    }

                    // Nonce for faucet: max(committed_nonce, pending_max_nonce)+1
                    let nonce = if let Some(store) = &storage {
                        let pending_max = {
                            let mp = mempool.read().await;
                            mp.max_nonce_for_sender(&faucet_pk)
                        };
                        tx_nonce_expected_next(store.as_ref(), &faucet_pk, pending_max).await
                    } else {
                        counter
                    };

                    let tx = catalyst_core::protocol::Transaction {
                        core: catalyst_core::protocol::TransactionCore {
                            tx_type: catalyst_core::protocol::TransactionType::NonConfidentialTransfer,
                            entries: vec![
                                catalyst_core::protocol::TransactionEntry {
                                    public_key: faucet_pk,
                                    amount: catalyst_core::protocol::EntryAmount::NonConfidential(-1),
                                },
                                catalyst_core::protocol::TransactionEntry {
                                    public_key: recv_pk,
                                    amount: catalyst_core::protocol::EntryAmount::NonConfidential(1),
                                },
                            ],
                            nonce,
                            lock_time: now_secs as u32,
                            fees: 0,
                            data: Vec::new(),
                        },
                        signature_scheme: catalyst_core::protocol::sig_scheme::SCHNORR_V1,
                        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
                        sender_pubkey: None,
                        timestamp: now,
                    };
                    let mut tx = tx;
                    tx.core.fees = catalyst_core::protocol::min_fee(&tx);

                    // Real signature
                    let payload = if let Some(store) = &storage {
                        let chain_id = load_chain_id_u64(store.as_ref()).await;
                        let genesis_hash = load_genesis_hash_32(store.as_ref()).await;
                        tx.signing_payload_v2(chain_id, genesis_hash)
                            .or_else(|_| tx.signing_payload_v1(chain_id, genesis_hash))
                            .or_else(|_| tx.signing_payload())
                            .unwrap_or_else(|_| Vec::new())
                    } else {
                        tx.signing_payload().unwrap_or_else(|_| Vec::new())
                    };
                    if payload.is_empty() {
                        continue;
                    }
                    let mut rng = rand::rngs::OsRng;
                    let scheme = catalyst_crypto::signatures::SignatureScheme::new();
                    let sig = match scheme.sign(&mut rng, &faucet_sk, &payload) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    tx.signature = catalyst_core::protocol::AggregatedSignature(sig.to_bytes().to_vec());

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
            let validator_worker_ids_hex = validator_worker_ids_hex.clone();
            let max_entries_per_cycle = self.config.consensus.max_transactions_per_block as usize;
            self.consensus_task = Some(tokio::spawn(async move {
                // Seed for deterministic producer selection (paper uses previous LSU merkle root).
                // We approximate with the hash of the most recent LSU we produced/observed.
                let mut prev_seed: [u8; 32] = [0u8; 32];

                // Load last persisted seed (prefer applied head).
                if let Some(store) = &storage {
                    let seed_bytes = match store.get_metadata("consensus:last_applied_lsu_hash").await {
                        Ok(Some(b)) => Some(b),
                        _ => match store.get_metadata("consensus:last_lsu_hash").await {
                            Ok(Some(b)) => Some(b),
                            _ => None,
                        },
                    };

                    if let Some(bytes) = seed_bytes {
                        if bytes.len() == 32 {
                            prev_seed.copy_from_slice(&bytes[..32]);
                            info!("Loaded prev_seed from storage: {}", hex_encode(&prev_seed));
                        }
                    }
                }

                // Warmup: allow libp2p/mDNS to discover peers before the first cycle boundary.
                tokio::time::sleep(std::time::Duration::from_millis(2500)).await;

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
                    // - worker pool: configured validator set (worker ids)
                    // - seed: prev cycle LSU hash (approx)
                    // - producer_count: all active workers for this cycle (membership-driven)
                    let mut worker_pool: Vec<NodeId> = Vec::new();
                    if let Some(store) = &storage {
                        worker_pool.extend(load_workers_from_state(store.as_ref()));
                    }
                    // Bootstrap fallback: config file may carry an initial worker pool for brand-new networks.
                    if worker_pool.is_empty() {
                        info!(
                            "Cycle {} membership=bootstrap (no on-chain workers). Using config validator_worker_ids (n={}).",
                            cycle,
                            validator_worker_ids_hex.len()
                        );
                        worker_pool.extend(validator_worker_ids_hex.iter().filter_map(|s| parse_hex_32(s)));
                    } else {
                        info!(
                            "Cycle {} membership=onchain workers={}",
                            cycle,
                            worker_pool.len()
                        );
                    }
                    worker_pool.sort();
                    worker_pool.dedup();

                    let mut id_map: std::collections::HashMap<NodeId, String> = std::collections::HashMap::new();
                    for id in &worker_pool {
                        id_map.insert(*id, hex_encode(id));
                    }

                    let producer_count = worker_pool.len().max(1);
                    let required_majority = (producer_count / 2) + 1;
                    info!(
                        "Cycle {} expected_producers={} required_majority={}",
                        cycle,
                        producer_count,
                        required_majority
                    );
                    let selected_worker_ids =
                        select_producers_for_next_cycle(&worker_pool, &prev_seed, producer_count);
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
                        let candidate_txs = {
                            let mut mp = mempool.write().await;
                            // Snapshot protocol transactions only; legacy entries are ignored for Construction.
                            mp.snapshot_protocol_txs(max_entries_per_cycle)
                        };
                        let txs = if let Some(store) = &storage {
                            validate_and_select_protocol_txs_for_construction(
                                store.as_ref(),
                                candidate_txs,
                                max_entries_per_cycle,
                            )
                            .await
                        } else {
                            Vec::new()
                        };

                        // Persist per-cycle txids + mark Selected (so receipts can show progress).
                        if let Some(store) = &storage {
                            let txids: Vec<[u8; 32]> = txs.iter().filter_map(|t| mempool_txid(t)).collect();
                            persist_cycle_txids(store.as_ref(), cycle, txids).await;
                            for tx in &txs {
                                update_tx_meta_status_selected(store.as_ref(), tx, cycle).await;
                            }
                        }

                        let mut entries: Vec<catalyst_consensus::types::TransactionEntry> = Vec::new();
                        for tx in &txs {
                            entries.extend(tx_to_consensus_entries(tx));
                        }

                        info!("Cycle {} tx batch leader={} entries={}", cycle, leader, entries.len());
                        if let Ok(batch) = ProtocolTxBatch::new(cycle, txs) {
                            if let Ok(env) =
                                MessageEnvelope::from_message(&batch, "txbatch".to_string(), None)
                            {
                                // Gossipsub is best-effort; rebroadcast a few times to make
                                // early-cycle delivery reliable (prevents split first_hash).
                                for _ in 0..5 {
                                    let _ = network.broadcast_envelope(&env).await;
                                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                                }
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

                            // Proof-driven sync (step 4): bundle old/new proofs for touched keys.
                            // These are best-effort; followers fall back to apply-then-check if unavailable.
                            let mut prev_state_root_for_msg: [u8; 32] = [0u8; 32];
                            let mut proof_cid_for_msg: String = String::new();

                            // Persist latest LSU and seed.
                            if let Some(store) = &storage {
                                if let Ok(bytes) = update.serialize() {
                                    // Per-cycle history
                                    let _ = store
                                        .set_metadata(&format!("consensus:lsu:{}", cycle), &bytes)
                                        .await;
                                    let _ = store
                                        .set_metadata(&format!("consensus:lsu_hash:{}", cycle), &prev_seed)
                                        .await;
                                    let _ = store
                                        .set_metadata(&cycle_by_lsu_hash_key(&prev_seed), &cycle.to_le_bytes())
                                        .await;

                                    let _ = store.set_metadata("consensus:last_lsu", &bytes).await;
                                    let _ = store.set_metadata("consensus:last_lsu_hash", &prev_seed).await;
                                    let _ = store
                                        .set_metadata("consensus:last_lsu_cycle", &cycle.to_le_bytes())
                                        .await;
                                }

                                // Pre/post proofs for proof-driven sync bundle.
                                // NOTE: EVM execution touches dynamic storage keys; skip proof bundles for LSUs that include EVM markers.
                                let touched = touched_keys_for_lsu(&update);
                                // IMPORTANT: even if we skip proof bundles for EVM LSUs, we must still
                                // gossip the correct previous state root so followers can validate
                                // chain continuity and apply the LSU.
                                let current_root = store.get_state_root().unwrap_or([0u8; 32]);
                                let (prev_root, prev_proofs) = if lsu_contains_evm(&update) {
                                    (current_root, Vec::new())
                                } else {
                                    store
                                        .get_account_proofs_for_keys_with_absence(&touched)
                                        .await
                                        .unwrap_or((current_root, Vec::new()))
                                };
                                prev_state_root_for_msg = prev_root;

                                // Apply to state (leader path).
                                let new_root = apply_lsu_to_storage(store.as_ref(), &update)
                                    .await
                                    .unwrap_or([0u8; 32]);

                                let (post_root, post_proofs) = if lsu_contains_evm(&update) {
                                    ([0u8; 32], Vec::new())
                                } else {
                                    store
                                        .get_account_proofs_for_keys_with_absence(&touched)
                                        .await
                                        .unwrap_or(([0u8; 32], Vec::new()))
                                };

                                // Best-effort: build a proof bundle. If it fails, followers fall back.
                                if let Some(dfs) = &dfs {
                                    if let Ok(lsu_hash) = catalyst_consensus::types::hash_data(&update) {
                                        if prev_root != [0u8; 32] && post_root == new_root && !prev_proofs.is_empty() {
                                            let mut by_key_old: std::collections::BTreeMap<Vec<u8>, (Option<Vec<u8>>, catalyst_storage::merkle::MerkleProof)> =
                                                std::collections::BTreeMap::new();
                                            for (k, v, p) in prev_proofs {
                                                by_key_old.insert(k, (v, p));
                                            }
                                            let mut by_key_new: std::collections::BTreeMap<Vec<u8>, (Option<Vec<u8>>, catalyst_storage::merkle::MerkleProof)> =
                                                std::collections::BTreeMap::new();
                                            for (k, v, p) in post_proofs {
                                                by_key_new.insert(k, (v, p));
                                            }

                                            let mut changes: Vec<KeyProofChange> = Vec::new();
                                            for (k, (ov, op)) in by_key_old {
                                                if let Some((nv, np)) = by_key_new.get(&k).cloned() {
                                                    changes.push(KeyProofChange {
                                                        key: k,
                                                        old_value: ov.unwrap_or_default(),
                                                        old_proof: op,
                                                        new_value: nv.unwrap_or_default(),
                                                        new_proof: np,
                                                    });
                                                }
                                            }
                                            let bundle = StateProofBundle {
                                                cycle,
                                                lsu_hash,
                                                prev_state_root: prev_root,
                                                new_state_root: new_root,
                                                changes,
                                            };
                                            if let Ok(b) = bincode::serialize(&bundle) {
                                                proof_cid_for_msg = dfs.put(b).await.ok().unwrap_or_default();
                                            }
                                        }
                                    }
                                }
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
                                            let state_root = if let Some(store) = &storage {
                                                store.get_state_root().unwrap_or([0u8; 32])
                                            } else {
                                                [0u8; 32]
                                            };
                                            let msg = LsuCidGossip {
                                                cycle,
                                                lsu_hash,
                                                cid,
                                                prev_state_root: prev_state_root_for_msg,
                                                state_root,
                                                proof_cid: proof_cid_for_msg,
                                            };
                                            if let Ok(env) = MessageEnvelope::from_message(&msg, "lsu_cid".to_string(), None) {
                                                let _ = network.broadcast_envelope(&env).await;
                                            }
                                            info!("Stored LSU in DFS and broadcast CID cycle={} cid={}", cycle, msg.cid);

                                            if let Some(store) = &storage {
                                                let _ = store
                                                    .set_metadata("consensus:last_lsu_cid", msg.cid.as_bytes())
                                                    .await;
                                                let _ = store
                                                    .set_metadata("consensus:last_lsu_state_root", &msg.state_root)
                                                    .await;
                                                // Per-cycle CID history
                                                let _ = store
                                                    .set_metadata(
                                                        &format!("consensus:lsu_cid:{}", cycle),
                                                        msg.cid.as_bytes(),
                                                    )
                                                    .await;
                                                let _ = store
                                                    .set_metadata(
                                                        &format!("consensus:lsu_state_root:{}", cycle),
                                                        &msg.state_root,
                                                    )
                                                    .await;
                                            }
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

        if let Some(handle) = self.rpc_handle.take() {
            handle.stop().ok();
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

