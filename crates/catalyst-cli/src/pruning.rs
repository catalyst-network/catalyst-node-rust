use anyhow::Result;
use catalyst_storage::StorageManager;
use catalyst_utils::state::state_keys;

const META_PRUNE_ENABLED: &str = "storage:history_prune_enabled";
const META_KEEP_CYCLES: &str = "storage:history_keep_cycles";
const META_PRUNE_INTERVAL_SECS: &str = "storage:history_prune_interval_seconds";
const META_PRUNE_BATCH_CYCLES: &str = "storage:history_prune_batch_cycles";
const META_LAST_PRUNE_RUN_MS: &str = "storage:history_prune_last_run_ms";
const META_PRUNED_UP_TO_CYCLE: &str = "storage:history_pruned_up_to_cycle";

#[derive(Debug, Clone)]
struct PruneCfg {
    enabled: bool,
    keep_cycles: u64,
    interval_secs: u64,
    batch_cycles: u64,
}

#[derive(Debug, Clone)]
pub struct PruneReport {
    pub pruned_from_cycle: u64,
    pub pruned_to_cycle: u64,
    pub deleted_cycles: u64,
    pub deleted_tx_entries: u64,
    pub deleted_keys: u64,
}

fn decode_u64_le(bytes: &[u8]) -> Option<u64> {
    if bytes.len() != 8 {
        return None;
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(bytes);
    Some(u64::from_le_bytes(arr))
}

fn decode_bool(bytes: &[u8]) -> Option<bool> {
    if bytes.len() != 1 {
        return None;
    }
    Some(bytes[0] != 0)
}

fn now_ms() -> u64 {
    catalyst_utils::utils::current_timestamp_ms()
}

fn meta_delete(store: &StorageManager, key: &str) {
    let k = state_keys::metadata_key(key);
    let _ = store.engine().delete("metadata", &k);
}

fn consensus_lsu_key(cycle: u64) -> String {
    format!("consensus:lsu:{cycle}")
}
fn consensus_lsu_hash_key(cycle: u64) -> String {
    format!("consensus:lsu_hash:{cycle}")
}
fn consensus_lsu_cid_key(cycle: u64) -> String {
    format!("consensus:lsu_cid:{cycle}")
}
fn consensus_lsu_state_root_key(cycle: u64) -> String {
    format!("consensus:lsu_state_root:{cycle}")
}
fn cycle_by_lsu_hash_key(lsu_hash: &[u8; 32]) -> String {
    format!("consensus:cycle_by_lsu_hash:{}", hex::encode(lsu_hash))
}
fn cycle_txids_key(cycle: u64) -> String {
    format!("tx:cycle:{cycle}:txids")
}
fn tx_raw_key(txid: &[u8; 32]) -> String {
    format!("tx:raw:{}", hex::encode(txid))
}
fn tx_meta_key(txid: &[u8; 32]) -> String {
    format!("tx:meta:{}", hex::encode(txid))
}

async fn load_prune_cfg(store: &StorageManager) -> PruneCfg {
    let enabled = store
        .get_metadata(META_PRUNE_ENABLED)
        .await
        .ok()
        .flatten()
        .and_then(|b| decode_bool(&b))
        .unwrap_or(false);
    let keep_cycles = store
        .get_metadata(META_KEEP_CYCLES)
        .await
        .ok()
        .flatten()
        .and_then(|b| decode_u64_le(&b))
        .unwrap_or(0);
    let interval_secs = store
        .get_metadata(META_PRUNE_INTERVAL_SECS)
        .await
        .ok()
        .flatten()
        .and_then(|b| decode_u64_le(&b))
        .unwrap_or(300);
    let batch_cycles = store
        .get_metadata(META_PRUNE_BATCH_CYCLES)
        .await
        .ok()
        .flatten()
        .and_then(|b| decode_u64_le(&b))
        .unwrap_or(1_000);
    PruneCfg {
        enabled,
        keep_cycles,
        interval_secs: interval_secs.max(1),
        batch_cycles: batch_cycles.max(1),
    }
}

async fn prune_once(
    store: &StorageManager,
    keep_cycles: u64,
    batch_cycles: u64,
) -> Result<Option<PruneReport>> {
    if keep_cycles == 0 {
        return Ok(None);
    }

    let head = store
        .get_metadata("consensus:last_applied_cycle")
        .await
        .ok()
        .flatten()
        .and_then(|b| decode_u64_le(&b))
        .unwrap_or(0);

    // Keep the most recent `keep_cycles` cycles (inclusive of head).
    let prune_up_to = head.saturating_sub(keep_cycles).saturating_sub(1);
    if prune_up_to == 0 {
        return Ok(None);
    }

    let start = store
        .get_metadata(META_PRUNED_UP_TO_CYCLE)
        .await
        .ok()
        .flatten()
        .and_then(|b| decode_u64_le(&b))
        .map(|last| last.saturating_add(1))
        .unwrap_or(0);

    if start == 0 && prune_up_to == 0 {
        return Ok(None);
    }
    if start > prune_up_to {
        return Ok(None);
    }

    let end = prune_up_to.min(start.saturating_add(batch_cycles.saturating_sub(1)));

    let mut deleted_keys: u64 = 0;
    let mut deleted_tx_entries: u64 = 0;

    for cycle in start..=end {
        // Load txids BEFORE deleting the per-cycle index.
        let txids: Vec<[u8; 32]> = store
            .get_metadata(&cycle_txids_key(cycle))
            .await
            .ok()
            .flatten()
            .and_then(|b| bincode::deserialize::<Vec<[u8; 32]>>(&b).ok())
            .unwrap_or_default();

        for txid in &txids {
            meta_delete(store, &tx_raw_key(txid));
            meta_delete(store, &tx_meta_key(txid));
            deleted_tx_entries = deleted_tx_entries.saturating_add(1);
            deleted_keys = deleted_keys.saturating_add(2);
        }
        meta_delete(store, &cycle_txids_key(cycle));
        deleted_keys = deleted_keys.saturating_add(1);

        // Consensus history keys.
        let lsu_hash = store
            .get_metadata(&consensus_lsu_hash_key(cycle))
            .await
            .ok()
            .flatten()
            .and_then(|b| {
                if b.len() == 32 {
                    let mut h = [0u8; 32];
                    h.copy_from_slice(&b);
                    Some(h)
                } else {
                    None
                }
            });

        meta_delete(store, &consensus_lsu_key(cycle));
        meta_delete(store, &consensus_lsu_hash_key(cycle));
        meta_delete(store, &consensus_lsu_cid_key(cycle));
        meta_delete(store, &consensus_lsu_state_root_key(cycle));
        deleted_keys = deleted_keys.saturating_add(4);

        if let Some(h) = lsu_hash {
            meta_delete(store, &cycle_by_lsu_hash_key(&h));
            deleted_keys = deleted_keys.saturating_add(1);
        }
    }

    // Persist progress (best-effort).
    let _ = store
        .set_metadata(META_PRUNED_UP_TO_CYCLE, &end.to_le_bytes())
        .await;

    Ok(Some(PruneReport {
        pruned_from_cycle: start,
        pruned_to_cycle: end,
        deleted_cycles: end.saturating_sub(start).saturating_add(1),
        deleted_tx_entries,
        deleted_keys,
    }))
}

/// Opportunistically prune historical RPC/indexer metadata to keep disk usage bounded.
///
/// This is controlled by metadata keys written from the node config on startup.
pub async fn maybe_prune_history(store: &StorageManager) {
    let cfg = load_prune_cfg(store).await;
    if !cfg.enabled {
        return;
    }

    let now = now_ms();
    let last = store
        .get_metadata(META_LAST_PRUNE_RUN_MS)
        .await
        .ok()
        .flatten()
        .and_then(|b| decode_u64_le(&b))
        .unwrap_or(0);

    if now.saturating_sub(last) < cfg.interval_secs.saturating_mul(1000) {
        return;
    }

    // Set last-run first to avoid repeated work under heavy apply loops (best-effort).
    let _ = store
        .set_metadata(META_LAST_PRUNE_RUN_MS, &now.to_le_bytes())
        .await;

    let _ = prune_once(store, cfg.keep_cycles, cfg.batch_cycles).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyst_storage::StorageConfig as StorageConfigLib;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_prune_once_deletes_expected_keys() {
        let td = TempDir::new().unwrap();
        let mut cfg = StorageConfigLib::default();
        cfg.data_dir = td.path().to_path_buf();
        let store = StorageManager::new(cfg).await.unwrap();

        // Fake head at 100, keep last 10 -> prune up to 89.
        store.set_metadata("consensus:last_applied_cycle", &100u64.to_le_bytes())
            .await
            .unwrap();

        // Populate a couple cycles of history in the prune range.
        for cycle in 50u64..=52u64 {
            store.set_metadata(&consensus_lsu_key(cycle), b"lsu").await.unwrap();
            let h = [cycle as u8; 32];
            store.set_metadata(&consensus_lsu_hash_key(cycle), &h).await.unwrap();
            store.set_metadata(&cycle_by_lsu_hash_key(&h), &cycle.to_le_bytes())
                .await
                .unwrap();
            store.set_metadata(&consensus_lsu_cid_key(cycle), b"cid").await.unwrap();
            store.set_metadata(&consensus_lsu_state_root_key(cycle), &[0u8; 32])
                .await
                .unwrap();

            // One tx per cycle.
            let txid = [cycle as u8; 32];
            let txids = vec![txid];
            store.set_metadata(&cycle_txids_key(cycle), &bincode::serialize(&txids).unwrap())
                .await
                .unwrap();
            store.set_metadata(&tx_raw_key(&txid), b"raw").await.unwrap();
            store.set_metadata(&tx_meta_key(&txid), b"meta").await.unwrap();
        }

        let rep = prune_once(&store, 10, 3).await.unwrap().unwrap();
        assert_eq!(rep.pruned_from_cycle, 0); // first run starts at 0
        assert_eq!(rep.pruned_to_cycle, 2); // batch of 3 cycles (0..2)

        // Now set progress to 49 and prune the populated cycles.
        store.set_metadata(META_PRUNED_UP_TO_CYCLE, &49u64.to_le_bytes())
            .await
            .unwrap();
        let rep2 = prune_once(&store, 10, 10).await.unwrap().unwrap();
        assert_eq!(rep2.pruned_from_cycle, 50);
        assert_eq!(rep2.pruned_to_cycle, 59.min(89)); // limited by batch

        // Ensure one known key was deleted.
        let k = store.get_metadata(&consensus_lsu_key(50)).await.unwrap();
        assert!(k.is_none());
    }
}

