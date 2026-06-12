# Consensus wall clock, cycles, and time sync

Validator nodes derive the **logical ledger cycle** from **wall-clock time** and a configured **cycle duration**. This note documents behavior, risks, and operator requirements so deployments stay aligned across regions.

## How `cycle` is computed

In `catalyst-cli` the consensus loop uses:

```text
cycle = current_timestamp_ms() / cycle_ms
```

where `cycle_ms = consensus.cycle_duration_seconds × 1000` from node config (see also the pure helper `ledger_cycle_index` in `crates/catalyst-cli/src/consensus_limits.rs`, covered by unit tests).

Implications:

- All validators that share **chain parameters** and **sufficiently synchronized clocks** advance the same `cycle` index at the same wall times (modulo startup alignment and the initial wait-to-boundary logic).
- **Large skew** between validators can place two honest nodes on **adjacent cycle numbers** at the same real instant, which harms batch delivery, producer selection, and quorum overlap until clocks realign.

## Depth-gate: never produce a wall-clock cycle while behind (safety)

Because `cycle` is wall-clock-derived but the canonical chain links by **`prev_root`** (and cycle numbers may legitimately skip when a wall-clock slot produces no quorum), a node that has **fallen behind** on *applying* the canonical chain must not mint or vote on the *current* wall-clock cycle. If it did, it would chain a fresh LSU onto its **stale applied root** and fork itself off canon. The apply head is keyed by cycle number, so once a node's `consensus:last_applied_cycle` advances ahead of its real chain position it rejects every subsequent canonical LSU (`prev_root mismatch`) and **freezes permanently** — this was the root cause of the recurring multi-day testnet freezes.

The consensus loop therefore **defers production** whenever the highest network head observed over gossip is ahead of the local applied head:

```text
defer if  applied != 0  &&  observed_head_cycle > applied_cycle
```

This is **skip-safe**: when caught up (`observed_head ≤ applied`) the node produces normally, even across legitimately skipped cycle numbers; it only steps aside while it is genuinely behind, letting the concurrent catch-up / backfill path advance the applied head first. See `should_defer_production_when_behind` and the `Deferring consensus cycle …` log line in `node.rs`. Metric: `consensus_cycle_deferred_behind_total`.

### Defer-while-behind + ≥1-behind catch-up (root cause of the Jun-2026 freezes)

The depth-gate must defer **without ever force-producing**, paired with a catch-up that triggers the moment we are behind:

1. **Catch-up at ≥1 behind:** the follower range-fetch triggers on `observed > head` (not `observed > head + 1`). A node exactly one cycle behind immediately requests + forward-applies the missing certified LSU (`try_advance_applied_head_from_storage`) and rejoins the quorum. Previously the gate deferred at ≥1 behind but the fetch only fired at ≥2 behind — a one-cycle-behind node was wedged (gated **and** not fetching), which deadlocked a 3-validator network with one node down (`Insufficient data collected: got 1, required 2`).
2. **Never release the gate to "preserve liveness."** An earlier attempt bounded the defer streak and, once exhausted, *resumed producing* to break the deadlock. That was a mistake: producing while behind mints a fresh wall-clock cycle onto the node's **stale applied root**, silently forking it off canon (observed on the testnet — a lagging validator forked instead of catching up). Deferring indefinitely is the safe choice: a behind-but-canonical node stays canonical and converges via forward catch-up; the worst case is a frozen-but-intact node (operator-recoverable), never a corrupt fork. With any healthy quorum the rest of the fleet keeps advancing while the laggard catches up.

> Self-heal scope: a behind node whose head root is still **canonical** converges automatically via the ≥1-behind forward catch-up. A node that is already on a **divergent root** (true fork, e.g. from an ill-timed restart) cannot forward-apply (its `prev_root` never matches) and is not auto-healed in place — recover it operationally by restoring a snapshot from a healthy peer or a coordinated reset (see `docs/automated-ops.md`). A general forked-node state-resync (replay canonical history from the chain's genesis cycle, or load a peer checkpoint) is the tracked follow-up; the wall-clock cycle numbering being decoupled from chain height is the underlying issue it must address.

## NTP / chrony (required for multi-region)

**Recommendation:** run `chrony` or `systemd-timesyncd` (or cloud provider time sync) on every validator host. Target **&lt; 100–250 ms** offset to upstream stratum sources for WAN testnets; stricter is better.

If a host **sleeps** (laptop), **hibernates**, or **live-migrates** a VM, its cycle index can **jump forward** many steps. After resume, the node may be **ahead** of the chain head it has applied; catch-up paths (LSU gossip, range requests) must bring state forward. Do not assume “one cycle per ticker wake” after long suspend.

## Leap seconds and clock steps

Most production systems use **smoothed** leap handling (e.g. leap smear) or a **step** of several hundred milliseconds. A **backward** step of system time is rare and hazardous for any wall-clock–driven protocol:

- If time moves **backward**, `cycle` may **repeat** or appear to go “back”; local state must not assume strict monotonicity of `(cycle)` alone without comparing to stored `consensus:last_applied_cycle` and `state_root`.

Operators should treat **unexpected time jumps** like partial outages: inspect logs for `prev_root mismatch`, `TX_BATCH_MISS_FATAL`, and reconcile/prefetch messages.

## Initial alignment

The node sleeps until the next **epoch boundary** modulo `cycle_ms` before starting the interval ticker, so peers begin cycles together after bootstrap (see consensus loop warmup in `node.rs`).

## Related environment knobs

Transaction batch delivery and fork reconcile use additional time budgets (see [`protocol-params.md`](./protocol-params.md)):

- `CATALYST_TX_BATCH_WAIT_BUDGET_MS`
- `CATALYST_RECONCILE_PREFETCH_MS`
- `CATALYST_P2P_TX_BATCH_MAX_CYCLE_SLACK` — tight ±window vs wall `cycle` for tx-batch gossip ingress.
- `CATALYST_P2P_LSU_MAX_WALL_LEAD` — maximum **future** `cycle` (ahead of wall index) accepted for LSU-related P2P payloads; historical cycles behind the wall index are not limited by this knob.

These are independent of NTP but only work well when clocks and P2P connectivity are healthy.

## References (code)

- `crates/catalyst-cli/src/node.rs` — consensus loop, `current_timestamp_ms() / cycle_ms`, batch wait.
- [`consensus-quorum-and-fork-choice.md`](./consensus-quorum-and-fork-choice.md) — fork choice and replay intent.
