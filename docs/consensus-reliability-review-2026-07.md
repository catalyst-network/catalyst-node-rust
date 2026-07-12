# Consensus reliability review (2026-07)

**Trigger:** repeated testnet freezes and forks (May–July 2026) that kept recurring after point
fixes, worsening over time rather than converging. This document is the design-vs-implementation
review called for by [`consensus-implementation-completion-checklist.md`](./consensus-implementation-completion-checklist.md)
§8: *"if failures persist without Byzantine nodes, capture traces and only then escalate to a
protocol design review."* That condition is now met.

**Conclusion up front:** the individual patches landed since May (checklist batches 1–15, plus the
two fixes from this week) are each independently correct, but they treat symptoms of a **single
underlying design gap**: the state root produced by applying an LSU is not part of what BFT
consensus agrees on or certifies. Only the *recipe* (`lsu_hash`) is agreed. The *result*
(`state_root`) is locally computed by each node and only loosely, optionally cross-checked. Once
any two nodes' local state ever differs — for any reason — the protocol has no cryptographic way to
detect that divergence as a consensus fact, and the one best-effort mechanism that does detect it
(a gossip-envelope `prev_state_root` comparison) feeds into a reconcile/replay path that is
structurally broken for a wall-clock-cycle-numbered chain. The result: divergences are silent,
permanent, and compound every cycle, which is exactly the "gets worse over time" behavior reported.

---

## 1. Live evidence (2026-07-03)

Queried `catalyst_head` on all three public validators mid-investigation:

| Node | Role | `applied_cycle` | `lsu_hash` | `state_root` |
|------|------|-----------------|-----------|--------------|
| US (`45.76.21.153`) | validator | 89155911 | `0x0a12dd3c...` | `0x9642c65f...` |
| Asia (`207.148.126.35`) | validator | 89155911 | `0x0a12dd3c...` | `0x8f396537...` |
| EU (`45.32.177.248`) | observer/RPC | 89155882 (29 cycles behind) | — | — |

US and Asia — the only two active validators — agree on **cycle and `lsu_hash`** (i.e. BFT
consensus succeeded: they agreed on the same LSU) but computed **different `state_root`s** from
applying it. This is not a liveness symptom (nobody is frozen); it is a live, silent correctness
fork that the protocol itself does not know exists.

This matches the earlier (2026-07-01/02) incident on this same fleet, where a restart-after-freeze
produced the identical signature: same cycle, same `lsu_hash`, different `state_root` between two
validators (see prior session). It also matches the *shape* of divergence the team already fixed
once before — commit `82992fc` ("deterministic fee-credit settlement") — which was itself triggered
by a node-local (non-LSU-derived) input leaking into settlement math. That specific bug is fixed
(verified below); this is the same *class* of problem recurring through a different mechanism.

## 2. Two fixes landed this week (recap, for context)

1. `fbed062`-era spurious-reconcile / depth-gate fixes (already in place before this review).
2. This week: `stale_observed_head_reset_ms` backstop (liveness: stops a node deferring forever on
   an unreachable phantom head) and making `reconcile_prefetch_lsu_metadata_if_missing` fire-and-forget
   instead of blocking the single shared gossip task for up to 8s (liveness: stopped EU's repeated
   freeze).

Both are correct and necessary. **Neither touches correctness of the applied state.** They explain
why the network stopped *freezing*; they say nothing about why it keeps *forking*. That's this
document.

## 3. The architectural gap

### 3.1 Consensus agrees on the recipe, not the result

`LedgerStateUpdate` (`crates/catalyst-consensus/src/types.rs:79-85`) is the object BFT quorum votes
on, and the object ADR 0001 finality certificates cover:

```rust
pub struct LedgerStateUpdate {
    pub partial_update: PartialLedgerStateUpdate,   // tx entries
    pub compensation_entries: Vec<CompensationEntry>, // producer/reward payouts
    pub cycle_number: CycleNumber,
    pub producer_list: Vec<ProducerId>,
    pub vote_list: Vec<ProducerId>,
}
```

There is **no `state_root` or `prev_state_root` field**. Fork choice
(`crates/catalyst-cli/src/fork_choice.rs`) confirms the same: `ForkChoiceCandidate` carries only
`cycle`, `lsu_hash`, `tier` (`Certified` / `QuorumInferred` / `None`). Two competing LSUs at a cycle
are ranked purely by evidence-of-agreement-on-the-recipe. The *result* of applying that recipe —
the actual account balances, i.e. `state_root` — is never part of what's voted on, certified, or
ranked.

Each node computes `state_root` independently via `StorageManager::compute_state_root()`
(`crates/catalyst-storage/src/manager.rs:371`): a fresh sparse-Merkle-tree hash over every key in
the local `accounts` column family, on every normal apply. This part is fine in isolation — RocksDB
iteration is byte-sorted, the SMT construction is deterministic, and I did not find non-determinism
in the settlement/reward code paths (`distribute_waiting_pool_rewards_and_fee_credits`,
`settle_fee_credit_spend_for_applied_cycle` both already use `BTreeMap`/`BTreeSet` iteration and
`cycle_number`-derived — not wall-clock-derived — day-bucket math, with an explicit comment
recording the prior incident this guards against). **The gap is not "the hash is computed wrong."
It's "nothing forces two nodes' pre-apply state to be provably identical before or after
applying an agreed LSU."**

### 3.2 The one place divergence *is* checked — and why it doesn't self-heal

`node.rs`'s gossip handler (~line 5346) does compare a sender's claimed `prev_state_root` (carried
in the gossip envelope, *not* the consensus-agreed LSU) against the receiver's own local
`local_applied_state_root()`. This check exists (`850bb61 "sync: enforce prev_state_root
continuity"`) and is the *only* place a divergence becomes visible. On mismatch it calls
`try_reconcile_fork_from_quorum_lsu`, which is designed (per
[`consensus-quorum-and-fork-choice.md`](./consensus-quorum-and-fork-choice.md) — "adopt a higher
certified head only after a contiguous verified prefix... not from gossiped cycle number alone") to
replay `consensus:lsu:{checkpoint+1..cycle}` to rebuild a verified prefix.

In practice this path almost always aborts with `Quorum fork reconcile skipped: missing stored LSU
for cycle 1` (observed repeatedly on both the local and public deployments this session, including
immediately after a fresh reset). Root cause: `cycle` is wall-clock-derived
(`current_timestamp_ms() / cycle_ms`, documented in
[`consensus-wall-clock-and-time.md`](./consensus-wall-clock-and-time.md)) — a number like
`89,153,xxx` — while the reconcile path's literal fallback is `replay_start = 1`. "Cycle 1" is
epoch-adjacent and meaningless for any wall-clock-numbered chain; the checkpoint-anchoring code
(`reconcile_checkpoint_for_missing_prefix`) is supposed to supply a usable recent anchor instead,
but for a near-head conflict (the receiver is only 1–2 cycles off, as in the observed US/Asia case)
it evidently doesn't find/use one reliably, and the path falls through to the "cycle 1" case and
gives up. The checklist has flagged this exact gap as open since its second revision batch
(2026-05-13) and it is still listed "TBD" as of the latest revision (§4.2, §7.1): *"full
multi-process nodes + real protocol txs... still TBD"*, *"checkpointed state-sync for
replay-from-pruned-genesis without archival peers... still not done."*

**Net effect: the only detector fires, but the only healer doesn't work for the numbering scheme in
production use.** A detected mismatch is logged and dropped, not fixed.

### 3.3 The mechanism that plausibly *introduces* the divergence in the first place

Separate from detection/recovery, there is a fast/trusted apply path used during catch-up:
`apply_lsu_to_storage_without_root_check` (`node.rs:1258`). When a peer supplies a `StateProofBundle`
whose Merkle proofs check out for the *changed* keys, this path applies the LSU's deltas locally and
then does:

```rust
// Persist head metadata (trusted).
store.set_state_root_cache(new_root);
```

`set_state_root_cache` (`catalyst-storage/src/manager.rs:415`) just overwrites the cached root field
directly — it does **not** call `compute_state_root()` to verify that this node's own local
mutation of the `accounts` column family actually produces `new_root`. `verify_state_transition_bundle`
checks that the *proof steps* for the touched keys are internally consistent with the bundle's
claimed old/new roots — it does not prove that *this node's* pre-state matches the bundle's claimed
`prev_state_root` beyond the same best-effort envelope comparison from §3.2. If this node's actual
local account values were already even slightly off (from any prior cause), this path will cheerfully
accept a peer's claimed result, cache it as truth, and move on. The lie is self-correcting on the
*next* full apply (which always recomputes from scratch) — but by then the divergent mutation is
already baked into the stored account values, and the next honest recompute simply reports the
divergence as fact.

This is exactly the pattern that produces "identical `lsu_hash`, different `state_root`, forever
after": the recipe both nodes apply each cycle is genuinely identical (hence identical `lsu_hash`),
but it's applied on top of two already-different account books, so the results diverge and *keep*
diverging every subsequent cycle as producer/waiting-pool compensation continues to accrue onto the
wrong balances on each side independently. This matches the user's own description — "it feels like
it's gotten worse over time" — precisely: it's not that new forks are being created at a high rate,
it's that **once introduced, a fork cannot heal, and independently-computed per-cycle rewards
compound the gap forever.**

### 3.4 This is a known-and-reverted design intent, not an oversight

`git log` shows this project *did* originally treat the state root as consensus-critical:

- `9556308` *"state: canonical blake2b state root + verifiable proofs (E1 #156)"*
- `2f121b8` *"docs: ADR 0006 canonical state root + proofs"* — explicitly: *"State roots and proofs
  become consensus-critical and MUST use Blake2b-256... Proof verification MUST be key-derived (do
  not trust provided left/right direction)."*

Then, in `a3b2d06` *"Updated for test network"* (2026-02-07), **seven ADRs were deleted in one
commit** — 0001 (canonical encoding/wire versioning), 0002 (hash algorithm alignment), 0003
(producer-selection PRNG domain separation), 0004 (wire vs internal types), 0005 (canonical tx
derivations), **0006 (state root + proofs)**, 0007 (freeze window/cycle input selection) — along
with `public_testnet_plan.md`, `public_testnet_runbook.md`, `spec_index.md`, `test_vectors.md`, and
the GitHub issue templates that tracked "public testnet readiness." The SMT/proof *code*
(`catalyst-storage/src/sparse_merkle.rs`, `StateProofBundle`, `verify_state_transition_bundle`)
survived, but the **discipline that would have kept it mandatory and continuously verified did
not** — no ADR governs it today, no test-vector suite pins its behavior, and (today's ADR 0001, for
LSU finality certificates, is a *different, later, renumbered* document — dated 2026-03-28, i.e.
created *after* the old ADR 0001 was deleted).

This wasn't a mistake so much as a deliberate simplification to move faster on testnet — but it
removed the one design element that would have made today's fork class detectable and provable, and
nothing has replaced it since.

## 4. Why point-fixes keep not sticking

Every fix landed since May (`dd42f5f` through this week) has targeted **liveness**: freeze recovery,
depth-gating, catch-up triggers, reconcile blocking. All were real bugs and all were correctly
fixed — the checklist's own revision history (batches 1–15) documents a methodical, well-tested
effort. But liveness and correctness are different axes:

- Liveness bugs make an *honest, unforked* node stop advancing. Fixing them (correctly) restores
  cycle production.
- Correctness/fork bugs make two nodes silently compute different results while both keep advancing
  happily. No amount of liveness fixing touches this, and — worse — **restoring liveness after a
  freeze is exactly the moment a stale/divergent node resumes independently accruing rewards on top
  of whatever state it had when it froze**, which is likely why forks appear to correlate with
  freeze-recovery events in the logs from this session.

The project's own testing gate has never covered the scenario that's actually failing. The
checklist (§7.1, §7.2, §7.4) explicitly and repeatedly (across 15 revision batches) lists as **"still
TBD"**: full multi-process nodes under real restart/catch-up, real clock skew, and real batch-loss
conditions. The existing CI/local gate (`consensus-three-validator-e2e.sh`) only checks a few cycles
of convergence from a synchronized fresh genesis — it has never once, in this project's history,
exercised "two real node processes, one of them restarted or artificially delayed mid-run, then
assert converged `state_root`." That is exactly the condition under which every fork observed this
session has occurred. A test suite that cannot reach the failure mode cannot catch regressions in
it, however carefully each individual PR is reviewed.

## 5. What "reliable consensus" actually requires here

In order, most important first:

1. **Make `state_root` (or a canonical proof of it) part of what's cryptographically agreed, not
   locally trusted.** Concretely: either (a) fold a state-transition proof requirement into the
   ADR 0001 finality certificate path — a certificate should not be considered valid unless a
   `StateProofBundle`-style proof against the certified `state_root` also verifies, or (b) at
   minimum, delete `apply_lsu_to_storage_without_root_check`'s trust-and-cache shortcut and always
   force a full `compute_state_root()` recompute-and-compare after any catch-up apply, treating a
   mismatch as a fatal, alerting condition rather than a silently-cached fact. This closes §3.3.
2. **Fix reconcile so a detected mismatch actually heals.** The checkpoint-anchored replay path
   needs to reliably resolve near-head conflicts (the common case observed this session) without
   ever falling through to a literal "replay from cycle 1" — that fallback should probably be
   deleted entirely in favor of "no usable checkpoint → fail loud and page an operator" rather than
   "log a warning and silently continue diverged." This closes §3.2. This is explicitly the
   still-open item in checklist §4.2/§4.4 and Phase 4 of the phased execution plan.
3. **Restore an ADR for state root and proofs**, informed by the deleted ADR 0006, updated for the
   wall-clock cycle-numbering scheme that didn't exist when it was written, and make it normative
   (test-vector-pinned) again like ADR 0001 is today.
4. **Build the multi-process restart/skew/batch-loss CI gate the checklist has called for since
   batch 1.** Specifically: spin up 3 real node processes, let them run a few cycles, kill and
   restart one with a stale/rolled-back data directory (simulating exactly what "freeze then
   restart" produces today), and assert convergence within N cycles or fail loudly. Until this
   exists in CI, this exact failure class will keep reaching production because nothing before
   production can see it.
5. Once 1–4 hold, run the existing [`wan-soak-load-chaos-gate.md`](./wan-soak-load-chaos-gate.md)
   24h soak — which, per this review's evidence gathering, does not appear to have been run to
   completion against this failure class yet.

## 6. Non-findings (checked and ruled out)

- Tokenomics settlement determinism (`distribute_waiting_pool_rewards_and_fee_credits`,
  `settle_fee_credit_spend_for_applied_cycle`): already fixed for determinism (BTreeMap/BTreeSet
  ordering, cycle-derived day-bucket), with an explicit code comment documenting the prior incident.
  Not the current cause.
- `compute_state_root()`'s own hashing: RocksDB iteration is byte-sorted; no HashMap-ordering
  non-determinism found in the Merkle construction path itself.
- The two liveness fixes landed this week: verified correct via full local unit suite (65/65) and
  two independent fresh 3-node E2E convergence runs, and directly observed working in production
  (EU's reconcile-prefetch warnings now resolve sub-millisecond instead of blocking 8s). These are
  not implicated in the fork described here.

## References

- [`consensus-quorum-and-fork-choice.md`](./consensus-quorum-and-fork-choice.md) — normative fork-choice policy
- [`consensus-implementation-completion-checklist.md`](./consensus-implementation-completion-checklist.md) — engineering gate + revision history
- [`consensus-wall-clock-and-time.md`](./consensus-wall-clock-and-time.md) — cycle numbering
- [`adr/0001-lsu-finality-certificate.md`](./adr/0001-lsu-finality-certificate.md) — current ADR 0001 (LSU finality)
- Deleted `docs/adr/0006-state-root-and-proofs.md` (recoverable via `git show 2f121b8:docs/adr/0006-state-root-and-proofs.md`)
- `crates/catalyst-consensus/src/types.rs` — `LedgerStateUpdate`
- `crates/catalyst-cli/src/fork_choice.rs` — `ForkChoiceCandidate`, tiering
- `crates/catalyst-cli/src/node.rs` — `apply_lsu_to_storage`, `apply_lsu_to_storage_without_root_check`, `try_reconcile_fork_from_quorum_lsu`, `reconcile_prefetch_lsu_metadata_if_missing`
- `crates/catalyst-storage/src/manager.rs` — `compute_state_root`, `set_state_root_cache`
