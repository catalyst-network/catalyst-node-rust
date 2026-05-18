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
        // Scan downward from max_cycle in case latest pointer is stale (best-effort).
        for c in (1..=max_cycle).rev() {
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
pub async fn restore_reconcile_from_checkpoint(
    store: &StorageManager,
    record: &CheckpointRecord,
) -> Result<()> {
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

    tracing::info!(
        target: "catalyst.consensus.checkpoint",
        cycle = record.cycle,
        state_root = %hex::encode(record.state_root),
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

/// If cycles `1..target_cycle` have gaps only because of pruning, return a checkpoint to start replay.
pub async fn reconcile_checkpoint_for_missing_prefix(
    store: &StorageManager,
    target_cycle: u64,
    pruned_up_to: u64,
) -> Option<CheckpointRecord> {
    if target_cycle <= 1 || pruned_up_to == 0 {
        return None;
    }
    let mut missing_in_pruned_range = false;
    for i in 1..target_cycle {
        if !lsu_metadata_present(store, i).await {
            if i <= pruned_up_to {
                missing_in_pruned_range = true;
            } else {
                return None;
            }
        }
    }
    if !missing_in_pruned_range {
        return None;
    }
    let cp = find_latest_checkpoint_at_or_below(store, target_cycle.saturating_sub(1)).await?;
    if cp.cycle == 0 {
        return None;
    }
  // All missing cycles must lie within the pruned gap at or before the checkpoint.
    for i in 1..target_cycle {
        if !lsu_metadata_present(store, i).await && i > cp.cycle {
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

        restore_reconcile_from_checkpoint(&store, &cp)
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
}
