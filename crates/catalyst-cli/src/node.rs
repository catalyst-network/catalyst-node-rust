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
    CatalystDeserialize, CatalystSerialize, MessageType, MessageEnvelope,
    utils::current_timestamp_ms,
};
use catalyst_consensus::types::hash_data;
use catalyst_utils::state::StateManager;

use crate::config::NodeConfig;
use crate::tx::{Mempool, ProtocolTxBatch, ProtocolTxGossip, TxBatch, TxGossip};
use crate::sync::{FileRequestMsg, FileResponseMsg, KeyProofChange, LsuCidGossip, LsuGossip, StateProofBundle};
use crate::evm::{decode_evm_marker, encode_evm_marker, EvmTxKind, EvmTxPayload};
use crate::evm_revm::{execute_call_and_persist, execute_deploy_and_persist};

use catalyst_dfs::ContentId;

use crate::dfs_store::LocalContentStore;

use catalyst_storage::{StorageConfig as StorageConfigLib, StorageManager};

use alloy_primitives::Address as EvmAddress;

const DEFAULT_EVM_GAS_LIMIT: u64 = 8_000_000;

// Dev/testnet faucet key (32-byte scalar); pubkey is derived and is a valid compressed Ristretto point.
const FAUCET_PRIVATE_KEY_BYTES: [u8; 32] = [0xFA; 32];
const FAUCET_INITIAL_BALANCE: i64 = 1_000_000;

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
    // Apply balance / worker / EVM updates.
    for e in &lsu.partial_update.transaction_entries {
        if is_worker_reg_marker(&e.signature) {
            let k = worker_key_for_pubkey(&e.public_key);
            let _ = store.set_state(&k, vec![1u8]).await;
            continue;
        }
        if is_evm_marker(&e.signature) {
            if let Some((payload, _sig64)) = decode_evm_marker(&e.signature) {
                let from20 = pubkey_to_evm_addr20(&e.public_key);
                let from = EvmAddress::from_slice(&from20);
                let gas_limit = DEFAULT_EVM_GAS_LIMIT.max(21_000);
                match payload.kind {
                    EvmTxKind::Deploy { bytecode } => {
                        let _ = execute_deploy_and_persist(store, from, payload.nonce, bytecode, gas_limit).await;
                    }
                    EvmTxKind::Call { to, input } => {
                        let to_addr = EvmAddress::from_slice(&to);
                        let _ = execute_call_and_persist(store, from, to_addr, payload.nonce, input, gas_limit).await;
                    }
                }
            }
            continue;
        }
        let bal = get_balance_i64(store, &e.public_key).await;
        let next = bal.saturating_add(e.amount);
        let _ = set_balance_i64(store, &e.public_key, next).await;
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

fn faucet_pubkey_bytes() -> [u8; 32] {
    catalyst_crypto::PrivateKey::from_bytes(FAUCET_PRIVATE_KEY_BYTES)
        .public_key()
        .to_bytes()
}

fn verify_protocol_tx_signature(tx: &catalyst_core::protocol::Transaction) -> bool {
    use catalyst_crypto::signatures::SignatureScheme;

    // Determine sender:
    // - transfers: the (single) pubkey with negative NonConfidential amount
    // - worker registration: entry[0].public_key
    // - smart contract: entry[0].public_key
    let sender_pk_bytes: [u8; 32] = match tx.core.tx_type {
        catalyst_core::protocol::TransactionType::WorkerRegistration => {
            let Some(e0) = tx.core.entries.get(0) else { return false };
            e0.public_key
        }
        catalyst_core::protocol::TransactionType::SmartContract => {
            let Some(e0) = tx.core.entries.get(0) else { return false };
            e0.public_key
        }
        _ => {
            let mut sender: Option<[u8; 32]> = None;
            for e in &tx.core.entries {
                if let catalyst_core::protocol::EntryAmount::NonConfidential(v) = e.amount {
                    if v < 0 {
                        match sender {
                            None => sender = Some(e.public_key),
                            Some(pk) if pk == e.public_key => {}
                            Some(_) => return false, // multi-sender not supported yet
                        }
                    }
                }
            }
            let Some(sender) = sender else { return false };
            sender
        }
    };

    // Signature bytes must be a valid Schnorr signature.
    if tx.signature.0.len() != 64 {
        return false;
    }
    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(&tx.signature.0);
    let sig = match catalyst_crypto::signatures::Signature::from_bytes(sig_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let sender_pk = match catalyst_crypto::PublicKey::from_bytes(sender_pk_bytes) {
        Ok(pk) => pk,
        Err(_) => return false,
    };
    let payload = match tx.signing_payload() {
        Ok(p) => p,
        Err(_) => return false,
    };

    SignatureScheme::new().verify(&payload, &sig, &sender_pk).unwrap_or(false)
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
    // - at least 2 entries
    // - sum of amounts is 0 (conservation)
    let sum: i64 = entries.iter().map(|e| e.amount).sum();
    !entries.is_empty() && sum == 0
}

async fn tx_is_funded(store: &StorageManager, entries: &[catalyst_consensus::types::TransactionEntry]) -> bool {
    // Sufficient-funds check with aggregation:
    // for each pubkey, ensure (balance + sum(deltas)) >= 0.
    use std::collections::BTreeMap;
    let mut deltas: BTreeMap<[u8; 32], i64> = BTreeMap::new();
    for e in entries {
        *deltas.entry(e.public_key).or_insert(0) = deltas
            .get(&e.public_key)
            .copied()
            .unwrap_or(0)
            .saturating_add(e.amount);
    }
    for (pk, delta) in deltas {
        if delta < 0 {
            let bal = get_balance_i64(store, &pk).await;
            if bal.saturating_add(delta) < 0 {
                return false;
            }
        }
    }
    true
}

async fn tx_is_funded_protocol(store: &StorageManager, tx: &catalyst_core::protocol::Transaction) -> bool {
    use catalyst_core::protocol::{EntryAmount, TransactionType};
    use std::collections::BTreeMap;

    match tx.core.tx_type {
        TransactionType::NonConfidentialTransfer => {}
        // No balance impact (for now) for these scaffolded tx types.
        TransactionType::WorkerRegistration | TransactionType::SmartContract => return true,
        _ => return false,
    }

    let mut deltas: BTreeMap<[u8; 32], i64> = BTreeMap::new();
    for e in &tx.core.entries {
        let v = match e.amount {
            EntryAmount::NonConfidential(v) => v,
            EntryAmount::Confidential { .. } => return false,
        };
        *deltas.entry(e.public_key).or_insert(0) =
            deltas.get(&e.public_key).copied().unwrap_or(0).saturating_add(v);
    }

    for (pk, delta) in deltas {
        if delta < 0 {
            let bal = get_balance_i64(store, &pk).await;
            if bal.saturating_add(delta) < 0 {
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
    catalyst_core::protocol::transaction_sender_pubkey(tx)
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
        // Lock_time gate (same as mempool)
        if tx.core.lock_time as u64 > now_secs {
            continue;
        }
        // Signature check (already done in mempool, but re-check at formation time)
        if !verify_protocol_tx_signature(&tx) {
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
    let bytes = bincode::serialize(tx).ok()?;
    let mut hasher = blake2::Blake2b512::new();
    use blake2::Digest;
    hasher.update(&bytes);
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result[..32]);
    Some(out)
}

fn mempool_tx_key(txid: &[u8; 32]) -> String {
    format!("mempool:tx:{}", hex_encode(txid))
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
    let key = mempool_tx_key(&txid);
    let Ok(bytes) = bincode::serialize(tx) else { return };
    let _ = store.set_metadata(&key, &bytes).await;

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
        if tx.validate_basic().is_err() || !verify_protocol_tx_signature(&tx) {
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
        if tx.validate_basic().is_err() || !verify_protocol_tx_signature(&tx) {
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
        if tx.validate_basic().is_err() || !verify_protocol_tx_signature(&tx) {
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
            continue;
        }
        if is_evm_marker(&e.signature) {
            if let Some((payload, _sig64)) = decode_evm_marker(&e.signature) {
                let from20 = pubkey_to_evm_addr20(&e.public_key);
                let from = EvmAddress::from_slice(&from20);
                let gas_limit = DEFAULT_EVM_GAS_LIMIT.max(21_000);
                match payload.kind {
                    EvmTxKind::Deploy { bytecode } => {
                        match execute_deploy_and_persist(store, from, payload.nonce, bytecode, gas_limit).await {
                            Ok((created, _ret, _persisted)) => {
                                info!("EVM deploy applied addr=0x{}", hex::encode(created.as_slice()));
                            }
                            Err(err) => {
                                tracing::warn!("EVM deploy failed: {err}");
                            }
                        }
                    }
                    EvmTxKind::Call { to, input } => {
                        let to_addr = EvmAddress::from_slice(&to);
                        match execute_call_and_persist(store, from, to_addr, payload.nonce, input, gas_limit).await {
                            Ok((ret, _persisted)) => {
                                info!("EVM call applied to=0x{} ret_len={}", hex::encode(to_addr.as_slice()), ret.len());
                            }
                            Err(err) => {
                                tracing::warn!("EVM call failed: {err}");
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
        net_cfg.peer.bootstrap_peers = Vec::new();

        let network = Arc::new(P2pService::new(net_cfg).await?);
        network.start().await?;

        // Dial bootstrap peers from CLI config
        for peer in &self.config.network.bootstrap_peers {
            if let Ok(ma) = peer.parse::<Multiaddr>() {
                let _ = network.connect_multiaddr(&ma).await;
            }
        }

        // Keep trying to connect to bootstrap peers. This makes local testnet resilient to
        // restarting node1 (bootstrap) without having to restart all other nodes.
        if !self.config.network.bootstrap_peers.is_empty() {
            let peers = self.config.network.bootstrap_peers.clone();
            let net = network.clone();
            let mut shutdown_rx2 = shutdown_rx.clone();
            let handle = tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    if *shutdown_rx2.borrow() {
                        break;
                    }
                    for p in &peers {
                        if let Ok(ma) = p.parse::<Multiaddr>() {
                            let _ = net.connect_multiaddr(&ma).await;
                        }
                    }
                    tokio::select! {
                        _ = tick.tick() => {}
                        _ = shutdown_rx2.changed() => {}
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
                        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
                        timestamp: now_ms,
                    };

                    let payload = tx.signing_payload().map_err(anyhow::Error::msg)?;
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

        // Initialize faucet (dev/testnet): create a deterministic funded account if missing.
        if let Some(store) = &storage {
            let faucet_pk = faucet_pubkey_bytes();
            let existing = store
                .get_state(&balance_key_for_pubkey(&faucet_pk))
                .await
                .ok()
                .flatten();
            if existing.is_none() {
                let _ = set_balance_i64(store.as_ref(), &faucet_pk, FAUCET_INITIAL_BALANCE).await;
                let _ = set_nonce_u64(store.as_ref(), &faucet_pk, 0).await;
                let _ = store.commit().await;
                info!(
                    "Initialized faucet pubkey={} balance={}",
                    hex_encode(&faucet_pk),
                    FAUCET_INITIAL_BALANCE
                );
            }
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
                let handle = catalyst_rpc::start_rpc_http(bind, store.clone(), Some(network.clone()), Some(rpc_tx))
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
                            if !verify_protocol_tx_signature(&msg.tx) {
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
            let handle = tokio::spawn(async move {
                while let Some(ev) = events.recv().await {
                    if let catalyst_network::NetworkEvent::MessageReceived { envelope, .. } = ev {
                        if envelope.message_type == MessageType::Transaction {
                            // Prefer protocol-shaped txs; fall back to legacy TxGossip.
                            let now_secs = current_timestamp_ms() / 1000;
                            if let Ok(tx) = envelope.extract_message::<ProtocolTxGossip>() {
                                // Reject if tx_id does not match canonical tx id for the embedded tx.
                                if let Ok(expected) = catalyst_core::protocol::transaction_id(&tx.tx) {
                                    if expected != tx.tx_id {
                                        tracing::warn!("reject_tx: bad_txid");
                                        continue;
                                    }
                                } else {
                                    tracing::warn!("reject_tx: txid_compute_failed");
                                    continue;
                                }

                                // Basic format + lock-time.
                                if catalyst_core::protocol::validate_basic_and_unlocked(&tx.tx, now_secs).is_err() {
                                    tracing::warn!("reject_tx: basic_or_locktime");
                                    continue;
                                }
                                // Pure semantics (fees/conservation/type support).
                                if catalyst_core::protocol::validate_tx_semantics_for_mempool(&tx.tx).is_err() {
                                    tracing::warn!("reject_tx: semantics");
                                    continue;
                                }
                                if !verify_protocol_tx_signature(&tx.tx) {
                                    tracing::warn!("reject_tx: bad_signature");
                                    continue;
                                }
                                // Nonce check (single-sender only, requires storage).
                                if let Some(store) = &storage {
                                    let Some(sender_pk) = tx.sender_pubkey() else { continue };
                                    let pending_max = {
                                        let mp = mempool.read().await;
                                        mp.max_nonce_for_sender(&sender_pk)
                                    };
                                    let expected =
                                        tx_nonce_expected_next(store.as_ref(), &sender_pk, pending_max).await;
                                    if tx.tx.core.nonce != expected {
                                        tracing::warn!("reject_tx: bad_nonce expected={} got={}", expected, tx.tx.core.nonce);
                                        continue;
                                    }
                                }
                                // Enforce basic sufficient-funds using current storage.
                                if let Some(store) = &storage {
                                    if tx_is_funded_protocol(store.as_ref(), &tx.tx).await {
                                        let mut mp = mempool.write().await;
                                        if mp.insert_protocol(tx.clone(), now_secs) {
                                            persist_mempool_tx(store.as_ref(), &tx.tx).await;
                                        }
                                    } else {
                                        tracing::warn!("reject_tx: insufficient_funds");
                                    }
                                } else {
                                    let mut mp = mempool.write().await;
                                    let _ = mp.insert_protocol(tx, now_secs);
                                }
                            } else if let Ok(tx) = envelope.extract_message::<TxGossip>() {
                                let mut mp = mempool.write().await;
                                let _ = mp.insert(tx);
                            }
                        } else if envelope.message_type == MessageType::TransactionBatch {
                            // Prefer protocol tx batches (full signed txs), fallback to legacy entry batch.
                            if let Ok(batch) = envelope.extract_message::<ProtocolTxBatch>() {
                                if batch.verify_hash().unwrap_or(false) {
                                    if let Some(store) = &storage {
                                        let valid =
                                            validate_and_select_protocol_txs_for_construction(store.as_ref(), batch.txs, max_entries_per_cycle_cfg).await;
                                        let mut entries: Vec<catalyst_consensus::types::TransactionEntry> = Vec::new();
                                        for tx in &valid {
                                            entries.extend(tx_to_consensus_entries(tx));
                                        }
                                        tx_batches.write().await.insert(batch.cycle, entries);
                                    }
                                }
                            } else if let Ok(batch) = envelope.extract_message::<TxBatch>() {
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
                                                        if let Some(store) = &storage {
                                                            // Ensure we're applying onto the expected previous root.
                                                            let local_prev = store
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
                                                                .unwrap_or([0u8; 32]);

                                                            if local_prev != ref_msg.prev_state_root && local_prev != [0u8; 32] {
                                                                tracing::warn!(
                                                                    "Skipping LSU cycle={} cid={} (prev_root mismatch local={} expected={})",
                                                                    ref_msg.cycle,
                                                                    ref_msg.cid,
                                                                    hex_encode(&local_prev),
                                                                    hex_encode(&ref_msg.prev_state_root)
                                                                );
                                                                continue;
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
                                    }
                                }
                            }
                        } else if envelope.message_type == MessageType::FileRequest {
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
                                            // Ensure we're applying onto the expected previous root.
                                            let local_prev = store
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
                                                .unwrap_or([0u8; 32]);
                                            if local_prev != info.prev_state_root && local_prev != [0u8; 32] {
                                                continue;
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
            let storage = storage.clone();
            let mut shutdown_rx2 = shutdown_rx.clone();
            let handle = tokio::spawn(async move {
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
                        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
                        timestamp: now,
                    };

                    // Real signature
                    let mut tx = tx;
                    let payload = match tx.signing_payload() {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
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

