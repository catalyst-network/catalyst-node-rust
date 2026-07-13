use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

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
use catalyst_consensus::types::{hash_data, LedgerStateUpdateBroadcast};
use catalyst_utils::state::StateManager;

use crate::config::NodeConfig;
use crate::tx::{Mempool, ProtocolTxBatch, ProtocolTxGossip, TxBatch, TxBatchControl, TxGossip};
use crate::sync::{
    FileRequestMsg, FileResponseMsg, KeyProofChange, LsuCidGossip, LsuFinalityAttestationMsg,
    LsuFinalityCidMsg, LsuGossip, LsuRangeRequest, LsuRangeResponse, StateProofBundle,
    StateRootAttestationMsg, StateRootFinalityCidMsg,
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

pub(crate) async fn local_applied_state_root(store: &catalyst_storage::StorageManager) -> [u8; 32] {
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

/// Whether a validator should defer producing/participating in the current wall-clock cycle because
/// its applied head is behind the highest network head it has observed over gossip.
///
/// Cycle numbers are wall-clock-derived and the chain links by `prev_root`; a behind node that
/// produces the current cycle would chain a fresh LSU onto its stale applied root and fork itself
/// off the canonical chain (the root cause of the recurring testnet freeze). This is skip-safe: when
/// we are caught up (`observed_head <= applied`) we never defer, even if intervening wall-clock cycle
/// numbers were legitimately skipped (a slot with no producer quorum). `applied == 0` is the
/// bootstrap / fresh-network case, where we must produce to get the chain started.
fn should_defer_production_when_behind(applied: u64, observed_head: u64) -> bool {
    // Do not gate on `applied != 0`: a freshly-wiped node (applied=0) that receives gossip
    // from a live network (observed_head > 0) must also defer — otherwise it produces from
    // genesis state and forks itself off the canonical chain immediately on restart.
    observed_head > applied
}

/// Decision helper for the `stale_observed_head_reset_ms` liveness backstop: given how long
/// we've made zero applied-head progress and the configured budget, whether to abandon the
/// missing cycle now and reset `observed_head_cycle` down to our own applied head.
///
/// `peer_confirmed_reachable` is true when an `LsuRangeResponse` has positively confirmed a peer
/// has exactly the cycle we need next (see `CatchupState::confirmed_next_cycle`). Caught live: a
/// validator gave up and skipped several cycles ahead on the normal timer alone, while a peer had
/// legitimately applied the very cycle it abandoned — a silent fork with no correctness signal
/// (both nodes kept voting/producing normally afterward). Confirmed-reachable data gets
/// `CONFIRMED_STALE_BUDGET_MULTIPLIER`x longer before giving up, so the fetch pipeline has a real
/// chance to land it, while staying bounded in case that pipeline itself is broken.
fn should_reset_stale_observed_head(elapsed_ms: u64, budget_ms: u64, peer_confirmed_reachable: bool) -> bool {
    const CONFIRMED_STALE_BUDGET_MULTIPLIER: u64 = 5;
    let effective_budget_ms = if peer_confirmed_reachable {
        budget_ms.saturating_mul(CONFIRMED_STALE_BUDGET_MULTIPLIER)
    } else {
        budget_ms
    };
    elapsed_ms > effective_budget_ms
}

/// Deterministic per-cycle batch leader, rotating through the canonical (sorted, deduped) producer
/// set. Liveness: a single stuck/faulty producer must not permanently halt the network. With a fixed
/// `selected.first()` leader, if that node wedges (e.g. it diverged and is catching up, or is down)
/// every cycle blocks on a tx batch that never arrives and the whole network freezes. Rotating the
/// leader by `cycle` means a faulty node only owns ~1/n cycles; the remaining cycles have a healthy
/// leader and the quorum keeps advancing. All nodes derive the identical `selected` set, so the
/// elected leader is consistent fleet-wide without extra coordination.
fn rotating_batch_leader(selected: &[String], cycle: u64) -> Option<String> {
    if selected.is_empty() {
        return None;
    }
    let idx = (cycle % selected.len() as u64) as usize;
    selected.get(idx).cloned()
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

async fn load_stored_certified_hash_at_cycle(
    store: &StorageManager,
    cycle: u64,
) -> Option<[u8; 32]> {
    let b = store
        .get_metadata(&crate::fork_choice::certified_hash_metadata_key(cycle))
        .await
        .ok()
        .flatten()?;
    if b.len() != 32 {
        return None;
    }
    let mut h = [0u8; 32];
    h.copy_from_slice(&b[..32]);
    Some(h)
}

async fn local_applied_lsu_hash(store: &StorageManager) -> Option<[u8; 32]> {
    let b = store
        .get_metadata("consensus:last_applied_lsu_hash")
        .await
        .ok()
        .flatten()?;
    if b.len() != 32 {
        return None;
    }
    let mut h = [0u8; 32];
    h.copy_from_slice(&b[..32]);
    Some(h)
}

/// Gate apply/reconcile: weaker fork, non-cert under certified-only, or certified equivocation stall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ForkChoiceApplyGate {
    Allow,
    RejectWeaker,
    RejectNonCertified,
    StallCertifiedEquivocation,
}

async fn fork_choice_apply_gate(
    store: &StorageManager,
    dfs: Option<&crate::dfs_store::LocalContentStore>,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    cycle: u64,
    lsu_hash: [u8; 32],
    finality_cid_hint: &str,
    certified_only: bool,
) -> ForkChoiceApplyGate {
    let certified = match dfs {
        Some(dfs) => {
            lsu_has_verified_finality_cert(store, dfs, lsu, cycle, finality_cid_hint).await
        }
        None => false,
    };
    let state_root_certified = match dfs {
        Some(dfs) => lsu_has_verified_state_root_cert(store, dfs, lsu, cycle).await,
        None => false,
    };
    let incoming = crate::fork_choice::ForkChoiceCandidate::new(
        cycle,
        lsu_hash,
        lsu,
        certified,
        certified_only,
        state_root_certified,
    );

    if certified_only && incoming.tier != crate::fork_choice::ForkChoiceTier::Certified {
        return ForkChoiceApplyGate::RejectNonCertified;
    }

    if certified {
        let stored_cert = load_stored_certified_hash_at_cycle(store, cycle).await;
        match crate::fork_choice::register_certified_hash(stored_cert, lsu_hash) {
            crate::fork_choice::CertifiedHashRegister::Ok => {
                if stored_cert.is_none() {
                    let _ = store
                        .set_metadata(
                            &crate::fork_choice::certified_hash_metadata_key(cycle),
                            &lsu_hash,
                        )
                        .await;
                }
            }
            crate::fork_choice::CertifiedHashRegister::ConflictingCertified { existing, new } => {
                match crate::fork_choice::certified_equivocation_policy(certified_only) {
                    crate::fork_choice::CertifiedEquivocationPolicy::StallAndAlert => {
                        tracing::error!(
                            target: "catalyst.consensus.fork_choice",
                            cycle,
                            existing_hash = %hex_encode(&existing),
                            new_hash = %hex_encode(&new),
                            "Certified equivocation at cycle: two distinct verified certificates (stalling apply)"
                        );
                        catalyst_utils::increment_counter!(
                            "consensus_certified_equivocation_stall_total",
                            1
                        );
                        return ForkChoiceApplyGate::StallCertifiedEquivocation;
                    }
                    crate::fork_choice::CertifiedEquivocationPolicy::TieBreak => {
                        tracing::warn!(
                            target: "catalyst.consensus.fork_choice",
                            cycle,
                            existing_hash = %hex_encode(&existing),
                            new_hash = %hex_encode(&new),
                            "Certified equivocation: testnet tie-break policy active"
                        );
                        catalyst_utils::increment_counter!(
                            "consensus_certified_equivocation_tiebreak_total",
                            1
                        );
                    }
                }
            }
        }
    }

    if fork_choice_reject_weaker_incoming(store, dfs, &incoming, certified_only).await {
        return ForkChoiceApplyGate::RejectWeaker;
    }

    ForkChoiceApplyGate::Allow
}

/// Reorg when cycle `cycle` is already applied but incoming wins fork choice (certified / tie-break).
async fn try_reorg_stronger_lsu_at_applied_cycle(
    store: &StorageManager,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    cycle: u64,
    expected_prev: [u8; 32],
    expected_post: [u8; 32],
    expected_lsu_hash: &[u8; 32],
    fetch: Option<(&P2pService, &str)>,
    dfs: Option<&crate::dfs_store::LocalContentStore>,
    finality_cid_hint: &str,
    certified_only: bool,
) -> Result<bool> {
    let applied = local_applied_cycle(store).await;
    if applied != cycle {
        return Ok(false);
    }
    let Some(applied_hash) = local_applied_lsu_hash(store).await else {
        return Ok(false);
    };
    if applied_hash == *expected_lsu_hash {
        return Ok(false);
    }
    let incoming =
        fork_choice_candidate_for_lsu(store, dfs, lsu, cycle, *expected_lsu_hash, finality_cid_hint, certified_only)
            .await;
    let applied_candidate = load_fork_choice_candidate_at_cycle(store, dfs, cycle, certified_only)
        .await
        .filter(|c| c.lsu_hash == applied_hash)
        .unwrap_or(crate::fork_choice::ForkChoiceCandidate {
            cycle,
            lsu_hash: applied_hash,
            tier: crate::fork_choice::ForkChoiceTier::None,
            state_root_certified: false,
        });
    if !crate::fork_choice::incoming_beats_stored(&incoming, &applied_candidate) {
        return Ok(false);
    }
    tracing::warn!(
        target: "catalyst.consensus.fork_choice",
        cycle,
        applied_hash = %hex_encode(&applied_hash),
        incoming_hash = %hex_encode(expected_lsu_hash),
        "Reorg: stronger LSU at already-applied cycle"
    );
    try_reconcile_fork_from_quorum_lsu(
        store,
        lsu,
        cycle,
        expected_prev,
        expected_post,
        expected_lsu_hash,
        fetch,
        dfs,
        finality_cid_hint,
        certified_only,
    )
    .await
}

async fn persist_lsu_history(
    store: &catalyst_storage::StorageManager,
    dfs: Option<&crate::dfs_store::LocalContentStore>,
    cycle: u64,
    lsu_bytes: &[u8],
    lsu_hash: &[u8; 32],
    cid: &str,
    state_root: &[u8; 32],
    finality_cid_hint: &str,
    certified_only: bool,
) {
    // Per-cycle history used by RPC/indexers. Best-effort.
    let mut write_canonical_slot = true;
    if let Ok(lsu) = catalyst_consensus::types::LedgerStateUpdate::deserialize(lsu_bytes) {
        let incoming = fork_choice_candidate_for_lsu(
            store,
            dfs,
            &lsu,
            cycle,
            *lsu_hash,
            finality_cid_hint,
            certified_only,
        )
        .await;
        if fork_choice_reject_weaker_incoming(store, dfs, &incoming, certified_only).await {
            write_canonical_slot = false;
        }
    }
    if write_canonical_slot {
        let _ = store
            .set_metadata(&format!("consensus:lsu:{}", cycle), lsu_bytes)
            .await;
        let _ = store
            .set_metadata(&format!("consensus:lsu_hash:{}", cycle), lsu_hash)
            .await;
    }
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

// Tokenomics v1 constants (aligned with docs/tokenomics-spec.md).
// 1 KAT = 1_000_000_000 atoms so reward splits don't floor to zero.
const TOKENOMICS_BLOCK_REWARD_ATOMS: u64 = 1_000_000_000;
const TOKENOMICS_FEE_BURN_BPS: u64 = 7_000;
const TOKENOMICS_FEE_TO_REWARD_POOL_BPS: u64 = 3_000;
const TOKENOMICS_FEE_TO_TREASURY_BPS: u64 = 0;
const TOKENOMICS_WAITING_POOL_REWARD_BPS: u64 = 3_000;
const TOKENOMICS_FEE_CREDITS_ENABLED: bool = true;
const TOKENOMICS_FEE_CREDITS_WARMUP_DAYS: u64 = 14;
const TOKENOMICS_FEE_CREDITS_ACCRUAL_ATOMS_PER_DAY: u64 = 200;
const TOKENOMICS_FEE_CREDITS_MAX_BALANCE_ATOMS: u64 = 6_000;
const TOKENOMICS_FEE_CREDITS_DAILY_SPEND_CAP_ATOMS: u64 = 300;
const TOKENOMICS_WAITING_ELIGIBILITY_CHURN_PENALTY_DAYS: u64 = 3;
const TOKENOMICS_CYCLE_SECONDS: u64 = 20;

fn split_cycle_fees(total_fees: u64) -> (u64, u64, u64) {
    debug_assert_eq!(
        TOKENOMICS_FEE_BURN_BPS
            .saturating_add(TOKENOMICS_FEE_TO_REWARD_POOL_BPS)
            .saturating_add(TOKENOMICS_FEE_TO_TREASURY_BPS),
        10_000
    );
    // Deterministic floor-division split; any remainder is assigned to burn.
    let to_rewards = total_fees.saturating_mul(TOKENOMICS_FEE_TO_REWARD_POOL_BPS) / 10_000;
    let to_treasury = total_fees.saturating_mul(TOKENOMICS_FEE_TO_TREASURY_BPS) / 10_000;
    let used = to_rewards.saturating_add(to_treasury).min(total_fees);
    let burned = total_fees.saturating_sub(used);
    (burned, to_rewards, to_treasury)
}

#[cfg(test)]
mod tokenomics_tests {
    use super::*;
    use catalyst_storage::StorageConfig as StorageConfigLib;
    use catalyst_consensus::types::{LedgerStateUpdate, PartialLedgerStateUpdate};
    use tempfile::TempDir;

    #[test]
    fn split_cycle_fees_is_deterministic_with_remainder_to_burn() {
        assert_eq!(split_cycle_fees(1), (1, 0, 0));
        assert_eq!(split_cycle_fees(10), (7, 3, 0));
        assert_eq!(split_cycle_fees(11), (8, 3, 0));
    }

    #[tokio::test]
    async fn fee_credit_spend_enforces_daily_cap_and_resets_next_day() {
        let td = TempDir::new().unwrap();
        let mut cfg = StorageConfigLib::default();
        cfg.data_dir = td.path().to_path_buf();
        let store = StorageManager::new(cfg).await.unwrap();

        let worker = [9u8; 32];
        set_fee_credit_balance_u64(&store, &worker, 1_000).await.unwrap();

        let same_day_cycle = 5;
        let spent_1 = apply_fee_credit_spend_for_cycle(&store, &worker, 500, same_day_cycle).await;
        assert_eq!(spent_1, TOKENOMICS_FEE_CREDITS_DAILY_SPEND_CAP_ATOMS);

        let spent_2 = apply_fee_credit_spend_for_cycle(&store, &worker, 50, same_day_cycle).await;
        assert_eq!(spent_2, 0);

        let next_day_cycle = same_day_cycle.saturating_add(cycles_per_day());
        let spent_3 = apply_fee_credit_spend_for_cycle(&store, &worker, 100, next_day_cycle).await;
        assert_eq!(spent_3, 100);

        let bal = get_fee_credit_balance_u64(&store, &worker).await;
        assert_eq!(bal, 600);
    }

    #[test]
    fn rotating_batch_leader_rotates_per_cycle_and_is_stable() {
        // Liveness regression: with a fixed `selected.first()` leader, a single wedged producer
        // halted every cycle. The leader must rotate deterministically through the sorted set so a
        // faulty node owns only ~1/n cycles.
        let producers = vec!["aaa".to_string(), "bbb".to_string(), "ccc".to_string()];
        assert_eq!(rotating_batch_leader(&producers, 0).as_deref(), Some("aaa"));
        assert_eq!(rotating_batch_leader(&producers, 1).as_deref(), Some("bbb"));
        assert_eq!(rotating_batch_leader(&producers, 2).as_deref(), Some("ccc"));
        assert_eq!(rotating_batch_leader(&producers, 3).as_deref(), Some("aaa"));
        // Deterministic across nodes for the same (set, cycle).
        assert_eq!(
            rotating_batch_leader(&producers, 89_048_300),
            rotating_batch_leader(&producers, 89_048_300)
        );
        // No single node is leader for every cycle.
        let leaders: std::collections::BTreeSet<_> =
            (0u64..9).filter_map(|c| rotating_batch_leader(&producers, c)).collect();
        assert_eq!(leaders.len(), 3, "all producers must take turns leading");
        assert_eq!(rotating_batch_leader(&[], 5), None);
    }

    #[tokio::test]
    async fn waiting_eligibility_inputs_are_committed_by_state_root() {
        // Consensus-determinism regression (network freeze root cause): worker `first_seen` /
        // `last_registration` and fee-credit `daily_spend` steer waiting-pool reward eligibility and
        // fee-credit settlement, both of which mutate root-covered balances. If these inputs live
        // outside the state root (old behavior: metadata CF), two nodes can share a state root yet
        // disagree on eligibility, paying different rewards and forking the `accounts` root at the
        // cycle a worker crosses the warmup boundary. They MUST be committed by the state root.
        let td = TempDir::new().unwrap();
        let mut cfg = StorageConfigLib::default();
        cfg.data_dir = td.path().to_path_buf();
        let store = StorageManager::new(cfg).await.unwrap();

        let pk = [7u8; 32];
        let r0 = store.commit().await.unwrap();

        ensure_worker_first_seen_cycle(&store, &pk, 100).await;
        let r1 = store.commit().await.unwrap();
        assert_ne!(r0, r1, "first_seen must be committed by the state root");

        set_worker_last_registration_cycle(&store, &pk, 100).await;
        let r2 = store.commit().await.unwrap();
        assert_ne!(r1, r2, "last_registration must be committed by the state root");

        set_fee_credit_daily_spend(
            &store,
            &pk,
            &FeeCreditDailySpend { day_bucket: 1, spent_atoms: 5 },
        )
        .await;
        let r3 = store.commit().await.unwrap();
        assert_ne!(r2, r3, "daily_spend must be committed by the state root");

        // Values round-trip from root-covered state.
        assert_eq!(get_worker_first_seen_cycle(&store, &pk).await, Some(100));
        assert_eq!(get_worker_last_registration_cycle(&store, &pk).await, Some(100));
        assert_eq!(get_fee_credit_daily_spend(&store, &pk).await.spent_atoms, 5);
    }

    #[tokio::test]
    async fn applying_identical_lsu_yields_identical_root_across_independent_stores() {
        // Determinism: applying the same LSU on two independent stores from the same prior state
        // MUST yield the same root. Exercises the reward/eligibility path that previously read
        // non-root-covered metadata.
        async fn fresh_store() -> StorageManager {
            let td = Box::leak(Box::new(TempDir::new().unwrap()));
            let mut cfg = StorageConfigLib::default();
            cfg.data_dir = td.path().to_path_buf();
            StorageManager::new(cfg).await.unwrap()
        }
        let lsu = LedgerStateUpdate {
            partial_update: PartialLedgerStateUpdate {
                transaction_entries: vec![],
                transaction_signatures_hash: [0u8; 32],
                total_fees: 0,
                timestamp: 0,
            },
            compensation_entries: vec![catalyst_consensus::types::CompensationEntry {
                producer_id: String::new(),
                public_key: [3u8; 32],
                amount: 1_000,
            }],
            cycle_number: 42,
            producer_list: vec![],
            vote_list: vec![],
        };
        let a = fresh_store().await;
        let b = fresh_store().await;
        let ra = apply_lsu_to_storage(&a, &lsu).await.unwrap();
        let rb = apply_lsu_to_storage(&b, &lsu).await.unwrap();
        assert_eq!(ra, rb, "same LSU + same prior state must produce the same root");
    }

    #[tokio::test]
    async fn fee_credit_settlement_is_driven_by_lsu_not_local_txids() {
        // Regression (fork-safety): settlement must be reconstructable purely from the applied
        // LSU. A "follower" store that never persisted `cycle_txids` or raw tx bytes must still
        // decrement the fee-credit balance and reimburse the sender identically to the leader.
        let td = TempDir::new().unwrap();
        let mut cfg = StorageConfigLib::default();
        cfg.data_dir = td.path().to_path_buf();
        let store = StorageManager::new(cfg).await.unwrap();

        let sender = [5u8; 32];
        let recipient = [6u8; 32];
        set_fee_credit_balance_u64(&store, &sender, 1_000).await.unwrap();
        set_balance_i64(&store, &sender, 10_000).await.unwrap();

        // Transfer of 100 with a 50 fee, encoded the same way `fee_debit_entries` does:
        // transfers net to zero and the fee is a dedicated negative entry on the sender.
        let sig = vec![0xABu8; 64];
        let lsu = LedgerStateUpdate {
            partial_update: PartialLedgerStateUpdate {
                transaction_entries: vec![
                    catalyst_consensus::types::TransactionEntry {
                        public_key: sender,
                        amount: -100,
                        signature: sig.clone(),
                    },
                    catalyst_consensus::types::TransactionEntry {
                        public_key: recipient,
                        amount: 100,
                        signature: sig.clone(),
                    },
                    catalyst_consensus::types::TransactionEntry {
                        public_key: sender,
                        amount: -50,
                        signature: sig.clone(),
                    },
                ],
                transaction_signatures_hash: [0u8; 32],
                total_fees: 50,
                timestamp: 0,
            },
            compensation_entries: Vec::new(),
            cycle_number: 5,
            producer_list: Vec::new(),
            vote_list: Vec::new(),
        };

        settle_fee_credit_spend_for_applied_cycle(&store, &lsu).await;

        // Fee (50) is under the daily cap, fully covered by credits: credit balance is
        // decremented by 50 and the sender is reimbursed by 50 — with no local txid index.
        assert_eq!(get_fee_credit_balance_u64(&store, &sender).await, 950);
        assert_eq!(get_balance_i64(&store, &sender).await, 10_050);
    }

    #[tokio::test]
    async fn fee_credit_settlement_skips_positive_net_groups() {
        // Faucet/mint-style entries with a non-negative net carry no fee and must not spend
        // any fee credit during settlement.
        let td = TempDir::new().unwrap();
        let mut cfg = StorageConfigLib::default();
        cfg.data_dir = td.path().to_path_buf();
        let store = StorageManager::new(cfg).await.unwrap();

        let recipient = [7u8; 32];
        set_fee_credit_balance_u64(&store, &recipient, 1_000).await.unwrap();

        let sig = vec![0xCDu8; 64];
        let lsu = LedgerStateUpdate {
            partial_update: PartialLedgerStateUpdate {
                transaction_entries: vec![catalyst_consensus::types::TransactionEntry {
                    public_key: recipient,
                    amount: 500,
                    signature: sig,
                }],
                transaction_signatures_hash: [0u8; 32],
                total_fees: 0,
                timestamp: 0,
            },
            compensation_entries: Vec::new(),
            cycle_number: 5,
            producer_list: Vec::new(),
            vote_list: Vec::new(),
        };

        settle_fee_credit_spend_for_applied_cycle(&store, &lsu).await;
        assert_eq!(get_fee_credit_balance_u64(&store, &recipient).await, 1_000);
    }

    #[tokio::test]
    async fn fee_credit_accrual_respects_warmup_boundary() {
        let td = TempDir::new().unwrap();
        let mut cfg = StorageConfigLib::default();
        cfg.data_dir = td.path().to_path_buf();
        let store = StorageManager::new(cfg).await.unwrap();

        let worker = [7u8; 32];
        store
            .set_state(&worker_key_for_pubkey(&worker), vec![1u8])
            .await
            .unwrap();
        ensure_worker_first_seen_cycle(&store, &worker, 1).await;
        set_worker_last_registration_cycle(&store, &worker, 1).await;

        let warmup_cycles = TOKENOMICS_FEE_CREDITS_WARMUP_DAYS.saturating_mul(cycles_per_day());
        let before = 1u64.saturating_add(warmup_cycles).saturating_sub(1);
        let at = 1u64.saturating_add(warmup_cycles);

        let mk_lsu = |cycle: u64| LedgerStateUpdate {
            partial_update: PartialLedgerStateUpdate {
                transaction_entries: Vec::new(),
                transaction_signatures_hash: [0u8; 32],
                total_fees: 10, // ensures non-zero waiting-pool reward in v1 split math
                timestamp: 0,
            },
            compensation_entries: Vec::new(),
            cycle_number: cycle,
            producer_list: Vec::new(),
            vote_list: Vec::new(),
        };

        let bal0 = get_balance_i64(&store, &worker).await;
        distribute_waiting_pool_rewards_and_fee_credits(&store, &mk_lsu(before)).await;
        let bal_before = get_fee_credit_balance_u64(&store, &worker).await;
        assert_eq!(bal_before, 0);
        assert_eq!(get_balance_i64(&store, &worker).await, bal0);

        distribute_waiting_pool_rewards_and_fee_credits(&store, &mk_lsu(at)).await;
        let bal_at = get_fee_credit_balance_u64(&store, &worker).await;
        assert_eq!(bal_at, TOKENOMICS_FEE_CREDITS_ACCRUAL_ATOMS_PER_DAY);
        let reward_from_fees = 10u64.saturating_mul(TOKENOMICS_FEE_TO_REWARD_POOL_BPS) / 10_000;
        let expected_waiting_reward = TOKENOMICS_BLOCK_REWARD_ATOMS
            .saturating_add(reward_from_fees)
            .saturating_mul(TOKENOMICS_WAITING_POOL_REWARD_BPS)
            / 10_000;
        assert_eq!(
            get_balance_i64(&store, &worker).await,
            bal0.saturating_add(expected_waiting_reward as i64)
        );
    }

    #[tokio::test]
    async fn waiting_eligibility_churn_penalty_blocks_rewards_and_credits() {
        let td = TempDir::new().unwrap();
        let mut cfg = StorageConfigLib::default();
        cfg.data_dir = td.path().to_path_buf();
        let store = StorageManager::new(cfg).await.unwrap();

        let worker = [8u8; 32];
        store
            .set_state(&worker_key_for_pubkey(&worker), vec![1u8])
            .await
            .unwrap();

        // Old enough identity, but with a very recent registration cycle.
        let first_seen = 1u64;
        ensure_worker_first_seen_cycle(&store, &worker, first_seen).await;
        let cycle = first_seen
            .saturating_add(TOKENOMICS_FEE_CREDITS_WARMUP_DAYS.saturating_mul(cycles_per_day()))
            .saturating_add(10);
        set_worker_last_registration_cycle(&store, &worker, cycle.saturating_sub(1)).await;

        let lsu = LedgerStateUpdate {
            partial_update: PartialLedgerStateUpdate {
                transaction_entries: Vec::new(),
                transaction_signatures_hash: [0u8; 32],
                total_fees: 10,
                timestamp: 0,
            },
            compensation_entries: Vec::new(),
            cycle_number: cycle,
            producer_list: Vec::new(),
            vote_list: Vec::new(),
        };

        distribute_waiting_pool_rewards_and_fee_credits(&store, &lsu).await;
        assert_eq!(get_fee_credit_balance_u64(&store, &worker).await, 0);
        assert_eq!(get_balance_i64(&store, &worker).await, 0);
    }
}

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

fn fee_credit_balance_key_for_pubkey(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut k = b"feecred:bal:".to_vec();
    k.extend_from_slice(pubkey);
    k
}

// CONSENSUS DETERMINISM: worker `first_seen` / `last_registration` and fee-credit `daily_spend`
// drive waiting-pool reward eligibility and fee-credit settlement, both of which mutate
// root-covered balances. They MUST therefore live in the `accounts` column family (committed by
// the SMT state root) so every node with the same root agrees on them and they are reconciled by
// fork-heal/replay. Storing them in the (non-root-covered, non-reconciled) metadata CF let two
// nodes share a state root yet disagree on eligibility, producing different reward payouts and a
// divergent `accounts` root at the cycle a worker crossed the warmup boundary (network freeze).
//
// These prefixes are byte keys (not `workers:`) so `load_workers_from_state` never mistakes them
// for worker-registration markers.
fn fee_credit_first_seen_cycle_key_for_pubkey(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut k = b"wfs:".to_vec();
    k.extend_from_slice(pubkey);
    k
}

fn worker_last_registration_cycle_key_for_pubkey(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut k = b"wlr:".to_vec();
    k.extend_from_slice(pubkey);
    k
}

fn fee_credit_daily_spend_key_for_pubkey(pubkey: &[u8; 32]) -> Vec<u8> {
    let mut k = b"fcd:".to_vec();
    k.extend_from_slice(pubkey);
    k
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct FeeCreditDailySpend {
    day_bucket: u64,
    spent_atoms: u64,
}

fn cycles_per_day() -> u64 {
    let secs = TOKENOMICS_CYCLE_SECONDS.max(1);
    (86_400 / secs).max(1)
}

fn fee_credit_day_bucket(cycle: u64) -> u64 {
    cycle / cycles_per_day()
}

async fn get_fee_credit_balance_u64(store: &StorageManager, pubkey: &[u8; 32]) -> u64 {
    store
        .get_state(&fee_credit_balance_key_for_pubkey(pubkey))
        .await
        .ok()
        .flatten()
        .and_then(|b| decode_u64(&b))
        .unwrap_or(0)
}

async fn set_fee_credit_balance_u64(store: &StorageManager, pubkey: &[u8; 32], v: u64) -> Result<()> {
    store
        .set_state(&fee_credit_balance_key_for_pubkey(pubkey), encode_u64(v))
        .await?;
    Ok(())
}

async fn get_fee_credit_daily_spend(store: &StorageManager, pubkey: &[u8; 32]) -> FeeCreditDailySpend {
    let Some(bytes) = store
        .get_state(&fee_credit_daily_spend_key_for_pubkey(pubkey))
        .await
        .ok()
        .flatten()
    else {
        return FeeCreditDailySpend::default();
    };
    bincode::deserialize::<FeeCreditDailySpend>(&bytes).unwrap_or_default()
}

async fn set_fee_credit_daily_spend(
    store: &StorageManager,
    pubkey: &[u8; 32],
    daily: &FeeCreditDailySpend,
) {
    if let Ok(bytes) = bincode::serialize(daily) {
        let _ = store
            .set_state(&fee_credit_daily_spend_key_for_pubkey(pubkey), bytes)
            .await;
    }
}

async fn fee_credit_spendable_for_cycle(
    store: &StorageManager,
    pubkey: &[u8; 32],
    fee_due_atoms: u64,
    cycle: u64,
) -> u64 {
    if !TOKENOMICS_FEE_CREDITS_ENABLED || fee_due_atoms == 0 {
        return 0;
    }
    let bal = get_fee_credit_balance_u64(store, pubkey).await;
    if bal == 0 {
        return 0;
    }
    let day = fee_credit_day_bucket(cycle);
    let daily = get_fee_credit_daily_spend(store, pubkey).await;
    let spent_today = if daily.day_bucket == day { daily.spent_atoms } else { 0 };
    let daily_remaining =
        TOKENOMICS_FEE_CREDITS_DAILY_SPEND_CAP_ATOMS.saturating_sub(spent_today);
    fee_due_atoms.min(bal).min(daily_remaining)
}

async fn apply_fee_credit_spend_for_cycle(
    store: &StorageManager,
    pubkey: &[u8; 32],
    spend_atoms: u64,
    cycle: u64,
) -> u64 {
    if spend_atoms == 0 {
        return 0;
    }
    let allowed = fee_credit_spendable_for_cycle(store, pubkey, spend_atoms, cycle).await;
    if allowed == 0 {
        return 0;
    }
    let bal = get_fee_credit_balance_u64(store, pubkey).await;
    let next_bal = bal.saturating_sub(allowed);
    let _ = set_fee_credit_balance_u64(store, pubkey, next_bal).await;

    let day = fee_credit_day_bucket(cycle);
    let mut daily = get_fee_credit_daily_spend(store, pubkey).await;
    if daily.day_bucket != day {
        daily.day_bucket = day;
        daily.spent_atoms = 0;
    }
    daily.spent_atoms = daily.spent_atoms.saturating_add(allowed);
    set_fee_credit_daily_spend(store, pubkey, &daily).await;
    allowed
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

/// Verifies a single `KeyProofChange` leg (old or new) against its claimed root, binding the
/// proof to `key`/`value` instead of trusting the proof's own claims (see
/// `catalyst_storage::sparse_merkle::verify_proof_for_key`). `old_value`/`new_value` use an
/// empty `Vec` to represent "key absent" on the wire (`ov.unwrap_or_default()` when building the
/// bundle) — detect that case via the proof's leaf hash (an absence proof's leaf is the fixed
/// `empty_leaf_hash()`, which a real key/value leaf can't collide with, different domain byte)
/// rather than trusting an attacker-supplied empty value on its own.
fn verify_state_proof_for_change(
    root: &catalyst_utils::Hash,
    key: &[u8],
    value: &[u8],
    proof: &catalyst_storage::merkle::MerkleProof,
) -> bool {
    if proof.leaf == catalyst_storage::sparse_merkle::empty_leaf_hash() {
        value.is_empty() && catalyst_storage::sparse_merkle::verify_absence_proof_for_key(root, key, proof)
    } else {
        catalyst_storage::sparse_merkle::verify_proof_for_key(root, key, value, proof)
    }
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
        if !verify_state_proof_for_change(&bundle.prev_state_root, &c.key, &c.old_value, &c.old_proof) {
            return false;
        }
        if !verify_state_proof_for_change(&bundle.new_state_root, &c.key, &c.new_value, &c.new_proof) {
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

/// Process-scoped mutex that serialises all LSU-to-storage applies.
///
/// Three independent tokio tasks (gossip receiver, advance ticker, consensus runner) can all
/// call `apply_lsu_to_storage[_without_root_check]` concurrently.  The per-cycle idempotence
/// check inside those functions is a read-then-write and therefore not atomic: every concurrent
/// caller reads `last_applied_cycle = N-1` before any of them writes `N`, so all proceed and
/// apply the same cycle multiple times from different intermediate states, corrupting the chain.
/// Holding this lock for the full duration of each apply collapses concurrent callers into a
/// strict sequence; the idempotence check then correctly gates out duplicates.
fn lsu_apply_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[derive(Debug, Clone, Copy)]
struct ReconcileFailureEntry {
    consecutive_failures: u32,
    last_seen_ms: u64,
    /// Once tripped, stays tripped for the process lifetime: the underlying cause needs an
    /// operator (restart, resync, or a code fix), not a timer.
    broken: bool,
}

const RECONCILE_FAILURE_TRACKER_MAX_ENTRIES: usize = 2000;
const RECONCILE_FAILURE_TRACKER_TARGET_ENTRIES: usize = 1500;
const RECONCILE_FAILURE_TRACKER_RETENTION_MS: u64 = 10 * 60 * 1000;

/// Tracks consecutive `try_reconcile_fork_from_quorum_lsu` failures per **`local_cycle`** (the
/// node's own stuck applied-head position at the time of the attempt) to back the circuit breaker
/// (see `consensus_limits::reconcile_circuit_break_threshold`), so a genuinely unrecoverable
/// divergence stops retrying identically forever at WARN/ERROR level with no operator signal.
///
/// Keyed on `local_cycle`, NOT on the target `(cycle, lsu_hash)` being reconciled *to*: a stuck
/// node gets re-triggered with a *new, ever-increasing* target cycle/hash on every subsequent
/// wall-clock tick (each newly-gossiped LSU is a fresh reconcile attempt), so a target-keyed
/// breaker never accumulates enough repeats on the same key to trip — this was an actual bug in
/// the first version of this breaker, caught live: a node stuck unable to bridge a genuinely
/// missing gap cycle kept calling `resolve_replay_start` (and logging
/// "Quorum fork reconcile FAILED: missing stored LSU...") every ~20s indefinitely, because that
/// failure path returns before ever reaching the target-keyed check. `local_cycle` is the thing
/// that's actually invariant while stuck, and naturally stops mattering the moment it advances.
///
/// Bounded like `RelayCache` above: prune stale (non-broken) entries by timestamp, then cap by
/// count if still oversized.
struct ReconcileFailureTracker {
    entries: std::collections::HashMap<u64, ReconcileFailureEntry>,
}

impl ReconcileFailureTracker {
    fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    fn prune(&mut self, now_ms: u64) {
        let keep_after = now_ms.saturating_sub(RECONCILE_FAILURE_TRACKER_RETENTION_MS);
        self.entries
            .retain(|_, e| e.broken || e.last_seen_ms >= keep_after);
        if self.entries.len() > RECONCILE_FAILURE_TRACKER_MAX_ENTRIES {
            let mut v: Vec<(u64, u64)> = self
                .entries
                .iter()
                .map(|(k, e)| (*k, e.last_seen_ms))
                .collect();
            v.sort_by_key(|(_, ts)| *ts);
            let drop_n = v
                .len()
                .saturating_sub(RECONCILE_FAILURE_TRACKER_TARGET_ENTRIES);
            for (k, _) in v.into_iter().take(drop_n) {
                self.entries.remove(&k);
            }
        }
    }
}

fn reconcile_failure_tracker() -> &'static tokio::sync::Mutex<ReconcileFailureTracker> {
    static TRACKER: std::sync::OnceLock<tokio::sync::Mutex<ReconcileFailureTracker>> =
        std::sync::OnceLock::new();
    TRACKER.get_or_init(|| tokio::sync::Mutex::new(ReconcileFailureTracker::new()))
}

/// True if reconcile attempts while stuck at this `local_cycle` should be skipped entirely — the
/// circuit breaker has tripped.
async fn reconcile_circuit_is_broken(local_cycle: u64) -> bool {
    let tracker = reconcile_failure_tracker().lock().await;
    tracker
        .entries
        .get(&local_cycle)
        .map(|e| e.broken)
        .unwrap_or(false)
}

/// Records a reconcile failure while stuck at `local_cycle`, tripping the circuit breaker after
/// `reconcile_circuit_break_threshold()` consecutive failures within
/// `reconcile_circuit_break_window_ms()` of each other. Returns true exactly on the transition
/// into "broken" (so the caller can emit a one-time escalated log instead of repeating it).
async fn record_reconcile_failure(local_cycle: u64) -> bool {
    let now_ms = current_timestamp_ms();
    let mut tracker = reconcile_failure_tracker().lock().await;
    tracker.prune(now_ms);
    let window_ms = crate::consensus_limits::reconcile_circuit_break_window_ms();
    let threshold = crate::consensus_limits::reconcile_circuit_break_threshold();
    let entry = tracker
        .entries
        .entry(local_cycle)
        .or_insert(ReconcileFailureEntry {
            consecutive_failures: 0,
            last_seen_ms: now_ms,
            broken: false,
        });
    if now_ms.saturating_sub(entry.last_seen_ms) > window_ms {
        // Gap too long since the last failure: treat this as a fresh sequence.
        entry.consecutive_failures = 0;
    }
    entry.consecutive_failures = entry.consecutive_failures.saturating_add(1);
    entry.last_seen_ms = now_ms;
    if !entry.broken && entry.consecutive_failures >= threshold {
        entry.broken = true;
        return true;
    }
    false
}

/// Whether a locally-produced LSU for the *next* cycle must be refused rather than applied,
/// because reconcile's circuit breaker is open for the applied head it would be built on top of
/// (a divergent/unrecoverable local state, not merely a behind one). Returns the applied cycle
/// the caller should log/report if blocking is required.
///
/// Complements `should_defer_production_when_behind` (which handles *behind*): this handles
/// *diverged-but-not-behind* — a node whose own local head keeps advancing via self-production
/// is never "behind" by that gate's definition, so without this check it would keep minting new
/// cycles on top of state its own reconcile has already given up trying to heal. Caught live via
/// the consensus-follower-batch-drop-e2e (§7.2) harness: a node cut off from P2P self-produced
/// and applied several cycles on its own already-diverged state after reconcile circuit-broke.
async fn should_block_self_produced_apply(store: &catalyst_storage::StorageManager) -> Option<u64> {
    let applied = local_applied_cycle(store).await;
    if reconcile_circuit_is_broken(applied).await {
        Some(applied)
    } else {
        None
    }
}

/// Clears any failure history for `local_cycle` on a successful reconcile.
async fn clear_reconcile_failure(local_cycle: u64) {
    let mut tracker = reconcile_failure_tracker().lock().await;
    tracker.entries.remove(&local_cycle);
}

#[cfg(test)]
async fn reconcile_failure_state_for_test(local_cycle: u64) -> Option<(u32, bool)> {
    let tracker = reconcile_failure_tracker().lock().await;
    tracker
        .entries
        .get(&local_cycle)
        .map(|e| (e.consecutive_failures, e.broken))
}

/// Despite the name (kept for call-site continuity — both callers already treat a falsy result
/// as "fall back to the verifying path"), this now DOES verify: `new_root` is a peer's *claim*
/// (from a `StateProofBundle` whose Merkle proofs only cover the touched keys, not this node's
/// full prior state), and it used to be cached via `set_state_root_cache` without ever comparing
/// it to a fresh recompute over this node's actual `accounts` column family. If this node's own
/// prior state had ever silently diverged from the peer's (from any cause), that divergence was
/// accepted as fact here and baked in permanently. Returns `Ok(true)` only if a full recompute
/// matches the claim; `Ok(false)` on mismatch (mutations are reverted first — the caller's
/// existing `apply_lsu_with_root_check` fallback then re-derives the state honestly).
///
/// NOTE: this path intentionally has no cycle-number idempotence guard. It is used both
/// for forward application (proof-verified, chains onto the current root) and for same-cycle
/// reorg/reconcile that replaces the applied head's LSU with a stronger certified one; the callers
/// are responsible for verifying chaining / fork-choice before invoking it.
async fn apply_lsu_to_storage_without_root_check(
    store: &StorageManager,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    new_root: [u8; 32],
) -> Result<bool> {
    let _apply_guard = lsu_apply_lock().lock().await;

    let snap = format!("trusted_apply_verify_{}_{}", lsu.cycle_number, current_timestamp_ms());
    store
        .create_snapshot(&snap)
        .await
        .map_err(|e| anyhow::anyhow!("snapshot create failed: {e}"))?;

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
            ensure_worker_first_seen_cycle(store, &e.public_key, lsu.cycle_number).await;
            set_worker_last_registration_cycle(store, &e.public_key, lsu.cycle_number).await;
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

    // Apply producer compensation entries (issuance + producer fee share).
    for c in &lsu.compensation_entries {
        if c.amount == 0 {
            continue;
        }
        let cur = get_balance_i64(store, &c.public_key).await;
        let _ = set_balance_i64(store, &c.public_key, cur.saturating_add(c.amount as i64)).await;
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

    // Fee-credit spend reimbursement and waiting-pool rewards/credits are deterministic from cycle LSU.
    settle_fee_credit_spend_for_applied_cycle(store, lsu).await;
    distribute_waiting_pool_rewards_and_fee_credits(store, lsu).await;

    // Verify the claimed root against a full recompute over this node's actual `accounts` state
    // — do not trust `new_root` (see doc comment above). On mismatch, revert every mutation made
    // above and let the caller fall back to `apply_lsu_with_root_check`, which re-derives the
    // state honestly instead of silently adopting a peer's unverified claim.
    let recomputed = store
        .commit()
        .await
        .map_err(|e| anyhow::anyhow!("state root recompute failed: {e}"))?;
    if recomputed != new_root {
        tracing::warn!(
            target: "catalyst.consensus.apply",
            cycle = lsu.cycle_number,
            claimed = %hex_encode(&new_root),
            recomputed = %hex_encode(&recomputed),
            "Trusted apply path: claimed state_root did not match local recompute; reverting and falling back to verified apply"
        );
        catalyst_utils::increment_counter!("consensus_trusted_apply_root_mismatch_total", 1);
        let _ = store.load_snapshot(&snap).await;
        let _ = store.delete_snapshot(&snap).await;
        return Ok(false);
    }
    let _ = store.delete_snapshot(&snap).await;

    // Persist head metadata (verified above, not merely trusted).
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

    // Guarantee the per-cycle history record exists — see the matching comment in
    // `apply_lsu_to_storage`. This path (proof-driven fast catch-up) is exactly the one observed
    // in production leaving holes in local history when it was the only apply that ran for a cycle.
    if let Ok(lsu_bytes) = lsu.serialize() {
        let _ = store
            .set_metadata(&format!("consensus:lsu:{}", cycle), &lsu_bytes)
            .await;
        let _ = store
            .set_metadata(&format!("consensus:lsu_hash:{}", cycle), &lsu_hash)
            .await;
    }
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

    // Flush (root already recomputed and verified above via `store.commit()`).
    for cf_name in store.engine().cf_names() {
        let _ = store.engine().flush_cf(&cf_name);
    }
    Ok(true)
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

pub(crate) async fn ensure_chain_identity_and_genesis(
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

async fn protocol_tx_is_funded_with_fee_credits(
    store: &StorageManager,
    tx: &catalyst_core::protocol::Transaction,
    cycle: u64,
) -> bool {
    use std::collections::HashMap;
    let Some(sender) = tx_sender_pubkey(tx) else {
        return false;
    };
    let mut deltas: HashMap<[u8; 32], i64> = HashMap::new();
    for e in &tx.core.entries {
        let v = match e.amount {
            catalyst_core::protocol::EntryAmount::NonConfidential(v) => v,
            catalyst_core::protocol::EntryAmount::Confidential { .. } => return false,
        };
        *deltas.entry(e.public_key).or_insert(0) =
            deltas.get(&e.public_key).copied().unwrap_or(0).saturating_add(v);
    }

    let mut fee_due = tx.core.fees;
    if TOKENOMICS_FEE_CREDITS_ENABLED && fee_due > 0 {
        let spendable = fee_credit_spendable_for_cycle(store, &sender, fee_due, cycle).await;
        fee_due = fee_due.saturating_sub(spendable);
    }
    if let Ok(fee_i64) = i64::try_from(fee_due) {
        if fee_i64 > 0 {
            *deltas.entry(sender).or_insert(0) = deltas
                .get(&sender)
                .copied()
                .unwrap_or(0)
                .saturating_sub(fee_i64);
        }
    } else {
        return false;
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

/// Best-effort registration of counters used by consensus P2P ingress, fork reconcile, and LSU apply paths
/// (`increment_counter!` is a no-op unless the metric name exists in the global registry).
pub(crate) fn ensure_consensus_p2p_cli_metrics_registered() {
    use catalyst_utils::metrics::{
        get_metrics_registry, init_metrics, Metric, MetricCategory, MetricType,
    };
    if get_metrics_registry().is_none() {
        let _ = init_metrics();
    }
    let Some(registry) = get_metrics_registry() else {
        return;
    };
    let Ok(mut guard) = registry.lock() else {
        return;
    };
    const EXTRA: &[(&str, &str)] = &[
        (
            "consensus_p2p_tx_batch_cycle_rejected_total",
            "Protocol or legacy TxBatch dropped at ingress: cycle outside wall slack window",
        ),
        (
            "consensus_p2p_tx_batch_commit_cycle_rejected_total",
            "TxBatchControl::Commit ignored at ingress: cycle outside wall slack window",
        ),
        (
            "consensus_p2p_tx_batch_commit_disagrees_total",
            "ProtocolTxBatch dropped: disagrees with pinned TxBatchControl::Commit",
        ),
        (
            "consensus_p2p_lsu_cycle_rejected_total",
            "LSU-related P2P payload dropped at ingress: cycle too far ahead of wall clock",
        ),
        (
            "consensus_lsu_gossip_apply_rejected_finality_total",
            "LsuGossip not applied/relayed: finality policy or missing DFS / certificate",
        ),
        (
            "consensus_fork_reconcile_succeeded_total",
            "BFT-quorum fork reconcile replay completed and applied target LSU",
        ),
        (
            "consensus_fork_reconcile_aborted_total",
            "Fork reconcile not started or rolled back (prechecks, missing history, inner replay failure)",
        ),
        (
            "consensus_fork_reconcile_error_total",
            "Fork reconcile snapshot restore failed after replay error",
        ),
        (
            "consensus_fork_reconcile_noop_already_applied_total",
            "Fork reconcile short-circuited: fresh check confirmed the exact LSU was already applied at this cycle (TOCTOU self-echo)",
        ),
        (
            "consensus_fork_reconcile_no_checkpoint_total",
            "Fork reconcile aborted specifically because no checkpoint or peer bridged a missing LSU-history gap (operator action needed: db-backup/db-restore from a healthy peer, or wait for the next periodic checkpoint)",
        ),
        (
            "consensus_fork_reconcile_circuit_broken_total",
            "Fork reconcile skipped: consecutive-failure circuit breaker open for this (cycle, lsu_hash); needs operator action",
        ),
        (
            "consensus_lsu_apply_skipped_prev_root_total",
            "LsuCidGossip apply skipped after prev_root mismatch and reconcile did not complete",
        ),
        (
            "consensus_trusted_apply_root_mismatch_total",
            "Proof-driven fast-apply path: peer's claimed state_root did not match a local recompute; reverted and fell back to verified apply",
        ),
        (
            "consensus_fork_choice_rejected_weaker_total",
            "Competing LSU at same cycle rejected: weaker fork-choice rank (tier or lsu_hash tie-break)",
        ),
        (
            "consensus_fork_choice_rejected_non_certified_total",
            "LSU rejected: certified-only fork choice policy (no verified finality certificate)",
        ),
        (
            "consensus_certified_equivocation_stall_total",
            "Two distinct verified certificates at same cycle; apply stalled (production policy)",
        ),
        (
            "consensus_certified_equivocation_tiebreak_total",
            "Two distinct verified certificates at same cycle; testnet tie-break policy",
        ),
        (
            "consensus_state_root_certificate_published_total",
            "ADR 0002 LsuStateRootCertificateV1 published: BFT quorum of state-root attestations reached for a cycle",
        ),
        (
            "consensus_self_produced_apply_blocked_circuit_broken_total",
            "Locally-produced LSU not applied: reconcile circuit breaker is open for the current applied head (divergent/unrecoverable local state); needs operator action",
        ),
    ];
    for (name, desc) in EXTRA {
        let _ = guard.register_metric(Metric::new(
            (*name).to_string(),
            MetricCategory::Consensus,
            MetricType::Counter,
            desc.to_string(),
        ));
    }
}

fn parse_producer_id_pubkey(producer_id: &str) -> Option<[u8; 32]> {
    parse_hex_32(producer_id)
}

async fn ensure_worker_first_seen_cycle(store: &StorageManager, pubkey: &[u8; 32], cycle: u64) {
    let key = fee_credit_first_seen_cycle_key_for_pubkey(pubkey);
    if store.get_state(&key).await.ok().flatten().is_none() {
        let _ = store.set_state(&key, cycle.to_le_bytes().to_vec()).await;
    }
}

async fn set_worker_last_registration_cycle(store: &StorageManager, pubkey: &[u8; 32], cycle: u64) {
    let _ = store
        .set_state(&worker_last_registration_cycle_key_for_pubkey(pubkey), cycle.to_le_bytes().to_vec())
        .await;
}

async fn get_worker_last_registration_cycle(store: &StorageManager, pubkey: &[u8; 32]) -> Option<u64> {
    let bytes = store
        .get_state(&worker_last_registration_cycle_key_for_pubkey(pubkey))
        .await
        .ok()
        .flatten()?;
    decode_u64(&bytes)
}

async fn get_worker_first_seen_cycle(store: &StorageManager, pubkey: &[u8; 32]) -> Option<u64> {
    let bytes = store
        .get_state(&fee_credit_first_seen_cycle_key_for_pubkey(pubkey))
        .await
        .ok()
        .flatten()?;
    decode_u64(&bytes)
}

async fn waiting_worker_is_eligible(
    store: &StorageManager,
    pubkey: &[u8; 32],
    cycle: u64,
) -> bool {
    // Sybil resistance baseline for v1:
    // - identity must age through warmup before earning waiting-pool rewards/credits
    // - newly re-registered identities are blocked for a churn penalty window
    let Some(first_seen) = get_worker_first_seen_cycle(store, pubkey).await else {
        return false;
    };
    let warmup_cycles = TOKENOMICS_FEE_CREDITS_WARMUP_DAYS.saturating_mul(cycles_per_day());
    if cycle < first_seen.saturating_add(warmup_cycles) {
        return false;
    }
    if let Some(last_reg) = get_worker_last_registration_cycle(store, pubkey).await {
        let churn_cycles =
            TOKENOMICS_WAITING_ELIGIBILITY_CHURN_PENALTY_DAYS.saturating_mul(cycles_per_day());
        if cycle < last_reg.saturating_add(churn_cycles) {
            return false;
        }
    }
    true
}

async fn distribute_waiting_pool_rewards_and_fee_credits(
    store: &StorageManager,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
) {
    let mut producers: std::collections::BTreeSet<[u8; 32]> = std::collections::BTreeSet::new();
    for id in &lsu.producer_list {
        if let Some(pk) = parse_producer_id_pubkey(id) {
            producers.insert(pk);
        }
    }

    let total_fees = lsu.partial_update.total_fees;
    let (_burned_fees, reward_from_fees, _to_treasury) = split_cycle_fees(total_fees);
    let total_reward_pool = TOKENOMICS_BLOCK_REWARD_ATOMS.saturating_add(reward_from_fees);
    let waiting_pool_from_formula =
        total_reward_pool.saturating_mul(TOKENOMICS_WAITING_POOL_REWARD_BPS) / 10_000;
    let paid_to_producers: u64 = lsu.compensation_entries.iter().map(|e| e.amount).sum();
    let waiting_pool = waiting_pool_from_formula.min(total_reward_pool.saturating_sub(paid_to_producers));
    if waiting_pool == 0 && !TOKENOMICS_FEE_CREDITS_ENABLED {
        return;
    }

    let mut workers = load_workers_from_state(store);
    workers.sort();
    workers.dedup();
    let mut waiting: Vec<[u8; 32]> = Vec::new();
    for pk in workers {
        if producers.contains(&pk) {
            continue;
        }
        if waiting_worker_is_eligible(store, &pk, lsu.cycle_number).await {
            waiting.push(pk);
        }
    }
    if waiting.is_empty() {
        return;
    }

    let n = waiting.len() as u64;
    let per = waiting_pool / n;
    let mut rem = waiting_pool % n;
    for pk in waiting {
        if per > 0 || rem > 0 {
            let bonus = if rem > 0 {
                rem -= 1;
                1
            } else {
                0
            };
            let add = per.saturating_add(bonus);
            let cur = get_balance_i64(store, &pk).await;
            let _ = set_balance_i64(store, &pk, cur.saturating_add(add as i64)).await;
        }

        if TOKENOMICS_FEE_CREDITS_ENABLED {
            let cur = get_fee_credit_balance_u64(store, &pk).await;
            let next = cur
                .saturating_add(TOKENOMICS_FEE_CREDITS_ACCRUAL_ATOMS_PER_DAY)
                .min(TOKENOMICS_FEE_CREDITS_MAX_BALANCE_ATOMS);
            let _ = set_fee_credit_balance_u64(store, &pk, next).await;
        }
    }
}

/// Settle fee-credit spending for the applied cycle.
///
/// CRITICAL (consensus determinism / fork-safety): this MUST be a pure function of the
/// applied LSU. The previous implementation read `load_cycle_txids` + the raw transaction
/// `fees` from node-local metadata, which only the cycle's batch leader persists. Followers
/// found no txids and skipped settlement entirely, so on any cycle that spent fee credits the
/// leader credited/decremented balances that followers did not — permanently offsetting that
/// node's `accounts` state root by one cycle and forking it off the canonical chain (with no
/// path to auto-heal). Every node applies the same LSU, so we reconstruct each transaction's
/// (sender, fee) directly from the LSU's transaction entries instead.
///
/// Fee reconstruction: `fee_debit_entries` charges the full declared fee as a dedicated
/// negative entry under the transaction's signature boundary. Transfer entries net to zero
/// within a boundary, so the net-negative remainder of a signature group is exactly the fee.
async fn settle_fee_credit_spend_for_applied_cycle(
    store: &StorageManager,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
) {
    if !TOKENOMICS_FEE_CREDITS_ENABLED {
        return;
    }
    let cycle = lsu.cycle_number;

    // Group entries by signature (the tx-boundary marker), iterating in deterministic order.
    use std::collections::BTreeMap;
    let mut by_sig: BTreeMap<Vec<u8>, Vec<&catalyst_consensus::types::TransactionEntry>> =
        BTreeMap::new();
    for e in &lsu.partial_update.transaction_entries {
        by_sig.entry(e.signature.clone()).or_default().push(e);
    }

    for (_sig, entries) in by_sig {
        // The full fee is the net-negative remainder of the boundary (transfers net to zero).
        let net: i128 = entries.iter().map(|e| e.amount as i128).sum();
        if net >= 0 {
            continue;
        }
        let fee = (-net).min(u64::MAX as i128) as u64;
        if fee == 0 {
            continue;
        }

        // Sender selection mirrors nonce attribution: a single negative-amount pubkey, else
        // the worker-registration / EVM marker pubkey.
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
            if let Some(e0) = entries.iter().find(|e| is_worker_reg_marker(&e.signature)) {
                sender = Some(e0.public_key);
            }
        }
        if sender.is_none() {
            if let Some(e0) = entries.iter().find(|e| is_evm_marker(&e.signature)) {
                sender = Some(e0.public_key);
            }
        }
        let Some(sender) = sender else {
            continue;
        };

        let spend = apply_fee_credit_spend_for_cycle(store, &sender, fee, cycle).await;
        if spend > 0 {
            let cur = get_balance_i64(store, &sender).await;
            let _ = set_balance_i64(store, &sender, cur.saturating_add(spend as i64)).await;
        }
    }
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

pub(crate) fn tx_to_consensus_entries(tx: &catalyst_core::protocol::Transaction) -> Vec<catalyst_consensus::types::TransactionEntry> {
    // Reuse the canonical protocol->consensus conversion to keep fee-debit handling
    // consistent across mempool admission and cycle construction.
    let msg = ProtocolTxGossip {
        tx_id: [0u8; 32],
        tx: tx.clone(),
        received_at_ms: tx.timestamp,
    };
    msg.to_consensus_entries()
}

pub(crate) async fn validate_and_select_protocol_txs_for_construction(
    store: &StorageManager,
    cycle: u64,
    txs: Vec<catalyst_core::protocol::Transaction>,
    max_entries: usize,
) -> Vec<catalyst_core::protocol::Transaction> {
    use std::collections::HashMap;

    // Local simulation state.
    let mut balances: HashMap<[u8; 32], i64> = HashMap::new();
    let mut nonces: HashMap<[u8; 32], u64> = HashMap::new();
    let mut fee_credit_balances: HashMap<[u8; 32], u64> = HashMap::new();
    let mut fee_credit_spent_today: HashMap<[u8; 32], u64> = HashMap::new();

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
        let mut fee_due_tokens = tx.core.fees;
        if TOKENOMICS_FEE_CREDITS_ENABLED && fee_due_tokens > 0 {
            let bal = if let Some(v) = fee_credit_balances.get(&sender).copied() {
                v
            } else {
                let v = get_fee_credit_balance_u64(store, &sender).await;
                fee_credit_balances.insert(sender, v);
                v
            };
            let day = fee_credit_day_bucket(cycle);
            let spent = if let Some(v) = fee_credit_spent_today.get(&sender).copied() {
                v
            } else {
                let daily = get_fee_credit_daily_spend(store, &sender).await;
                let v = if daily.day_bucket == day { daily.spent_atoms } else { 0 };
                fee_credit_spent_today.insert(sender, v);
                v
            };
            let remaining = TOKENOMICS_FEE_CREDITS_DAILY_SPEND_CAP_ATOMS.saturating_sub(spent);
            let credit_spend = fee_due_tokens.min(bal).min(remaining);
            if credit_spend > 0 {
                fee_due_tokens = fee_due_tokens.saturating_sub(credit_spend);
            }
        }
        if let Ok(fee_i64) = i64::try_from(fee_due_tokens) {
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
        if TOKENOMICS_FEE_CREDITS_ENABLED && tx.core.fees > fee_due_tokens {
            let credit_spend = tx.core.fees.saturating_sub(fee_due_tokens);
            let cur_bal = fee_credit_balances.get(&sender).copied().unwrap_or(0);
            fee_credit_balances.insert(sender, cur_bal.saturating_sub(credit_spend));
            let cur_spent = fee_credit_spent_today.get(&sender).copied().unwrap_or(0);
            fee_credit_spent_today.insert(sender, cur_spent.saturating_add(credit_spend));
        }
        nonces.insert(sender, tx.core.nonce);
        used_entries += entry_count;
        out.push(tx);
    }

    out
}

pub(crate) fn mempool_txid(tx: &catalyst_core::protocol::Transaction) -> Option<[u8; 32]> {
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

pub(crate) async fn persist_cycle_txids(store: &StorageManager, cycle: u64, mut txids: Vec<[u8; 32]>) {
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

/// Wall-clock budget for followers to obtain the canonical `ProtocolTxBatch` (WAN-safe default).
fn catalyst_tx_batch_wait_budget_ms(cycle_ms: u64) -> u64 {
    crate::consensus_limits::tx_batch_follower_wait_budget_ms(cycle_ms)
}

/// **Unsafe:** if true, a producer may run Construction with an empty tx list when the batch
/// could not be recovered and no `TxBatchControl::Commit` was seen (pre-upgrade peers only).
fn catalyst_allow_legacy_ambiguous_empty_tx_batch() -> bool {
    matches!(
        std::env::var("CATALYST_ALLOW_LEGACY_AMBIGUOUS_EMPTY_TX_BATCH")
            .ok()
            .as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// Rebuild Construction inputs from persisted per-cycle tx ids + tx bytes (same validation as ingestion).
async fn recover_entries_from_persisted_cycle_txs(
    store: &StorageManager,
    cycle: u64,
    max_entries: usize,
) -> Option<Vec<catalyst_consensus::types::TransactionEntry>> {
    let index_present = store
        .get_metadata(&cycle_txids_key(cycle))
        .await
        .ok()
        .flatten()
        .is_some();
    let txids = load_cycle_txids(store, cycle).await;
    if txids.is_empty() {
        return index_present.then_some(Vec::new());
    }
    let mut txs: Vec<catalyst_core::protocol::Transaction> = Vec::with_capacity(txids.len());
    for txid in &txids {
        let raw = match store
            .get_metadata(&mempool_tx_key(txid))
            .await
            .ok()
            .flatten()
        {
            Some(b) => Some(b),
            None => store.get_metadata(&tx_raw_key(txid)).await.ok().flatten(),
        }?;
        let Ok(tx) = bincode::deserialize::<catalyst_core::protocol::Transaction>(&raw) else {
            return None;
        };
        txs.push(tx);
    }
    let valid =
        validate_and_select_protocol_txs_for_construction(store, cycle, txs, max_entries).await;
    if let Ok(expect) = ProtocolTxBatch::new(cycle, valid.clone()) {
        if let Some((h, c)) = read_tx_batch_commit(store, cycle).await {
            if h != expect.batch_hash || c != expect.txs.len() as u32 {
                tracing::warn!(
                    target: "catalyst.consensus.tx_batch",
                    "recover_entries: batch disagrees with commit (hash or tx_count) for cycle {}",
                    cycle
                );
                return None;
            }
        }
    }
    let mut entries: Vec<catalyst_consensus::types::TransactionEntry> = Vec::new();
    for tx in &valid {
        entries.extend(tx_to_consensus_entries(tx));
    }
    Some(entries)
}

pub(crate) async fn read_tx_batch_commit(
    store: &StorageManager,
    cycle: u64,
) -> Option<([u8; 32], u32)> {
    let key = format!("consensus:tx_batch_commit:{cycle}");
    let bytes = store.get_metadata(&key).await.ok().flatten()?;
    crate::consensus_limits::parse_tx_batch_commit_value(&bytes)
}

pub(crate) async fn persist_tx_batch_commit_metadata(
    store: &StorageManager,
    cycle: u64,
    batch_hash: [u8; 32],
    tx_count: u32,
) {
    let buf = crate::consensus_limits::encode_tx_batch_commit_value(batch_hash, tx_count);
    let _ = store
        .set_metadata(&format!("consensus:tx_batch_commit:{cycle}"), &buf)
        .await;
}

/// Wait for canonical construction entries: in-memory batch, store recovery, or resync from peers.
///
/// Never returns an ambiguous empty list for non-leaders unless legacy env is enabled.
///
/// `network`: when `None`, resync broadcasts are skipped (unit tests only; production passes `Some`).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn wait_for_tx_construction_entries(
    cycle: u64,
    cycle_ms: u64,
    producer_id: &str,
    leader: &str,
    is_leader: bool,
    max_entries: usize,
    tx_batches: &tokio::sync::RwLock<std::collections::HashMap<u64, Vec<catalyst_consensus::types::TransactionEntry>>>,
    commit_ram: &tokio::sync::RwLock<std::collections::HashMap<u64, ([u8; 32], u32)>>,
    storage: Option<&StorageManager>,
    network: Option<&dyn crate::tx_batch_p2p::TxBatchNetwork>,
) -> Option<Vec<catalyst_consensus::types::TransactionEntry>> {
    let now_ms = crate::consensus_limits::wall_now_ms();
    let budget = catalyst_tx_batch_wait_budget_ms(cycle_ms);
    let deadline_ms = crate::consensus_limits::tx_batch_follower_deadline_ms(
        now_ms,
        cycle,
        cycle_ms,
        budget,
    );
    let mut last_resync_ms = 0u64;
    let mut resync_sent: u32 = 0;

    loop {
        let t = crate::consensus_limits::wall_now_ms();
        if t >= deadline_ms {
            break;
        }
        if let Some(e) = tx_batches.read().await.get(&cycle).cloned() {
            return Some(e);
        }
        if let Some(store) = storage {
            if let Some(e) = recover_entries_from_persisted_cycle_txs(store, cycle, max_entries).await {
                tx_batches.write().await.insert(cycle, e.clone());
                return Some(e);
            }
        }
        if !is_leader && t.saturating_sub(last_resync_ms) >= 600 && resync_sent < 48 {
            last_resync_ms = t;
            resync_sent = resync_sent.saturating_add(1);
            let req = TxBatchControl::ResyncRequest {
                cycle,
                requester: producer_id.to_string(),
            };
            if let (Some(net), Ok(env)) = (
                network,
                MessageEnvelope::from_message(&req, "tx_batch_resync".to_string(), None),
            ) {
                let _ = net.broadcast_envelope(&env).await;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Final attempts after budget (up to cycle boundary).
    let hard_stop = crate::consensus_limits::tx_batch_follower_hard_stop_ms(cycle, cycle_ms);
    while crate::consensus_limits::wall_now_ms() < hard_stop {
        if let Some(e) = tx_batches.read().await.get(&cycle).cloned() {
            return Some(e);
        }
        if let Some(store) = storage {
            if let Some(e) = recover_entries_from_persisted_cycle_txs(store, cycle, max_entries).await {
                tx_batches.write().await.insert(cycle, e.clone());
                return Some(e);
            }
        }
        let t = crate::consensus_limits::wall_now_ms();
        if !is_leader && t.saturating_sub(last_resync_ms) >= 400 && resync_sent < 96 {
            last_resync_ms = t;
            resync_sent = resync_sent.saturating_add(1);
            let req = TxBatchControl::ResyncRequest {
                cycle,
                requester: producer_id.to_string(),
            };
            if let (Some(net), Ok(env)) = (
                network,
                MessageEnvelope::from_message(&req, "tx_batch_resync_late".to_string(), None),
            ) {
                let _ = net.broadcast_envelope(&env).await;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(40)).await;
    }

    if let Some(e) = tx_batches.read().await.get(&cycle).cloned() {
        return Some(e);
    }
    if let Some(store) = storage {
        if let Some(e) = recover_entries_from_persisted_cycle_txs(store, cycle, max_entries).await {
            tx_batches.write().await.insert(cycle, e.clone());
            return Some(e);
        }
    }

    let commit = commit_ram.read().await.get(&cycle).copied();
    let commit_meta = if let Some(store) = storage {
        read_tx_batch_commit(store, cycle).await.or(commit)
    } else {
        commit
    };

    match crate::consensus_limits::resolve_empty_construction_takeaway(
        commit_meta,
        is_leader,
        catalyst_allow_legacy_ambiguous_empty_tx_batch(),
    ) {
        crate::consensus_limits::EmptyConstructionResolution::AgreedEmpty => {
            tracing::info!(
                target: "catalyst.consensus.tx_batch",
                "Using empty construction set (commit tx_count=0) cycle={} leader={} self={}",
                cycle,
                leader,
                producer_id
            );
            Some(Vec::new())
        }
        crate::consensus_limits::EmptyConstructionResolution::LeaderMissFatal => {
            tracing::error!(
                target: "catalyst.consensus.tx_batch",
                "TX_BATCH_MISS_FATAL leader failed to retain batch cycle={} (check storage)",
                cycle
            );
            None
        }
        crate::consensus_limits::EmptyConstructionResolution::FollowerLegacyEmpty => {
            tracing::warn!(
                target: "catalyst.consensus.tx_batch",
                "CATALYST_ALLOW_LEGACY_AMBIGUOUS_EMPTY_TX_BATCH: proceeding with empty construction cycle={} leader={} self={}",
                cycle,
                leader,
                producer_id
            );
            Some(Vec::new())
        }
        crate::consensus_limits::EmptyConstructionResolution::FollowerRefuseEmpty => {
            tracing::error!(
                target: "catalyst.consensus.tx_batch",
                "TX_BATCH_MISS_FATAL cycle={} leader={} self={}: refuse empty construction (set CATALYST_ALLOW_LEGACY_AMBIGUOUS_EMPTY_TX_BATCH=1 only for pre-commit peers)",
                cycle,
                leader,
                producer_id
            );
            None
        }
    }
}

pub(crate) async fn update_tx_meta_status_selected(
    store: &StorageManager,
    tx: &catalyst_core::protocol::Transaction,
    cycle: u64,
) {
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

/// Acquires `lsu_apply_lock` then delegates to [`apply_lsu_to_storage_locked`]. Use this from any
/// caller that does not already hold the lock (production self-apply, standalone follower applies,
/// tests). Callers that already hold it (the reconcile transaction) must call
/// `apply_lsu_to_storage_locked` directly — `tokio::sync::Mutex` is not reentrant, so calling this
/// wrapper while already holding the lock deadlocks.
pub(crate) async fn apply_lsu_to_storage(
    store: &StorageManager,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
) -> Result<[u8; 32]> {
    // Serialize against all concurrent apply paths (gossip, advance ticker, consensus task).
    // Must be acquired BEFORE the idempotence check so that the check sees the updated
    // last_applied_cycle from whichever apply completed just before us.
    let _apply_guard = lsu_apply_lock().lock().await;
    apply_lsu_to_storage_locked(store, lsu).await
}

/// Caller must already hold `lsu_apply_lock` (see [`apply_lsu_to_storage`]).
async fn apply_lsu_to_storage_locked(
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
            ensure_worker_first_seen_cycle(store, &e.public_key, lsu.cycle_number).await;
            set_worker_last_registration_cycle(store, &e.public_key, lsu.cycle_number).await;
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

    // Apply producer compensation entries (issuance + producer fee share).
    for c in &lsu.compensation_entries {
        if c.amount == 0 {
            continue;
        }
        let cur = get_balance_i64(store, &c.public_key).await;
        let _ = set_balance_i64(store, &c.public_key, cur.saturating_add(c.amount as i64)).await;
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

    // Fee-credit spend reimbursement and waiting-pool rewards/credits are deterministic from cycle LSU.
    settle_fee_credit_spend_for_applied_cycle(store, lsu).await;
    distribute_waiting_pool_rewards_and_fee_credits(store, lsu).await;

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

    // Guarantee the per-cycle history record (`consensus:lsu:{cycle}` / `consensus:lsu_hash:{cycle}`)
    // exists for any cycle actually applied. This used to be left entirely to callers to remember
    // via `persist_lsu_history`, and at least two paths never called it — the reconcile L2
    // fast-path and the main per-cycle production/completion path — leaving holes in a node's own
    // local history that a later reconcile attempt would find missing and be unable to bridge
    // (observed in production: "missing stored LSU for cycle N" for cycles that were, in fact,
    // already correctly applied). By the time this function runs, the caller has already decided
    // this LSU is the one to apply for this cycle (via fork choice / reconcile), so recording it
    // as canonical here unconditionally is always correct.
    if let Ok(lsu_bytes) = lsu.serialize() {
        let _ = store
            .set_metadata(&format!("consensus:lsu:{}", lsu.cycle_number), &lsu_bytes)
            .await;
        let _ = store
            .set_metadata(&format!("consensus:lsu_hash:{}", lsu.cycle_number), &lsu_hash)
            .await;
    }

    info!(
        "Applied LSU to storage cycle={} state_root={}",
        lsu.cycle_number,
        hex_encode(&state_root)
    );

    // Prune persisted mempool against updated nonces.
    prune_persisted_mempool(store).await;

    // Opportunistic disk-bounding: prune old historical metadata (if enabled).
    crate::pruning::maybe_prune_history(store).await;

    // On the first successful apply after process start, always write a checkpoint here,
    // regardless of the periodic `CATALYST_CHECKPOINT_EVERY_CYCLES` interval. Without this, a
    // freshly (re)started node has no checkpoint at all until the next interval boundary (up to
    // `every` cycles away), so any near-head reconcile conflict in that window has no anchor to
    // bridge from and can only fail loud, not self-heal (docs/consensus-reliability-review-2026-07.md §3.2/§5.2).
    static STARTUP_CHECKPOINT_WRITTEN: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    let checkpoint_result = if STARTUP_CHECKPOINT_WRITTEN
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_ok()
    {
        crate::checkpoint::write_checkpoint(store, lsu.cycle_number, state_root, lsu_hash).await
    } else {
        crate::checkpoint::maybe_write_checkpoint_after_apply(
            store,
            lsu.cycle_number,
            state_root,
            lsu_hash,
        )
        .await
    };
    if let Err(e) = checkpoint_result {
        tracing::warn!(
            target: "catalyst.consensus.checkpoint",
            cycle = lsu.cycle_number,
            "checkpoint write failed: {e}"
        );
    }

    Ok(state_root)
}

/// Before giving up on fork reconcile, request missing `consensus:lsu:*` metadata via
/// `LsuRangeRequest` (best-effort WAN repair). `0` disables.
fn catalyst_reconcile_prefetch_wait_ms() -> u64 {
    std::env::var("CATALYST_RECONCILE_PREFETCH_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8000)
}

/// Ask peers for missing LSU metadata in `[scan_start, end_exclusive)`, fire-and-forget.
///
/// This runs inline inside the single shared gossip-processing task (the `events.recv().await`
/// loop), so it must return promptly: it used to poll local storage in a loop, sleeping up to
/// `wait_ms` (default 8s) between attempts, which stalled ALL inbound P2P message handling for the
/// full window on every call. When the requested history could never be satisfied (e.g. genesis-era
/// history that no longer exists after a reset), this recurred on every conflicting gossip message
/// and wedged the node indefinitely — no further gossip, votes, or LSUs could be processed.
///
/// Now it does one local scan, fires a single `LsuRangeRequest` broadcast if anything is missing,
/// and returns immediately without waiting for a response. `wait_ms` is kept only as an on/off gate
/// (`0` disables); a follow-up reconcile attempt — naturally retried on the next conflicting gossip
/// message, or by the periodic `advance_handle`/`gap_handle` background tickers — picks up whatever
/// arrived in response.
async fn reconcile_prefetch_lsu_metadata_if_missing(
    store: &StorageManager,
    network: &P2pService,
    requester: &str,
    scan_start: u64,
    end_exclusive: u64,
    wait_ms: u64,
) {
    let scan_start = scan_start.max(1);
    if wait_ms == 0 || end_exclusive <= scan_start {
        return;
    }
    let mut first_missing: Option<u64> = None;
    for i in scan_start..end_exclusive {
        let has = store
            .get_metadata(&format!("consensus:lsu:{}", i))
            .await
            .ok()
            .flatten()
            .is_some();
        if !has {
            first_missing = Some(i);
            break;
        }
    }
    let Some(start) = first_missing else {
        return;
    };
    let remaining = end_exclusive.saturating_sub(start);
    let count = remaining.min(256) as u32;
    let req = LsuRangeRequest {
        requester: requester.to_string(),
        start_cycle: start,
        count,
    };
    if let Ok(env) = MessageEnvelope::from_message(&req, "reconcile_prefetch_lsu".to_string(), None) {
        let _ = network.broadcast_envelope(&env).await;
    }
}

fn max_replay_cycles_limit() -> u64 {
    crate::consensus_limits::max_replay_cycles_from_env()
}

#[derive(Clone, Default)]
struct FinalityAttestationBucket {
    chain_id: Option<u64>,
    genesis_hash: Option<[u8; 32]>,
    committee_hash: Option<[u8; 32]>,
    vote_list_hash: Option<[u8; 32]>,
    votes: std::collections::BTreeMap<String, Vec<u8>>,
}

fn finality_bucket_matches_msg(b: &FinalityAttestationBucket, msg: &crate::sync::LsuFinalityAttestationMsg) -> bool {
    b.chain_id.is_none()
        || (b.chain_id == Some(msg.chain_id)
            && b.genesis_hash == Some(msg.genesis_hash)
            && b.committee_hash == Some(msg.committee_hash)
            && b.vote_list_hash == Some(msg.vote_list_hash))
}

fn finality_bucket_init_from_msg(b: &mut FinalityAttestationBucket, msg: &crate::sync::LsuFinalityAttestationMsg) {
    b.chain_id = Some(msg.chain_id);
    b.genesis_hash = Some(msg.genesis_hash);
    b.committee_hash = Some(msg.committee_hash);
    b.vote_list_hash = Some(msg.vote_list_hash);
}

/// Verify finality certificate when required or when CID is present (best-effort log on failure if not required).
async fn verify_lsu_finality_for_apply(
    store: &StorageManager,
    dfs: &crate::dfs_store::LocalContentStore,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    cycle: u64,
    finality_cid_hint: &str,
    require_finality: bool,
) -> bool {
    let cid = if !finality_cid_hint.is_empty() {
        finality_cid_hint.to_string()
    } else {
        match meta_string(store, &format!("consensus:lsu_finality_cid:{}", cycle)).await {
            Some(s) => s,
            None => String::new(),
        }
    };

    if !require_finality {
        if cid.is_empty() {
            return true;
        }
        if let Ok(bytes) = dfs.get(&cid).await {
            if let Ok(cert) = bincode::deserialize::<catalyst_consensus::LsuFinalityCertificateV1>(&bytes) {
                if catalyst_consensus::verify_lsu_finality_certificate(&cert, lsu).is_err() {
                    tracing::warn!(
                        target: "catalyst.consensus.apply",
                        "LSU finality certificate present but failed verification cycle={} cid={}",
                        cycle,
                        cid
                    );
                }
            }
        }
        return true;
    }

    if cid.is_empty() {
        tracing::warn!(
            target: "catalyst.consensus.apply",
            "LSU finality required but no finality CID for cycle={}",
            cycle
        );
        return false;
    }
    let Ok(bytes) = dfs.get(&cid).await else {
        tracing::warn!(
            target: "catalyst.consensus.apply",
            "LSU finality required: missing finality bytes cycle={} cid={}",
            cycle,
            cid
        );
        return false;
    };
    let Ok(cert) = bincode::deserialize::<catalyst_consensus::LsuFinalityCertificateV1>(&bytes) else {
        tracing::warn!(
            target: "catalyst.consensus.apply",
            "LSU finality required: bad finality certificate encoding cycle={}",
            cycle
        );
        return false;
    };
    if let Err(e) = catalyst_consensus::verify_lsu_finality_certificate(&cert, lsu) {
        tracing::warn!(
            target: "catalyst.consensus.apply",
            "LSU finality required: verify failed cycle={} err={}",
            cycle,
            e
        );
        return false;
    }
    true
}

/// ADR 0002: gate for the trusted/catch-up apply path (`apply_lsu_to_storage_without_root_check`).
///
/// Mirrors `verify_lsu_finality_for_apply`, but additionally checks that `claimed_root` — the peer's
/// claim the caller is about to trust and cache without a local recompute-and-compare against a
/// certificate — matches the certificate's own `state_root` field. A certificate that verifies but
/// certifies a *different* root than what the caller is about to trust must not pass: that would
/// let a peer claim quorum backing for one root while feeding the apply path another.
async fn verify_state_root_finality_for_apply(
    store: &StorageManager,
    dfs: &crate::dfs_store::LocalContentStore,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    cycle: u64,
    claimed_root: [u8; 32],
    state_root_cid_hint: &str,
    require_state_root_finality: bool,
) -> bool {
    let cid = if !state_root_cid_hint.is_empty() {
        state_root_cid_hint.to_string()
    } else {
        match meta_string(store, &format!("consensus:lsu_state_root_cid:{}", cycle)).await {
            Some(s) => s,
            None => String::new(),
        }
    };

    if !require_state_root_finality {
        if cid.is_empty() {
            return true;
        }
        if let Ok(bytes) = dfs.get(&cid).await {
            if let Ok(cert) = bincode::deserialize::<catalyst_consensus::LsuStateRootCertificateV1>(&bytes) {
                if catalyst_consensus::verify_lsu_state_root_certificate(&cert, lsu).is_err()
                    || cert.state_root != claimed_root
                {
                    tracing::warn!(
                        target: "catalyst.consensus.apply",
                        "State-root certificate present but failed verification or root mismatch cycle={} cid={}",
                        cycle,
                        cid
                    );
                }
            }
        }
        return true;
    }

    if cid.is_empty() {
        tracing::warn!(
            target: "catalyst.consensus.apply",
            "State-root finality required but no state-root CID for cycle={}",
            cycle
        );
        return false;
    }
    let Ok(bytes) = dfs.get(&cid).await else {
        tracing::warn!(
            target: "catalyst.consensus.apply",
            "State-root finality required: missing certificate bytes cycle={} cid={}",
            cycle,
            cid
        );
        return false;
    };
    let Ok(cert) = bincode::deserialize::<catalyst_consensus::LsuStateRootCertificateV1>(&bytes) else {
        tracing::warn!(
            target: "catalyst.consensus.apply",
            "State-root finality required: bad certificate encoding cycle={}",
            cycle
        );
        return false;
    };
    if let Err(e) = catalyst_consensus::verify_lsu_state_root_certificate(&cert, lsu) {
        tracing::warn!(
            target: "catalyst.consensus.apply",
            "State-root finality required: verify failed cycle={} err={}",
            cycle,
            e
        );
        return false;
    }
    if cert.state_root != claimed_root {
        tracing::warn!(
            target: "catalyst.consensus.apply",
            "State-root finality required: certified root does not match claimed root cycle={} certified={} claimed={}",
            cycle,
            hex_encode(&cert.state_root),
            hex_encode(&claimed_root)
        );
        return false;
    }
    true
}

/// Apply stored `consensus:lsu:{cycle}` when `cycle == local_applied + 1` (sequential follower catch-up).
///
/// Holds `lsu_apply_lock` across its whole read-decide-apply sequence (not just the final write) so
/// this decision can't race a concurrent production self-apply or reconcile transaction — see the
/// TOCTOU note in docs/consensus-reliability-review-2026-07.md. Self-contained: neither of its two
/// callers (the catch-up ticker, the `StateResponse` handler) holds the lock, and it never nests
/// inside `try_reconcile_fork_from_quorum_lsu`.
async fn try_apply_stored_lsu_at_cycle(
    store: &StorageManager,
    dfs: Option<&crate::dfs_store::LocalContentStore>,
    cycle: u64,
    require_lsu_finality: bool,
) -> bool {
    let _apply_guard = lsu_apply_lock().lock().await;
    let head = local_applied_cycle(store).await;
    if cycle != head.saturating_add(1) {
        return false;
    }
    let Some(bytes) = store
        .get_metadata(&format!("consensus:lsu:{}", cycle))
        .await
        .ok()
        .flatten()
    else {
        return false;
    };
    let Ok(lsu) = catalyst_consensus::types::LedgerStateUpdate::deserialize(&bytes) else {
        return false;
    };
    if lsu.cycle_number != cycle {
        return false;
    }
    let lsu_hash = catalyst_consensus::types::hash_data(&lsu).unwrap_or([0u8; 32]);
    if let Some(hb) = store
        .get_metadata(&format!("consensus:lsu_hash:{}", cycle))
        .await
        .ok()
        .flatten()
    {
        if hb.len() == 32 {
            let mut ah = [0u8; 32];
            ah.copy_from_slice(&hb[..32]);
            if lsu_hash != ah {
                return false;
            }
        }
    }
    let finality_cid = meta_string(store, &format!("consensus:lsu_finality_cid:{}", cycle))
        .await
        .unwrap_or_default();
    if !matches!(
        fork_choice_apply_gate(
            store,
            dfs,
            &lsu,
            cycle,
            lsu_hash,
            &finality_cid,
            require_lsu_finality,
        )
        .await,
        ForkChoiceApplyGate::Allow
    ) {
        return false;
    }
    let local_prev = local_applied_state_root(store).await;
    let expected_prev = if cycle == 0 {
        [0u8; 32]
    } else {
        meta_32(
            store,
            &format!("consensus:lsu_state_root:{}", cycle.saturating_sub(1)),
        )
        .await
        .unwrap_or(local_prev)
    };
    if local_prev != expected_prev && !(local_prev == [0u8; 32] && expected_prev == [0u8; 32]) {
        return false;
    }
    if require_lsu_finality {
        let Some(dfs) = dfs else {
            return false;
        };
        if !verify_lsu_finality_for_apply(store, dfs, &lsu, cycle, &finality_cid, true).await {
            return false;
        }
    } else if let Some(dfs) = dfs {
        let _ = verify_lsu_finality_for_apply(store, dfs, &lsu, cycle, &finality_cid, false).await;
    }
    let Some(expected_post) = meta_32(store, &format!("consensus:lsu_state_root:{}", cycle)).await
    else {
        return false;
    };
    apply_lsu_with_root_check_locked(store, &lsu, expected_post)
        .await
        .ok()
        .unwrap_or(false)
}

/// Apply consecutive stored LSUs starting at `local_applied + 1` (bounded batch).
async fn try_advance_applied_head_from_storage(
    store: &StorageManager,
    dfs: Option<&crate::dfs_store::LocalContentStore>,
    require_lsu_finality: bool,
    max_steps: u32,
) -> u32 {
    let mut applied = 0u32;
    for _ in 0..max_steps {
        let next = local_applied_cycle(store).await.saturating_add(1);
        if !try_apply_stored_lsu_at_cycle(store, dfs, next, require_lsu_finality).await {
            break;
        }
        applied = applied.saturating_add(1);
    }
    applied
}

/// True when a verified ADR 0001 certificate exists locally for this LSU at `cycle`.
async fn lsu_has_verified_finality_cert(
    store: &StorageManager,
    dfs: &crate::dfs_store::LocalContentStore,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    cycle: u64,
    finality_cid_hint: &str,
) -> bool {
    let cid = if !finality_cid_hint.is_empty() {
        finality_cid_hint.to_string()
    } else {
        match meta_string(store, &format!("consensus:lsu_finality_cid:{}", cycle)).await {
            Some(s) => s,
            None => return false,
        }
    };
    let Ok(bytes) = dfs.get(&cid).await else {
        return false;
    };
    let Ok(cert) = bincode::deserialize::<catalyst_consensus::LsuFinalityCertificateV1>(&bytes) else {
        return false;
    };
    catalyst_consensus::verify_lsu_finality_certificate(&cert, lsu).is_ok()
}

/// True when a verified ADR 0002 state-root certificate exists locally for this exact LSU at
/// `cycle` (best-effort fork-choice tie-break signal only — see `ForkChoiceCandidate::state_root_certified`;
/// correctness of applied state does not depend on this, only on the apply-path gating in
/// `verify_state_root_finality_for_apply`).
async fn lsu_has_verified_state_root_cert(
    store: &StorageManager,
    dfs: &crate::dfs_store::LocalContentStore,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    cycle: u64,
) -> bool {
    let Some(cid) = meta_string(store, &format!("consensus:lsu_state_root_cid:{}", cycle)).await else {
        return false;
    };
    let Ok(bytes) = dfs.get(&cid).await else {
        return false;
    };
    let Ok(cert) = bincode::deserialize::<catalyst_consensus::LsuStateRootCertificateV1>(&bytes) else {
        return false;
    };
    catalyst_consensus::verify_lsu_state_root_certificate(&cert, lsu).is_ok()
}

async fn fork_choice_candidate_for_lsu(
    store: &StorageManager,
    dfs: Option<&crate::dfs_store::LocalContentStore>,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    cycle: u64,
    lsu_hash: [u8; 32],
    finality_cid_hint: &str,
    certified_only: bool,
) -> crate::fork_choice::ForkChoiceCandidate {
    let certified = match dfs {
        Some(dfs) => {
            lsu_has_verified_finality_cert(store, dfs, lsu, cycle, finality_cid_hint).await
        }
        None => false,
    };
    let state_root_certified = match dfs {
        Some(dfs) => lsu_has_verified_state_root_cert(store, dfs, lsu, cycle).await,
        None => false,
    };
    crate::fork_choice::ForkChoiceCandidate::new(
        cycle,
        lsu_hash,
        lsu,
        certified,
        certified_only,
        state_root_certified,
    )
}

async fn load_fork_choice_candidate_at_cycle(
    store: &StorageManager,
    dfs: Option<&crate::dfs_store::LocalContentStore>,
    cycle: u64,
    certified_only: bool,
) -> Option<crate::fork_choice::ForkChoiceCandidate> {
    let hash_bytes = store
        .get_metadata(&format!("consensus:lsu_hash:{}", cycle))
        .await
        .ok()
        .flatten()?;
    if hash_bytes.len() != 32 {
        return None;
    }
    let mut lsu_hash = [0u8; 32];
    lsu_hash.copy_from_slice(&hash_bytes[..32]);
    let lsu_bytes = store
        .get_metadata(&format!("consensus:lsu:{}", cycle))
        .await
        .ok()
        .flatten()?;
    let lsu = catalyst_consensus::types::LedgerStateUpdate::deserialize(&lsu_bytes).ok()?;
    if lsu.cycle_number != cycle {
        return None;
    }
    Some(
        fork_choice_candidate_for_lsu(store, dfs, &lsu, cycle, lsu_hash, "", certified_only).await,
    )
}

/// If a weaker competing LSU is already canonical at `cycle`, reject apply/reconcile paths.
async fn fork_choice_reject_weaker_incoming(
    store: &StorageManager,
    dfs: Option<&crate::dfs_store::LocalContentStore>,
    incoming: &crate::fork_choice::ForkChoiceCandidate,
    certified_only: bool,
) -> bool {
    let Some(stored) =
        load_fork_choice_candidate_at_cycle(store, dfs, incoming.cycle, certified_only).await
    else {
        return false;
    };
    if stored.lsu_hash == incoming.lsu_hash {
        return false;
    }
    if crate::fork_choice::incoming_weaker_than_stored(incoming, &stored) {
        tracing::warn!(
            target: "catalyst.consensus.fork_choice",
            cycle = incoming.cycle,
            incoming_hash = %hex_encode(&incoming.lsu_hash),
            stored_hash = %hex_encode(&stored.lsu_hash),
            incoming_tier = ?incoming.tier,
            stored_tier = ?stored.tier,
            "Rejecting weaker competing LSU at same cycle (certified quorum + hash tie-break)"
        );
        catalyst_utils::increment_counter!("consensus_fork_choice_rejected_weaker_total", 1);
        return true;
    }
    false
}

async fn try_publish_finality_certificate_from_bucket(
    store: &StorageManager,
    dfs: &crate::dfs_store::LocalContentStore,
    net: &Arc<P2pService>,
    buckets: &Arc<RwLock<std::collections::HashMap<(u64, [u8; 32]), FinalityAttestationBucket>>>,
    cycle: u64,
    lsu_hash: [u8; 32],
    producer_id_for_broadcast: &str,
) {
    let meta_key = format!("consensus:lsu_finality_cid:{}", cycle);
    if meta_string(store, &meta_key).await.is_some() {
        return;
    }
    let Some(lsu_bytes) = store
        .get_metadata(&format!("consensus:lsu:{}", cycle))
        .await
        .ok()
        .flatten()
    else {
        return;
    };
    let Ok(lsu) = catalyst_consensus::types::LedgerStateUpdate::deserialize(&lsu_bytes) else {
        return;
    };
    let Ok(h) = catalyst_consensus::types::hash_data(&lsu) else {
        return;
    };
    if h != lsu_hash {
        return;
    }
    let bucket = {
        let g = buckets.read().await;
        g.get(&(cycle, lsu_hash)).cloned()
    };
    let Some(bucket) = bucket else { return };
    let (Some(chain_id), Some(genesis_hash), Some(_ch), Some(_vh)) = (
        bucket.chain_id,
        bucket.genesis_hash,
        bucket.committee_hash,
        bucket.vote_list_hash,
    ) else {
        return;
    };

    let mut vote_entries: Vec<catalyst_consensus::ProducerFinalityVote> = Vec::new();
    for (pid, sig) in bucket.votes {
        if sig.len() != 64 {
            continue;
        }
        if !lsu.vote_list.iter().any(|v| v == &pid) || !lsu.producer_list.iter().any(|v| v == &pid) {
            continue;
        }
        vote_entries.push(catalyst_consensus::ProducerFinalityVote {
            producer_id: pid,
            signature: sig,
        });
    }
    if vote_entries.len() < crate::fork_choice::bft_vote_threshold(lsu.producer_list.len()) {
        return;
    }
    let Ok(cert) = catalyst_consensus::LsuFinalityCertificateV1::from_lsu_and_votes(
        &lsu,
        chain_id,
        genesis_hash,
        lsu_hash,
        vote_entries,
    ) else {
        return;
    };
    if catalyst_consensus::verify_lsu_finality_certificate(&cert, &lsu).is_err() {
        return;
    }
    let Ok(blob) = bincode::serialize(&cert) else {
        return;
    };
    let Some(cid) = dfs.put(blob).await.ok() else {
        return;
    };
    let _ = store.set_metadata(&meta_key, cid.as_bytes()).await;
    let msg = crate::sync::LsuFinalityCidMsg {
        cycle,
        lsu_hash,
        finality_cid: cid,
    };
    if let Ok(env) = MessageEnvelope::from_message(&msg, producer_id_for_broadcast.to_string(), None) {
        let _ = net.broadcast_envelope(&env).await;
    }
    info!("Published LSU finality certificate cycle={}", cycle);
}

async fn ingest_finality_attestation(
    store: &StorageManager,
    dfs: &crate::dfs_store::LocalContentStore,
    net: &Arc<P2pService>,
    buckets: &Arc<RwLock<std::collections::HashMap<(u64, [u8; 32]), FinalityAttestationBucket>>>,
    msg: &crate::sync::LsuFinalityAttestationMsg,
    producer_id_for_broadcast: &str,
) {
    if msg.signature.len() != 64 {
        return;
    }
    if catalyst_consensus::verify_finality_attestation(
        msg.chain_id,
        &msg.genesis_hash,
        msg.cycle,
        &msg.lsu_hash,
        &msg.committee_hash,
        &msg.vote_list_hash,
        &msg.producer_id,
        &msg.signature,
    )
    .is_err()
    {
        return;
    }
    {
        let mut g = buckets.write().await;
        let b = g.entry((msg.cycle, msg.lsu_hash)).or_default();
        if b.chain_id.is_none() {
            finality_bucket_init_from_msg(b, msg);
        } else if !finality_bucket_matches_msg(b, msg) {
            return;
        }
        b.votes.insert(msg.producer_id.clone(), msg.signature.clone());
    }
    try_publish_finality_certificate_from_bucket(
        store,
        dfs,
        net,
        buckets,
        msg.cycle,
        msg.lsu_hash,
        producer_id_for_broadcast,
    )
    .await;
}

/// ADR 0002: accumulates `StateRootAttestationMsg`s that agree on the same `(cycle, lsu_hash,
/// state_root)`, kept as the HashMap key so a divergent minority's claimed root can never mix
/// into a majority bucket for the same LSU (see `state_root_buckets` at node construction).
#[derive(Clone, Default)]
struct StateRootAttestationBucket {
    chain_id: Option<u64>,
    genesis_hash: Option<[u8; 32]>,
    committee_hash: Option<[u8; 32]>,
    attestations: std::collections::BTreeMap<String, Vec<u8>>,
}

fn state_root_bucket_matches_msg(
    b: &StateRootAttestationBucket,
    msg: &crate::sync::StateRootAttestationMsg,
) -> bool {
    b.chain_id.is_none()
        || (b.chain_id == Some(msg.chain_id)
            && b.genesis_hash == Some(msg.genesis_hash)
            && b.committee_hash == Some(msg.committee_hash))
}

fn state_root_bucket_init_from_msg(
    b: &mut StateRootAttestationBucket,
    msg: &crate::sync::StateRootAttestationMsg,
) {
    b.chain_id = Some(msg.chain_id);
    b.genesis_hash = Some(msg.genesis_hash);
    b.committee_hash = Some(msg.committee_hash);
}

async fn try_publish_state_root_certificate_from_bucket(
    store: &StorageManager,
    dfs: &crate::dfs_store::LocalContentStore,
    net: &Arc<P2pService>,
    buckets: &Arc<RwLock<std::collections::HashMap<(u64, [u8; 32], [u8; 32]), StateRootAttestationBucket>>>,
    cycle: u64,
    lsu_hash: [u8; 32],
    state_root: [u8; 32],
    producer_id_for_broadcast: &str,
) {
    let meta_key = format!("consensus:lsu_state_root_cid:{}", cycle);
    if meta_string(store, &meta_key).await.is_some() {
        return;
    }
    let Some(lsu_bytes) = store
        .get_metadata(&format!("consensus:lsu:{}", cycle))
        .await
        .ok()
        .flatten()
    else {
        return;
    };
    let Ok(lsu) = catalyst_consensus::types::LedgerStateUpdate::deserialize(&lsu_bytes) else {
        return;
    };
    let Ok(h) = catalyst_consensus::types::hash_data(&lsu) else {
        return;
    };
    if h != lsu_hash {
        return;
    }
    let bucket = {
        let g = buckets.read().await;
        g.get(&(cycle, lsu_hash, state_root)).cloned()
    };
    let Some(bucket) = bucket else { return };
    let (Some(chain_id), Some(genesis_hash), Some(_ch)) =
        (bucket.chain_id, bucket.genesis_hash, bucket.committee_hash)
    else {
        return;
    };

    let mut attestation_entries: Vec<catalyst_consensus::StateRootAttestation> = Vec::new();
    for (pid, sig) in bucket.attestations {
        if sig.len() != 64 {
            continue;
        }
        if !lsu.producer_list.iter().any(|v| v == &pid) {
            continue;
        }
        attestation_entries.push(catalyst_consensus::StateRootAttestation {
            producer_id: pid,
            signature: sig,
        });
    }
    if attestation_entries.len() < crate::fork_choice::bft_vote_threshold(lsu.producer_list.len()) {
        return;
    }
    let Ok(cert) = catalyst_consensus::LsuStateRootCertificateV1::from_lsu_and_attestations(
        &lsu,
        chain_id,
        genesis_hash,
        lsu_hash,
        state_root,
        attestation_entries,
    ) else {
        return;
    };
    if catalyst_consensus::verify_lsu_state_root_certificate(&cert, &lsu).is_err() {
        return;
    }
    let Ok(blob) = bincode::serialize(&cert) else {
        return;
    };
    let Some(cid) = dfs.put(blob).await.ok() else {
        return;
    };
    let _ = store.set_metadata(&meta_key, cid.as_bytes()).await;
    let msg = crate::sync::StateRootFinalityCidMsg {
        cycle,
        lsu_hash,
        state_root,
        state_root_cid: cid,
    };
    if let Ok(env) = MessageEnvelope::from_message(&msg, producer_id_for_broadcast.to_string(), None) {
        let _ = net.broadcast_envelope(&env).await;
    }
    catalyst_utils::increment_counter!("consensus_state_root_certificate_published_total", 1);
    info!("Published LSU state-root certificate cycle={} state_root={}", cycle, hex_encode(&state_root));
}

async fn ingest_state_root_attestation(
    store: &StorageManager,
    dfs: &crate::dfs_store::LocalContentStore,
    net: &Arc<P2pService>,
    buckets: &Arc<RwLock<std::collections::HashMap<(u64, [u8; 32], [u8; 32]), StateRootAttestationBucket>>>,
    msg: &crate::sync::StateRootAttestationMsg,
    producer_id_for_broadcast: &str,
) {
    if msg.signature.len() != 64 {
        return;
    }
    if catalyst_consensus::verify_state_root_attestation(
        msg.chain_id,
        &msg.genesis_hash,
        msg.cycle,
        &msg.lsu_hash,
        &msg.committee_hash,
        &msg.state_root,
        &msg.producer_id,
        &msg.signature,
    )
    .is_err()
    {
        return;
    }
    {
        let mut g = buckets.write().await;
        let b = g.entry((msg.cycle, msg.lsu_hash, msg.state_root)).or_default();
        if b.chain_id.is_none() {
            state_root_bucket_init_from_msg(b, msg);
        } else if !state_root_bucket_matches_msg(b, msg) {
            return;
        }
        b.attestations.insert(msg.producer_id.clone(), msg.signature.clone());
    }
    try_publish_state_root_certificate_from_bucket(
        store,
        dfs,
        net,
        buckets,
        msg.cycle,
        msg.lsu_hash,
        msg.state_root,
        producer_id_for_broadcast,
    )
    .await;
}

fn metadata_logical_name_is_preserved(name: &[u8]) -> bool {
    let Ok(s) = std::str::from_utf8(name) else {
        return false;
    };
    s.starts_with("protocol:")
        || s.starts_with("consensus:lsu_hash:")
        || s.starts_with("consensus:lsu_cid:")
        || s.starts_with("consensus:lsu_state_root:")
        || s.starts_with("consensus:cycle_by_lsu_hash:")
        || s.starts_with("consensus:lsu:")
        || s.starts_with("consensus:lsu_finality_cid:")
        || s.starts_with("consensus:tx_batch_commit:")
        || s.starts_with("consensus:checkpoint:")
        || s == crate::checkpoint::META_CHECKPOINT_LATEST_CYCLE
}

/// Wipe mutable chain state while keeping `protocol:*` metadata and per-cycle LSU history keys
/// (`consensus:lsu:*`, hashes, CIDs, state roots) for bounded replay.
///
/// Used only when reconciling a cycle we have already (wrongly) applied: there is no way to
/// "undo" forward from an already-applied wrong LSU, so we deliberately discard all mutable
/// state and rebuild it via hash-verified replay from a trustworthy anchor (the chain's true
/// first cycle, or a checkpoint restored immediately afterward). Do NOT call this when merely
/// catching up from behind — that path replays forward onto the existing, still-correct state.
async fn purge_mutable_chain_state_preserving_lsu_history(
    store: &StorageManager,
) -> anyhow::Result<()> {
    let engine = store.engine().clone();
    tokio::task::spawn_blocking(move || {
        for cf in ["accounts", "transactions", "consensus", "contracts", "dfs_refs"] {
            let keys: Vec<Vec<u8>> = engine
                .iterator(cf)
                .map_err(|e| anyhow::anyhow!("iter {cf}: {e}"))?
                .filter_map(|r| r.ok().map(|(k, _)| k.to_vec()))
                .collect();
            for k in keys {
                engine
                    .delete(cf, &k)
                    .map_err(|e| anyhow::anyhow!("del {cf}: {e}"))?;
            }
        }
        let iter = engine
            .iterator("metadata")
            .map_err(|e| anyhow::anyhow!("iter metadata: {e}"))?;
        let mut del: Vec<Vec<u8>> = Vec::new();
        for item in iter {
            let (k, _) = item.map_err(|e| anyhow::anyhow!("metadata iter: {e}"))?;
            if !k.starts_with(catalyst_utils::state::state_keys::METADATA_PREFIX) {
                continue;
            }
            let logical = &k[catalyst_utils::state::state_keys::METADATA_PREFIX.len()..];
            if metadata_logical_name_is_preserved(logical) {
                continue;
            }
            del.push(k.to_vec());
        }
        for k in del {
            engine
                .delete("metadata", &k)
                .map_err(|e| anyhow::anyhow!("del metadata: {e}"))?;
        }
        for cf in engine.cf_names() {
            let _ = engine.flush_cf(&cf);
        }
        Ok::<(), anyhow::Error>(())
    })
    .await
    .map_err(|e| anyhow::anyhow!("purge join: {e}"))?
}

/// On `prev_root` mismatch, if this LSU carries a BFT quorum and we have contiguous stored
/// history `consensus:lsu:1..cycle-1` (after optional P2P prefetch), purge mutable state and replay
/// up to this LSU (capped by `CATALYST_MAX_REPLAY_CYCLES`, `0` disables). Uses a snapshot for rollback if replay/apply fails.
///
/// When `fetch` is set, missing `consensus:lsu:{i}` for `i < cycle` triggers bounded P2P backfill
/// (`LsuRangeRequest` + local poll) before reconcile aborts.
async fn try_reconcile_fork_from_quorum_lsu(
    store: &StorageManager,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    cycle: u64,
    expected_prev: [u8; 32],
    expected_post: [u8; 32],
    expected_lsu_hash: &[u8; 32],
    fetch: Option<(&P2pService, &str)>,
    dfs: Option<&crate::dfs_store::LocalContentStore>,
    finality_cid_hint: &str,
    certified_only: bool,
) -> Result<bool> {
    let incoming =
        fork_choice_candidate_for_lsu(store, dfs, lsu, cycle, *expected_lsu_hash, finality_cid_hint, certified_only)
            .await;
    if certified_only && incoming.tier != crate::fork_choice::ForkChoiceTier::Certified {
        tracing::warn!(
            target: "catalyst.consensus.reconcile",
            cycle,
            "Quorum fork reconcile skipped: certified-only policy requires verified finality certificate"
        );
        catalyst_utils::increment_counter!("consensus_fork_reconcile_aborted_total", 1);
        return Ok(false);
    }
    if !crate::fork_choice::lsu_has_bft_vote_quorum(lsu) {
        catalyst_utils::increment_counter!("consensus_fork_reconcile_aborted_total", 1);
        return Ok(false);
    }
    if lsu.cycle_number != cycle {
        catalyst_utils::increment_counter!("consensus_fork_reconcile_aborted_total", 1);
        return Ok(false);
    }
    let h = catalyst_consensus::types::hash_data(lsu).unwrap_or([0u8; 32]);
    if &h != expected_lsu_hash {
        catalyst_utils::increment_counter!("consensus_fork_reconcile_aborted_total", 1);
        return Ok(false);
    }

    if fork_choice_reject_weaker_incoming(store, dfs, &incoming, certified_only).await {
        catalyst_utils::increment_counter!("consensus_fork_reconcile_aborted_total", 1);
        return Ok(false);
    }

    // Hold the apply lock for the rest of this function (freshness check through commit-or-revert).
    // This makes the whole decide-then-mutate transaction atomic with respect to concurrent
    // production self-applies and other reconcile/catch-up attempts, closing the TOCTOU window that
    // let a stale caller trigger this function in the first place. Every apply call below this point
    // must use the `_locked` variant (apply_lsu_to_storage_locked / apply_lsu_with_root_check_locked)
    // — the public wrappers re-acquire this same non-reentrant mutex and would deadlock.
    let _apply_guard = lsu_apply_lock().lock().await;

    // Freshness re-check: callers (gossip handlers) may invoke this with a local state snapshot
    // taken before a concurrent self-apply completed (see the TOCTOU note in
    // docs/consensus-reliability-review-2026-07.md). fork_choice_reject_weaker_incoming above does
    // NOT reject an equal-hash incoming LSU (self-echo of what we already applied), so without this
    // check a stale caller falls all the way into the purge/rebuild path below for state that is
    // already exactly correct — deterministically failing the same way on every re-gossip. Re-read
    // authoritatively here (local_applied_lsu_hash, not consensus:lsu_hash:{cycle} — the latter is
    // written by persist_lsu_history for any fork-choice-winning LSU whether or not it was ever
    // actually applied, so it isn't a valid "was applied" signal) and no-op if we've already applied
    // exactly this LSU at this exact cycle. Scoped to local_cycle_now == cycle only: it can never
    // short-circuit a legitimate forward catch-up, which requires local_cycle_now < cycle.
    let local_cycle_now = local_applied_cycle(store).await;
    if local_cycle_now == cycle {
        if let Some(applied_hash_now) = local_applied_lsu_hash(store).await {
            if applied_hash_now == *expected_lsu_hash {
                catalyst_utils::increment_counter!(
                    "consensus_fork_reconcile_noop_already_applied_total",
                    1
                );
                return Ok(true);
            }
        }
    }

    // L2 self-heal FAST-PATH (forward apply): if this certified/quorum LSU chains directly onto our
    // CURRENT applied head (its prev_root equals our applied root and it is exactly head+1), we do
    // not need the purge+replay-from-genesis machinery below. That machinery anchors at cycle 1 and
    // dead-ends with "missing stored LSU for cycle 1" on wall-clock chains (whose first cycle is
    // ~10^9), which is exactly how a node that rolled back a divergent tip got permanently wedged
    // one cycle behind while it already held the canonical next LSU. Forward-applying here is as safe
    // as a normal follower apply (root-checked, auto-reverting on mismatch) and resolves the common
    // shallow/tip-fork recovery without touching historical state.
    {
        let local_cycle = local_applied_cycle(store).await;
        let local_root = local_applied_state_root(store).await;
        if cycle == local_cycle.saturating_add(1) && expected_prev == local_root {
            match apply_lsu_with_root_check_locked(store, lsu, expected_post).await {
                Ok(true) => {
                    catalyst_utils::increment_counter!(
                        "consensus_fork_reconcile_fastpath_total",
                        1
                    );
                    info!(
                        target: "catalyst.consensus.reconcile",
                        cycle,
                        "Fork self-heal fast-path: applied certified next LSU onto current head"
                    );
                    return Ok(true);
                }
                Ok(false) => {
                    // Resulting root did not match the certified post-root (already reverted).
                    // Fall through to the full replay path as a backstop.
                }
                Err(e) => {
                    warn!(
                        target: "catalyst.consensus.reconcile",
                        cycle,
                        "Fork self-heal fast-path apply errored ({e}); falling back to replay"
                    );
                }
            }
        }
    }

    let max_cycles = max_replay_cycles_limit();
    if max_cycles == 0 {
        catalyst_utils::increment_counter!("consensus_fork_reconcile_aborted_total", 1);
        return Ok(false);
    }
    let local_cycle = local_applied_cycle(store).await;

    // Circuit breaker: a genuinely unrecoverable divergence (distinct from the freshness/idempotency
    // no-op above, which already handles the common spurious case) would otherwise fail identically
    // on every re-gossip, forever, burning CPU on repeated snapshot/purge/replay (or, for the
    // "missing stored LSU" gap case below, repeated metadata scans + P2P prefetch broadcasts) --
    // exactly the busy-loop observed in production (55k+ failed attempts over 3 days with no
    // operator signal). Checked here, before `resolve_replay_start`, because that function's own
    // "missing stored LSU, can't bridge" failure path returns before ever reaching a check placed
    // later (this was caught live: a node stuck on a genuinely-unbridgeable gap kept retrying every
    // ~20s indefinitely because the later check only guarded the purge/replay transaction, not this
    // earlier gap-resolution step). Keyed on `local_cycle`, not the target `(cycle, hash)` -- see
    // `ReconcileFailureTracker`'s doc comment for why.
    if reconcile_circuit_is_broken(local_cycle).await {
        catalyst_utils::increment_counter!("consensus_fork_reconcile_circuit_broken_total", 1);
        return Ok(false);
    }

    let replay_depth = cycle.saturating_sub(local_cycle);
    if replay_depth > max_cycles {
        warn!(
            target: "catalyst.consensus.reconcile",
            "Quorum fork reconcile skipped: replay depth {} (cycle {} local {}) exceeds CATALYST_MAX_REPLAY_CYCLES={}",
            replay_depth, cycle, local_cycle, max_cycles
        );
        catalyst_utils::increment_counter!("consensus_fork_reconcile_aborted_total", 1);
        return Ok(false);
    }

    let pruned_up_to: u64 = store
        .get_metadata(crate::pruning::META_PRUNED_UP_TO_CYCLE)
        .await
        .ok()
        .flatten()
        .and_then(|b| {
            if b.len() == 8 {
                let mut a = [0u8; 8];
                a.copy_from_slice(&b);
                Some(u64::from_le_bytes(a))
            } else {
                None
            }
        })
        .unwrap_or(0);

    // Two distinct scenarios reach this point, and they require different repair strategies:
    //
    // (A) We are BEHIND (`local_cycle < cycle`): we have not yet applied anything for `cycle`.
    //     Our own applied state up to `local_cycle` is presumptively correct, so we build
    //     FORWARD onto it — replaying only `local_cycle+1..cycle` — and never touch existing
    //     account balances. This is the common case (a few cycles behind, not diverged).
    //
    // (B) We are AT OR PAST `cycle` (`local_cycle >= cycle`): we already applied a *different*
    //     (weaker) LSU for `cycle` and a stronger one has since arrived. There is no "forward"
    //     fix here — the only way to get the correct pre-`cycle` state is to discard everything
    //     we hold and rebuild it via a hash-verified replay of stored history, anchored at
    //     either the chain's true first cycle (`1`, for a from-genesis archival node/test chain)
    //     or the most recent checkpoint. This is the one case where wiping mutable state
    //     (`purge_mutable_chain_state_preserving_lsu_history`) is correct: we are intentionally
    //     discarding the wrong state, not risking a correct one.
    //
    // Using case (A)'s forward-only strategy for case (B), or vice versa, silently produces
    // wrong account balances (case A's purge would erase trusted history; case B's forward-only
    // replay can never undo an already-applied wrong LSU at the same cycle).
    let behind = local_cycle < cycle;

    // Bounded lookup: find the best checkpoint (if any) that bridges the needed window, and
    // fail loudly — never guess — if it can't be bridged locally, via checkpoint, or via peers.
    // Returns the resolved `replay_start` on success.
    async fn resolve_replay_start(
        store: &StorageManager,
        cycle: u64,
        pruned_up_to: u64,
        fetch: Option<(&P2pService, &str)>,
        default_start: u64,
    ) -> Result<Option<u64>> {
        let mut replay_start = default_start;
        if replay_start >= cycle {
            return Ok(Some(replay_start));
        }

        let mut gap: Option<u64> = None;
        for i in replay_start..cycle {
            if store.get_metadata(&format!("consensus:lsu:{}", i)).await.ok().flatten().is_none() {
                gap = Some(i);
                break;
            }
        }
        if gap.is_none() {
            return Ok(Some(replay_start));
        }

        // This is a bounded lookup (never a walk from literal cycle 1 — see
        // `reconcile_checkpoint_for_missing_prefix`).
        if let Some(cp) =
            crate::checkpoint::reconcile_checkpoint_for_missing_prefix(store, cycle, pruned_up_to).await
        {
            info!(
                target: "catalyst.consensus.checkpoint",
                checkpoint_cycle = cp.cycle,
                target_cycle = cycle,
                "Using checkpoint for reconcile prefix"
            );
            replay_start = cp.cycle.saturating_add(1);
        }

        if let Some((net, requester)) = fetch {
            let prefetch_ms = catalyst_reconcile_prefetch_wait_ms();
            if prefetch_ms > 0 {
                reconcile_prefetch_lsu_metadata_if_missing(store, net, requester, replay_start, cycle, prefetch_ms)
                    .await;
            }
        }

        for i in replay_start..cycle {
            if store.get_metadata(&format!("consensus:lsu:{}", i)).await.ok().flatten().is_none() {
                if i <= pruned_up_to {
                    error!(
                        target: "catalyst.consensus.reconcile",
                        "Quorum fork reconcile FAILED (needs operator action): LSU metadata for cycle {} was pruned (pruned_up_to={}) and no checkpoint covers it; restore archival metadata or a peer snapshot",
                        i, pruned_up_to
                    );
                } else {
                    error!(
                        target: "catalyst.consensus.reconcile",
                        "Quorum fork reconcile FAILED: missing stored LSU for cycle {} and no checkpoint or peer bridged it before cycle {}; this node cannot safely verify a path forward and will keep deferring until this resolves",
                        i, cycle
                    );
                }
                catalyst_utils::increment_counter!("consensus_fork_reconcile_aborted_total", 1);
                catalyst_utils::increment_counter!("consensus_fork_reconcile_no_checkpoint_total", 1);
                return Ok(None);
            }
        }
        Ok(Some(replay_start))
    }

    let default_start = if behind { local_cycle.saturating_add(1).max(1) } else { 1u64 };
    let Some(mut replay_start) =
        resolve_replay_start(store, cycle, pruned_up_to, fetch, default_start).await?
    else {
        if record_reconcile_failure(local_cycle).await {
            error!(
                target: "catalyst.consensus.reconcile",
                local_cycle,
                threshold = crate::consensus_limits::reconcile_circuit_break_threshold(),
                "Reconcile circuit-broken after repeated consecutive failures resolving a replay gap; needs operator action (restart, resync, or investigate divergence)"
            );
        }
        return Ok(false);
    };

    let snap = format!(
        "quorum_reconcile_c{}_{}",
        cycle,
        current_timestamp_ms()
    );
    store
        .create_snapshot(&snap)
        .await
        .map_err(|e| anyhow::anyhow!("reconcile snapshot create: {e}"))?;

    let reconcile_inner = async {
        if !behind {
            // Case (B): discard whatever we applied at/after `cycle` and rebuild from scratch.
            // Purge FIRST, then (if a checkpoint anchors the prefix) restore it — restoring
            // onto an already-empty base makes the checkpoint's snapshot a true replace rather
            // than a merge on top of stale data.
            purge_mutable_chain_state_preserving_lsu_history(store).await?;
            if replay_start > 1 {
                let cp = crate::checkpoint::load_checkpoint_at_cycle(store, replay_start.saturating_sub(1))
                    .await
                    .ok_or_else(|| anyhow::anyhow!("checkpoint at cycle {} vanished", replay_start - 1))?;
                crate::checkpoint::restore_reconcile_from_checkpoint(store, &cp, local_cycle)
                    .await
                    .map_err(|e| anyhow::anyhow!("checkpoint restore: {e}"))?;
            } else {
                store
                    .refresh_cached_state_root()
                    .await
                    .map_err(|e| anyhow::anyhow!("refresh root: {e}"))?;
            }
        }
        // Case (A) needs no purge: we replay directly onto our own trusted current state.

        for i in replay_start..cycle {
            let key = format!("consensus:lsu:{}", i);
            let Some(bytes) = store.get_metadata(&key).await.ok().flatten() else {
                anyhow::bail!("missing lsu during replay {i}");
            };
            let lsu_i = catalyst_consensus::types::LedgerStateUpdate::deserialize(&bytes)
                .map_err(|e| anyhow::anyhow!("deserialize lsu {i}: {e}"))?;
            if lsu_i.cycle_number != i {
                anyhow::bail!("stored lsu cycle mismatch at {i}");
            }
            if let Some(hb) = store
                .get_metadata(&format!("consensus:lsu_hash:{}", i))
                .await
                .ok()
                .flatten()
            {
                if hb.len() == 32 {
                    let mut ah = [0u8; 32];
                    ah.copy_from_slice(&hb[..32]);
                    let gh = catalyst_consensus::types::hash_data(&lsu_i).unwrap_or([0u8; 32]);
                    if gh != ah {
                        anyhow::bail!("lsu hash mismatch at cycle {i}");
                    }
                }
            }
            apply_lsu_to_storage_locked(store, &lsu_i)
                .await
                .map_err(|e| anyhow::anyhow!("apply replay {i}: {e}"))?;
        }

        let prev_after = local_applied_state_root(store).await;
        if prev_after != expected_prev {
            anyhow::bail!(
                "replay prev root mismatch: got {} expected {}",
                hex_encode(&prev_after),
                hex_encode(&expected_prev)
            );
        }

        match apply_lsu_with_root_check_locked(store, lsu, expected_post).await {
            Ok(true) => Ok(()),
            Ok(false) => anyhow::bail!("final lsu state_root mismatch"),
            Err(e) => Err(e),
        }
    };

    match reconcile_inner.await {
        Ok(()) => {
            let _ = store.delete_snapshot(&snap).await;
            catalyst_utils::increment_counter!("consensus_fork_reconcile_succeeded_total", 1);
            clear_reconcile_failure(local_cycle).await;
            Ok(true)
        }
        Err(e) => {
            warn!(
                target: "catalyst.consensus.reconcile",
                "Quorum fork reconcile failed, restoring snapshot: {}",
                e
            );
            if let Err(e2) = store.load_snapshot(&snap).await {
                catalyst_utils::increment_counter!("consensus_fork_reconcile_error_total", 1);
                return Err(anyhow::anyhow!(
                    "reconcile failed ({e}) and snapshot restore failed ({e2})"
                ));
            }
            let _ = store.delete_snapshot(&snap).await;
            catalyst_utils::increment_counter!("consensus_fork_reconcile_aborted_total", 1);
            if record_reconcile_failure(local_cycle).await {
                // Transition into "broken": the per-attempt warn above will stop recurring from
                // here on (the circuit-breaker check short-circuits before reconcile_inner runs
                // again), so escalate once, loudly, since nothing else will.
                error!(
                    target: "catalyst.consensus.reconcile",
                    cycle,
                    local_cycle,
                    lsu_hash = %hex_encode(expected_lsu_hash),
                    threshold = crate::consensus_limits::reconcile_circuit_break_threshold(),
                    "Reconcile circuit-broken after repeated consecutive failures; needs operator action (restart, resync, or investigate divergence)"
                );
            }
            Ok(false)
        }
    }
}

/// Acquires `lsu_apply_lock` then delegates to [`apply_lsu_with_root_check_locked`]. Use this from
/// any caller that does not already hold the lock (the two direct gossip-handler fallback call
/// sites). Callers that already hold it (reconcile, `try_apply_stored_lsu_at_cycle`) must call
/// `apply_lsu_with_root_check_locked` directly to avoid deadlocking on the non-reentrant mutex.
async fn apply_lsu_with_root_check(
    store: &StorageManager,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    expected_state_root: [u8; 32],
) -> Result<bool> {
    let _apply_guard = lsu_apply_lock().lock().await;
    apply_lsu_with_root_check_locked(store, lsu, expected_state_root).await
}

/// Caller must already hold `lsu_apply_lock` (see [`apply_lsu_with_root_check`]).
async fn apply_lsu_with_root_check_locked(
    store: &StorageManager,
    lsu: &catalyst_consensus::types::LedgerStateUpdate,
    expected_state_root: [u8; 32],
) -> Result<bool> {
    let snap = format!("verify_lsu_{}_{}", lsu.cycle_number, current_timestamp_ms());
    let _ = store
        .create_snapshot(&snap)
        .await
        .map_err(|e| anyhow::anyhow!("snapshot create failed: {e}"))?;

    let got = apply_lsu_to_storage_locked(store, lsu).await?;
    if got == expected_state_root {
        // NOTE: this snapshot used to leak (neither branch deleted it). With `max_snapshots`
        // capped (default 10), this function is called far more often than a checkpoint is
        // written, so the leaked snapshots accumulated and eventually caused
        // `cleanup_old_snapshots` to evict the checkpoint snapshot itself as "oldest" — breaking
        // reconcile's ability to restore it (observed as "checkpoint restore: load checkpoint
        // snapshot ... failed" once reconcile could otherwise succeed).
        let _ = store.delete_snapshot(&snap).await;
        return Ok(true);
    }

    // Revert
    let _ = store
        .load_snapshot(&snap)
        .await
        .map_err(|e| anyhow::anyhow!("snapshot revert failed: {e}"))?;
    let _ = store.delete_snapshot(&snap).await;
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

        if self.config.validator
            && !crate::consensus_limits::effective_require_lsu_finality(&self.config)
        {
            warn!(
                target: "catalyst.consensus.policy",
                "Validator mode: LSU finality verification is disabled (consensus.require_lsu_finality=false or CATALYST_REQUIRE_LSU_FINALITY=0). Production validators should require ADR 0001 certificates (default true)."
            );
        }

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
                            let next_cycle = local_applied_cycle(storage.as_ref()).await.saturating_add(1);
                            if !protocol_tx_is_funded_with_fee_credits(storage.as_ref(), &msg.tx, next_cycle).await {
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
        let tx_batch_commits: Arc<
            tokio::sync::RwLock<std::collections::HashMap<u64, ([u8; 32], u32)>>,
        > = Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));
        let max_entries_per_cycle_cfg = self.config.consensus.max_transactions_per_block as usize;

        let finality_buckets: Arc<
            RwLock<std::collections::HashMap<(u64, [u8; 32]), FinalityAttestationBucket>>,
        > = Arc::new(RwLock::new(std::collections::HashMap::new()));

        // ADR 0002: state-root attestation buckets, keyed by (cycle, lsu_hash, state_root) so a
        // divergent minority's claimed root can never poison a majority bucket for the same LSU.
        let state_root_buckets: Arc<
            RwLock<std::collections::HashMap<(u64, [u8; 32], [u8; 32]), StateRootAttestationBucket>>,
        > = Arc::new(RwLock::new(std::collections::HashMap::new()));

        let validator_signing_key = crate::identity::load_or_generate_private_key(
            &self.config.node.private_key_file,
            self.config.node.auto_generate_identity,
        )
        .ok();

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

        // Highest network head observed over gossip, shared from the inbound/catch-up tasks to the
        // consensus production loop so a node that is behind defers producing (see depth-gate below).
        let observed_head_shared = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));

        // Inbound: envelopes received from peers → update validator map / handle tx gossip / handle LSU gossip / forward consensus messages.
        {
            let mut events = network.subscribe_events().await;
            let net = network.clone();
            let observed_head_pub = observed_head_shared.clone();
            let mempool = mempool.clone();
            let tx_batches = tx_batches.clone();
            let tx_batch_commits_in = tx_batch_commits.clone();
            let max_entries_per_cycle_cfg = max_entries_per_cycle_cfg;
            let dfs = dfs.clone();
            let storage = storage.clone();
            let producer_id_self = producer_id.clone();
            let gossip_cycle_ms: u64 = (self.config.consensus.cycle_duration_seconds as u64)
                .saturating_mul(1000);
            let finality_buckets_in = finality_buckets.clone();
            let state_root_buckets_in = state_root_buckets.clone();
            #[derive(Clone)]
            struct PendingLsuFetch {
                cycle: u64,
                lsu_hash: [u8; 32],
                prev_state_root: [u8; 32],
                expected_state_root: [u8; 32],
                proof_cid: String,
                finality_cid: String,
                state_root_cid: String,
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
                // Liveness backstop for a stale/unreachable `observed_head_cycle` (see
                // `stale_observed_head_reset_ms`): tracks the applied head last seen while behind,
                // and when that "behind" state started, so we can detect zero progress over time.
                stale_last_head: u64,
                stale_since_ms: Option<u64>,
                // Asymmetric-reset mitigation (docs/consensus-reliability-review-2026-07.md): the
                // stale-reset backstop above used to abandon a missing cycle purely on elapsed time,
                // with no way to know whether a peer had actually SUCCEEDED at producing it -- caught
                // live: one validator gave up and resumed several cycles ahead while a peer had
                // legitimately applied the cycle it abandoned, silently diverging their account
                // histories underneath an apparently-healthy vote count. When an `LsuRangeResponse`
                // positively confirms a peer has exactly the cycle we need next (`head+1`), record it
                // here so the reset logic gives that confirmed-reachable data substantially more time
                // before giving up, instead of treating "peer has it but our fetch hasn't landed yet"
                // the same as "genuinely nobody produced it".
                confirmed_next_cycle: Option<u64>,
                confirmed_next_cycle_since_ms: Option<u64>,
            }
            let catchup: Arc<tokio::sync::Mutex<CatchupState>> =
                Arc::new(tokio::sync::Mutex::new(CatchupState::default()));
            // Background: sequential follower apply (head+1) from stored LSU metadata + range backfill.
            let storage_advance = storage.clone();
            let dfs_advance = dfs.clone();
            let net_advance = net.clone();
            let producer_id_advance = producer_id_self.clone();
            let catchup_advance = catchup.clone();
            let observed_head_advance = observed_head_pub.clone();
            let mut shutdown_rx_advance = shutdown_rx.clone();
            let require_fin_advance =
                crate::consensus_limits::effective_require_lsu_finality(&self.config);
            let advance_handle = tokio::spawn(async move {
                let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    tokio::select! {
                        _ = tick.tick() => {},
                        _ = shutdown_rx_advance.changed() => {},
                    }
                    if *shutdown_rx_advance.borrow() {
                        break;
                    }
                    let Some(store) = &storage_advance else {
                        continue;
                    };
                    let applied = try_advance_applied_head_from_storage(
                        store.as_ref(),
                        dfs_advance.as_ref(),
                        require_fin_advance,
                        32,
                    )
                    .await;
                    if applied > 0 {
                        info!(
                            "Follower advanced applied head by {} cycle(s) to {}",
                            applied,
                            local_applied_cycle(store.as_ref()).await
                        );
                    }
                    let head = local_applied_cycle(store.as_ref()).await;
                    let now_ms = crate::consensus_limits::wall_now_ms();
                    // Liveness backstop: `observed_head_cycle` is monotonic (only ever raised by P2P
                    // gossip) so a phantom/unreachable head can never self-correct. If we've made zero
                    // applied-head progress for `stale_observed_head_reset_ms()` despite sitting behind
                    // and re-requesting the range every tick, nobody on the network can actually serve
                    // that cycle — abandon it and resume production from our own confirmed head rather
                    // than deferring forever (see `consensus_limits::stale_observed_head_reset_ms`).
                    let observed = {
                        let mut c = catchup_advance.lock().await;
                        if c.observed_head_cycle > head {
                            if c.stale_last_head != head {
                                c.stale_last_head = head;
                                c.stale_since_ms = Some(now_ms);
                            } else {
                                let since = *c.stale_since_ms.get_or_insert(now_ms);
                                let stale_budget_ms = crate::consensus_limits::stale_observed_head_reset_ms();
                                let need = head.saturating_add(1);
                                let peer_confirmed_reachable = c.confirmed_next_cycle == Some(need);
                                let elapsed_ms = now_ms.saturating_sub(since);
                                if should_reset_stale_observed_head(elapsed_ms, stale_budget_ms, peer_confirmed_reachable) {
                                    if peer_confirmed_reachable {
                                        error!(
                                            "observed_head_cycle {} stale for {}ms despite a peer confirming they have cycle {}; resetting anyway (fetch pipeline may be broken) — needs operator attention",
                                            c.observed_head_cycle,
                                            elapsed_ms,
                                            need
                                        );
                                        catalyst_utils::increment_counter!(
                                            "consensus_observed_head_stale_reset_despite_confirmed_total",
                                            1
                                        );
                                    } else {
                                        warn!(
                                            "observed_head_cycle {} stale for {}ms with no applied-head progress past {}; resetting to applied head (peers are not serving the requested range)",
                                            c.observed_head_cycle,
                                            elapsed_ms,
                                            head
                                        );
                                    }
                                    catalyst_utils::increment_counter!(
                                        "consensus_observed_head_stale_reset_total",
                                        1
                                    );
                                    c.observed_head_cycle = head;
                                    c.stale_since_ms = Some(now_ms);
                                    c.confirmed_next_cycle = None;
                                    c.confirmed_next_cycle_since_ms = None;
                                }
                            }
                        } else {
                            c.stale_last_head = head;
                            c.stale_since_ms = None;
                        }
                        c.observed_head_cycle
                    };
                    observed_head_advance.store(observed, std::sync::atomic::Ordering::Relaxed);
                    // Fetch the missing certified LSU(s) whenever we are behind by even ONE cycle.
                    // CRITICAL liveness fix: the production depth-gate defers at `observed > applied`
                    // (>=1 behind), so the catch-up fetch MUST also trigger at >=1 behind. The previous
                    // `observed > head + 1` threshold left a node that was exactly one cycle behind in a
                    // dead zone: gated from producing/voting, yet never requesting the one LSU it lacked.
                    // At N=3 with one validator down, that wedged node holds a vote the quorum needs, so a
                    // single missed tip-LSU froze the whole network permanently (observed testnet freeze).
                    if observed > head {
                        let mut c = catchup_advance.lock().await;
                        if now_ms.saturating_sub(c.last_req_ms) > 2000 {
                            let start = head.saturating_add(1);
                            let remaining = observed.saturating_sub(start).saturating_add(1);
                            let count = remaining.min(256) as u32;
                            let req = LsuRangeRequest {
                                requester: producer_id_advance.clone(),
                                start_cycle: start,
                                count,
                            };
                            if let Ok(env) =
                                MessageEnvelope::from_message(&req, "lsu_range_req_follower".to_string(), None)
                            {
                                let _ = net_advance.broadcast_envelope(&env).await;
                                c.last_req_ms = now_ms;
                            }
                        }
                    }
                }
            });
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

                    let now_ms = crate::consensus_limits::wall_now_ms();
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
            let require_lsu_finality_effective =
                crate::consensus_limits::effective_require_lsu_finality(&self.config);
            let require_state_root_finality_effective =
                crate::consensus_limits::effective_require_state_root_finality(&self.config);
            let handle = tokio::spawn(async move {
                while let Some(ev) = events.recv().await {
                    if let catalyst_network::NetworkEvent::MessageReceived { envelope, .. } = ev {
                        let now_ms = crate::consensus_limits::wall_now_ms();
                        let lsu_wall_lead_cap =
                            crate::consensus_limits::p2p_lsu_cycle_max_wall_lead();
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
                                    let next_cycle = local_applied_cycle(store.as_ref()).await.saturating_add(1);
                                    if tx_is_sane(&entries)
                                        && protocol_tx_is_funded_with_fee_credits(
                                            store.as_ref(),
                                            &tx.tx,
                                            next_cycle,
                                        )
                                        .await
                                    {
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
                        } else if envelope.message_type == MessageType::TransactionRequest {
                            if let Ok(ctrl) = envelope.extract_message::<TxBatchControl>() {
                                match ctrl {
                                    TxBatchControl::Commit { cycle, .. } => {
                                        let slack =
                                            crate::consensus_limits::p2p_tx_batch_max_cycle_slack();
                                        if !crate::consensus_limits::p2p_tx_batch_cycle_allows_ingress(
                                            cycle,
                                            now_ms,
                                            gossip_cycle_ms,
                                            slack,
                                        ) {
                                            tracing::debug!(
                                                target: "catalyst.consensus.tx_batch",
                                                cycle,
                                                now_ms,
                                                gossip_cycle_ms,
                                                slack,
                                                "ignoring TxBatchControl::Commit at ingress: cycle outside wall slack window"
                                            );
                                            catalyst_utils::increment_counter!(
                                                "consensus_p2p_tx_batch_commit_cycle_rejected_total",
                                                1
                                            );
                                        } else {
                                            let p2p_state = crate::tx_batch_p2p::TxBatchP2pState {
                                                tx_batches: tx_batches.clone(),
                                                commit_ram: tx_batch_commits_in.clone(),
                                            };
                                            let _ = crate::tx_batch_p2p::ingest_p2p_tx_batch_envelope(
                                                &p2p_state,
                                                &envelope,
                                                now_ms,
                                                gossip_cycle_ms,
                                                storage.as_deref(),
                                                max_entries_per_cycle_cfg,
                                            )
                                            .await;
                                            let do_relay = {
                                                let mut rc = relay_cache.lock().await;
                                                rc.should_relay(&envelope, now_ms)
                                            };
                                            if do_relay {
                                                let _ = net.broadcast_envelope(&envelope).await;
                                            }
                                        }
                                    }
                                    TxBatchControl::ResyncRequest { cycle, .. } => {
                                        // Always re-broadcast the commit if we have it.
                                        // This is the only recovery path for empty batches
                                        // (zero transactions): the resync-reply below only
                                        // sends a ProtocolTxBatch, which is skipped when
                                        // txids is empty, so without this a follower that
                                        // missed the one-shot Commit broadcast can never
                                        // resolve AgreedEmpty and stalls with TX_BATCH_MISS_FATAL.
                                        let stored_commit = {
                                            let ram = tx_batch_commits_in.read().await.get(&cycle).copied();
                                            if let (None, Some(store)) = (ram, &storage) {
                                                read_tx_batch_commit(store.as_ref(), cycle).await
                                            } else {
                                                ram
                                            }
                                        };
                                        if let Some((batch_hash, tx_count)) = stored_commit {
                                            let reply = TxBatchControl::Commit {
                                                cycle,
                                                batch_hash,
                                                tx_count,
                                            };
                                            if let Ok(env2) = MessageEnvelope::from_message(
                                                &reply,
                                                "tx_batch_commit_relay".to_string(),
                                                None,
                                            ) {
                                                let _ = net.broadcast_envelope(&env2).await;
                                            }
                                        }
                                        // Also relay the full ProtocolTxBatch when we have txids.
                                        if let Some(store) = &storage {
                                            let txids = load_cycle_txids(store.as_ref(), cycle).await;
                                            if !txids.is_empty() {
                                                let mut txs: Vec<catalyst_core::protocol::Transaction> =
                                                    Vec::with_capacity(txids.len());
                                                let mut ok = true;
                                                for txid in &txids {
                                                    let raw = match store
                                                        .get_metadata(&mempool_tx_key(txid))
                                                        .await
                                                        .ok()
                                                        .flatten()
                                                    {
                                                        Some(b) => Some(b),
                                                        None => store
                                                            .get_metadata(&tx_raw_key(txid))
                                                            .await
                                                            .ok()
                                                            .flatten(),
                                                    };
                                                    let Some(raw) = raw else {
                                                        ok = false;
                                                        break;
                                                    };
                                                    let Ok(tx) =
                                                        bincode::deserialize::<catalyst_core::protocol::Transaction>(
                                                            &raw,
                                                        )
                                                    else {
                                                        ok = false;
                                                        break;
                                                    };
                                                    txs.push(tx);
                                                }
                                                if ok {
                                                    let valid = validate_and_select_protocol_txs_for_construction(
                                                        store.as_ref(),
                                                        cycle,
                                                        txs,
                                                        max_entries_per_cycle_cfg,
                                                    )
                                                    .await;
                                                    if let Ok(batch) = ProtocolTxBatch::new(cycle, valid) {
                                                        if let Ok(env2) = MessageEnvelope::from_message(
                                                            &batch,
                                                            "tx_batch_resync_reply".to_string(),
                                                            None,
                                                        ) {
                                                            let _ = net.broadcast_envelope(&env2).await;
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else if envelope.message_type == MessageType::TransactionBatch {
                            let p2p_state = crate::tx_batch_p2p::TxBatchP2pState {
                                tx_batches: tx_batches.clone(),
                                commit_ram: tx_batch_commits_in.clone(),
                            };
                            let accepted = crate::tx_batch_p2p::ingest_p2p_tx_batch_envelope(
                                &p2p_state,
                                &envelope,
                                now_ms,
                                gossip_cycle_ms,
                                storage.as_deref(),
                                max_entries_per_cycle_cfg,
                            )
                            .await;
                            if !accepted {
                                if let Ok(batch) = envelope.extract_message::<ProtocolTxBatch>() {
                                    if !crate::consensus_limits::p2p_tx_batch_cycle_allows_ingress(
                                        batch.cycle,
                                        now_ms,
                                        gossip_cycle_ms,
                                        crate::consensus_limits::p2p_tx_batch_max_cycle_slack(),
                                    ) {
                                        catalyst_utils::increment_counter!(
                                            "consensus_p2p_tx_batch_cycle_rejected_total",
                                            1
                                        );
                                    } else if batch.verify_hash().unwrap_or(false) {
                                        if let Some(store) = &storage {
                                            let pinned = {
                                                let ram = tx_batch_commits_in
                                                    .read()
                                                    .await
                                                    .get(&batch.cycle)
                                                    .copied();
                                                let disk =
                                                    read_tx_batch_commit(store.as_ref(), batch.cycle)
                                                        .await;
                                                ram.or(disk)
                                            };
                                            if pinned.map(|cm| {
                                                crate::consensus_limits::protocol_tx_batch_disagrees_with_commit(
                                                    &batch, cm,
                                                )
                                            }) == Some(true)
                                            {
                                                catalyst_utils::increment_counter!(
                                                    "consensus_p2p_tx_batch_commit_disagrees_total",
                                                    1
                                                );
                                            }
                                        }
                                    }
                                } else if let Ok(batch) = envelope.extract_message::<TxBatch>() {
                                    if !crate::consensus_limits::p2p_tx_batch_cycle_allows_ingress(
                                        batch.cycle,
                                        now_ms,
                                        gossip_cycle_ms,
                                        crate::consensus_limits::p2p_tx_batch_max_cycle_slack(),
                                    ) {
                                        catalyst_utils::increment_counter!(
                                            "consensus_p2p_tx_batch_cycle_rejected_total",
                                            1
                                        );
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
                                if !crate::consensus_limits::p2p_lsu_cycle_within_wall_lead(
                                    ref_msg.cycle,
                                    now_ms,
                                    gossip_cycle_ms,
                                    lsu_wall_lead_cap,
                                ) {
                                    tracing::debug!(
                                        target: "catalyst.consensus.policy",
                                        cycle = ref_msg.cycle,
                                        now_ms,
                                        gossip_cycle_ms,
                                        lead_cap = lsu_wall_lead_cap,
                                        "dropping LsuCidGossip: cycle too far ahead of wall clock"
                                    );
                                    catalyst_utils::increment_counter!(
                                        "consensus_p2p_lsu_cycle_rejected_total",
                                        1
                                    );
                                    continue;
                                }
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
                                                                Some(dfs),
                                                                ref_msg.cycle,
                                                                &bytes,
                                                                &ref_msg.lsu_hash,
                                                                &ref_msg.cid,
                                                                &ref_msg.state_root,
                                                                &ref_msg.finality_cid,
                                                                require_lsu_finality_effective,
                                                            )
                                                            .await;

                                                            match fork_choice_apply_gate(
                                                                store.as_ref(),
                                                                Some(dfs),
                                                                &lsu,
                                                                ref_msg.cycle,
                                                                ref_msg.lsu_hash,
                                                                &ref_msg.finality_cid,
                                                                require_lsu_finality_effective,
                                                            )
                                                            .await
                                                            {
                                                                ForkChoiceApplyGate::Allow => {}
                                                                ForkChoiceApplyGate::RejectWeaker => {
                                                                    continue;
                                                                }
                                                                ForkChoiceApplyGate::RejectNonCertified => {
                                                                    catalyst_utils::increment_counter!(
                                                                        "consensus_fork_choice_rejected_non_certified_total",
                                                                        1
                                                                    );
                                                                    continue;
                                                                }
                                                                ForkChoiceApplyGate::StallCertifiedEquivocation => {
                                                                    continue;
                                                                }
                                                            }

                                                            // Ensure we're applying onto the expected previous root.
                                                            //
                                                            // IMPORTANT: do not apply onto an "unknown" root. The only
                                                            // safe bypass is the explicit bootstrap case where both
                                                            // local and expected prev roots are zero.
                                                            let local_prev = local_applied_state_root(store.as_ref()).await;
                                                            let local_applied_now = local_applied_cycle(store.as_ref()).await;
                                                            let mut already_applied = false;
                                                            if try_reorg_stronger_lsu_at_applied_cycle(
                                                                store.as_ref(),
                                                                &lsu,
                                                                ref_msg.cycle,
                                                                ref_msg.prev_state_root,
                                                                ref_msg.state_root,
                                                                &ref_msg.lsu_hash,
                                                                Some((net.as_ref(), producer_id_self.as_str())),
                                                                Some(dfs),
                                                                &ref_msg.finality_cid,
                                                                require_lsu_finality_effective,
                                                            )
                                                            .await
                                                            .unwrap_or(false)
                                                            {
                                                                already_applied = true;
                                                            } else if local_applied_now == ref_msg.cycle {
                                                                // Cycle already at this head and incoming LSU was same-or-weaker
                                                                // (try_reorg returned false). Never reconcile: reconcile does
                                                                // restore_checkpoint then purge_mutable_chain_state, which takes
                                                                // the snapshot at the checkpoint state (pre-cycle), not the
                                                                // post-cycle state — so a failed reconcile would revert the node
                                                                // one cycle backward. Since the applied state is already correct,
                                                                // no reconcile is needed.
                                                                already_applied = true;
                                                            } else if local_prev != ref_msg.prev_state_root {
                                                                if local_prev == [0u8; 32] && ref_msg.prev_state_root == [0u8; 32] {
                                                                    // bootstrap ok
                                                                } else {
                                                                    match try_reconcile_fork_from_quorum_lsu(
                                                                        store.as_ref(),
                                                                        &lsu,
                                                                        ref_msg.cycle,
                                                                        ref_msg.prev_state_root,
                                                                        ref_msg.state_root,
                                                                        &ref_msg.lsu_hash,
                                                                        Some((net.as_ref(), producer_id_self.as_str())),
                                                                        Some(dfs),
                                                                        &ref_msg.finality_cid,
                                                                        require_lsu_finality_effective,
                                                                    )
                                                                    .await
                                                                    {
                                                                        Ok(true) => {
                                                                            info!(
                                                                                "Reconciled fork via BFT-quorum LSU replay cycle={} cid={}",
                                                                                ref_msg.cycle, ref_msg.cid
                                                                            );
                                                                            already_applied = true;
                                                                        }
                                                                        Ok(false) => {
                                                                            tracing::warn!(
                                                                                target: "catalyst.consensus.prev_root",
                                                                                "Skipping LSU cycle={} cid={} (prev_root mismatch local={} expected={})",
                                                                                ref_msg.cycle,
                                                                                ref_msg.cid,
                                                                                hex_encode(&local_prev),
                                                                                hex_encode(&ref_msg.prev_state_root)
                                                                            );
                                                                            catalyst_utils::increment_counter!(
                                                                                "consensus_lsu_apply_skipped_prev_root_total",
                                                                                1
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
                                                                        Err(e) => {
                                                                            tracing::warn!(
                                                                                target: "catalyst.consensus.reconcile",
                                                                                "Quorum reconcile error cycle={} cid={}: {}",
                                                                                ref_msg.cycle,
                                                                                ref_msg.cid,
                                                                                e
                                                                            );
                                                                            continue;
                                                                        }
                                                                    }
                                                                }
                                                            }

                                                            // Proof-driven path: if we have a proof bundle CID locally, verify it and apply without recomputing root.
                                                            let mut applied = already_applied;
                                                            if !applied && !ref_msg.proof_cid.is_empty() {
                                                                if let Ok(pb) = dfs.get(&ref_msg.proof_cid).await {
                                                                    if let Ok(bundle) = bincode::deserialize::<StateProofBundle>(&pb) {
                                                                        if bundle.prev_state_root == ref_msg.prev_state_root
                                                                            && bundle.new_state_root == ref_msg.state_root
                                                                            && bundle.lsu_hash == ref_msg.lsu_hash
                                                                            && verify_state_transition_bundle(&lsu, &bundle)
                                                                            && verify_state_root_finality_for_apply(
                                                                                store.as_ref(),
                                                                                dfs,
                                                                                &lsu,
                                                                                ref_msg.cycle,
                                                                                bundle.new_state_root,
                                                                                &ref_msg.state_root_cid,
                                                                                require_state_root_finality_effective,
                                                                            )
                                                                            .await
                                                                        {
                                                                            if apply_lsu_to_storage_without_root_check(
                                                                                store.as_ref(),
                                                                                &lsu,
                                                                                bundle.new_state_root,
                                                                            )
                                                                            .await
                                                                            .unwrap_or(false)
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
                                                            if !applied {
                                                                continue;
                                                            }
                                                            if !verify_lsu_finality_for_apply(
                                                                store.as_ref(),
                                                                dfs,
                                                                &lsu,
                                                                ref_msg.cycle,
                                                                &ref_msg.finality_cid,
                                                                require_lsu_finality_effective,
                                                            )
                                                            .await
                                                            {
                                                                continue;
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
                                                    finality_cid: ref_msg.finality_cid.clone(),
                                                    state_root_cid: ref_msg.state_root_cid.clone(),
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
                                        if !crate::consensus_limits::p2p_lsu_cycle_within_wall_lead(
                                            lsu_msg.cycle,
                                            now_ms,
                                            gossip_cycle_ms,
                                            lsu_wall_lead_cap,
                                        ) {
                                            tracing::debug!(
                                                target: "catalyst.consensus.policy",
                                                cycle = lsu_msg.cycle,
                                                now_ms,
                                                gossip_cycle_ms,
                                                lead_cap = lsu_wall_lead_cap,
                                                "dropping LsuGossip: cycle too far ahead of wall clock"
                                            );
                                            catalyst_utils::increment_counter!(
                                                "consensus_p2p_lsu_cycle_rejected_total",
                                                1
                                            );
                                            continue;
                                        }
                                        let lsu = lsu_msg.lsu.clone();
                                        let mut relay_ok = true;

                                        if let Some(store) = &storage {
                                            let apply_ok = match dfs.as_ref() {
                                                None if require_lsu_finality_effective => {
                                                    tracing::warn!(
                                                        target: "catalyst.consensus.apply",
                                                        cycle = lsu_msg.cycle,
                                                        "Rejecting LsuGossip apply: require_lsu_finality enabled but DFS/content store is not configured"
                                                    );
                                                    catalyst_utils::increment_counter!(
                                                        "consensus_lsu_gossip_apply_rejected_finality_total",
                                                        1
                                                    );
                                                    false
                                                }
                                                None => true,
                                                Some(dfs) => {
                                                    let ok = verify_lsu_finality_for_apply(
                                                        store.as_ref(),
                                                        dfs,
                                                        &lsu,
                                                        lsu_msg.cycle,
                                                        "",
                                                        require_lsu_finality_effective,
                                                    )
                                                    .await;
                                                    if !ok {
                                                        catalyst_utils::increment_counter!(
                                                            "consensus_lsu_gossip_apply_rejected_finality_total",
                                                            1
                                                        );
                                                    }
                                                    ok
                                                }
                                            };

                                            if !apply_ok {
                                                relay_ok = false;
                                            } else {
                                                match fork_choice_apply_gate(
                                                    store.as_ref(),
                                                    dfs.as_ref(),
                                                    &lsu,
                                                    lsu_msg.cycle,
                                                    lsu_msg.lsu_hash,
                                                    "",
                                                    require_lsu_finality_effective,
                                                )
                                                .await
                                                {
                                                    ForkChoiceApplyGate::Allow => {}
                                                    ForkChoiceApplyGate::RejectWeaker => {
                                                        relay_ok = false;
                                                    }
                                                    ForkChoiceApplyGate::RejectNonCertified => {
                                                        catalyst_utils::increment_counter!(
                                                            "consensus_fork_choice_rejected_non_certified_total",
                                                            1
                                                        );
                                                        relay_ok = false;
                                                    }
                                                    ForkChoiceApplyGate::StallCertifiedEquivocation => {
                                                        relay_ok = false;
                                                    }
                                                }
                                                if relay_ok {
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
                                                        .set_metadata(
                                                            "consensus:last_lsu_cycle",
                                                            &lsu_msg.cycle.to_le_bytes(),
                                                        )
                                                        .await;
                                                }
                                                last_lsu.write().await.insert(lsu_msg.cycle, lsu);
                                                }
                                            }
                                        } else {
                                            last_lsu.write().await.insert(lsu_msg.cycle, lsu);
                                        }
                                        if relay_ok {
                                            // Relay only after hash check and policy allow.
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
                            } else if let Ok(_) = envelope.extract_message::<LedgerStateUpdateBroadcast>() {
                                // LedgerStateUpdateBroadcast from a producer's synchronization phase.
                                // Relay so multi-hop WAN topologies can deliver it, then forward to
                                // the consensus engine where the observer path reads it.
                                let do_relay = {
                                    let mut rc = relay_cache.lock().await;
                                    rc.should_relay(&envelope, now_ms)
                                };
                                if do_relay {
                                    let _ = net.broadcast_envelope(&envelope).await;
                                }
                                let _ = in_tx.send(envelope.clone());
                            }
                        } else if envelope.message_type == MessageType::LsuFinalityAttestation {
                            if let Ok(msg) = envelope.extract_message::<LsuFinalityAttestationMsg>() {
                                if !crate::consensus_limits::p2p_lsu_cycle_within_wall_lead(
                                    msg.cycle,
                                    now_ms,
                                    gossip_cycle_ms,
                                    lsu_wall_lead_cap,
                                ) {
                                    tracing::debug!(
                                        target: "catalyst.consensus.policy",
                                        cycle = msg.cycle,
                                        "dropping LsuFinalityAttestationMsg: cycle too far ahead of wall clock"
                                    );
                                    catalyst_utils::increment_counter!(
                                        "consensus_p2p_lsu_cycle_rejected_total",
                                        1
                                    );
                                    continue;
                                }
                                let Some(store) = &storage else {
                                    continue;
                                };
                                let Some(dfs) = &dfs else {
                                    continue;
                                };
                                ingest_finality_attestation(
                                    store.as_ref(),
                                    dfs,
                                    &net,
                                    &finality_buckets_in,
                                    &msg,
                                    &producer_id_self,
                                )
                                .await;
                                let do_relay = {
                                    let mut rc = relay_cache.lock().await;
                                    rc.should_relay(&envelope, now_ms)
                                };
                                if do_relay {
                                    let _ = net.broadcast_envelope(&envelope).await;
                                }
                            }
                        } else if envelope.message_type == MessageType::LsuFinalityCid {
                            if let Ok(msg) = envelope.extract_message::<LsuFinalityCidMsg>() {
                                if !crate::consensus_limits::p2p_lsu_cycle_within_wall_lead(
                                    msg.cycle,
                                    now_ms,
                                    gossip_cycle_ms,
                                    lsu_wall_lead_cap,
                                ) {
                                    tracing::debug!(
                                        target: "catalyst.consensus.policy",
                                        cycle = msg.cycle,
                                        "dropping LsuFinalityCidMsg: cycle too far ahead of wall clock"
                                    );
                                    catalyst_utils::increment_counter!(
                                        "consensus_p2p_lsu_cycle_rejected_total",
                                        1
                                    );
                                    continue;
                                }
                                if let Some(store) = &storage {
                                    let _ = store
                                        .set_metadata(
                                            &format!("consensus:lsu_finality_cid:{}", msg.cycle),
                                            msg.finality_cid.as_bytes(),
                                        )
                                        .await;
                                }
                                // Finality bytes live in the local DFS cache (not IPFS API). Fetch via P2P if missing.
                                if !msg.finality_cid.is_empty() {
                                    let need_fetch = match &dfs {
                                        Some(d) => !d.has(&msg.finality_cid).await,
                                        None => true,
                                    };
                                    if need_fetch {
                                        let req = FileRequestMsg {
                                            requester: producer_id_self.clone(),
                                            cid: msg.finality_cid.clone(),
                                        };
                                        if let Ok(env) = MessageEnvelope::from_message(
                                            &req,
                                            "file_req_finality_cid".to_string(),
                                            None,
                                        ) {
                                            let _ = net.broadcast_envelope(&env).await;
                                        }
                                    }
                                }
                                let do_relay = {
                                    let mut rc = relay_cache.lock().await;
                                    rc.should_relay(&envelope, now_ms)
                                };
                                if do_relay {
                                    let _ = net.broadcast_envelope(&envelope).await;
                                }
                            }
                        } else if envelope.message_type == MessageType::StateRootAttestation {
                            if let Ok(msg) = envelope.extract_message::<StateRootAttestationMsg>() {
                                if !crate::consensus_limits::p2p_lsu_cycle_within_wall_lead(
                                    msg.cycle,
                                    now_ms,
                                    gossip_cycle_ms,
                                    lsu_wall_lead_cap,
                                ) {
                                    tracing::debug!(
                                        target: "catalyst.consensus.policy",
                                        cycle = msg.cycle,
                                        "dropping StateRootAttestationMsg: cycle too far ahead of wall clock"
                                    );
                                    catalyst_utils::increment_counter!(
                                        "consensus_p2p_lsu_cycle_rejected_total",
                                        1
                                    );
                                    continue;
                                }
                                let Some(store) = &storage else {
                                    continue;
                                };
                                let Some(dfs) = &dfs else {
                                    continue;
                                };
                                ingest_state_root_attestation(
                                    store.as_ref(),
                                    dfs,
                                    &net,
                                    &state_root_buckets_in,
                                    &msg,
                                    &producer_id_self,
                                )
                                .await;
                                let do_relay = {
                                    let mut rc = relay_cache.lock().await;
                                    rc.should_relay(&envelope, now_ms)
                                };
                                if do_relay {
                                    let _ = net.broadcast_envelope(&envelope).await;
                                }
                            }
                        } else if envelope.message_type == MessageType::StateRootFinalityCid {
                            if let Ok(msg) = envelope.extract_message::<StateRootFinalityCidMsg>() {
                                if !crate::consensus_limits::p2p_lsu_cycle_within_wall_lead(
                                    msg.cycle,
                                    now_ms,
                                    gossip_cycle_ms,
                                    lsu_wall_lead_cap,
                                ) {
                                    tracing::debug!(
                                        target: "catalyst.consensus.policy",
                                        cycle = msg.cycle,
                                        "dropping StateRootFinalityCidMsg: cycle too far ahead of wall clock"
                                    );
                                    catalyst_utils::increment_counter!(
                                        "consensus_p2p_lsu_cycle_rejected_total",
                                        1
                                    );
                                    continue;
                                }
                                if let Some(store) = &storage {
                                    let _ = store
                                        .set_metadata(
                                            &format!("consensus:lsu_state_root_cid:{}", msg.cycle),
                                            msg.state_root_cid.as_bytes(),
                                        )
                                        .await;
                                }
                                // State-root cert bytes live in the local DFS cache. Fetch via P2P if missing.
                                if !msg.state_root_cid.is_empty() {
                                    let need_fetch = match &dfs {
                                        Some(d) => !d.has(&msg.state_root_cid).await,
                                        None => true,
                                    };
                                    if need_fetch {
                                        let req = FileRequestMsg {
                                            requester: producer_id_self.clone(),
                                            cid: msg.state_root_cid.clone(),
                                        };
                                        if let Ok(env) = MessageEnvelope::from_message(
                                            &req,
                                            "file_req_state_root_cid".to_string(),
                                            None,
                                        ) {
                                            let _ = net.broadcast_envelope(&env).await;
                                        }
                                    }
                                }
                                let do_relay = {
                                    let mut rc = relay_cache.lock().await;
                                    rc.should_relay(&envelope, now_ms)
                                };
                                if do_relay {
                                    let _ = net.broadcast_envelope(&envelope).await;
                                }
                            }
                        } else if envelope.message_type == MessageType::StateRequest {
                            if let Ok(req) = envelope.extract_message::<LsuRangeRequest>() {
                                if !crate::consensus_limits::p2p_lsu_cycle_within_wall_lead(
                                    req.start_cycle,
                                    now_ms,
                                    gossip_cycle_ms,
                                    lsu_wall_lead_cap,
                                ) {
                                    tracing::debug!(
                                        target: "catalyst.consensus.policy",
                                        start_cycle = req.start_cycle,
                                        "ignoring LsuRangeRequest: start_cycle too far ahead of wall clock"
                                    );
                                    catalyst_utils::increment_counter!(
                                        "consensus_p2p_lsu_cycle_rejected_total",
                                        1
                                    );
                                    continue;
                                }
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
                                    let finality_cid = meta_string(
                                        store.as_ref(),
                                        &format!("consensus:lsu_finality_cid:{}", cycle),
                                    )
                                    .await
                                    .unwrap_or_default();
                                    let state_root_cid = meta_string(
                                        store.as_ref(),
                                        &format!("consensus:lsu_state_root_cid:{}", cycle),
                                    )
                                    .await
                                    .unwrap_or_default();
                                    refs.push(LsuCidGossip {
                                        cycle,
                                        lsu_hash,
                                        cid,
                                        prev_state_root,
                                        state_root,
                                        proof_cid: String::new(),
                                        finality_cid,
                                        state_root_cid,
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
                                // Also push full LSU bytes alongside the CID reference so the
                                // requester can apply without a DFS file-fetch round-trip.
                                // Cap at 8 cycles to avoid flooding; each node requesting
                                // sequential catch-up will issue its own range request.
                                for gossip_ref in resp.refs.iter().take(8) {
                                    let cycle_g = gossip_ref.cycle;
                                    if let Some(bytes) = store
                                        .get_metadata(&format!("consensus:lsu:{}", cycle_g))
                                        .await
                                        .ok()
                                        .flatten()
                                    {
                                        if let Ok(lsu) = catalyst_consensus::types::LedgerStateUpdate::deserialize(&bytes) {
                                            if let Ok(gossip) = LsuGossip::new(lsu) {
                                                if let Ok(env) = MessageEnvelope::from_message(
                                                    &gossip, "lsu_push".to_string(), None,
                                                ) {
                                                    let _ = net.broadcast_envelope(&env).await;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        } else if envelope.message_type == MessageType::StateResponse {
                            if let Ok(resp) = envelope.extract_message::<LsuRangeResponse>() {
                                if resp.requester != producer_id_self {
                                    continue;
                                }
                                // Update observed head based on response (ignore absurd far-future refs).
                                if let Some(max_cycle) = resp
                                    .refs
                                    .iter()
                                    .filter_map(|r| {
                                        crate::consensus_limits::p2p_lsu_cycle_within_wall_lead(
                                            r.cycle,
                                            now_ms,
                                            gossip_cycle_ms,
                                            lsu_wall_lead_cap,
                                        )
                                        .then_some(r.cycle)
                                    })
                                    .max()
                                {
                                    let mut c = catchup.lock().await;
                                    c.observed_head_cycle = c.observed_head_cycle.max(max_cycle);
                                }
                                // Asymmetric-reset mitigation: if this response positively confirms a
                                // peer has exactly the cycle we need next, record it so the stale-reset
                                // backstop (advance_handle ticker) doesn't abandon it out from under
                                // them -- see the CatchupState doc comment and
                                // docs/consensus-reliability-review-2026-07.md.
                                if let Some(store) = &storage {
                                    let need = local_applied_cycle(store.as_ref()).await.saturating_add(1);
                                    if resp.refs.iter().any(|r| r.cycle == need) {
                                        let mut c = catchup.lock().await;
                                        c.confirmed_next_cycle = Some(need);
                                        c.confirmed_next_cycle_since_ms = Some(now_ms);
                                    }
                                }
                                // For each reference, if we don't have bytes locally, request via FileRequest.
                                for r in resp.refs {
                                    if !crate::consensus_limits::p2p_lsu_cycle_within_wall_lead(
                                        r.cycle,
                                        now_ms,
                                        gossip_cycle_ms,
                                        lsu_wall_lead_cap,
                                    ) {
                                        tracing::debug!(
                                            target: "catalyst.consensus.policy",
                                            cycle = r.cycle,
                                            "skipping LsuRangeResponse ref: cycle too far ahead of wall clock"
                                        );
                                        catalyst_utils::increment_counter!(
                                            "consensus_p2p_lsu_cycle_rejected_total",
                                            1
                                        );
                                        continue;
                                    }
                                    if let Some(store) = &storage {
                                        // If LSU bytes exist locally, still try sequential apply at head+1.
                                        if store
                                            .get_metadata(&format!("consensus:lsu:{}", r.cycle))
                                            .await
                                            .ok()
                                            .flatten()
                                            .is_some()
                                        {
                                            let head = local_applied_cycle(store.as_ref()).await;
                                            if r.cycle == head.saturating_add(1) {
                                                let _ = try_apply_stored_lsu_at_cycle(
                                                    store.as_ref(),
                                                    dfs.as_ref(),
                                                    r.cycle,
                                                    require_lsu_finality_effective,
                                                )
                                                .await;
                                            }
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
                                            finality_cid: r.finality_cid.clone(),
                                            state_root_cid: r.state_root_cid.clone(),
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
                                // Always persist bytes (finality certs, LSU blobs, etc.) in local DFS cache.
                                if let Some(dfs) = &dfs {
                                    let _ = dfs.put(resp.bytes.clone()).await;
                                }
                                let Some(info) = pending_lsu_fetch_inbound.write().await.remove(&resp.cid) else {
                                    continue;
                                };

                                if !crate::consensus_limits::p2p_lsu_cycle_within_wall_lead(
                                    info.cycle,
                                    now_ms,
                                    gossip_cycle_ms,
                                    lsu_wall_lead_cap,
                                ) {
                                    tracing::debug!(
                                        target: "catalyst.consensus.policy",
                                        cycle = info.cycle,
                                        cid = %resp.cid,
                                        "dropping FileResponse LSU fetch: cycle too far ahead of wall clock"
                                    );
                                    catalyst_utils::increment_counter!(
                                        "consensus_p2p_lsu_cycle_rejected_total",
                                        1
                                    );
                                    continue;
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
                                                    dfs.as_ref(),
                                                    info.cycle,
                                                    &resp.bytes,
                                                    &info.lsu_hash,
                                                    &resp.cid,
                                                    &info.expected_state_root,
                                                    &info.finality_cid,
                                                    require_lsu_finality_effective,
                                                )
                                                .await;

                                            match fork_choice_apply_gate(
                                                store.as_ref(),
                                                dfs.as_ref(),
                                                &lsu,
                                                info.cycle,
                                                info.lsu_hash,
                                                &info.finality_cid,
                                                require_lsu_finality_effective,
                                            )
                                            .await
                                            {
                                                ForkChoiceApplyGate::Allow => {}
                                                ForkChoiceApplyGate::RejectWeaker => continue,
                                                ForkChoiceApplyGate::RejectNonCertified => {
                                                    catalyst_utils::increment_counter!(
                                                        "consensus_fork_choice_rejected_non_certified_total",
                                                        1
                                                    );
                                                    continue;
                                                }
                                                ForkChoiceApplyGate::StallCertifiedEquivocation => {
                                                    continue;
                                                }
                                            }

                                            // Ensure we're applying onto the expected previous root.
                                            //
                                            // IMPORTANT: do not apply onto an "unknown" root. The only
                                            // safe bypass is the explicit bootstrap case where both
                                            // local and expected prev roots are zero.
                                            let local_prev = local_applied_state_root(store.as_ref()).await;
                                            let local_applied_now = local_applied_cycle(store.as_ref()).await;
                                            let mut ok = false;
                                            if try_reorg_stronger_lsu_at_applied_cycle(
                                                store.as_ref(),
                                                &lsu,
                                                info.cycle,
                                                info.prev_state_root,
                                                info.expected_state_root,
                                                &info.lsu_hash,
                                                Some((net.as_ref(), producer_id_self.as_str())),
                                                dfs.as_ref(),
                                                &info.finality_cid,
                                                require_lsu_finality_effective,
                                            )
                                            .await
                                            .unwrap_or(false)
                                            {
                                                ok = true;
                                            } else if local_applied_now == info.cycle {
                                                // Cycle already applied; incoming same-or-weaker. Skip reconcile
                                                // to prevent checkpoint-restore reversion (see LsuCidGossip path).
                                                ok = true;
                                            } else if local_prev != info.prev_state_root {
                                                if local_prev == [0u8; 32] && info.prev_state_root == [0u8; 32] {
                                                    // bootstrap ok
                                                } else {
                                                    match try_reconcile_fork_from_quorum_lsu(
                                                        store.as_ref(),
                                                        &lsu,
                                                        info.cycle,
                                                        info.prev_state_root,
                                                        info.expected_state_root,
                                                        &info.lsu_hash,
                                                        Some((net.as_ref(), producer_id_self.as_str())),
                                                        dfs.as_ref(),
                                                        &info.finality_cid,
                                                        require_lsu_finality_effective,
                                                    )
                                                    .await
                                                    {
                                                        Ok(true) => {
                                                            info!(
                                                                "Reconciled fork via BFT-quorum LSU replay cycle={} cid={}",
                                                                info.cycle, resp.cid
                                                            );
                                                            ok = true;
                                                        }
                                                        Ok(false) => {
                                                            info!(
                                                                "Backfilled LSU bytes (history only) cycle={} cid={}",
                                                                info.cycle, resp.cid
                                                            );
                                                            continue;
                                                        }
                                                        Err(e) => {
                                                            warn!(
                                                                target: "catalyst.consensus.reconcile",
                                                                "Quorum reconcile error cycle={} cid={}: {}",
                                                                info.cycle, resp.cid, e
                                                            );
                                                            continue;
                                                        }
                                                    }
                                                }
                                            }

                                            // Proof-driven path if proof bundle is present locally.
                                            if !ok {
                                                if let Some(dfs) = &dfs {
                                                    if !info.proof_cid.is_empty() {
                                                        if let Ok(pb) = dfs.get(&info.proof_cid).await {
                                                            if let Ok(bundle) =
                                                                bincode::deserialize::<StateProofBundle>(&pb)
                                                            {
                                                                if bundle.prev_state_root == info.prev_state_root
                                                                    && bundle.new_state_root
                                                                        == info.expected_state_root
                                                                    && bundle.lsu_hash == info.lsu_hash
                                                                    && verify_state_transition_bundle(
                                                                        &lsu, &bundle,
                                                                    )
                                                                    && verify_state_root_finality_for_apply(
                                                                        store.as_ref(),
                                                                        dfs,
                                                                        &lsu,
                                                                        info.cycle,
                                                                        bundle.new_state_root,
                                                                        &info.state_root_cid,
                                                                        require_state_root_finality_effective,
                                                                    )
                                                                    .await
                                                                {
                                                                    ok = apply_lsu_to_storage_without_root_check(
                                                                        store.as_ref(),
                                                                        &lsu,
                                                                        bundle.new_state_root,
                                                                    )
                                                                    .await
                                                                    .unwrap_or(false);
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
                                            }
                                            if ok {
                                                if let Some(dfs) = &dfs {
                                                    if !verify_lsu_finality_for_apply(
                                                        store.as_ref(),
                                                        dfs,
                                                        &lsu,
                                                        info.cycle,
                                                        &info.finality_cid,
                                                        require_lsu_finality_effective,
                                                    )
                                                    .await
                                                    {
                                                        ok = false;
                                                    }
                                                } else if require_lsu_finality_effective {
                                                    warn!(
                                                        "CATALYST_REQUIRE_LSU_FINALITY set but DFS unavailable (FileResponse path) cycle={}",
                                                        info.cycle
                                                    );
                                                    ok = false;
                                                }
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
            self.network_tasks.push(advance_handle);
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
            let finality_buckets_consensus = finality_buckets.clone();
            let state_root_buckets_consensus = state_root_buckets.clone();
            let validator_signing_key_consensus = validator_signing_key.clone();
            let tx_batches_consensus = tx_batches.clone();
            let tx_batch_commits_consensus = tx_batch_commits.clone();
            let observed_head_consensus = observed_head_shared.clone();
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
                let now_ms = crate::consensus_limits::wall_now_ms();
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

                    // A stable cycle number based on wall-clock epoch time (honors `CATALYST_TEST_WALL_OFFSET_MS`).
                    let cycle = crate::consensus_limits::wall_now_ms() / cycle_ms;

                    // Depth-gate (safety, root-cause fix): never mint a fresh wall-clock cycle while our
                    // applied head is behind the network head we have already observed over gossip.
                    //
                    // Cycle numbers are wall-clock-derived and the chain links by `prev_root`. If a node
                    // that has fallen behind produces the *current* wall-clock cycle, it chains a fresh LSU
                    // onto its stale applied root and forks itself off the canonical chain (the recurring
                    // freeze). We therefore defer production whenever the observed network head is ahead of
                    // our applied head, and let the concurrent catch-up/backfill path advance us first.
                    //
                    // This is skip-safe: cycle numbers may legitimately have gaps (a wall-clock slot with no
                    // quorum produces nothing), so we compare against the *observed* head rather than doing
                    // wall-clock arithmetic. When we are caught up (`observed_head <= applied`) we produce
                    // normally, even across skipped cycles.
                    // Depth-gate: never mint a fresh wall-clock cycle while our applied head is behind the
                    // network head observed over gossip. Producing on a stale applied root forks us off
                    // canon. We DEFER (never force-produce): a behind node stays canonical and converges
                    // via the concurrent forward catch-up (`try_advance_applied_head_from_storage`) once the
                    // >=1-behind range backfill fetches the missing certified LSUs. We deliberately do NOT
                    // release the gate to "preserve liveness" by producing — that just mints onto stale
                    // state and silently forks the node (observed regression). Worst case while a peer is
                    // unreachable is a frozen-but-intact node (operator-recoverable), never a corrupt fork;
                    // and with any healthy quorum the rest of the fleet keeps advancing meanwhile.
                    if let Some(store) = &storage {
                        let applied = local_applied_cycle(store.as_ref()).await;
                        let observed_head =
                            observed_head_consensus.load(std::sync::atomic::Ordering::Relaxed);
                        if should_defer_production_when_behind(applied, observed_head) {
                            info!(
                                "Deferring consensus cycle {}: applied head {} is behind observed network head {}; catching up before producing",
                                cycle, applied, observed_head
                            );
                            catalyst_utils::increment_counter!("consensus_cycle_deferred_behind_total", 1);
                            continue;
                        }
                    }

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
                    // Simple majority for logging / operator intuition; BFT vote quorum on LSUs
                    // is enforced separately via `lsu_has_bft_vote_quorum` (⌈2n/3⌉ distinct voters).
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

                    // Deterministic, per-cycle-ROTATING batch leader: all producers must execute
                    // Construction on the same protocol tx list. Followers wait (WAN budget), resync
                    // over P2P, and may recover from persisted per-cycle tx ids — they never silently
                    // use an empty list unless `TxBatchControl::Commit` advertises tx_count=0.
                    // Rotation (vs fixed first) is the liveness fix: one stuck producer no longer
                    // halts every cycle (see `rotating_batch_leader`).
                    let leader = rotating_batch_leader(&selected, cycle).unwrap_or_else(|| producer_id.clone());
                    let transactions = if producer_id == leader {
                        let candidate_txs = {
                            let mut mp = mempool.write().await;
                            // Snapshot protocol transactions only; legacy entries are ignored for Construction.
                            mp.snapshot_protocol_txs(max_entries_per_cycle)
                        };
                        let txs = if let Some(store) = &storage {
                            validate_and_select_protocol_txs_for_construction(
                                store.as_ref(),
                                cycle,
                                candidate_txs,
                                max_entries_per_cycle,
                            )
                            .await
                        } else {
                            Vec::new()
                        };

                        // Persist per-cycle txids + mark Selected (so followers can recover if gossip drops).
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
                        if let Ok(batch) = ProtocolTxBatch::new(cycle, txs.clone()) {
                            let count = batch.txs.len() as u32;
                            tx_batch_commits_consensus
                                .write()
                                .await
                                .insert(cycle, (batch.batch_hash, count));
                            if let Some(store) = &storage {
                                persist_tx_batch_commit_metadata(
                                    store.as_ref(),
                                    cycle,
                                    batch.batch_hash,
                                    count,
                                )
                                .await;
                            }
                            let commit = TxBatchControl::Commit {
                                cycle,
                                batch_hash: batch.batch_hash,
                                tx_count: count,
                            };
                            if let Ok(ce) =
                                MessageEnvelope::from_message(&commit, "tx_batch_commit".to_string(), None)
                            {
                                let _ = network.broadcast_envelope(&ce).await;
                            }
                            if let Ok(env) =
                                MessageEnvelope::from_message(&batch, "txbatch".to_string(), None)
                            {
                                // Gossipsub is best-effort; rebroadcast to improve WAN delivery.
                                for _ in 0..8 {
                                    let _ = network.broadcast_envelope(&env).await;
                                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                                }
                            }
                        }
                        entries
                    } else {
                        let entries_opt = wait_for_tx_construction_entries(
                            cycle,
                            cycle_ms,
                            &producer_id,
                            &leader,
                            false,
                            max_entries_per_cycle,
                            tx_batches_consensus.as_ref(),
                            tx_batch_commits_consensus.as_ref(),
                            storage.as_ref().map(|s| s.as_ref()),
                            Some(network.as_ref() as &dyn crate::tx_batch_p2p::TxBatchNetwork),
                        )
                        .await;
                        let Some(entries) = entries_opt else {
                            tracing::error!(
                                target: "catalyst.consensus.tx_batch",
                                "Skipping consensus cycle={}: canonical tx batch unavailable (see TX_BATCH_MISS_FATAL)",
                                cycle
                            );
                            continue;
                        };
                        info!(
                            "Cycle {} tx batch follower={} leader={} entries={}",
                            cycle,
                            producer_id,
                            leader,
                            entries.len()
                        );
                        let _ = tx_batches_consensus.write().await.remove(&cycle);
                        entries
                    };

                    match consensus.start_cycle(cycle, selected, transactions).await {
                        Ok(Some(update)) => {
                            // Refuse to commit a locally-produced LSU on top of local state whose
                            // reconcile has circuit-broken (repeated, unrecoverable prev_root
                            // mismatches against gossiped peer LSUs). Applying anyway compounds
                            // the divergence instead of healing it: the four-phase round above
                            // (start_cycle) can take many seconds (phase timeouts), long enough
                            // for the circuit to trip on *this exact* applied head in the
                            // meantime -- caught live via the consensus-follower-batch-drop-e2e
                            // (§7.2) harness, where a node cut off from P2P kept self-producing
                            // and applying new cycles atop its own already-diverged state even
                            // after reconcile gave up, reproducing the "recipe agreed, result
                            // diverges" fork class (docs/consensus-reliability-review-2026-07.md)
                            // via self-production instead of the peer-apply path. Same
                            // "never release the gate to preserve liveness" principle as
                            // should_defer_production_when_behind.
                            if let Some(store) = &storage {
                                if let Some(applied_before) = should_block_self_produced_apply(store.as_ref()).await {
                                    warn!(
                                        "Refusing to apply self-produced LSU for cycle {}: reconcile circuit breaker is open for applied head {} (divergent/unrecoverable local state); needs operator action",
                                        cycle, applied_before
                                    );
                                    catalyst_utils::increment_counter!(
                                        "consensus_self_produced_apply_blocked_circuit_broken_total",
                                        1
                                    );
                                    continue;
                                }
                            }

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
                                            let finality_cid_for_gossip = if let Some(store) = &storage {
                                                meta_string(
                                                    store.as_ref(),
                                                    &format!("consensus:lsu_finality_cid:{}", cycle),
                                                )
                                                .await
                                                .unwrap_or_default()
                                            } else {
                                                String::new()
                                            };
                                            let state_root_cid_for_gossip = if let Some(store) = &storage {
                                                meta_string(
                                                    store.as_ref(),
                                                    &format!("consensus:lsu_state_root_cid:{}", cycle),
                                                )
                                                .await
                                                .unwrap_or_default()
                                            } else {
                                                String::new()
                                            };
                                            let msg = LsuCidGossip {
                                                cycle,
                                                lsu_hash,
                                                cid,
                                                prev_state_root: prev_state_root_for_msg,
                                                state_root,
                                                proof_cid: proof_cid_for_msg,
                                                finality_cid: finality_cid_for_gossip,
                                                state_root_cid: state_root_cid_for_gossip,
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

                                            // ADR 0001: sign and gossip finality attestation; merge quorum certificate.
                                            if let (Some(store), Some(sk)) =
                                                (storage.as_ref(), validator_signing_key_consensus.as_ref())
                                            {
                                                if update.vote_list.iter().any(|v| v == &producer_id)
                                                    && catalyst_consensus::committee_hash_ordered_producer_list(
                                                        &update.producer_list,
                                                    )
                                                    .is_some()
                                                {
                                                    let chain_id = load_chain_id_u64(store.as_ref()).await;
                                                    let genesis_hash = load_genesis_hash_32(store.as_ref()).await;
                                                    if let Ok(lsu_hash) = catalyst_consensus::types::hash_data(&update) {
                                                        let ch = catalyst_consensus::committee_hash_ordered_producer_list(
                                                            &update.producer_list,
                                                        )
                                                        .unwrap();
                                                        let vh = catalyst_consensus::hash_producer_list_merkle(&update.vote_list);
                                                        let h_cert = catalyst_consensus::h_cert_v1(
                                                            chain_id,
                                                            &genesis_hash,
                                                            cycle,
                                                            &lsu_hash,
                                                            &ch,
                                                            &vh,
                                                        );
                                                        let mut rng = rand::rngs::OsRng;
                                                        let scheme = catalyst_crypto::signatures::SignatureScheme::new();
                                                        if let Ok(sig) = scheme.sign(&mut rng, sk, &h_cert) {
                                                            let attest = LsuFinalityAttestationMsg {
                                                                chain_id,
                                                                genesis_hash,
                                                                cycle,
                                                                lsu_hash,
                                                                committee_hash: ch,
                                                                vote_list_hash: vh,
                                                                producer_id: producer_id.clone(),
                                                                signature: sig.to_bytes().to_vec(),
                                                            };
                                                            ingest_finality_attestation(
                                                                store.as_ref(),
                                                                dfs,
                                                                &network,
                                                                &finality_buckets_consensus,
                                                                &attest,
                                                                &producer_id,
                                                            )
                                                            .await;
                                                            if let Ok(env) =
                                                                MessageEnvelope::from_message(&attest, "fin_att".to_string(), None)
                                                            {
                                                                let _ = network.broadcast_envelope(&env).await;
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            // ADR 0002: sign and gossip this validator's independently-computed
                                            // state_root attestation; merge quorum certificate. Every producer in
                                            // producer_list attests (not just the leader), matching ADR 0001's model —
                                            // this is what turns `state_root` from one peer's claim into a BFT fact.
                                            if let (Some(store), Some(sk)) =
                                                (storage.as_ref(), validator_signing_key_consensus.as_ref())
                                            {
                                                if update.producer_list.iter().any(|v| v == &producer_id) {
                                                    let chain_id = load_chain_id_u64(store.as_ref()).await;
                                                    let genesis_hash = load_genesis_hash_32(store.as_ref()).await;
                                                    if let (Ok(lsu_hash), Some(ch)) = (
                                                        catalyst_consensus::types::hash_data(&update),
                                                        catalyst_consensus::committee_hash_ordered_producer_list(
                                                            &update.producer_list,
                                                        ),
                                                    ) {
                                                        let h_root = catalyst_consensus::h_root_v1(
                                                            chain_id,
                                                            &genesis_hash,
                                                            cycle,
                                                            &lsu_hash,
                                                            &ch,
                                                            &state_root,
                                                        );
                                                        let mut rng = rand::rngs::OsRng;
                                                        let scheme = catalyst_crypto::signatures::SignatureScheme::new();
                                                        if let Ok(sig) = scheme.sign(&mut rng, sk, &h_root) {
                                                            let attest = StateRootAttestationMsg {
                                                                chain_id,
                                                                genesis_hash,
                                                                cycle,
                                                                lsu_hash,
                                                                committee_hash: ch,
                                                                state_root,
                                                                producer_id: producer_id.clone(),
                                                                signature: sig.to_bytes().to_vec(),
                                                            };
                                                            ingest_state_root_attestation(
                                                                store.as_ref(),
                                                                dfs,
                                                                &network,
                                                                &state_root_buckets_consensus,
                                                                &attest,
                                                                &producer_id,
                                                            )
                                                            .await;
                                                            if let Ok(env) = MessageEnvelope::from_message(
                                                                &attest,
                                                                "root_att".to_string(),
                                                                None,
                                                            ) {
                                                                let _ = network.broadcast_envelope(&env).await;
                                                            }
                                                        }
                                                    }
                                                }
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

#[cfg(test)]
mod bft_quorum_tests {
    use crate::fork_choice::{bft_vote_threshold, lsu_has_bft_vote_quorum};
    use catalyst_consensus::types::{
        CompensationEntry, LedgerStateUpdate, PartialLedgerStateUpdate, TransactionEntry,
    };

    #[test]
    fn bft_threshold_matches_ceil_two_thirds() {
        assert_eq!(bft_vote_threshold(1), 1);
        assert_eq!(bft_vote_threshold(2), 2);
        assert_eq!(bft_vote_threshold(3), 2);
        assert_eq!(bft_vote_threshold(4), 3);
        assert_eq!(bft_vote_threshold(5), 4);
        assert_eq!(bft_vote_threshold(6), 4);
        assert_eq!(bft_vote_threshold(7), 5);
    }

    #[test]
    fn depth_gate_defers_only_when_behind_observed_head() {
        use super::should_defer_production_when_behind;
        // True fresh genesis start (no applied head, no peer gossip observed yet either):
        // never defer, we must start the chain.
        assert!(!should_defer_production_when_behind(0, 0));
        // Fresh-restart node (wiped data, applied=0) that has already heard gossip from a live
        // network (observed_head > 0): must defer, not produce from genesis state, or it forks
        // itself off the canonical chain. Deliberately NOT gated on `applied != 0` (see
        // `should_defer_production_when_behind` doc comment).
        assert!(should_defer_production_when_behind(0, 89_043_100));
        // Caught up (observed <= applied): produce, even across skipped cycle numbers.
        assert!(!should_defer_production_when_behind(89_043_100, 89_043_100));
        assert!(!should_defer_production_when_behind(89_043_100, 89_043_090));
        // Behind the observed network head: defer and catch up before minting onto stale state.
        // We never force-produce while behind (producing on stale state forks the node), so a
        // lagging node defers indefinitely and relies on the >=1-behind forward catch-up to converge.
        assert!(should_defer_production_when_behind(89_043_100, 89_043_101));
        assert!(should_defer_production_when_behind(89_043_100, 89_045_827));
    }

    /// Regression test for the asymmetric-reset fork (docs/consensus-reliability-review-2026-07.md):
    /// a validator must not abandon a cycle a peer has positively confirmed having, on the same
    /// timer as a cycle nobody has. Confirmed-reachable data gets a longer budget before giving up.
    #[test]
    fn stale_observed_head_reset_gives_confirmed_reachable_cycles_more_time() {
        use super::should_reset_stale_observed_head;
        let budget = 60_000u64;
        // Unconfirmed: reset exactly at the normal budget boundary.
        assert!(!should_reset_stale_observed_head(budget, budget, false));
        assert!(should_reset_stale_observed_head(budget + 1, budget, false));
        // Confirmed reachable: must NOT reset at the normal budget -- this is exactly the case
        // that caused a live silent fork (one validator gave up while a peer legitimately had the
        // cycle it abandoned).
        assert!(!should_reset_stale_observed_head(budget + 1, budget, true));
        assert!(!should_reset_stale_observed_head(budget * 4, budget, true));
        // Still bounded, not literally forever: eventually resets even when confirmed, in case the
        // fetch pipeline itself is broken.
        assert!(should_reset_stale_observed_head(budget * 5 + 1, budget, true));
    }

    fn empty_lsu(
        cycle: u64,
        producers: Vec<String>,
        votes: Vec<String>,
    ) -> LedgerStateUpdate {
        LedgerStateUpdate {
            partial_update: PartialLedgerStateUpdate {
                transaction_entries: vec![TransactionEntry {
                    public_key: [0u8; 32],
                    amount: 0,
                    signature: vec![0u8; 1],
                }],
                transaction_signatures_hash: [0u8; 32],
                total_fees: 0,
                timestamp: 0,
            },
            compensation_entries: vec![CompensationEntry {
                producer_id: producers.first().cloned().unwrap_or_default(),
                public_key: [0u8; 32],
                amount: 0,
            }],
            cycle_number: cycle,
            producer_list: producers,
            vote_list: votes,
        }
    }

    #[test]
    fn quorum_requires_distinct_valid_votes() {
        let p = vec!["a".into(), "b".into(), "c".into()];
        // n=3 ⇒ threshold 2 distinct votes from producer_list.
        assert!(!lsu_has_bft_vote_quorum(&empty_lsu(1, p.clone(), vec!["a".into()])));
        assert!(lsu_has_bft_vote_quorum(&empty_lsu(1, p.clone(), vec!["a".into(), "b".into()])));
        assert!(lsu_has_bft_vote_quorum(&empty_lsu(
            1,
            p.clone(),
            vec!["a".into(), "b".into(), "c".into()]
        )));
        assert!(!lsu_has_bft_vote_quorum(&empty_lsu(
            1,
            p.clone(),
            vec!["a".into(), "b".into(), "x".into()]
        )));
        assert!(!lsu_has_bft_vote_quorum(&empty_lsu(1, p, vec!["a".into(), "a".into(), "a".into()])));
    }
}

/// Checklist §7.2: follower **stall** (no silent empty construction) when `Commit` promises txs
/// but the canonical `ProtocolTxBatch` never arrives — plus agreed-empty and leader-miss tails.
#[cfg(test)]
mod tx_batch_follower_wait_tests {
    use super::wait_for_tx_construction_entries;
    use catalyst_consensus::types::TransactionEntry;
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    /// Tiny `cycle_ms` with `cycle == 0` makes wall-clock epoch ms dominate cycle boundaries so
    /// wait loops exit immediately (production uses large `cycle` aligned with `now / cycle_ms`).
    const TEST_CYCLE_MS: u64 = 100;

    #[tokio::test]
    async fn follower_pinned_nonzero_commit_without_batch_returns_none() {
        let tx_batches = RwLock::new(HashMap::new());
        let commit_ram = RwLock::new(HashMap::from([(0u64, ([9u8; 32], 2u32))]));
        let out = wait_for_tx_construction_entries(
            0,
            TEST_CYCLE_MS,
            "follower_a",
            "leader_b",
            false,
            4096,
            &tx_batches,
            &commit_ram,
            None,
            None,
        )
        .await;
        assert_eq!(out, None, "must not apply ambiguous empty construction");
    }

    #[tokio::test]
    async fn follower_commit_tx_count_zero_returns_empty_vec() {
        let tx_batches = RwLock::new(HashMap::new());
        let commit_ram = RwLock::new(HashMap::from([(0u64, ([0u8; 32], 0u32))]));
        let out = wait_for_tx_construction_entries(
            0,
            TEST_CYCLE_MS,
            "follower_a",
            "leader_b",
            false,
            4096,
            &tx_batches,
            &commit_ram,
            None,
            None,
        )
        .await;
        assert_eq!(out, Some(Vec::new()));
    }

    #[tokio::test]
    async fn leader_miss_returns_none_when_commit_nonzero_but_batch_missing() {
        let tx_batches = RwLock::new(HashMap::new());
        let commit_ram = RwLock::new(HashMap::from([(0u64, ([8u8; 32], 4u32))]));
        let out = wait_for_tx_construction_entries(
            0,
            TEST_CYCLE_MS,
            "leader_b",
            "leader_b",
            true,
            4096,
            &tx_batches,
            &commit_ram,
            None,
            None,
        )
        .await;
        assert_eq!(out, None);
    }

    #[tokio::test]
    async fn ram_batch_short_circuits_wait() {
        let e = TransactionEntry {
            public_key: [2u8; 32],
            amount: 1,
            signature: vec![1u8; 64],
        };
        let tx_batches = RwLock::new(HashMap::from([(0u64, vec![e.clone()])]));
        let commit_ram = RwLock::new(HashMap::new());
        let out = wait_for_tx_construction_entries(
            0,
            TEST_CYCLE_MS,
            "follower_a",
            "leader_b",
            false,
            4096,
            &tx_batches,
            &commit_ram,
            None,
            None,
        )
        .await;
        assert_eq!(out, Some(vec![e]));
    }

    /// §7.2: when the canonical batch appears **before** the soft budget elapses, the follower
    /// obtains the same entries as if the batch had been present immediately (no stall).
    #[tokio::test]
    async fn follower_batch_arrives_mid_wait_returns_entries() {
        use catalyst_utils::utils::current_timestamp_ms;

        let cycle_ms = 100u64;
        let now_ms = current_timestamp_ms();
        let w = now_ms / cycle_ms;
        let cycle = w.saturating_add(50);

        let e = TransactionEntry {
            public_key: [3u8; 32],
            amount: 1,
            signature: vec![2u8; 64],
        };

        let tx_batches = std::sync::Arc::new(RwLock::new(HashMap::new()));
        let commit_ram = RwLock::new(HashMap::from([(cycle, ([9u8; 32], 2u32))]));
        let bc = std::sync::Arc::clone(&tx_batches);
        let ec = e.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            bc.write().await.insert(cycle, vec![ec]);
        });

        let out = wait_for_tx_construction_entries(
            cycle,
            cycle_ms,
            "follower_a",
            "leader_b",
            false,
            4096,
            tx_batches.as_ref(),
            &commit_ram,
            None,
            None,
        )
        .await;
        assert_eq!(out, Some(vec![e]));
    }

    /// Checklist §7.2 (in-process): two followers with the same pinned commit + batch agree;
    /// a third follower with commit but no batch stalls (`None`) instead of empty construction.
    #[tokio::test]
    async fn three_followers_one_missing_batch_stalls_while_peers_match() {
        use catalyst_consensus::convergence_test_support::make_transactions;

        let cycle = 0u64;
        let commit = ([7u8; 32], 5u32);
        let entries = make_transactions(1);
        let tx_batches_ab = RwLock::new(HashMap::from([(cycle, entries.clone())]));
        let tx_batches_c = RwLock::new(HashMap::new());
        let commit_ram = RwLock::new(HashMap::from([(cycle, commit)]));

        let out_a = wait_for_tx_construction_entries(
            cycle,
            TEST_CYCLE_MS,
            "follower_a",
            "leader",
            false,
            4096,
            &tx_batches_ab,
            &commit_ram,
            None,
            None,
        )
        .await;
        let out_b = wait_for_tx_construction_entries(
            cycle,
            TEST_CYCLE_MS,
            "follower_b",
            "leader",
            false,
            4096,
            &tx_batches_ab,
            &commit_ram,
            None,
            None,
        )
        .await;
        let out_c = wait_for_tx_construction_entries(
            cycle,
            TEST_CYCLE_MS,
            "follower_c",
            "leader",
            false,
            4096,
            &tx_batches_c,
            &commit_ram,
            None,
            None,
        )
        .await;

        assert_eq!(out_a, Some(entries.clone()));
        assert_eq!(out_b, Some(entries));
        assert_eq!(out_c, None);
    }
}

/// Checklist §7.1: identical **applied** `state_root` across isolated `StorageManager` instances when
/// each applies the same converged LSU from the in-process multi-validator harness.
#[cfg(test)]
mod storage_state_root_convergence_tests {
    use super::{apply_lsu_to_storage, ensure_chain_identity_and_genesis, local_applied_state_root};
    use crate::config::NodeConfig;
    use catalyst_consensus::convergence_test_support::{produce_single_cycle_converged_lsu, SimNetConfig};
    use catalyst_storage::{StorageConfig, StorageManager};
    use tempfile::TempDir;

    #[tokio::test]
    async fn three_validators_identical_applied_state_roots_per_cycle() {
        let n = 3usize;
        let net = SimNetConfig::default();
        let protocol = NodeConfig::default().protocol;

        let mut _keep_temp = Vec::new();
        let mut stores: Vec<StorageManager> = Vec::new();
        for _ in 0..n {
            let td = TempDir::new().unwrap();
            let mut sc = StorageConfig::default();
            sc.data_dir = td.path().to_path_buf();
            let store = StorageManager::new(sc).await.expect("storage open");
            ensure_chain_identity_and_genesis(&store, &protocol)
                .await
                .expect("genesis");
            _keep_temp.push(td);
            stores.push(store);
        }

        for cycle in 1u64..=8u64 {
            let lsu = produce_single_cycle_converged_lsu(n, cycle, net).await;
            let mut roots: Vec<[u8; 32]> = Vec::new();
            for store in &stores {
                let r = apply_lsu_to_storage(store, &lsu)
                    .await
                    .unwrap_or_else(|e| panic!("apply cycle {cycle}: {e}"));
                roots.push(r);
            }
            let r0 = roots[0];
            assert!(
                roots.iter().all(|r| *r == r0),
                "cycle {cycle}: stores diverged in committed state_root: {:?}",
                roots
            );
        }

        for store in &stores {
            let r_meta = local_applied_state_root(store).await;
            let r_cache = store.get_state_root().expect("state root after commit");
            assert_eq!(r_meta, r_cache, "metadata vs engine cached root");
        }
    }

    /// One validator deliberately skips a cycle apply (batch stall); others advance; straggler
    /// does not commit a divergent root, then catches up when applying the same converged LSU.
    #[tokio::test]
    async fn straggler_stall_then_catch_up_same_state_root() {
        let n = 3usize;
        let net = SimNetConfig::default();
        let protocol = NodeConfig::default().protocol;

        let mut _keep_temp = Vec::new();
        let mut stores: Vec<StorageManager> = Vec::new();
        for _ in 0..n {
            let td = TempDir::new().unwrap();
            let mut sc = StorageConfig::default();
            sc.data_dir = td.path().to_path_buf();
            let store = StorageManager::new(sc).await.expect("storage open");
            ensure_chain_identity_and_genesis(&store, &protocol)
                .await
                .expect("genesis");
            _keep_temp.push(td);
            stores.push(store);
        }

        let lsu1 = produce_single_cycle_converged_lsu(n, 1, net).await;
        for store in &stores {
            apply_lsu_to_storage(store, &lsu1)
                .await
                .expect("cycle 1 apply");
        }

        let lsu2 = produce_single_cycle_converged_lsu(n, 2, net).await;
        for store in stores.iter().take(2) {
            apply_lsu_to_storage(store, &lsu2)
                .await
                .expect("cycle 2 apply on non-straggler");
        }

        let root_straggler = local_applied_state_root(&stores[2]).await;
        let root_peer = local_applied_state_root(&stores[0]).await;
        assert_ne!(
            root_straggler, root_peer,
            "straggler should remain on cycle 1 root while peers advanced"
        );

        apply_lsu_to_storage(&stores[2], &lsu2)
            .await
            .expect("straggler catch-up apply");
        let r0 = local_applied_state_root(&stores[0]).await;
        let r2 = local_applied_state_root(&stores[2]).await;
        assert_eq!(r0, r2, "after catch-up, straggler matches peer head");
    }
}

/// Checklist §4.1 / §7: fork-choice integration (storage + DFS + apply/reorg gates).
#[cfg(test)]
mod fork_choice_integration_tests {
    use super::{
        apply_lsu_to_storage, apply_lsu_to_storage_without_root_check, ensure_chain_identity_and_genesis,
        fork_choice_apply_gate, get_balance_i64, hex_encode, load_stored_certified_hash_at_cycle,
        local_applied_cycle, local_applied_lsu_hash, local_applied_state_root, persist_lsu_history,
        reconcile_failure_state_for_test, record_reconcile_failure, set_balance_i64,
        should_block_self_produced_apply, try_reconcile_fork_from_quorum_lsu,
        try_reorg_stronger_lsu_at_applied_cycle, verify_state_root_finality_for_apply, ForkChoiceApplyGate,
    };
    use crate::config::NodeConfig;
    use crate::dfs_store::LocalContentStore;
    use catalyst_consensus::types::{
        hash_data, CompensationEntry, LedgerStateUpdate, PartialLedgerStateUpdate,
        TransactionEntry,
    };
    use catalyst_consensus::{
        h_cert_v1, h_root_v1, committee_hash_ordered_producer_list, verify_lsu_finality_certificate,
        LsuFinalityCertificateV1, LsuStateRootCertificateV1, ProducerFinalityVote, StateRootAttestation,
        FINALITY_CERT_STYLE_INDIVIDUAL,
    };
    use catalyst_crypto::{PrivateKey, SignatureScheme};
    use catalyst_storage::{StorageConfig, StorageManager};
    use catalyst_utils::{CatalystSerialize, StateManager};
    use rand::rngs::OsRng;
    use tempfile::TempDir;

    struct TestCommittee {
        ids: Vec<String>,
        keys: Vec<PrivateKey>,
    }

    impl TestCommittee {
        fn three() -> Self {
            let mut rng = OsRng;
            let keys: Vec<_> = (0..3).map(|_| PrivateKey::generate(&mut rng)).collect();
            let ids: Vec<_> = keys
                .iter()
                .map(|k| hex::encode(k.public_key().to_bytes()))
                .collect();
            Self { ids, keys }
        }
    }

    struct TestEnv {
        _store_dir: TempDir,
        _dfs_dir: TempDir,
        store: StorageManager,
        dfs: LocalContentStore,
        chain_id: u64,
        genesis_hash: [u8; 32],
    }

    async fn test_env() -> TestEnv {
        let store_dir = TempDir::new().unwrap();
        let dfs_dir = TempDir::new().unwrap();
        let mut sc = StorageConfig::default();
        sc.data_dir = store_dir.path().to_path_buf();
        let store = StorageManager::new(sc).await.expect("storage");
        let protocol = NodeConfig::default().protocol;
        ensure_chain_identity_and_genesis(&store, &protocol)
            .await
            .expect("genesis");
        let chain_id = protocol.chain_id;
        let genesis_hash = store
            .get_metadata("protocol:genesis_hash")
            .await
            .ok()
            .flatten()
            .and_then(|b| {
                if b.len() == 32 {
                    let mut g = [0u8; 32];
                    g.copy_from_slice(&b[..32]);
                    Some(g)
                } else {
                    None
                }
            })
            .unwrap_or([0u8; 32]);
        let dfs = LocalContentStore::new(dfs_dir.path().to_path_buf());
        TestEnv {
            _store_dir: store_dir,
            _dfs_dir: dfs_dir,
            store,
            dfs,
            chain_id,
            genesis_hash,
        }
    }

    fn lsu_with_tx_seed(committee: &TestCommittee, cycle: u64, tx_seed: u8) -> LedgerStateUpdate {
        LedgerStateUpdate {
            partial_update: PartialLedgerStateUpdate {
                transaction_entries: vec![TransactionEntry {
                    public_key: committee.keys[0].public_key().to_bytes(),
                    amount: tx_seed as i64,
                    signature: vec![tx_seed; 64],
                }],
                transaction_signatures_hash: [tx_seed; 32],
                total_fees: 0,
                timestamp: 1,
            },
            compensation_entries: vec![CompensationEntry {
                producer_id: committee.ids[0].clone(),
                public_key: committee.keys[0].public_key().to_bytes(),
                amount: 0,
            }],
            cycle_number: cycle,
            producer_list: committee.ids.clone(),
            vote_list: vec![committee.ids[0].clone(), committee.ids[1].clone()],
        }
    }

    async fn install_finality_cert(
        env: &TestEnv,
        lsu: &LedgerStateUpdate,
        committee: &TestCommittee,
    ) -> String {
        let lsu_hash = hash_data(lsu).expect("lsu hash");
        let committee_hash =
            committee_hash_ordered_producer_list(&lsu.producer_list).expect("committee hash");
        let vote_list_hash = catalyst_consensus::hash_producer_list_merkle(&lsu.vote_list);
        let h = h_cert_v1(
            env.chain_id,
            &env.genesis_hash,
            lsu.cycle_number,
            &lsu_hash,
            &committee_hash,
            &vote_list_hash,
        );
        let mut rng = OsRng;
        let scheme = SignatureScheme::new();
        let sig_a = scheme
            .sign(&mut rng, &committee.keys[0], &h)
            .expect("sign a")
            .to_bytes();
        let sig_b = scheme
            .sign(&mut rng, &committee.keys[1], &h)
            .expect("sign b")
            .to_bytes();
        let cert = LsuFinalityCertificateV1 {
            version: 1,
            style: FINALITY_CERT_STYLE_INDIVIDUAL,
            chain_id: env.chain_id,
            genesis_hash: env.genesis_hash,
            cycle: lsu.cycle_number,
            lsu_hash,
            committee_hash,
            vote_list_hash,
            votes: vec![
                ProducerFinalityVote {
                    producer_id: committee.ids[0].clone(),
                    signature: sig_a.to_vec(),
                },
                ProducerFinalityVote {
                    producer_id: committee.ids[1].clone(),
                    signature: sig_b.to_vec(),
                },
            ],
        };
        verify_lsu_finality_certificate(&cert, lsu).expect("cert verifies");
        let blob = bincode::serialize(&cert).expect("encode cert");
        let cid = env.dfs.put(blob).await.expect("dfs put");
        let _ = env
            .store
            .set_metadata(
                &format!("consensus:lsu_finality_cid:{}", lsu.cycle_number),
                cid.as_bytes(),
            )
            .await;
        cid
    }

    /// Builds and DFS-publishes an ADR 0002 `LsuStateRootCertificateV1` for `state_root`, signed by
    /// the first two committee members (matches `install_finality_cert`'s quorum-of-2-of-3 shape).
    /// Does NOT persist the `consensus:lsu_state_root_cid:{cycle}` metadata pointer — callers pass
    /// the returned CID directly as a hint, mirroring how a fresh gossip message would arrive.
    async fn install_state_root_cert(
        env: &TestEnv,
        lsu: &LedgerStateUpdate,
        committee: &TestCommittee,
        state_root: [u8; 32],
    ) -> String {
        let lsu_hash = hash_data(lsu).expect("lsu hash");
        let committee_hash =
            committee_hash_ordered_producer_list(&lsu.producer_list).expect("committee hash");
        let h = h_root_v1(
            env.chain_id,
            &env.genesis_hash,
            lsu.cycle_number,
            &lsu_hash,
            &committee_hash,
            &state_root,
        );
        let mut rng = OsRng;
        let scheme = SignatureScheme::new();
        let sig_a = scheme.sign(&mut rng, &committee.keys[0], &h).expect("sign a").to_bytes();
        let sig_b = scheme.sign(&mut rng, &committee.keys[1], &h).expect("sign b").to_bytes();
        let cert = LsuStateRootCertificateV1 {
            version: 1,
            chain_id: env.chain_id,
            genesis_hash: env.genesis_hash,
            cycle: lsu.cycle_number,
            lsu_hash,
            state_root,
            committee_hash,
            attestations: vec![
                StateRootAttestation { producer_id: committee.ids[0].clone(), signature: sig_a.to_vec() },
                StateRootAttestation { producer_id: committee.ids[1].clone(), signature: sig_b.to_vec() },
            ],
        };
        catalyst_consensus::verify_lsu_state_root_certificate(&cert, lsu).expect("cert verifies");
        let blob = bincode::serialize(&cert).expect("encode cert");
        env.dfs.put(blob).await.expect("dfs put")
    }

    async fn canonical_hash_at_cycle(store: &StorageManager, cycle: u64) -> Option<[u8; 32]> {
        let b = store
            .get_metadata(&format!("consensus:lsu_hash:{cycle}"))
            .await
            .ok()
            .flatten()?;
        if b.len() != 32 {
            return None;
        }
        let mut h = [0u8; 32];
        h.copy_from_slice(&b[..32]);
        Some(h)
    }

    fn pick_two_tx_seeds(committee: &TestCommittee, cycle: u64) -> (u8, u8) {
        let mut seeds: Vec<(u8, [u8; 32])> = (0u8..=255)
            .map(|s| {
                let lsu = lsu_with_tx_seed(committee, cycle, s);
                (s, hash_data(&lsu).expect("hash"))
            })
            .collect();
        seeds.sort_by_key(|(_, h)| *h);
        (seeds[0].0, seeds[seeds.len() - 1].0)
    }

    #[tokio::test]
    async fn persist_smaller_hash_wins_same_tier() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let (seed_lo, seed_hi) = pick_two_tx_seeds(&committee, 1);
        let lsu_hi = lsu_with_tx_seed(&committee, 1, seed_hi);
        let lsu_lo = lsu_with_tx_seed(&committee, 1, seed_lo);
        let hash_hi = hash_data(&lsu_hi).expect("hash hi");
        let hash_lo = hash_data(&lsu_lo).expect("hash lo");
        assert!(hash_lo < hash_hi, "test setup: pick distinct hashes");

        let bytes_hi = lsu_hi.serialize().expect("ser");
        let bytes_lo = lsu_lo.serialize().expect("ser");

        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            1,
            &bytes_hi,
            &hash_hi,
            "",
            &[0u8; 32],
            "",
            false,
        )
        .await;
        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            1,
            &bytes_lo,
            &hash_lo,
            "",
            &[0u8; 32],
            "",
            false,
        )
        .await;

        assert_eq!(canonical_hash_at_cycle(&env.store, 1).await, Some(hash_lo));
    }

    #[tokio::test]
    async fn certified_persist_supersedes_quorum_inferred() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let (seed_lo, seed_hi) = pick_two_tx_seeds(&committee, 1);
        let lsu_inferred = lsu_with_tx_seed(&committee, 1, seed_hi);
        let lsu_cert = lsu_with_tx_seed(&committee, 1, seed_lo);
        let hash_inferred = hash_data(&lsu_inferred).expect("hash");
        let hash_cert = hash_data(&lsu_cert).expect("hash");
        let cid = install_finality_cert(&env, &lsu_cert, &committee).await;

        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            1,
            &lsu_inferred.serialize().expect("ser"),
            &hash_inferred,
            "",
            &[0u8; 32],
            "",
            false,
        )
        .await;
        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            1,
            &lsu_cert.serialize().expect("ser"),
            &hash_cert,
            "cid",
            &[0u8; 32],
            &cid,
            true,
        )
        .await;

        assert_eq!(
            canonical_hash_at_cycle(&env.store, 1).await,
            Some(hash_cert)
        );
    }

    #[tokio::test]
    async fn apply_gate_rejects_non_certified_when_certified_only() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let lsu = lsu_with_tx_seed(&committee, 1, 7);
        let hash = hash_data(&lsu).expect("hash");

        let gate = fork_choice_apply_gate(
            &env.store,
            Some(&env.dfs),
            &lsu,
            1,
            hash,
            "",
            true,
        )
        .await;
        assert_eq!(gate, ForkChoiceApplyGate::RejectNonCertified);
    }

    #[tokio::test]
    async fn apply_gate_stalls_on_double_certified_production_policy() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let (seed_a, seed_b) = pick_two_tx_seeds(&committee, 1);
        assert_ne!(seed_a, seed_b);
        let lsu_a = lsu_with_tx_seed(&committee, 1, seed_a);
        let lsu_b = lsu_with_tx_seed(&committee, 1, seed_b);
        let hash_a = hash_data(&lsu_a).expect("hash a");
        let hash_b = hash_data(&lsu_b).expect("hash b");
        let cid_a = install_finality_cert(&env, &lsu_a, &committee).await;
        let cid_b = install_finality_cert(&env, &lsu_b, &committee).await;

        let g1 = fork_choice_apply_gate(
            &env.store,
            Some(&env.dfs),
            &lsu_a,
            1,
            hash_a,
            &cid_a,
            true,
        )
        .await;
        assert_eq!(g1, ForkChoiceApplyGate::Allow);
        assert_eq!(
            load_stored_certified_hash_at_cycle(&env.store, 1).await,
            Some(hash_a)
        );

        let g2 = fork_choice_apply_gate(
            &env.store,
            Some(&env.dfs),
            &lsu_b,
            1,
            hash_b,
            &cid_b,
            true,
        )
        .await;
        assert_eq!(g2, ForkChoiceApplyGate::StallCertifiedEquivocation);
    }

    #[tokio::test]
    async fn apply_gate_testnet_allows_second_cert_with_tiebreak_policy() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let (seed_lo, seed_hi) = pick_two_tx_seeds(&committee, 1);
        let lsu_lo = lsu_with_tx_seed(&committee, 1, seed_lo);
        let lsu_hi = lsu_with_tx_seed(&committee, 1, seed_hi);
        let hash_lo = hash_data(&lsu_lo).expect("hash lo");
        let hash_hi = hash_data(&lsu_hi).expect("hash hi");
        let cid_lo = install_finality_cert(&env, &lsu_lo, &committee).await;
        let cid_hi = install_finality_cert(&env, &lsu_hi, &committee).await;

        assert_eq!(
            fork_choice_apply_gate(
                &env.store,
                Some(&env.dfs),
                &lsu_hi,
                1,
                hash_hi,
                &cid_hi,
                false,
            )
            .await,
            ForkChoiceApplyGate::Allow
        );
        // Testnet policy: tie-break, not stall — second cert may proceed (then weaker check).
        let g2 = fork_choice_apply_gate(
            &env.store,
            Some(&env.dfs),
            &lsu_lo,
            1,
            hash_lo,
            &cid_lo,
            false,
        )
        .await;
        assert_eq!(g2, ForkChoiceApplyGate::Allow);
    }

    #[tokio::test]
    async fn reorg_replaces_applied_head_with_stronger_certified_lsu() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let (seed_lo, seed_hi) = pick_two_tx_seeds(&committee, 1);
        let lsu_c1 = lsu_with_tx_seed(&committee, 1, seed_hi);
        let lsu_inferred_c2 = lsu_with_tx_seed(&committee, 2, seed_hi);
        let mut lsu_cert_c2 = lsu_with_tx_seed(&committee, 2, seed_lo);
        lsu_cert_c2.cycle_number = 2;
        let hash_inferred_c2 = hash_data(&lsu_inferred_c2).expect("hash inferred c2");
        let hash_cert_c2 = hash_data(&lsu_cert_c2).expect("hash cert c2");
        let cid = install_finality_cert(&env, &lsu_cert_c2, &committee).await;

        let root_c1 = apply_lsu_to_storage(&env.store, &lsu_c1)
            .await
            .expect("apply cycle 1");

        let snap = "fork_choice_reorg_expect";
        env.store
            .create_snapshot(snap)
            .await
            .expect("snapshot create");
        let expected_post = apply_lsu_to_storage(&env.store, &lsu_cert_c2)
            .await
            .expect("expected certified apply");
        env.store
            .load_snapshot(snap)
            .await
            .expect("snapshot restore");
        let _ = env.store.delete_snapshot(snap).await;

        let root_inferred_c2 = apply_lsu_to_storage(&env.store, &lsu_inferred_c2)
            .await
            .expect("apply inferred cycle 2");
        assert_eq!(
            local_applied_lsu_hash(&env.store).await,
            Some(hash_inferred_c2)
        );

        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            1,
            &lsu_c1.serialize().expect("ser"),
            &hash_data(&lsu_c1).expect("h1"),
            "",
            &root_c1,
            "",
            false,
        )
        .await;
        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            2,
            &lsu_inferred_c2.serialize().expect("ser"),
            &hash_inferred_c2,
            "",
            &root_inferred_c2,
            "",
            false,
        )
        .await;
        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            2,
            &lsu_cert_c2.serialize().expect("ser"),
            &hash_cert_c2,
            "cid",
            &[0u8; 32],
            &cid,
            true,
        )
        .await;

        let reorged = try_reorg_stronger_lsu_at_applied_cycle(
            &env.store,
            &lsu_cert_c2,
            2,
            root_c1,
            expected_post,
            &hash_cert_c2,
            None,
            Some(&env.dfs),
            &cid,
            true,
        )
        .await
        .expect("reorg");

        assert!(
            reorged,
            "try_reorg should reconcile to stronger certified LSU at cycle 2"
        );
        assert_eq!(
            local_applied_lsu_hash(&env.store).await,
            Some(hash_cert_c2)
        );
        assert_eq!(local_applied_state_root(&env.store).await, expected_post);
        assert_ne!(expected_post, root_inferred_c2);
    }

    #[tokio::test]
    async fn reconcile_stronger_certified_over_inferred_prev_root_mismatch() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let (seed_lo, seed_hi) = pick_two_tx_seeds(&committee, 1);
        let lsu_c1 = lsu_with_tx_seed(&committee, 1, seed_hi);
        let lsu_inferred_c2 = lsu_with_tx_seed(&committee, 2, seed_hi);
        let mut lsu_cert_c2 = lsu_with_tx_seed(&committee, 2, seed_lo);
        lsu_cert_c2.cycle_number = 2;
        let hash_inferred_c2 = hash_data(&lsu_inferred_c2).expect("hash inferred c2");
        let hash_cert_c2 = hash_data(&lsu_cert_c2).expect("hash cert c2");
        let cid = install_finality_cert(&env, &lsu_cert_c2, &committee).await;

        let root_c1 = apply_lsu_to_storage(&env.store, &lsu_c1)
            .await
            .expect("apply cycle 1");

        let snap = "fork_choice_reconcile_expect";
        env.store
            .create_snapshot(snap)
            .await
            .expect("snapshot create");
        let expected_post = apply_lsu_to_storage(&env.store, &lsu_cert_c2)
            .await
            .expect("expected certified apply");
        env.store
            .load_snapshot(snap)
            .await
            .expect("snapshot restore");
        let _ = env.store.delete_snapshot(snap).await;

        let _ = apply_lsu_to_storage(&env.store, &lsu_inferred_c2)
            .await
            .expect("apply inferred cycle 2");

        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            1,
            &lsu_c1.serialize().expect("ser"),
            &hash_data(&lsu_c1).expect("h1"),
            "",
            &root_c1,
            "",
            false,
        )
        .await;
        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            2,
            &lsu_inferred_c2.serialize().expect("ser"),
            &hash_inferred_c2,
            "",
            &[0u8; 32],
            "",
            false,
        )
        .await;

        let reconciled = try_reconcile_fork_from_quorum_lsu(
            &env.store,
            &lsu_cert_c2,
            2,
            root_c1,
            expected_post,
            &hash_cert_c2,
            None,
            Some(&env.dfs),
            &cid,
            true,
        )
        .await
        .expect("reconcile");
        assert!(
            reconciled,
            "reconcile should replay cycle 1 and apply certified LSU at cycle 2"
        );
        assert_eq!(
            local_applied_lsu_hash(&env.store).await,
            Some(hash_cert_c2)
        );
    }

    /// Regression test for the traced TOCTOU incident (docs/consensus-reliability-review-2026-07.md):
    /// a gossip handler can invoke `try_reconcile_fork_from_quorum_lsu` with a caller-side
    /// local_prev/local_applied_now snapshot that went stale because a concurrent self-apply landed
    /// the exact same LSU in the meantime — i.e. this call is effectively a self-echo of what's
    /// already correctly applied. `fork_choice_reject_weaker_incoming` does not reject an
    /// equal-hash incoming LSU, so without an idempotency check this used to fall all the way into
    /// the purge/rebuild path for state that was already exactly correct, deterministically failing
    /// the same way on every re-gossip. Must instead no-op cheaply.
    #[tokio::test]
    async fn reconcile_is_idempotent_noop_for_self_echo_of_own_applied_lsu() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let lsu_c1 = lsu_with_tx_seed(&committee, 1, 5);
        let hash_c1 = hash_data(&lsu_c1).expect("hash");
        let root_c1 = apply_lsu_to_storage(&env.store, &lsu_c1)
            .await
            .expect("apply cycle 1");
        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            1,
            &lsu_c1.serialize().expect("ser"),
            &hash_c1,
            "",
            &root_c1,
            "",
            false,
        )
        .await;

        // Prove no purge happens: mark an unrelated account. purge_mutable_chain_state_
        // preserving_lsu_history unconditionally wipes the whole accounts column family; only
        // metadata keys are selectively preserved. If reconcile purges, this sentinel is gone
        // regardless of whether the final replayed root happens to match (replaying the same
        // single LSU from empty state is also deterministic, so root/balance equality on the
        // *tx* account alone would not prove "no purge occurred").
        let sentinel_pk = committee.keys[2].public_key().to_bytes();
        set_balance_i64(&env.store, &sentinel_pk, 4242)
            .await
            .expect("set sentinel balance");
        let snaps_before = env.store.list_snapshots().await.expect("list snapshots");

        let reconciled = try_reconcile_fork_from_quorum_lsu(
            &env.store,
            &lsu_c1,
            1,
            [0u8; 32],
            root_c1,
            &hash_c1,
            None,
            Some(&env.dfs),
            "",
            false,
        )
        .await
        .expect("reconcile");

        assert!(
            reconciled,
            "self-echo reconcile of an already-applied identical LSU must succeed"
        );
        assert_eq!(local_applied_cycle(&env.store).await, 1);
        assert_eq!(local_applied_lsu_hash(&env.store).await, Some(hash_c1));
        assert_eq!(local_applied_state_root(&env.store).await, root_c1);
        assert_eq!(
            get_balance_i64(&env.store, &sentinel_pk).await,
            4242,
            "no purge: unrelated account balance must survive"
        );
        assert_eq!(
            env.store.list_snapshots().await.expect("list snapshots"),
            snaps_before,
            "no quorum_reconcile_* snapshot created: idempotent path must never reach create_snapshot"
        );
    }

    /// Regression test for the transactional-atomicity gap: a concurrent, unrelated apply (standing
    /// in for a production self-apply on a separate task) must never be corrupted or lost by an
    /// in-flight `try_reconcile_fork_from_quorum_lsu` transaction's purge/revert, regardless of
    /// scheduling order. Before this fix, `lsu_apply_lock` only guarded the individual storage-write
    /// step inside `apply_lsu_to_storage`, not reconcile's whole snapshot-purge-replay-revert
    /// transaction, so a concurrent apply landing inside that window could be silently wiped by
    /// reconcile's purge and never restored by its revert-to-pre-transaction-snapshot (taken before
    /// the concurrent apply happened). Forces the destructive case-B purge path deterministically
    /// (a certified LSU always beats a non-certified one, regardless of hash tie-break luck) with a
    /// deliberately-wrong `expected_prev` so reconcile is guaranteed to fail its final check and
    /// revert — the concurrent apply's effect must survive regardless of interleaving.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_apply_survives_in_flight_reconcile_purge_and_revert() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let pk0 = committee.keys[0].public_key().to_bytes();

        let lsu_c1 = lsu_with_tx_seed(&committee, 1, 5);
        let hash_c1 = hash_data(&lsu_c1).expect("hash c1");
        let root_c1 = apply_lsu_to_storage(&env.store, &lsu_c1)
            .await
            .expect("apply cycle 1");
        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            1,
            &lsu_c1.serialize().expect("ser"),
            &hash_c1,
            "",
            &root_c1,
            "",
            false,
        )
        .await;

        // Concurrent, unrelated apply for the next cycle — stands in for a production self-apply
        // racing an in-flight reconcile transaction on a separate task.
        let lsu_c2 = lsu_with_tx_seed(&committee, 2, 7);
        let apply_fut = apply_lsu_to_storage(&env.store, &lsu_c2);

        // Reconcile targeting the ALREADY-applied cycle 1 with a different, certified LSU (so
        // fork_choice_reject_weaker_incoming reliably lets it through regardless of hash bytes) and
        // a deliberately-wrong expected_prev, forcing the destructive purge-then-rebuild (case B)
        // path and guaranteeing it fails the final root check and reverts.
        let lsu_c1_alt = lsu_with_tx_seed(&committee, 1, 9);
        let hash_c1_alt = hash_data(&lsu_c1_alt).expect("hash c1 alt");
        let cid_alt = install_finality_cert(&env, &lsu_c1_alt, &committee).await;
        let reconcile_fut = try_reconcile_fork_from_quorum_lsu(
            &env.store,
            &lsu_c1_alt,
            1,
            [0xFFu8; 32],
            [0u8; 32],
            &hash_c1_alt,
            None,
            Some(&env.dfs),
            &cid_alt,
            true,
        );

        let (apply_result, reconcile_result) = tokio::join!(apply_fut, reconcile_fut);
        apply_result.expect("concurrent cycle-2 apply must not error");
        reconcile_result.expect("reconcile must fail its internal check and revert cleanly, not hard-error");

        // Regardless of which critical section ran first, both effects must be present: cycle 2
        // fully applied, and cycle 1's original balance intact underneath it. Under the old race,
        // one of these could be silently lost depending on scheduling.
        assert_eq!(local_applied_cycle(&env.store).await, 2);
        assert_eq!(
            get_balance_i64(&env.store, &pk0).await,
            5 + 7,
            "concurrent reconcile purge/revert must never lose or duplicate a concurrently-applied cycle"
        );
    }

    /// Regression test for the circuit breaker: a genuinely unrecoverable reconcile (forced to
    /// fail every time via a deliberately-wrong `expected_prev`, same as the concurrency test
    /// above) must stop attempting the expensive purge/replay after
    /// `reconcile_circuit_break_threshold()` consecutive failures for the same `(cycle, lsu_hash)`,
    /// instead of retrying identically forever — the busy-loop failure mode observed in production.
    #[tokio::test]
    async fn reconcile_circuit_breaker_trips_after_threshold_and_skips_further_attempts() {
        let env = test_env().await;
        let committee = TestCommittee::three();

        let lsu_c1 = lsu_with_tx_seed(&committee, 1, 5);
        let hash_c1 = hash_data(&lsu_c1).expect("hash c1");
        let root_c1 = apply_lsu_to_storage(&env.store, &lsu_c1)
            .await
            .expect("apply cycle 1");
        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            1,
            &lsu_c1.serialize().expect("ser"),
            &hash_c1,
            "",
            &root_c1,
            "",
            false,
        )
        .await;

        // Same shape as concurrent_apply_survives_in_flight_reconcile_purge_and_revert: a
        // certified LSU at the already-applied cycle 1 with a deliberately-wrong expected_prev,
        // guaranteed to fail the final root check and revert on every attempt.
        let lsu_c1_alt = lsu_with_tx_seed(&committee, 1, 9);
        let hash_c1_alt = hash_data(&lsu_c1_alt).expect("hash c1 alt");
        let cid_alt = install_finality_cert(&env, &lsu_c1_alt, &committee).await;

        let threshold = crate::consensus_limits::reconcile_circuit_break_threshold();
        for attempt in 1..=threshold {
            let reconciled = try_reconcile_fork_from_quorum_lsu(
                &env.store,
                &lsu_c1_alt,
                1,
                [0xFFu8; 32],
                [0u8; 32],
                &hash_c1_alt,
                None,
                Some(&env.dfs),
                &cid_alt,
                true,
            )
            .await
            .expect("reconcile must fail cleanly, not hard-error");
            assert!(!reconciled, "deliberately-wrong expected_prev must never succeed");

            let (count, broken) = reconcile_failure_state_for_test(1)
                .await
                .expect("failure entry must exist after a failed attempt");
            assert_eq!(count, attempt, "consecutive_failures must track each attempt exactly");
            assert_eq!(
                broken,
                attempt >= threshold,
                "must trip exactly on reaching the threshold, not before"
            );
        }

        // Circuit is now open. The (threshold + 1)th attempt must be skipped entirely — assert via
        // list_snapshots() that it never reaches create_snapshot (i.e. never touches the expensive
        // purge/replay machinery at all).
        let snaps_before = env.store.list_snapshots().await.expect("list snapshots");
        let reconciled = try_reconcile_fork_from_quorum_lsu(
            &env.store,
            &lsu_c1_alt,
            1,
            [0xFFu8; 32],
            [0u8; 32],
            &hash_c1_alt,
            None,
            Some(&env.dfs),
            &cid_alt,
            true,
        )
        .await
        .expect("circuit-broken reconcile must return cleanly, not error");
        assert!(!reconciled);
        assert_eq!(
            env.store.list_snapshots().await.expect("list snapshots"),
            snaps_before,
            "circuit-broken attempt must never reach create_snapshot"
        );

        // A different local_cycle key is unaffected by this open circuit: apply cycle 2 for real
        // (advancing local_cycle to 2), then reconcile a self-echo of it (hits the item-1
        // idempotency no-op path) and confirm it succeeds cleanly rather than being caught by
        // local_cycle=1's tripped breaker.
        let lsu_c2 = lsu_with_tx_seed(&committee, 2, 7);
        let hash_c2 = hash_data(&lsu_c2).expect("hash c2");
        let root_c2 = apply_lsu_to_storage(&env.store, &lsu_c2)
            .await
            .expect("apply cycle 2");
        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            2,
            &lsu_c2.serialize().expect("ser"),
            &hash_c2,
            "",
            &root_c2,
            "",
            false,
        )
        .await;
        let reconciled = try_reconcile_fork_from_quorum_lsu(
            &env.store,
            &lsu_c2,
            2,
            root_c1,
            root_c2,
            &hash_c2,
            None,
            Some(&env.dfs),
            "",
            false,
        )
        .await
        .expect("reconcile for an unrelated (cycle, hash) key must not be affected by cycle 1's open circuit");
        assert!(reconciled, "self-echo of cycle 2 must hit the idempotent no-op path and succeed");
    }

    /// Regression test for a bug in the circuit breaker's first implementation, caught live on a
    /// running testnet during this session: a node stuck unable to bridge a genuinely-missing gap
    /// cycle gets re-invoked with a *different, ever-increasing* target `cycle` on every
    /// subsequent wall-clock tick (each newly-gossiped LSU is a fresh call), and
    /// `resolve_replay_start`'s "missing stored LSU, can't bridge" failure returns *before* a
    /// breaker keyed on the target `(cycle, hash)` would ever see repeats on the same key — so it
    /// never tripped, and the node retried every ~20s indefinitely. The fix keys the breaker on
    /// `local_cycle` (the node's own unchanging stuck position) instead, checked before
    /// `resolve_replay_start` runs. This test calls with a different target `cycle` every time,
    /// exactly reproducing the live failure shape, and asserts the breaker still trips.
    #[tokio::test]
    async fn reconcile_circuit_breaker_trips_on_unbridgeable_gap_despite_increasing_target_cycle() {
        let env = test_env().await;
        let committee = TestCommittee::three();

        // Use a distinctive, unlikely-to-collide base cycle for local_cycle: the tracker this
        // test exercises is keyed on a bare u64 in a process-global static, and cargo runs tests
        // in the same process concurrently -- a small number like 1 collides with
        // `reconcile_circuit_breaker_trips_after_threshold_and_skips_further_attempts` above,
        // which also gets stuck at local_cycle=1 (caught live: this test flaked exactly that way
        // the first time both tests ran together). Applying a single LSU with a large
        // `cycle_number` directly succeeds without replaying every intermediate cycle, since the
        // idempotence check is `cycle_number <= already_applied`, not "sequential".
        const BASE: u64 = 900_001;

        // BASE applied and recorded; BASE+1's LSU is never stored anywhere (no peer, no
        // checkpoint) -- a permanent, unbridgeable gap, matching "leader-miss skipped this cycle
        // network-wide" in the live incident.
        let lsu_base = lsu_with_tx_seed(&committee, BASE, 5);
        let hash_base = hash_data(&lsu_base).expect("hash base");
        let root_base = apply_lsu_to_storage(&env.store, &lsu_base)
            .await
            .expect("apply base cycle");
        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            BASE,
            &lsu_base.serialize().expect("ser"),
            &hash_base,
            "",
            &root_base,
            "",
            false,
        )
        .await;

        let threshold = crate::consensus_limits::reconcile_circuit_break_threshold();
        // Target cycle increases on every attempt, exactly like each new wall-clock tick's gossip
        // in production -- never the same (cycle, hash) key twice.
        for attempt in 1..=threshold {
            let target_cycle = BASE + 1 + attempt as u64;
            let lsu_target = lsu_with_tx_seed(&committee, target_cycle, 100 + attempt as u8);
            let hash_target = hash_data(&lsu_target).expect("hash target");
            let reconciled = try_reconcile_fork_from_quorum_lsu(
                &env.store,
                &lsu_target,
                target_cycle,
                [0u8; 32],
                [0u8; 32],
                &hash_target,
                None, // no P2P fetch available -- the gap can never be bridged
                Some(&env.dfs),
                "",
                false,
            )
            .await
            .expect("reconcile must fail cleanly on an unbridgeable gap, not hard-error");
            assert!(!reconciled, "BASE+1 is permanently missing; must never succeed");
            assert_eq!(
                local_applied_cycle(&env.store).await,
                BASE,
                "stuck node's local_cycle must never move"
            );

            let (count, broken) = reconcile_failure_state_for_test(BASE)
                .await
                .expect("failure entry must exist, keyed on local_cycle=BASE, after a failed attempt");
            assert_eq!(
                count, attempt,
                "consecutive_failures must accumulate across attempts despite each having a distinct target cycle/hash"
            );
            assert_eq!(broken, attempt >= threshold);
        }

        assert!(
            reconcile_failure_state_for_test(BASE).await.unwrap().1,
            "breaker must be tripped after `threshold` attempts, even though no target (cycle, hash) ever repeated"
        );
    }

    /// Regression test for a node that keeps self-producing/applying new cycles on top of its
    /// own already-diverged state even after reconcile has given up on it — caught live via the
    /// consensus-follower-batch-drop-e2e (§7.2) harness: a node cut off from P2P for a few
    /// cycles reconnected, its reconcile circuit-broke trying to heal the resulting mismatch, but
    /// nothing stopped it from producing and applying several more self-consistent-but-wrong
    /// cycles anyway, landing on a different `state_root` than its peers at the same cycle
    /// number. `should_block_self_produced_apply` is the guard that closes this: it must report
    /// the applied head as blocked once that head's reconcile circuit is open, and not before.
    #[tokio::test]
    async fn self_produced_apply_blocked_once_reconcile_circuit_broken_for_applied_head() {
        let env = test_env().await;
        let committee = TestCommittee::three();

        // Distinctive base cycle -- see the comment on the unbridgeable-gap test above for why
        // (process-global tracker shared across concurrently-run tests).
        const BASE: u64 = 900_501;

        let lsu_base = lsu_with_tx_seed(&committee, BASE, 5);
        apply_lsu_to_storage(&env.store, &lsu_base)
            .await
            .expect("apply base cycle");
        assert_eq!(local_applied_cycle(&env.store).await, BASE);

        assert_eq!(
            should_block_self_produced_apply(&env.store).await,
            None,
            "must not block before the circuit has tripped for this applied head"
        );

        let threshold = crate::consensus_limits::reconcile_circuit_break_threshold();
        for _ in 1..threshold {
            let tripped = record_reconcile_failure(BASE).await;
            assert!(!tripped, "must not report tripped before reaching threshold");
            assert_eq!(
                should_block_self_produced_apply(&env.store).await,
                None,
                "must not block while under threshold"
            );
        }
        let tripped = record_reconcile_failure(BASE).await;
        assert!(tripped, "must report tripped exactly on reaching threshold");

        assert_eq!(
            should_block_self_produced_apply(&env.store).await,
            Some(BASE),
            "must block self-produced apply once the circuit is open for the current applied head"
        );
    }

    /// Regression test for the "purge before checkpoint restore" / "purge from an arbitrary
    /// replay_start" bug: catching up more than one cycle behind (so the single-cycle L2
    /// fast-path in `try_reconcile_fork_from_quorum_lsu` doesn't apply) must replay forward
    /// onto the existing account state, never wipe it. A prior version of this function always
    /// purged mutable state and replayed from either literal cycle 1 or a checkpoint, which
    /// silently truncated every balance accumulated before the replay window.
    #[tokio::test]
    async fn reconcile_forward_catchup_beyond_one_cycle_preserves_prior_balances() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let pk0 = committee.keys[0].public_key().to_bytes();

        // Cycles 1 and 2 are applied normally, accumulating a non-trivial balance.
        let lsu_c1 = lsu_with_tx_seed(&committee, 1, 5);
        let root_c1 = apply_lsu_to_storage(&env.store, &lsu_c1)
            .await
            .expect("apply cycle 1");
        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            1,
            &lsu_c1.serialize().expect("ser"),
            &hash_data(&lsu_c1).expect("h1"),
            "",
            &root_c1,
            "",
            false,
        )
        .await;

        let lsu_c2 = lsu_with_tx_seed(&committee, 2, 7);
        let root_c2 = apply_lsu_to_storage(&env.store, &lsu_c2)
            .await
            .expect("apply cycle 2");
        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            2,
            &lsu_c2.serialize().expect("ser"),
            &hash_data(&lsu_c2).expect("h2"),
            "",
            &root_c2,
            "",
            false,
        )
        .await;

        let balance_before = get_balance_i64(&env.store, &pk0).await;
        assert_eq!(balance_before, 5 + 7, "sanity: balances accrue normally");

        // Cycle 3's LSU is already known/stored locally (e.g. from earlier gossip) but has not
        // yet been applied as our own head — we are still at applied cycle 2. Compute its
        // effect and cycle 4's (the reconcile target) via a scratch snapshot, then restore back
        // to the cycle-2 state so the test's starting point is genuinely "2 cycles behind".
        let lsu_c3 = lsu_with_tx_seed(&committee, 3, 11);
        let lsu_c4 = lsu_with_tx_seed(&committee, 4, 13);
        let scratch_snap = "scratch_c3_c4";
        env.store
            .create_snapshot(scratch_snap)
            .await
            .expect("scratch snapshot");
        let root_c3 = apply_lsu_to_storage(&env.store, &lsu_c3)
            .await
            .expect("compute cycle 3 root");
        let expected_post_c4 = apply_lsu_to_storage(&env.store, &lsu_c4)
            .await
            .expect("compute cycle 4 root");
        env.store
            .load_snapshot(scratch_snap)
            .await
            .expect("restore to cycle 2");
        let _ = env.store.delete_snapshot(scratch_snap).await;
        assert_eq!(
            local_applied_cycle(&env.store).await,
            2,
            "sanity: still 2 cycles behind after scratch restore"
        );

        persist_lsu_history(
            &env.store,
            Some(&env.dfs),
            3,
            &lsu_c3.serialize().expect("ser"),
            &hash_data(&lsu_c3).expect("h3"),
            "",
            &root_c3,
            "",
            false,
        )
        .await;

        let hash_c4 = hash_data(&lsu_c4).expect("h4");
        let cid = install_finality_cert(&env, &lsu_c4, &committee).await;

        let reconciled = try_reconcile_fork_from_quorum_lsu(
            &env.store,
            &lsu_c4,
            4,
            root_c3,
            expected_post_c4,
            &hash_c4,
            None,
            Some(&env.dfs),
            &cid,
            true,
        )
        .await
        .expect("reconcile");
        assert!(
            reconciled,
            "reconcile should replay stored cycle 3 forward onto the existing cycle-2 state, then apply cycle 4"
        );

        let balance_after = get_balance_i64(&env.store, &pk0).await;
        assert_eq!(
            balance_after,
            5 + 7 + 11 + 13,
            "forward catch-up must accumulate onto prior balances, not truncate them"
        );
        assert_eq!(local_applied_cycle(&env.store).await, 4);
    }

    /// Regression test for the "trust a peer's claimed state_root without verifying it" bug in
    /// `apply_lsu_to_storage_without_root_check`: a mismatched claim must be rejected (not
    /// silently cached as fact) and every mutation it made must be fully reverted, so the
    /// caller's existing fallback (`apply_lsu_with_root_check`) starts from a clean state.
    #[tokio::test]
    async fn trusted_apply_rejects_and_reverts_mismatched_claimed_root() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let pk0 = committee.keys[0].public_key().to_bytes();

        let lsu_c1 = lsu_with_tx_seed(&committee, 1, 5);
        let _root_c1 = apply_lsu_to_storage(&env.store, &lsu_c1)
            .await
            .expect("apply cycle 1");
        assert_eq!(get_balance_i64(&env.store, &pk0).await, 5);
        assert_eq!(local_applied_cycle(&env.store).await, 1);

        // Compute the TRUE root cycle 2 would produce, via a scratch snapshot, then restore so
        // the "trusted" call below starts from the same state a real caller would see.
        let lsu_c2 = lsu_with_tx_seed(&committee, 2, 7);
        let scratch_snap = "scratch_true_root_c2";
        env.store.create_snapshot(scratch_snap).await.expect("snap");
        let true_root_c2 = apply_lsu_to_storage(&env.store, &lsu_c2)
            .await
            .expect("compute true cycle 2 root");
        env.store.load_snapshot(scratch_snap).await.expect("restore");
        let _ = env.store.delete_snapshot(scratch_snap).await;

        // A deliberately wrong claimed root (flip the true root's first byte).
        let mut wrong_root = true_root_c2;
        wrong_root[0] ^= 0xFF;

        let ok = apply_lsu_to_storage_without_root_check(&env.store, &lsu_c2, wrong_root)
            .await
            .expect("no hard error, just a verified rejection");
        assert!(!ok, "mismatched claimed root must be rejected");
        assert_eq!(
            get_balance_i64(&env.store, &pk0).await,
            5,
            "rejected claim must leave no partial mutation behind"
        );
        assert_eq!(
            local_applied_cycle(&env.store).await,
            1,
            "rejected claim must not advance the applied head"
        );

        // The correct claimed root is accepted and applied.
        let ok = apply_lsu_to_storage_without_root_check(&env.store, &lsu_c2, true_root_c2)
            .await
            .expect("apply with correct root");
        assert!(ok, "correctly-claimed root must be accepted");
        assert_eq!(get_balance_i64(&env.store, &pk0).await, 5 + 7);
        assert_eq!(local_applied_cycle(&env.store).await, 2);
    }

    /// `StorageManager::load_snapshot` restores by writing back the snapshot's captured keys —
    /// it does not first delete keys that exist now but did not exist when the snapshot was
    /// taken (a separate, pre-existing gap: `clear_database` is a no-op). So a revert-on-mismatch
    /// only fully undoes a failed apply if every mutation it made overwrote an *existing* key;
    /// a mutation that *creates* a new key (e.g. this session's new unconditional
    /// `consensus:lsu:{cycle}` write) would survive a "clean" revert as a stray leftover. This
    /// test uses a cycle with NO prior history at all, so any survival is unambiguous.
    #[tokio::test]
    async fn trusted_apply_revert_does_not_leave_stray_lsu_history_on_rejection() {
        let env = test_env().await;
        let committee = TestCommittee::three();

        // No cycle has ever been applied or recorded here — a totally clean slate.
        assert_eq!(local_applied_cycle(&env.store).await, 0);
        assert!(
            env.store.get_metadata("consensus:lsu:1").await.ok().flatten().is_none(),
            "sanity: no history before the call"
        );

        let lsu_c1 = lsu_with_tx_seed(&committee, 1, 5);
        let bogus_root = [0xABu8; 32];

        let ok = apply_lsu_to_storage_without_root_check(&env.store, &lsu_c1, bogus_root)
            .await
            .expect("no hard error, just a verified rejection");
        assert!(!ok, "bogus claimed root must be rejected");

        assert_eq!(
            local_applied_cycle(&env.store).await,
            0,
            "rejected claim must not advance the applied head"
        );
        assert!(
            env.store.get_metadata("consensus:lsu:1").await.ok().flatten().is_none(),
            "rejected claim must not leave a stray consensus:lsu:{{cycle}} record behind"
        );
        assert!(
            env.store.get_metadata("consensus:lsu_hash:1").await.ok().flatten().is_none(),
            "rejected claim must not leave a stray consensus:lsu_hash:{{cycle}} record behind"
        );
    }

    /// ADR 0002 apply-path gate (`verify_state_root_finality_for_apply`): when
    /// `require_state_root_finality` is off (default during rollout), the trusted apply path must
    /// proceed even with no certificate present — this is the coordinated-upgrade tolerance every
    /// prior ADR 0001 field also needed.
    #[tokio::test]
    async fn state_root_finality_gate_passes_when_not_required_and_no_cert() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let lsu = lsu_with_tx_seed(&committee, 1, 5);
        let claimed_root = [7u8; 32];

        let ok = verify_state_root_finality_for_apply(
            &env.store,
            &env.dfs,
            &lsu,
            lsu.cycle_number,
            claimed_root,
            "",
            false,
        )
        .await;
        assert!(ok, "must not block the trusted apply path while the feature is off");
    }

    /// When `require_state_root_finality` is on, a missing certificate must block the trusted
    /// apply path rather than silently trusting the peer's claimed root (closes the gap the
    /// reliability review flagged: "nothing forces two nodes' pre-apply state to be provably
    /// identical" — a required cert is exactly that "something").
    #[tokio::test]
    async fn state_root_finality_gate_rejects_missing_cert_when_required() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let lsu = lsu_with_tx_seed(&committee, 1, 5);
        let claimed_root = [7u8; 32];

        let ok = verify_state_root_finality_for_apply(
            &env.store,
            &env.dfs,
            &lsu,
            lsu.cycle_number,
            claimed_root,
            "",
            true,
        )
        .await;
        assert!(!ok, "required finality with no certificate must reject");
    }

    /// A valid, BFT-quorum-signed `LsuStateRootCertificateV1` whose certified root matches the
    /// claimed root must pass the gate when required — the intended happy path.
    #[tokio::test]
    async fn state_root_finality_gate_accepts_valid_matching_cert_when_required() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let lsu = lsu_with_tx_seed(&committee, 1, 5);
        let state_root = [42u8; 32];
        let cid = install_state_root_cert(&env, &lsu, &committee, state_root).await;

        let ok = verify_state_root_finality_for_apply(
            &env.store,
            &env.dfs,
            &lsu,
            lsu.cycle_number,
            state_root,
            &cid,
            true,
        )
        .await;
        assert!(ok, "valid certificate matching the claimed root must be accepted");
    }

    /// A certificate that verifies on its own terms but certifies a *different* root than the one
    /// the caller is about to trust must still be rejected — otherwise a peer could present a
    /// genuine quorum certificate for one root while feeding the apply path a different claimed
    /// root, defeating the point of certifying `state_root` at all.
    #[tokio::test]
    async fn state_root_finality_gate_rejects_cert_for_different_root_than_claimed() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let lsu = lsu_with_tx_seed(&committee, 1, 5);
        let certified_root = [42u8; 32];
        let cid = install_state_root_cert(&env, &lsu, &committee, certified_root).await;

        let mut claimed_root = certified_root;
        claimed_root[0] ^= 0xFF;

        let ok = verify_state_root_finality_for_apply(
            &env.store,
            &env.dfs,
            &lsu,
            lsu.cycle_number,
            claimed_root,
            &cid,
            true,
        )
        .await;
        assert!(!ok, "certificate for a different root than the claimed one must be rejected");
    }

    /// Regression test for the "LSU history not persisted" bug: applying a cycle via the L2
    /// self-heal fast-path in `try_reconcile_fork_from_quorum_lsu` (chains directly onto the
    /// current head) must leave `consensus:lsu:{cycle}` / `consensus:lsu_hash:{cycle}` behind,
    /// just like a normal forward apply would. Previously this fast-path applied the LSU (via
    /// `apply_lsu_with_root_check`) and returned success without ever persisting the history
    /// record, leaving a hole that a later reconcile attempt would find missing and be unable to
    /// bridge — observed in production as "missing stored LSU for cycle N" for cycles that had,
    /// in fact, already been correctly applied.
    #[tokio::test]
    async fn l2_fastpath_reconcile_persists_lsu_history() {
        let env = test_env().await;
        let committee = TestCommittee::three();

        let lsu_c1 = lsu_with_tx_seed(&committee, 1, 3);
        let root_c1 = apply_lsu_to_storage(&env.store, &lsu_c1)
            .await
            .expect("apply cycle 1");
        assert_eq!(local_applied_cycle(&env.store).await, 1);

        // Cycle 2's LSU chains directly onto the current head (root_c1), so this reconcile call
        // will hit the L2 fast-path, not the replay machinery.
        let lsu_c2 = lsu_with_tx_seed(&committee, 2, 9);
        let hash_c2 = hash_data(&lsu_c2).expect("hash c2");
        let cid = install_finality_cert(&env, &lsu_c2, &committee).await;

        // Compute the expected post-cycle-2 root using a separate, isolated store (not a
        // snapshot/restore on `env.store`): `StorageManager::load_snapshot` does not actually
        // clear existing keys before restoring (a separate, pre-existing bug), so a scratch
        // apply on the same store would leave a stray `consensus:lsu:2` behind that confuses
        // this test's own fork-choice pre-check before ever reaching the L2 fast-path.
        let scratch_env = test_env().await;
        let _ = apply_lsu_to_storage(&scratch_env.store, &lsu_c1)
            .await
            .expect("scratch apply cycle 1");
        let expected_post_c2 = apply_lsu_to_storage(&scratch_env.store, &lsu_c2)
            .await
            .expect("compute expected cycle 2 root");

        let reconciled = try_reconcile_fork_from_quorum_lsu(
            &env.store,
            &lsu_c2,
            2,
            root_c1,
            expected_post_c2,
            &hash_c2,
            None,
            Some(&env.dfs),
            &cid,
            true,
        )
        .await
        .expect("reconcile");
        assert!(reconciled, "should hit the L2 fast-path and apply cycle 2");
        assert_eq!(local_applied_cycle(&env.store).await, 2);

        let stored_lsu = env
            .store
            .get_metadata("consensus:lsu:2")
            .await
            .ok()
            .flatten();
        assert!(
            stored_lsu.is_some(),
            "L2 fast-path must persist consensus:lsu:{{cycle}}, not just apply state"
        );
        assert_eq!(
            canonical_hash_at_cycle(&env.store, 2).await,
            Some(hash_c2),
            "L2 fast-path must persist consensus:lsu_hash:{{cycle}} matching what was applied"
        );
    }

    #[tokio::test]
    async fn apply_gate_allows_certified_quorum_lsu() {
        let env = test_env().await;
        let committee = TestCommittee::three();
        let lsu = lsu_with_tx_seed(&committee, 1, 42);
        let hash = hash_data(&lsu).expect("hash");
        let cid = install_finality_cert(&env, &lsu, &committee).await;

        let gate = fork_choice_apply_gate(
            &env.store,
            Some(&env.dfs),
            &lsu,
            1,
            hash,
            &cid,
            true,
        )
        .await;
        assert_eq!(gate, ForkChoiceApplyGate::Allow);
    }
}

#[cfg(test)]
mod worker_registry_load_tests {
    use super::{load_workers_from_state, worker_key_for_pubkey};
    use catalyst_storage::{StorageConfig, StorageManager};
    use catalyst_utils::StateManager;
    use tempfile::TempDir;

    #[tokio::test]
    async fn load_workers_empty_then_registered_pubkey() {
        let td = TempDir::new().unwrap();
        let mut sc = StorageConfig::default();
        sc.data_dir = td.path().to_path_buf();
        let store = StorageManager::new(sc).await.expect("storage open");
        assert!(load_workers_from_state(&store).is_empty());

        let pk = [11u8; 32];
        store
            .set_state(&worker_key_for_pubkey(&pk), vec![1u8])
            .await
            .expect("set workers marker");
        assert_eq!(load_workers_from_state(&store), vec![pk]);
    }
}

