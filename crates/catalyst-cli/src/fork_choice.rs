//! Fork choice for competing LSUs at the same cycle (§4.1).
//!
//! Normative policy: [`docs/consensus-quorum-and-fork-choice.md`](../../../docs/consensus-quorum-and-fork-choice.md).

use catalyst_consensus::types::LedgerStateUpdate;

/// Evidence strength for fork choice at a fixed cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ForkChoiceTier {
    /// No BFT quorum on `vote_list` and no verified finality certificate for this LSU.
    None = 0,
    /// `vote_list` meets ⌈2n/3⌉ distinct producers ⊆ `producer_list` (MVP quorum; testnet / migration).
    QuorumInferred = 1,
    /// Verified `LsuFinalityCertificateV1` for this `lsu_hash` (ADR 0001).
    Certified = 2,
}

/// Competing LSU head at one cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForkChoiceCandidate {
    pub cycle: u64,
    pub lsu_hash: [u8; 32],
    pub tier: ForkChoiceTier,
    /// Whether a verified `LsuStateRootCertificateV1` (ADR 0002) backs this candidate's applied
    /// `state_root`. Only a tie-break signal within `tier` — it never promotes a candidate above
    /// a higher `tier`, since `tier` is still the LSU-recipe evidence fork choice is defined over.
    pub state_root_certified: bool,
}

impl ForkChoiceCandidate {
    pub fn new(
        cycle: u64,
        lsu_hash: [u8; 32],
        lsu: &LedgerStateUpdate,
        certified: bool,
        certified_only: bool,
        state_root_certified: bool,
    ) -> Self {
        Self {
            cycle,
            lsu_hash,
            tier: effective_fork_choice_tier(lsu, certified, certified_only),
            state_root_certified,
        }
    }
}

/// Tier used for fork choice given production policy.
///
/// When `certified_only` is true (production validators), only [`ForkChoiceTier::Certified`] competes;
/// quorum-inferred LSUs are treated as [`ForkChoiceTier::None`].
pub fn effective_fork_choice_tier(
    lsu: &LedgerStateUpdate,
    certified: bool,
    certified_only: bool,
) -> ForkChoiceTier {
    if certified {
        return ForkChoiceTier::Certified;
    }
    if certified_only {
        return ForkChoiceTier::None;
    }
    if lsu_has_bft_vote_quorum(lsu) {
        ForkChoiceTier::QuorumInferred
    } else {
        ForkChoiceTier::None
    }
}

/// BFT-style quorum: distinct agreeing producers must be at least ⌈2n/3⌉ of `producer_list.len()`.
pub fn bft_vote_threshold(n: usize) -> usize {
    if n == 0 {
        return usize::MAX;
    }
    (2 * n + 2) / 3
}

/// True when `vote_list` ⊆ `producer_list` and distinct voter count meets the BFT threshold.
pub fn lsu_has_bft_vote_quorum(lsu: &LedgerStateUpdate) -> bool {
    let n = lsu.producer_list.len();
    if n == 0 {
        return false;
    }
    let need = bft_vote_threshold(n);
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for v in &lsu.vote_list {
        if !lsu.producer_list.iter().any(|p| p == v) {
            return false;
        }
        seen.insert(v.as_str());
    }
    seen.len() >= need
}

/// Policy when two **verified certified** LSUs at the same cycle are observed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertifiedEquivocationPolicy {
    /// Production: alert and do not apply either new conflicting cert (stall).
    StallAndAlert,
    /// Testnet / migration: deterministic smaller-`lsu_hash` tie-break among certs.
    TieBreak,
}

/// Production (`certified_only`) stalls on certified equivocation unless env override is set.
pub fn certified_equivocation_policy(certified_only: bool) -> CertifiedEquivocationPolicy {
    if certified_only && !allow_cert_equivocation_tiebreak_from_env() {
        CertifiedEquivocationPolicy::StallAndAlert
    } else {
        CertifiedEquivocationPolicy::TieBreak
    }
}

/// `CATALYST_ALLOW_CERT_EQUIVOCATION_TIEBREAK=1` — dev/test only; disables stall on double-cert.
pub fn allow_cert_equivocation_tiebreak_from_env() -> bool {
    match std::env::var("CATALYST_ALLOW_CERT_EQUIVOCATION_TIEBREAK") {
        Ok(s) => matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes"
        ),
        Err(_) => false,
    }
}

/// Metadata key: first verified certified `lsu_hash` recorded at this cycle (32 bytes).
pub fn certified_hash_metadata_key(cycle: u64) -> String {
    format!("consensus:certified_lsu_hash:{cycle}")
}

/// Result of registering a newly verified certified hash at `cycle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertifiedHashRegister {
    /// First certified hash at this cycle (or same hash again).
    Ok,
    /// Second distinct certified hash; caller should stall (production policy).
    ConflictingCertified { existing: [u8; 32], new: [u8; 32] },
}

/// Compare `new_hash` with the stored first certified hash at `cycle` (`stored` = None if unset).
pub fn register_certified_hash(
    stored: Option<[u8; 32]>,
    new_hash: [u8; 32],
) -> CertifiedHashRegister {
    match stored {
        None => CertifiedHashRegister::Ok,
        Some(existing) if existing == new_hash => CertifiedHashRegister::Ok,
        Some(existing) => CertifiedHashRegister::ConflictingCertified {
            existing,
            new: new_hash,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForkChoicePrefer {
    Left,
    Right,
    Tie,
}

/// Compare two candidates at the **same** `cycle`. Higher tier wins; equal tier → smaller `lsu_hash`.
pub fn fork_choice_prefer(a: &ForkChoiceCandidate, b: &ForkChoiceCandidate) -> ForkChoicePrefer {
    debug_assert_eq!(a.cycle, b.cycle, "fork_choice_prefer requires the same cycle");
    if a.lsu_hash == b.lsu_hash {
        return ForkChoicePrefer::Tie;
    }
    match a.tier.cmp(&b.tier) {
        std::cmp::Ordering::Greater => ForkChoicePrefer::Left,
        std::cmp::Ordering::Less => ForkChoicePrefer::Right,
        std::cmp::Ordering::Equal => {
            // Same LSU-recipe tier: prefer additional state-root (ADR 0002) backing before
            // falling back to the hash tie-break, so a candidate whose *result* is also
            // quorum-certified wins over one that's merely recipe-certified.
            match (a.state_root_certified, b.state_root_certified) {
                (true, false) => ForkChoicePrefer::Left,
                (false, true) => ForkChoicePrefer::Right,
                _ => {
                    if a.lsu_hash < b.lsu_hash {
                        ForkChoicePrefer::Left
                    } else {
                        ForkChoicePrefer::Right
                    }
                }
            }
        }
    }
}

pub fn incoming_beats_stored(
    incoming: &ForkChoiceCandidate,
    stored: &ForkChoiceCandidate,
) -> bool {
    debug_assert_eq!(incoming.cycle, stored.cycle);
    matches!(
        fork_choice_prefer(incoming, stored),
        ForkChoicePrefer::Left
    )
}

pub fn incoming_weaker_than_stored(
    incoming: &ForkChoiceCandidate,
    stored: &ForkChoiceCandidate,
) -> bool {
    debug_assert_eq!(incoming.cycle, stored.cycle);
    matches!(
        fork_choice_prefer(incoming, stored),
        ForkChoicePrefer::Right
    )
}

/// Cross-cycle adoption requires a verifiable contiguous prefix (or checkpoint), not max gossiped cycle alone.
pub const CROSS_CYCLE_PREFIX_REQUIRED: &str =
    "adopt higher certified head only after contiguous verified prefix or checkpoint";

#[cfg(test)]
mod tests {
    use super::*;
    use catalyst_consensus::types::{
        CompensationEntry, LedgerStateUpdate, PartialLedgerStateUpdate, TransactionEntry,
    };

    fn empty_lsu(cycle: u64, producers: Vec<String>, votes: Vec<String>) -> LedgerStateUpdate {
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

    fn cand(cycle: u64, hash_byte: u8, tier: ForkChoiceTier) -> ForkChoiceCandidate {
        ForkChoiceCandidate {
            cycle,
            lsu_hash: [hash_byte; 32],
            tier,
            state_root_certified: false,
        }
    }

    #[test]
    fn certified_only_downgrades_inferred_quorum() {
        let p = vec!["a".into(), "b".into(), "c".into()];
        let lsu = empty_lsu(1, p, vec!["a".into(), "b".into()]);
        let c = ForkChoiceCandidate::new(1, [1u8; 32], &lsu, false, true, false);
        assert_eq!(c.tier, ForkChoiceTier::None);
        let c2 = ForkChoiceCandidate::new(1, [1u8; 32], &lsu, false, false, false);
        assert_eq!(c2.tier, ForkChoiceTier::QuorumInferred);
    }

    #[test]
    fn certified_beats_quorum_inferred() {
        let a = cand(5, 0xff, ForkChoiceTier::Certified);
        let b = cand(5, 0x01, ForkChoiceTier::QuorumInferred);
        assert_eq!(fork_choice_prefer(&a, &b), ForkChoicePrefer::Left);
    }

    #[test]
    fn state_root_certified_wins_tie_break_over_hash_within_same_tier() {
        let mut a = cand(5, 0x01, ForkChoiceTier::Certified);
        let mut b = cand(5, 0xff, ForkChoiceTier::Certified);
        // Without state-root backing, smaller hash (`a`) would normally win.
        assert_eq!(fork_choice_prefer(&a, &b), ForkChoicePrefer::Left);
        // With only `b` state-root-certified, it wins despite the larger hash.
        b.state_root_certified = true;
        assert_eq!(fork_choice_prefer(&a, &b), ForkChoicePrefer::Right);
        // With both state-root-certified, falls back to the hash tie-break again.
        a.state_root_certified = true;
        assert_eq!(fork_choice_prefer(&a, &b), ForkChoicePrefer::Left);
    }

    #[test]
    fn register_certified_hash_detects_conflict() {
        assert_eq!(
            register_certified_hash(None, [1u8; 32]),
            CertifiedHashRegister::Ok
        );
        assert_eq!(
            register_certified_hash(Some([1u8; 32]), [1u8; 32]),
            CertifiedHashRegister::Ok
        );
        assert!(matches!(
            register_certified_hash(Some([1u8; 32]), [2u8; 32]),
            CertifiedHashRegister::ConflictingCertified { .. }
        ));
    }

    #[test]
    fn equivocation_policy_stalls_when_certified_only() {
        assert_eq!(
            certified_equivocation_policy(true),
            CertifiedEquivocationPolicy::StallAndAlert
        );
        assert_eq!(
            certified_equivocation_policy(false),
            CertifiedEquivocationPolicy::TieBreak
        );
    }
}
