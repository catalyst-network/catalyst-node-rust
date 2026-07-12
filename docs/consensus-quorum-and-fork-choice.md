# Consensus quorum, finality, and fork choice (requirements)

This document captures **protocol intent** and **implementation** for Catalyst validator networks.

**Implementation:** `crates/catalyst-cli/src/fork_choice.rs`, `crates/catalyst-cli/src/node.rs` (apply, reconcile, P2P LSU paths).

## Problem statement

Operators have observed that validators can **diverge** (same logical cycle/height, different `state_root`) and **do not self-heal**. Follower logic must **reorganize** to **quorum-finalized / certified** history—not skip indefinitely.

This document originally covered only the LSU **recipe** (`lsu_hash`, ADR 0001) — a quorum can be certified as having agreed on the same recipe while still independently computing different `state_root`s from it, exactly the divergence described above; see [`consensus-reliability-review-2026-07.md`](./consensus-reliability-review-2026-07.md) for the production incident that surfaced this. [ADR 0002](./adr/0002-canonical-state-root-and-proofs.md) closes that gap with a second, additive certificate over the applied *result*.

For production and for growing validator sets, the network must:

1. **Finalize** ledger updates only when a **defined quorum** of block producers agrees (ADR 0001 certificate).
2. **Treat certified history as authoritative** for all nodes (including lagging or partitioned replicas).
3. **Reconcile** when local state **conflicts** with certified updates—not **skip indefinitely**.

Snapshot-based manual restore remains a **break-glass** operational tool, not the liveness mechanism.

## Quorum size (scaling producer count)

We target **Byzantine fault tolerance** semantics: tolerate up to **f** faulty producers among **n**, with **n ≥ 3f + 1**, using a quorum of **⌈2n/3⌉** agreeing votes (equivalent to **2f + 1** in the classical 3f+1 layout).

| Producers (n) | Quorum (⌈2n/3⌉) | Notes |
|---------------|------------------|--------|
| 3 | 2 | Minimal BFT committee; 1 fault |
| 5 | 4 | Stricter than bare majority (3) |

The **exact** quorum formula, how **inactive** or **jailed** producers affect **n**, and how **epoch boundaries** rotate the set must be specified in protocol code and kept consistent with this table.

## Finality and fork choice (normative policy)

### Finality

A ledger state update (LSU) at cycle *C* is **canonical** for production validators only with a **verified** `LsuFinalityCertificateV1` (ADR 0001) for that LSU’s `lsu_hash` (`[consensus] require_lsu_finality = true` by default; overridable via `CATALYST_REQUIRE_LSU_FINALITY`).

Testnets may set `require_lsu_finality = false` to allow **quorum-inferred** fork choice during migration.

### Fork choice at a fixed cycle

When two different LSU payloads compete at the **same** `cycle_number`, nodes use **`fork_choice.rs`**:

1. **Evidence tier** (higher wins):
   - **`Certified`**: verified ADR 0001 certificate for that `lsu_hash`.
   - **`QuorumInferred`**: `vote_list` meets ⌈2n/3⌉ ⊆ `producer_list` (only when **not** `certified_only`).
   - **`None`**: does not compete for canonical slot.
2. **State-root tie-break** (equal tier, different hash): a candidate additionally backed by a verified
   ADR 0002 `LsuStateRootCertificateV1` (`ForkChoiceCandidate.state_root_certified`) wins over one that
   is not. This never promotes a candidate above a higher tier — tier remains defined purely over
   LSU-recipe evidence.
3. **Hash tie-break** (equal tier, equal state-root-certified status, different hash): **lexicographically smaller `lsu_hash`**.

Weaker competitors are rejected (`consensus_fork_choice_rejected_weaker_total`). Under `certified_only`, non-certified LSUs are rejected (`consensus_fork_choice_rejected_non_certified_total`).

### Certified equivocation (two certs, same cycle)

If two **distinct** verified certificates exist at the same cycle:

| Mode | Policy |
|------|--------|
| **Production** (`require_lsu_finality` effective) | **Stall apply** + error log + `consensus_certified_equivocation_stall_total` |
| **Testnet** (`require_lsu_finality` off) | Deterministic **hash tie-break** + `consensus_certified_equivocation_tiebreak_total` |
| **Dev override** | `CATALYST_ALLOW_CERT_EQUIVOCATION_TIEBREAK=1` forces tie-break even with finality required |

First certified hash at a cycle is stored in metadata `consensus:certified_lsu_hash:{cycle}`.

### Cross-cycle head adoption

Adopt a **higher** certified head **only** after a **contiguous verified prefix** from genesis, local head, or a **signed checkpoint**—not from gossiped cycle number alone. Implementation: `try_reconcile_fork_from_quorum_lsu` replays `consensus:lsu:1..C-1` (with optional `LsuRangeRequest` prefetch); does not apply a distant head without prefix.

### Reorg on stronger certified LSU

If cycle *C* is **already applied** but a **stronger** LSU at *C* arrives (higher fork-choice rank), the node **purges and replays** via the same reconcile path (`try_reorg_stronger_lsu_at_applied_cycle`), not “first applied wins.”

### Pruned validators

Do not treat a stronger certified LSU as **canonical applied head** until the **prefix** is verifiable (local metadata, P2P range fetch, or checkpoint). Observed alternates may be indexed via `cycle_by_lsu_hash:*` without overwriting `consensus:lsu:{C}`.

### RPC / indexer semantics

| Key / view | Role |
|------------|------|
| `consensus:lsu:{C}`, `consensus:lsu_hash:{C}` | **Canonical** LSU at cycle *C* (fork-choice winner) |
| `cycle_by_lsu_hash:{hash}` | **Observed** index (any hash seen; not necessarily canonical) |
| `consensus:certified_lsu_hash:{C}` | First verified certified hash recorded at *C* (equivocation detection) |

Application-facing APIs should expose **canonical** head by default; debug/explorer surfaces may list **observed** forks with labels.

### Follower reconciliation

On `prev_root` mismatch with a **certified** (or quorum, on testnet) LSU:

- Roll back via bounded replay (`CATALYST_MAX_REPLAY_CYCLES`, snapshot on failure).
- Optional prefetch (`CATALYST_RECONCILE_PREFETCH_MS`) before aborting when metadata is missing.

### Safety

Two **conflicting** certified LSUs at the same cycle indicate **equivocation or verifier failure**; production **stalls** rather than silently picking a winner.

## Relationship to implementation

- **Certified apply:** `verify_lsu_finality_for_apply`, `require_lsu_finality` default **true** in `NodeConfig`.
- **Fork choice:** `fork_choice_apply_gate`, `persist_lsu_history`, P2P LSU handlers.
- **Replay:** `try_reconcile_fork_from_quorum_lsu`, `try_reorg_stronger_lsu_at_applied_cycle`.

## Testnet deployment gate

- Automated multi-node tests for competing LSUs, certified vs inferred, and reorg.
- Testnet verifies re-convergence without manual snapshot for defined fault scenarios.

## References (internal)

- `docs/consensus-implementation-completion-checklist.md`
- `docs/adr/0001-lsu-finality-certificate.md`
- `docs/protocol-params.md`
- `docs/node-operator-guide.md`

---

*Document status: **normative policy** (agreed 2026-05-16). Implementation ongoing for cross-cycle checkpoint sync and full integration test matrix.*
