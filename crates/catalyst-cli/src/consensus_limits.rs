//! Consensus-related environment parsing, replay limits, and small **pure** policy helpers
//! (unit-tested) used by the node consensus / tx-batch / LSU P2P ingress paths.

use crate::config::NodeConfig;
use crate::tx::ProtocolTxBatch;

const DEFAULT_MAX_REPLAY_CYCLES: u64 = 1_000_000;

/// Default follower wait for canonical `ProtocolTxBatch` when `CATALYST_TX_BATCH_WAIT_BUDGET_MS` is unset.
///
/// WAN-oriented: leave ~8s margin before the nominal cycle end, but never less than 12s.
pub fn tx_batch_follower_wait_budget_default(cycle_ms: u64) -> u64 {
    cycle_ms.saturating_sub(8_000).max(12_000)
}

pub fn tx_batch_follower_wait_budget_ms(cycle_ms: u64) -> u64 {
    std::env::var("CATALYST_TX_BATCH_WAIT_BUDGET_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| tx_batch_follower_wait_budget_default(cycle_ms))
}

/// Upper bound on quorum fork replay depth (`CATALYST_MAX_REPLAY_CYCLES`). `0` disables replay.
pub fn max_replay_cycles_from_env() -> u64 {
    match std::env::var("CATALYST_MAX_REPLAY_CYCLES") {
        Ok(s) => parse_max_replay_cycles_str(&s),
        Err(_) => DEFAULT_MAX_REPLAY_CYCLES,
    }
}

pub(crate) fn parse_max_replay_cycles_str(s: &str) -> u64 {
    match s.trim().parse::<u64>() {
        Ok(v) => v,
        Err(_) => DEFAULT_MAX_REPLAY_CYCLES,
    }
}

/// Whether P2P LSU apply paths require a verified `LsuFinalityCertificateV1` (ADR 0001).
///
/// Precedence: if `CATALYST_REQUIRE_LSU_FINALITY` is set and non-empty after trim, it wins
/// (`1`/`true`/`yes` vs `0`/`false`/`no`). Otherwise [`NodeConfig::consensus`] `require_lsu_finality` applies.
pub fn effective_require_lsu_finality(config: &NodeConfig) -> bool {
    match std::env::var("CATALYST_REQUIRE_LSU_FINALITY") {
        Ok(s) => {
            let t = s.trim();
            if t.is_empty() {
                return config.consensus.require_lsu_finality;
            }
            match t.to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" => true,
                "0" | "false" | "no" => false,
                _ => config.consensus.require_lsu_finality,
            }
        }
        Err(_) => config.consensus.require_lsu_finality,
    }
}

/// Wall time for consensus helpers (`catalyst_utils::utils::wall_now_ms`, honors test offset).
pub fn wall_now_ms() -> u64 {
    catalyst_utils::utils::wall_now_ms()
}

/// Ledger cycle index from wall time: `floor(now_ms / cycle_duration_ms)`.
///
/// Matches the node’s wall-clock cycle derivation when `cycle_duration_ms > 0`. If the
/// duration is `0`, returns `0` (caller should validate config so this never happens in production).
pub fn ledger_cycle_index(now_ms: u64, cycle_duration_ms: u64) -> u64 {
    if cycle_duration_ms == 0 {
        return 0;
    }
    now_ms / cycle_duration_ms
}

/// Signed difference between a gossip message’s `cycle` and the wall-derived ledger cycle index.
///
/// Positive means the message names a cycle **ahead** of what wall time implies (`floor(now_ms / cycle_ms)`).
pub fn ledger_cycle_skew_vs_wall(
    message_cycle: u64,
    wall_now_ms: u64,
    cycle_duration_ms: u64,
) -> i128 {
    if cycle_duration_ms == 0 {
        return 0;
    }
    (message_cycle as i128) - (ledger_cycle_index(wall_now_ms, cycle_duration_ms) as i128)
}

/// Max allowed distance (in whole cycles) between a tx-batch gossip `cycle` and the wall clock cycle index
/// for **ingress** acceptance (`CATALYST_P2P_TX_BATCH_MAX_CYCLE_SLACK`, default `3`, max `128`).
///
/// Used to drop obviously stale or far-future `ProtocolTxBatch` / legacy `TxBatch` / `TxBatchControl::Commit`
/// spam without affecting `ResyncRequest` (late peers still recover by cycle).
pub fn p2p_tx_batch_max_cycle_slack() -> u64 {
    const MAX: u64 = 128;
    const DEFAULT: u64 = 3;
    std::env::var("CATALYST_P2P_TX_BATCH_MAX_CYCLE_SLACK")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|&v| v <= MAX)
        .unwrap_or(DEFAULT)
}

/// Whether a tx-batch–related gossip payload for `message_cycle` should be accepted at ingress given wall time.
///
/// When `cycle_duration_ms == 0`, returns `true` (caller should not run production with zero duration).
pub fn p2p_tx_batch_cycle_allows_ingress(
    message_cycle: u64,
    wall_now_ms: u64,
    cycle_duration_ms: u64,
    slack: u64,
) -> bool {
    if cycle_duration_ms == 0 {
        return true;
    }
    let w = ledger_cycle_index(wall_now_ms, cycle_duration_ms);
    let lo = w.saturating_sub(slack);
    let hi = w.saturating_add(slack);
    message_cycle >= lo && message_cycle <= hi
}

/// Upper bound on how many whole cycles **ahead** of wall time (`floor(now_ms / cycle_ms)`) an LSU-related
/// P2P payload may name (`CATALYST_P2P_LSU_MAX_WALL_LEAD`, default `8192`, max `2_000_000`).
///
/// Cycles at or behind the wall index always pass [`p2p_lsu_cycle_within_wall_lead`]. This cap only drops
/// absurd far-future spam; it does **not** limit historical catch-up (old cycles remain well below `W + lead`).
pub fn p2p_lsu_cycle_max_wall_lead() -> u64 {
    const MAX_CAP: u64 = 2_000_000;
    const DEFAULT: u64 = 8192;
    std::env::var("CATALYST_P2P_LSU_MAX_WALL_LEAD")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .filter(|&v| v > 0 && v <= MAX_CAP)
        .unwrap_or(DEFAULT)
}

/// True if `cycle` is not absurdly far in the future vs wall clock (see [`p2p_lsu_cycle_max_wall_lead`]).
pub fn p2p_lsu_cycle_within_wall_lead(
    cycle: u64,
    wall_now_ms: u64,
    cycle_duration_ms: u64,
    max_lead_cycles: u64,
) -> bool {
    if cycle_duration_ms == 0 {
        return true;
    }
    let w = ledger_cycle_index(wall_now_ms, cycle_duration_ms);
    cycle <= w.saturating_add(max_lead_cycles)
}

/// On-disk / metadata encoding for `consensus:tx_batch_commit:{cycle}` (36 bytes).
pub fn parse_tx_batch_commit_value(bytes: &[u8]) -> Option<([u8; 32], u32)> {
    if bytes.len() != 36 {
        return None;
    }
    let mut h = [0u8; 32];
    h.copy_from_slice(&bytes[..32]);
    let mut c = [0u8; 4];
    c.copy_from_slice(&bytes[32..36]);
    Some((h, u32::from_le_bytes(c)))
}

pub fn encode_tx_batch_commit_value(batch_hash: [u8; 32], tx_count: u32) -> [u8; 36] {
    let mut buf = [0u8; 36];
    buf[..32].copy_from_slice(&batch_hash);
    buf[32..36].copy_from_slice(&tx_count.to_le_bytes());
    buf
}

/// After `ProtocolTxBatch::verify_hash()` succeeds: should we drop this batch because it
/// disagrees with a pinned `TxBatchControl::Commit` (RAM or metadata)?
pub fn protocol_tx_batch_disagrees_with_commit(
    batch: &ProtocolTxBatch,
    commit: ([u8; 32], u32),
) -> bool {
    let (h, tx_count) = commit;
    batch.batch_hash != h || batch.txs.len() as u32 != tx_count
}

/// Soft deadline for the main follower wait loop (`wait_for_tx_construction_entries`).
///
/// Caps `now + budget` at roughly two seconds before the nominal cycle end so late
/// batches and resync still have margin before the hard stop.
pub fn tx_batch_follower_deadline_ms(
    now_ms: u64,
    cycle: u64,
    cycle_ms: u64,
    budget_ms: u64,
) -> u64 {
    let cycle_end_ms = cycle.saturating_add(1).saturating_mul(cycle_ms);
    now_ms
        .saturating_add(budget_ms)
        .min(cycle_end_ms.saturating_sub(2_000))
}

/// Wall-clock bound for the final post-budget polling loop (~500 ms before cycle end).
pub fn tx_batch_follower_hard_stop_ms(cycle: u64, cycle_ms: u64) -> u64 {
    cycle
        .saturating_add(1)
        .saturating_mul(cycle_ms)
        .saturating_sub(500)
}

/// After RAM and store recovery find no construction entries: classify terminal outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyConstructionResolution {
    /// `Commit` agreed zero transactions for this cycle.
    AgreedEmpty,
    /// Leader must not apply with zero / non-recovered txs.
    LeaderMissFatal,
    /// Follower: legacy env allows ambiguous empty apply.
    FollowerLegacyEmpty,
    /// Follower: refuse empty without agreed `tx_count = 0`.
    FollowerRefuseEmpty,
}

pub fn resolve_empty_construction_takeaway(
    commit_meta: Option<([u8; 32], u32)>,
    is_leader: bool,
    allow_legacy_ambiguous_empty: bool,
) -> EmptyConstructionResolution {
    if matches!(commit_meta.map(|(_, c)| c), Some(0)) {
        return EmptyConstructionResolution::AgreedEmpty;
    }
    if is_leader {
        EmptyConstructionResolution::LeaderMissFatal
    } else if allow_legacy_ambiguous_empty {
        EmptyConstructionResolution::FollowerLegacyEmpty
    } else {
        EmptyConstructionResolution::FollowerRefuseEmpty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NodeConfig;

    #[test]
    fn parse_max_replay_cycles_accepts_zero_and_large() {
        assert_eq!(parse_max_replay_cycles_str("0"), 0);
        assert_eq!(parse_max_replay_cycles_str(" 42 "), 42);
        assert_eq!(
            parse_max_replay_cycles_str("999999999"),
            999_999_999
        );
    }

    #[test]
    fn parse_max_replay_cycles_invalid_falls_back() {
        assert_eq!(parse_max_replay_cycles_str("not-a-number"), DEFAULT_MAX_REPLAY_CYCLES);
        assert_eq!(parse_max_replay_cycles_str(""), DEFAULT_MAX_REPLAY_CYCLES);
    }

    #[test]
    fn ledger_cycle_index_monotonic_in_time() {
        let m = 30_000u64;
        assert!(ledger_cycle_index(10, m) <= ledger_cycle_index(20, m));
        assert!(ledger_cycle_index(29_999, m) < ledger_cycle_index(30_000, m));
    }

    #[test]
    fn tx_batch_follower_wait_budget_default_edges() {
        assert_eq!(tx_batch_follower_wait_budget_default(60_000), 52_000);
        assert_eq!(tx_batch_follower_wait_budget_default(20_000), 12_000);
        assert_eq!(tx_batch_follower_wait_budget_default(12_000), 12_000);
        assert_eq!(tx_batch_follower_wait_budget_default(8_000), 12_000);
    }

    #[test]
    fn tx_batch_follower_deadline_ms_caps_before_cycle_end() {
        let cycle_ms = 60_000u64;
        let budget = 52_000u64;
        assert_eq!(
            tx_batch_follower_deadline_ms(0, 0, cycle_ms, budget),
            52_000,
            "now+budget below cycle_end-2s"
        );
        assert_eq!(
            tx_batch_follower_deadline_ms(57_000, 0, cycle_ms, budget),
            58_000,
            "cap at cycle_end - 2000"
        );
    }

    #[test]
    fn tx_batch_follower_hard_stop_ms_edges() {
        assert_eq!(tx_batch_follower_hard_stop_ms(0, 60_000), 59_500);
        assert_eq!(tx_batch_follower_hard_stop_ms(2, 30_000), 89_500);
    }

    #[test]
    fn resolve_empty_construction_takeaway_matrix() {
        let h = [1u8; 32];
        assert_eq!(
            resolve_empty_construction_takeaway(Some((h, 0)), true, false),
            EmptyConstructionResolution::AgreedEmpty
        );
        assert_eq!(
            resolve_empty_construction_takeaway(Some((h, 0)), false, false),
            EmptyConstructionResolution::AgreedEmpty
        );
        assert_eq!(
            resolve_empty_construction_takeaway(Some((h, 3)), true, false),
            EmptyConstructionResolution::LeaderMissFatal
        );
        assert_eq!(
            resolve_empty_construction_takeaway(Some((h, 3)), false, false),
            EmptyConstructionResolution::FollowerRefuseEmpty
        );
        assert_eq!(
            resolve_empty_construction_takeaway(Some((h, 3)), false, true),
            EmptyConstructionResolution::FollowerLegacyEmpty
        );
        assert_eq!(
            resolve_empty_construction_takeaway(None, true, false),
            EmptyConstructionResolution::LeaderMissFatal
        );
        assert_eq!(
            resolve_empty_construction_takeaway(None, false, false),
            EmptyConstructionResolution::FollowerRefuseEmpty
        );
        assert_eq!(
            resolve_empty_construction_takeaway(None, false, true),
            EmptyConstructionResolution::FollowerLegacyEmpty
        );
    }

    #[test]
    fn ledger_cycle_index_basic() {
        assert_eq!(ledger_cycle_index(0, 60_000), 0);
        assert_eq!(ledger_cycle_index(59_999, 60_000), 0);
        assert_eq!(ledger_cycle_index(60_000, 60_000), 1);
        assert_eq!(ledger_cycle_index(60_000, 0), 0);
    }

    #[test]
    fn ledger_cycle_skew_vs_wall_matches_index() {
        let m = 60_000u64;
        let now = 125_000u64;
        let w = ledger_cycle_index(now, m);
        assert_eq!(
            ledger_cycle_skew_vs_wall(w, now, m),
            0,
            "message cycle equals wall index"
        );
        assert_eq!(ledger_cycle_skew_vs_wall(w + 2, now, m), 2);
        assert_eq!(ledger_cycle_skew_vs_wall(w.saturating_sub(1), now, m), -1);
    }

    #[test]
    fn p2p_tx_batch_cycle_ingress_slack_window() {
        let m = 30_000u64;
        let now = 100_000u64;
        let w = ledger_cycle_index(now, m);
        assert!(p2p_tx_batch_cycle_allows_ingress(w, now, m, 0));
        assert!(!p2p_tx_batch_cycle_allows_ingress(w + 1, now, m, 0));
        assert!(p2p_tx_batch_cycle_allows_ingress(w + 3, now, m, 3));
        assert!(!p2p_tx_batch_cycle_allows_ingress(w + 4, now, m, 3));
        assert!(p2p_tx_batch_cycle_allows_ingress(0, now, m, 128));
        assert!(!p2p_tx_batch_cycle_allows_ingress(w.saturating_add(500), now, m, 128));
    }

    /// Checklist §7.4 (unit): two validators with different wall clocks reject/accept the same
    /// gossip `cycle` differently when slack is tight — models skew without a full multi-node harness.
    #[test]
    fn wall_now_ms_honors_test_offset_env() {
        let base = catalyst_utils::utils::current_timestamp_ms();
        {
            let _e = EnvGuard::set("CATALYST_TEST_WALL_OFFSET_MS", "60000");
            let shifted = wall_now_ms();
            assert!(shifted >= base.saturating_add(60_000));
            assert!(shifted <= base.saturating_add(60_500));
        }
        {
            let _e = EnvGuard::set("CATALYST_TEST_WALL_OFFSET_MS", "-30000");
            let shifted = wall_now_ms();
            assert!(shifted <= base);
            assert!(shifted >= base.saturating_sub(30_500));
        }
    }

    #[test]
    fn clock_skew_two_nodes_disagree_on_batch_ingress() {
        let cycle_ms = 60_000u64;
        let slack = 1u64;
        let message_cycle = 10u64;
        let node_a_now = message_cycle * cycle_ms + 5_000;
        let node_b_now = (message_cycle + 3) * cycle_ms + 5_000;

        assert!(p2p_tx_batch_cycle_allows_ingress(
            message_cycle,
            node_a_now,
            cycle_ms,
            slack
        ));
        assert!(!p2p_tx_batch_cycle_allows_ingress(
            message_cycle,
            node_b_now,
            cycle_ms,
            slack
        ));
        assert_eq!(
            ledger_cycle_skew_vs_wall(message_cycle, node_b_now, cycle_ms),
            -3
        );
    }

    #[test]
    fn p2p_lsu_cycle_within_wall_lead_caps_future_only() {
        let m = 60_000u64;
        let now = 3_600_000u64;
        let w = ledger_cycle_index(now, m);
        let cap = 100u64;
        assert!(p2p_lsu_cycle_within_wall_lead(0, now, m, cap));
        assert!(p2p_lsu_cycle_within_wall_lead(w, now, m, cap));
        assert!(p2p_lsu_cycle_within_wall_lead(w + cap, now, m, cap));
        assert!(!p2p_lsu_cycle_within_wall_lead(w + cap + 1, now, m, cap));
    }

    #[test]
    fn p2p_lsu_cycle_max_wall_lead_env_and_fallbacks() {
        {
            let _e = EnvGuard::set("CATALYST_P2P_LSU_MAX_WALL_LEAD", "5000");
            assert_eq!(p2p_lsu_cycle_max_wall_lead(), 5000);
        }
        {
            let _e = EnvGuard::set("CATALYST_P2P_LSU_MAX_WALL_LEAD", "0");
            assert_eq!(p2p_lsu_cycle_max_wall_lead(), 8192);
        }
        {
            let _e = EnvGuard::set("CATALYST_P2P_LSU_MAX_WALL_LEAD", "9999999999");
            assert_eq!(p2p_lsu_cycle_max_wall_lead(), 8192);
        }
    }

    #[test]
    fn p2p_tx_batch_max_cycle_slack_env_and_fallbacks() {
        {
            let _e = EnvGuard::set("CATALYST_P2P_TX_BATCH_MAX_CYCLE_SLACK", "7");
            assert_eq!(p2p_tx_batch_max_cycle_slack(), 7);
        }
        {
            let _e = EnvGuard::set("CATALYST_P2P_TX_BATCH_MAX_CYCLE_SLACK", "not-a-number");
            assert_eq!(p2p_tx_batch_max_cycle_slack(), 3);
        }
        {
            let _e = EnvGuard::set("CATALYST_P2P_TX_BATCH_MAX_CYCLE_SLACK", "9999");
            assert_eq!(p2p_tx_batch_max_cycle_slack(), 3);
        }
    }

    #[test]
    fn tx_batch_commit_blob_roundtrip() {
        let h = [3u8; 32];
        let blob = encode_tx_batch_commit_value(h, 42);
        assert_eq!(parse_tx_batch_commit_value(&blob), Some((h, 42)));
        assert_eq!(parse_tx_batch_commit_value(&blob[..35]), None);
    }

    #[test]
    fn protocol_tx_batch_disagrees_with_commit_counts_hash_and_len() {
        use crate::tx::ProtocolTxBatch;
        use catalyst_core::protocol as corep;
        use catalyst_core::protocol::EntryAmount;

        let now_ms = 1_700_000_000_000u64;
        let sender = [5u8; 32];
        let recv = [6u8; 32];
        let tx = corep::Transaction {
            core: corep::TransactionCore {
                tx_type: corep::TransactionType::NonConfidentialTransfer,
                entries: vec![
                    corep::TransactionEntry {
                        public_key: sender,
                        amount: EntryAmount::NonConfidential(-10),
                    },
                    corep::TransactionEntry {
                        public_key: recv,
                        amount: EntryAmount::NonConfidential(10),
                    },
                ],
                nonce: 1,
                lock_time: 0,
                fees: 0,
                data: Vec::new(),
            },
            signature_scheme: corep::sig_scheme::SCHNORR_V1,
            signature: corep::AggregatedSignature(vec![7u8; 64]),
            sender_pubkey: None,
            timestamp: now_ms,
        };
        let mut tx = tx;
        tx.core.fees = corep::min_fee(&tx);

        let batch = ProtocolTxBatch::new(9, vec![tx]).unwrap();
        assert!(batch.verify_hash().unwrap());
        let h = batch.batch_hash;
        let n = batch.txs.len() as u32;
        assert!(!protocol_tx_batch_disagrees_with_commit(&batch, (h, n)));
        assert!(protocol_tx_batch_disagrees_with_commit(&batch, (h, n + 1)));
        assert!(protocol_tx_batch_disagrees_with_commit(&batch, ([0u8; 32], n)));
    }

    #[test]
    fn effective_require_lsu_finality_precedence() {
        let mut cfg = NodeConfig::default();

        {
            let _e = EnvGuard::unset("CATALYST_REQUIRE_LSU_FINALITY");
            cfg.consensus.require_lsu_finality = false;
            assert!(!effective_require_lsu_finality(&cfg));
            cfg.consensus.require_lsu_finality = true;
            assert!(effective_require_lsu_finality(&cfg));
        }
        {
            let _e = EnvGuard::set("CATALYST_REQUIRE_LSU_FINALITY", "1");
            cfg.consensus.require_lsu_finality = false;
            assert!(effective_require_lsu_finality(&cfg));
        }
        {
            let _e = EnvGuard::set("CATALYST_REQUIRE_LSU_FINALITY", "false");
            cfg.consensus.require_lsu_finality = true;
            assert!(!effective_require_lsu_finality(&cfg));
        }
    }

    /// Restores previous `std::env` value on drop (tests may run in parallel across crates; keep mutations scoped).
    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn unset(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::remove_var(key);
            Self { key, previous }
        }

        fn set(key: &'static str, val: &str) -> Self {
            let previous = std::env::var(key).ok();
            std::env::set_var(key, val);
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(ref v) = self.previous {
                std::env::set_var(self.key, v);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}
