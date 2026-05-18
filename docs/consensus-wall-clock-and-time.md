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
