//! Minimal consensus checkpoints for pruned-node reconcile (checklist §4.2).
//!
//! A checkpoint records `(cycle, state_root, lsu_hash)` plus a RocksDB snapshot name so
//! `try_reconcile_fork_from_quorum_lsu` can replay from `checkpoint.cycle + 1` when LSU
//! metadata for earlier cycles was pruned.

use anyhow::{Context, Result};
use catalyst_storage::StorageManager;

pub const META_CHECKPOINT_LATEST_CYCLE: &str = "consensus:checkpoint:latest_cycle";

fn checkpoint_meta_key(cycle: u64) -> String {
    format!("consensus:checkpoint:{cycle}")
}

fn snapshot_name_for_cycle(cycle: u64) -> String {
    format!("consensus_checkpoint_{cycle}")
}

/// On-disk checkpoint record (metadata value for `consensus:checkpoint:{cycle}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointRecord {
    pub cycle: u64,
    pub state_root: [u8; 32],
    pub lsu_hash: [u8; 32],
    pub snapshot_name: String,
}

/// Blob: `cycle(8 le) | state_root(32) | lsu_hash(32) | snapshot_name utf-8`
pub fn encode_checkpoint_blob(record: &CheckpointRecord) -> Vec<u8> {
    let name_bytes = record.snapshot_name.as_bytes();
    let mut out = Vec::with_capacity(72 + name_bytes.len());
    out.extend_from_slice(&record.cycle.to_le_bytes());
    out.extend_from_slice(&record.state_root);
    out.extend_from_slice(&record.lsu_hash);
    out.extend_from_slice(name_bytes);
    out
}

pub fn decode_checkpoint_blob(bytes: &[u8]) -> Option<CheckpointRecord> {
    if bytes.len() < 72 {
        return None;
    }
    let mut cycle_b = [0u8; 8];
    cycle_b.copy_from_slice(&bytes[..8]);
    let cycle = u64::from_le_bytes(cycle_b);
    let mut state_root = [0u8; 32];
    state_root.copy_from_slice(&bytes[8..40]);
    let mut lsu_hash = [0u8; 32];
    lsu_hash.copy_from_slice(&bytes[40..72]);
    let snapshot_name = std::str::from_utf8(&bytes[72..]).ok()?.to_string();
    if snapshot_name.is_empty() {
        return None;
    }
    Some(CheckpointRecord {
        cycle,
        state_root,
        lsu_hash,
        snapshot_name,
    })
}

/// How often to persist a checkpoint after apply (`CATALYST_CHECKPOINT_EVERY_CYCLES`, default `32`).
pub fn checkpoint_every_cycles_from_env() -> u64 {
    const DEFAULT: u64 = 32;
    const MAX: u64 = 1_000_000;
    std::env::var("CATALYST_CHECKPOINT_EVERY_CYCLES")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|&v| v > 0 && v <= MAX)
        .unwrap_or(DEFAULT)
}

pub fn should_write_checkpoint_at_cycle(cycle: u64, every: u64) -> bool {
    every > 0 && cycle > 0 && (cycle % every == 0)
}

async fn set_metadata_u64(store: &StorageManager, key: &str, v: u64) -> Result<()> {
    store
        .set_metadata(key, &v.to_le_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("set_metadata {key}: {e}"))
}

async fn get_metadata_u64(store: &StorageManager, key: &str) -> Option<u64> {
    let bytes = store.get_metadata(key).await.ok().flatten()?;
    if bytes.len() != 8 {
        return None;
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&bytes);
    Some(u64::from_le_bytes(arr))
}

/// After a successful LSU apply, optionally snapshot + record a checkpoint.
pub async fn maybe_write_checkpoint_after_apply(
    store: &StorageManager,
    cycle: u64,
    state_root: [u8; 32],
    lsu_hash: [u8; 32],
) -> Result<()> {
    let every = checkpoint_every_cycles_from_env();
    if !should_write_checkpoint_at_cycle(cycle, every) {
        return Ok(());
    }
    write_checkpoint(store, cycle, state_root, lsu_hash).await
}

/// Persist checkpoint snapshot + metadata for `cycle`.
pub async fn write_checkpoint(
    store: &StorageManager,
    cycle: u64,
    state_root: [u8; 32],
    lsu_hash: [u8; 32],
) -> Result<()> {
    let snapshot_name = snapshot_name_for_cycle(cycle);
    store
        .create_snapshot(&snapshot_name)
        .await
        .with_context(|| format!("create checkpoint snapshot {snapshot_name}"))?;

    let record = CheckpointRecord {
        cycle,
        state_root,
        lsu_hash,
        snapshot_name,
    };
    let blob = encode_checkpoint_blob(&record);
    store
        .set_metadata(&checkpoint_meta_key(cycle), &blob)
        .await
        .map_err(|e| anyhow::anyhow!("set checkpoint metadata: {e}"))?;
    set_metadata_u64(store, META_CHECKPOINT_LATEST_CYCLE, cycle).await?;
    tracing::info!(
        target: "catalyst.consensus.checkpoint",
        cycle,
        state_root = %hex::encode(state_root),
        lsu_hash = %hex::encode(lsu_hash),
        snapshot = %record.snapshot_name,
        "Wrote consensus checkpoint"
    );
    Ok(())
}

/// Latest checkpoint with `cycle <= max_cycle`.
pub async fn find_latest_checkpoint_at_or_below(
    store: &StorageManager,
    max_cycle: u64,
) -> Option<CheckpointRecord> {
    let latest = get_metadata_u64(store, META_CHECKPOINT_LATEST_CYCLE).await?;
    if latest == 0 || latest > max_cycle {
        // Scan downward from max_cycle in case the latest pointer is stale (best-effort).
        // Cycle numbers are wall-clock-derived (~10^9), so the scan MUST be bounded to a small
        // recent window; an unbounded `1..=max_cycle` walk would be billions of async reads.
        const MAX_SCAN: u64 = 4096;
        let lo = max_cycle.saturating_sub(MAX_SCAN).max(1);
        for c in (lo..=max_cycle).rev() {
            if let Some(rec) = load_checkpoint_at_cycle(store, c).await {
                return Some(rec);
            }
        }
        return None;
    }
    load_checkpoint_at_cycle(store, latest).await
}

pub async fn load_checkpoint_at_cycle(
    store: &StorageManager,
    cycle: u64,
) -> Option<CheckpointRecord> {
    let bytes = store
        .get_metadata(&checkpoint_meta_key(cycle))
        .await
        .ok()
        .flatten()?;
    decode_checkpoint_blob(&bytes)
}

/// Load checkpoint snapshot and align applied-head metadata to the checkpoint.
///
/// The full-DB `load_snapshot` reverts ALL RocksDB state including LSU history metadata
/// (`consensus:lsu:{N}`, `consensus:lsu_hash:{N}`, `consensus:lsu_state_root:{N}`) for
/// every cycle after `record.cycle`.  Those keys are essential for the forward replay that
/// immediately follows the checkpoint restore, so we save them before the snapshot and
/// write them back afterwards.
///
/// `current_head` is the locally-applied cycle number before the restore; it bounds the
/// range of metadata we need to preserve.
pub async fn restore_reconcile_from_checkpoint(
    store: &StorageManager,
    record: &CheckpointRecord,
    current_head: u64,
) -> Result<()> {
    // Save post-checkpoint LSU history before the snapshot wipes it.
    let save_start = record.cycle.saturating_add(1);
    let save_end = current_head.saturating_add(1); // inclusive upper bound + 1

    struct SavedCycleMeta {
        cycle: u64,
        lsu_bytes: Option<Vec<u8>>,
        lsu_hash: Option<Vec<u8>>,
        lsu_state_root: Option<Vec<u8>>,
        lsu_cid: Option<Vec<u8>>,
    }

    let mut saved: Vec<SavedCycleMeta> = Vec::new();
    if save_start < save_end {
        for n in save_start..save_end {
            let lsu_bytes = store.get_metadata(&format!("consensus:lsu:{n}")).await.ok().flatten();
            let lsu_hash = store.get_metadata(&format!("consensus:lsu_hash:{n}")).await.ok().flatten();
            let lsu_state_root = store.get_metadata(&format!("consensus:lsu_state_root:{n}")).await.ok().flatten();
            let lsu_cid = store.get_metadata(&format!("consensus:lsu_cid:{n}")).await.ok().flatten();
            if lsu_bytes.is_some() || lsu_state_root.is_some() {
                saved.push(SavedCycleMeta { cycle: n, lsu_bytes, lsu_hash, lsu_state_root, lsu_cid });
            }
        }
    }

    store
        .load_snapshot(&record.snapshot_name)
        .await
        .with_context(|| format!("load checkpoint snapshot {}", record.snapshot_name))?;

    store.set_state_root_cache(record.state_root);
    store
        .set_metadata(
            "consensus:last_applied_cycle",
            &record.cycle.to_le_bytes(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("set last_applied_cycle: {e}"))?;
    store
        .set_metadata("consensus:last_applied_state_root", &record.state_root)
        .await
        .map_err(|e| anyhow::anyhow!("set last_applied_state_root: {e}"))?;
    store
        .set_metadata("consensus:last_applied_lsu_hash", &record.lsu_hash)
        .await
        .map_err(|e| anyhow::anyhow!("set last_applied_lsu_hash: {e}"))?;

    // Re-write the post-checkpoint LSU history so the forward replay can proceed.
    for meta in &saved {
        let n = meta.cycle;
        if let Some(b) = &meta.lsu_bytes {
            let _ = store.set_metadata(&format!("consensus:lsu:{n}"), b).await;
        }
        if let Some(b) = &meta.lsu_hash {
            let _ = store.set_metadata(&format!("consensus:lsu_hash:{n}"), b).await;
        }
        if let Some(b) = &meta.lsu_state_root {
            let _ = store.set_metadata(&format!("consensus:lsu_state_root:{n}"), b).await;
        }
        if let Some(b) = &meta.lsu_cid {
            let _ = store.set_metadata(&format!("consensus:lsu_cid:{n}"), b).await;
        }
    }

    tracing::info!(
        target: "catalyst.consensus.checkpoint",
        cycle = record.cycle,
        state_root = %hex::encode(record.state_root),
        preserved_cycles = saved.len(),
        "Restored reconcile prefix from checkpoint"
    );
    Ok(())
}

async fn lsu_metadata_present(store: &StorageManager, cycle: u64) -> bool {
    store
        .get_metadata(&format!("consensus:lsu:{cycle}"))
        .await
        .ok()
        .flatten()
        .is_some()
}

/// If there is a checkpoint that can bridge the gap up to `target_cycle`, return it.
///
/// Two scenarios are handled identically:
///
/// * **Pruned chains** (`pruned_up_to > 0`): LSU history before `pruned_up_to` was
///   intentionally removed; only a checkpoint can seed the replay.
///
/// * **Wall-clock chains without pruning** (`pruned_up_to == 0`): the chain's first
///   real cycle may be far into the wall-clock sequence (e.g. cycle ~89 million on a
///   fresh testnet), so cycles 1..first_real_cycle are simply absent from storage.
///   A checkpoint written after the first real cycle covers this gap.
///
/// The requirement is: a checkpoint exists, and every cycle strictly *after* the
/// checkpoint (or the pruned boundary, whichever is later) up to `target_cycle` is
/// present locally, so a bounded forward replay from `checkpoint.cycle + 1` can reach
/// `target_cycle`. Cycles at or before that boundary are never inspected: they are
/// always considered "covered" by the checkpoint's restored snapshot (or intentional
/// pruning) regardless of whether metadata for them happens to still exist locally.
///
/// This scan is intentionally bounded to `(safe_gap_end, target_cycle)` — cycle numbers
/// are wall-clock-derived (~10^9), so scanning from literal cycle `1` (as an earlier
/// version of this function did) meant up to ~10^9 individual async storage reads on
/// every call once any checkpoint existed, effectively hanging the caller.
pub async fn reconcile_checkpoint_for_missing_prefix(
    store: &StorageManager,
    target_cycle: u64,
    pruned_up_to: u64,
) -> Option<CheckpointRecord> {
    if target_cycle <= 1 {
        return None;
    }

    // Find the most recent checkpoint at or below target_cycle.
    let cp = find_latest_checkpoint_at_or_below(store, target_cycle.saturating_sub(1)).await?;
    if cp.cycle == 0 {
        return None;
    }

    // Cycles at or before the checkpoint are covered by its snapshot — whether they were
    // explicitly pruned or never existed on a wall-clock chain.
    let safe_gap_end = cp.cycle.max(pruned_up_to);

    for i in safe_gap_end.saturating_add(1)..target_cycle {
        if !lsu_metadata_present(store, i).await {
            // Gap after checkpoint and outside pruned range — forward replay would stall.
            return None;
        }
    }

    Some(cp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyst_storage::{StorageConfig, StorageManager};
    use tempfile::TempDir;

    #[test]
    fn checkpoint_blob_roundtrip() {
        let rec = CheckpointRecord {
            cycle: 42,
            state_root: [1u8; 32],
            lsu_hash: [2u8; 32],
            snapshot_name: "consensus_checkpoint_42".to_string(),
        };
        let blob = encode_checkpoint_blob(&rec);
        let dec = decode_checkpoint_blob(&blob).expect("decode");
        assert_eq!(dec, rec);
    }

    #[test]
    fn should_write_on_interval() {
        assert!(!should_write_checkpoint_at_cycle(0, 8));
        assert!(should_write_checkpoint_at_cycle(8, 8));
        assert!(!should_write_checkpoint_at_cycle(9, 8));
    }

    /// Pruned LSU metadata before `target_cycle` is recoverable via checkpoint restore + replay start.
    #[tokio::test]
    async fn reconcile_uses_checkpoint_after_pruned_lsu_metadata() {
        let td = TempDir::new().unwrap();
        let mut cfg = StorageConfig::default();
        cfg.data_dir = td.path().to_path_buf();
        let store = StorageManager::new(cfg).await.unwrap();

        let cp_cycle = 8u64;
        let state_root = [11u8; 32];
        let lsu_hash = [22u8; 32];
        write_checkpoint(&store, cp_cycle, state_root, lsu_hash)
            .await
            .expect("write checkpoint");

        store
            .set_metadata(crate::pruning::META_PRUNED_UP_TO_CYCLE, &cp_cycle.to_le_bytes())
            .await
            .unwrap();
        for c in 1..cp_cycle {
            assert!(
                !lsu_metadata_present(&store, c).await,
                "simulate pruned LSU metadata for cycle {c}"
            );
        }

        let target_cycle = 12u64;
        for c in cp_cycle.saturating_add(1)..target_cycle {
            store
                .set_metadata(&format!("consensus:lsu:{c}"), b"lsu-stub")
                .await
                .unwrap();
        }

        let cp = reconcile_checkpoint_for_missing_prefix(&store, target_cycle, cp_cycle)
            .await
            .expect("checkpoint for pruned prefix");
        assert_eq!(cp.cycle, cp_cycle);

        restore_reconcile_from_checkpoint(&store, &cp, target_cycle.saturating_sub(1))
            .await
            .expect("restore");

        let last = store
            .get_metadata("consensus:last_applied_cycle")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(u64::from_le_bytes(last.try_into().unwrap()), cp_cycle);

        let replay_start = cp.cycle.saturating_add(1);
        for i in replay_start..target_cycle {
            assert!(
                lsu_metadata_present(&store, i).await,
                "replay should continue from cycle {i}"
            );
        }
    }

    /// Wall-clock chains never have cycles 1..first_real_cycle stored.  Even without pruning
    /// (`pruned_up_to == 0`), a checkpoint written after the first real cycle should be returned
    /// so that reconcile can anchor its replay there instead of failing at cycle 1.
    #[tokio::test]
    async fn checkpoint_found_on_unprunded_wall_clock_chain() {
        let td = TempDir::new().unwrap();
        let mut cfg = StorageConfig::default();
        cfg.data_dir = td.path().to_path_buf();
        let store = StorageManager::new(cfg).await.unwrap();

        // Simulated wall-clock chain: first real cycle is 89_000_000.  Checkpoint at 89_000_032.
        // Cycles 1..89_000_000 are ABSENT (wall-clock gap).
        // Cycles 89_000_000..89_000_064 (=target) ARE present.
        let cp_cycle = 89_000_032u64;
        let target = 89_000_064u64;

        write_checkpoint(&store, cp_cycle, [5u8; 32], [6u8; 32])
            .await
            .unwrap();

        for c in 89_000_000..target {
            store
                .set_metadata(&format!("consensus:lsu:{c}"), b"stub")
                .await
                .unwrap();
        }

        // pruned_up_to == 0 (no pruning), but checkpoint should still be found.
        let cp = reconcile_checkpoint_for_missing_prefix(&store, target, 0)
            .await
            .expect("checkpoint found on unprunded wall-clock chain");
        assert_eq!(cp.cycle, cp_cycle);
    }

    /// The fallback downward scan in `find_latest_checkpoint_at_or_below` must stay bounded: cycle
    /// numbers are wall-clock-derived (~10^9), so an unbounded `1..=max_cycle` walk would be billions
    /// of async reads. With a stale/absent latest pointer and no recent checkpoint, it returns quickly.
    #[tokio::test]
    async fn find_latest_checkpoint_scan_is_bounded() {
        let td = TempDir::new().unwrap();
        let mut cfg = StorageConfig::default();
        cfg.data_dir = td.path().to_path_buf();
        let store = StorageManager::new(cfg).await.unwrap();

        // Latest checkpoint is far ABOVE the query bound, forcing the downward-scan fallback. The scan
        // must stay bounded (it would otherwise walk ~10^9 wall-clock cycles, one async read each).
        write_checkpoint(&store, 89_050_000, [1u8; 32], [2u8; 32])
            .await
            .unwrap();

        let started = std::time::Instant::now();
        let out = find_latest_checkpoint_at_or_below(&store, 89_043_072).await;
        assert!(out.is_none(), "no checkpoint within the bounded window below max_cycle");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "bounded scan must not walk ~10^9 cycles"
        );
    }
}
