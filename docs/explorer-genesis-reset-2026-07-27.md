# catalyst-testnet genesis reset — 2026-07-27 (explorer/indexer action required)

Root cause not yet found (four rounds of live diagnostics deployed instead — see below). Reset
restores liveness only. `genesis_hash` **did not change** (unchanged since the 2026-07-23 reset).

## TL;DR

`applied_cycle` at or above **`89256702`** is current (post-reset). Only re-indexing from the new
starting cycle is needed — no chain-identity change, no `state_root` formula change.

## What happened

A `state_root` divergence among the three validators (`eu`/`us`/`asia`) began at cycle
`89253580-81` — pinned precisely by bisecting retained diagnostic logs, not by catching it live.
Unlike the 2026-07-22/07-23/07-25 incidents, this one is *not* a repeat of the "wrong cycle skip"
bug class: `asia`'s own self-produced-`state_root`-mismatch detector fired at that exact cycle, and
its cycle-binding `root_counter` fingerprint matched `eu`'s exactly on every single subsequent
cycle. The divergence is isolated entirely to account **balances**, and is not a one-time fork —
`asia`'s balance state kept falling progressively further behind (measured via diagnostic
fingerprints as a growing lag: ~0 cycles behind up to 89251478, ~10 by 89253580, ~28 by 89253610),
consistent with `asia` under-accumulating reward credit over time rather than computing one wrong
value and diverging cleanly. The growing-lag window lines up with a recurring cluster of `asia`
refusing its own self-produced apply (circuit breaker open) interleaved with declining
peer-synced LSUs (`prev_root` mismatch) — the exact internal mechanism is still not pinned to a
line of code.

By the time this was investigated, the fork had compounded to a full 4-way `state_root` split
across all four nodes (same `applied_cycle`/`applied_lsu_hash`, four different roots), and `rpc`
had separately tripped its own reconcile circuit breaker.

## Diagnostics shipped this investigation (all deployed, all still live post-reset)

No fix has shipped for this bug yet — root-causing it live turned out to need instrumentation the
codebase didn't have. Four diagnostic-only commits were deployed in sequence, all still active on
this rebuilt fleet so the *next* occurrence (if any) is fully traceable without another
investigation cycle:

1. **`e7d21f6`** — per-key-prefix `accounts` fingerprint logged on every apply (`bal`, `nonce`,
   `workers`, `wfs`, `wlr`, `feecred_bal`, `fcd`, `evm_*`, `root_counter`). This is what let the
   divergence be isolated to the `bal:` category specifically.
2. **`16eacb1`** — explicit `APPLY_REVERTED` logging at all three snapshot-revert-on-mismatch
   sites, so a reverted (non-committed) fingerprint line can be told apart from a genuinely
   committed one.
3. **`c0a6d88`** — `distribute_waiting_pool_rewards_and_fee_credits` logs its computed
   `waiting_pool`/`n_eligible`/`per`/`rem` and pubkey-list fingerprints every cycle. This
   incidentally proved the waiting-pool path is a **permanent no-op in the current worker
   configuration** (all 3 registered workers are always the 3 rotating producers, so it's always
   `n_eligible=0`, on all four nodes identically) — ruling it out as this bug's mechanism.
4. **`bf27f31`** — `compensation_entries` (producer reward) application logs
   `count`/`applied_count`/`total_amount`/`entries_hash` from both apply paths on every cycle.

**Next occurrence playbook**: as soon as two nodes' `catalyst_head` shows matching
`applied_cycle`/`applied_lsu_hash` but different `applied_state_root`, pull all four logs'
`compensation_entries` and `accounts fingerprint` lines for that exact cycle across nodes before
doing anything else. If `compensation_entries` matches everywhere, the deficit predates that cycle
(check one cycle earlier); if it doesn't match, that's the bug, directly.

## Verification

Post-reset: single genesis hash
(`0x32bceec02712a1184f788ce4aebf3472e98be2f09ffd5e356148e13a01f7ea9d`, unchanged), lockstep
verified by the reset script and independently via direct RPC — all four nodes byte-identical at
cycle `89256702`.

## What the explorer/indexer needs to do

Same as every prior reset: no chain-identity update, just wipe and re-index from cycle `89256698`
forward (first post-reset cycle observed in lockstep).

## Endpoints

Unchanged — see `docs/testnet-handoff-catalyst-testnet.md`.
