# catalyst-testnet genesis reset — 2026-07-28 (explorer/indexer action required)

Root cause found and fixed this time (see below). `genesis_hash` **did not change** (unchanged
since the 2026-07-23 reset).

## TL;DR

`applied_cycle` at or above **`89262084`** is current (post-reset). Only re-indexing from the new
starting cycle is needed — no chain-identity change, no `state_root` formula change.

## What happened

Continuation of the investigation recorded in `docs/explorer-genesis-reset-2026-07-27.md`. That
reset restored liveness but did not find the root cause; the accounts-fingerprint,
`APPLY_REVERTED`, `waiting_pool_reward`, and `compensation_entries` diagnostics deployed the same
day (commits `e7d21f6`, `16eacb1`, `c0a6d88`, `bf27f31`) were left running to catch the next
occurrence live. They did, within hours.

`asia` fell ~10 cycles behind a real, peer-served range — `eu`/`us`/`rpc` all had and correctly
applied those cycles. After `stale_observed_head_reset_ms()`'s 60-second no-progress budget
elapsed, that liveness backstop reset `asia`'s `observed_head_cycle` down to its own applied head
and let it resume production — **without ever backfilling the abandoned range**. `asia` silently
dropped ~10 real cycles' worth of `compensation_entries` credit from its own balance history
forever, with no error raised about the gap itself (only a downstream `state_root` mismatch,
cycles later, once the deficit was already permanent). Confirmed directly:
`compensation_entries` for the origin cycle logged byte-identical on all four nodes (same
`entries_hash`/`total_amount`) — proving the one cycle's own mutation was correct; the divergence
was entirely in the *starting* balance `asia` applied it on top of.

This backstop's original design (from an earlier liveness fix, 2026-07-01..04) assumed a
"phantom/unreachable head" — a single bad or reordered gossip message, safe to forgive because
nothing real is lost. That premise didn't hold here: the head was real and reachable, `asia`'s own
fetch pipeline was just too slow, and the backstop paved over that instead of surfacing it.

## Fix shipped in this reset (commit `2a1810a`)

**`stale_observed_head_reset_ms()`'s reset is now capped by gap size, not just elapsed time.** A
new `stale_observed_head_max_forgivable_gap()` (default 3 cycles) bounds how large a gap the
backstop may silently abandon. Above that cap, it now refuses to reset — regardless of elapsed
time or peer confirmation of just the next cycle — and stays deferred instead, logging a loud,
repeating `error!` and a new counter
(`consensus_observed_head_stale_reset_refused_gap_too_large_total`) so the stall is visible and
operator-actionable rather than silent. The existing 2-second backfill loop keeps retrying
regardless, so a node that can genuinely catch up still self-heals — this only closes the
silent-data-loss path.

## Verification

Post-reset: single genesis hash
(`0x32bceec02712a1184f788ce4aebf3472e98be2f09ffd5e356148e13a01f7ea9d`, unchanged), lockstep
verified by the reset script and independently via direct RPC — all four nodes byte-identical at
cycle `89262084`.

## Still open

This fix stops the specific silent-skip mechanism from recurring; it does not address *why*
`asia`'s own LSU range-fetch pipeline is slow/flaky enough to fall behind in the first place. If
`asia` (or any node) falls behind again and this time stays correctly deferred (loud error, no
silent divergence) rather than resuming with a gap, that is the fetch-pipeline question — a
liveness/performance issue, not a correctness one — worth investigating separately. All four
diagnostic-logging commits from the 2026-07-27 investigation remain deployed on this rebuilt
fleet.

## What the explorer/indexer needs to do

Same as every prior reset: no chain-identity update, just wipe and re-index from cycle `89262084`
forward.

## Endpoints

Unchanged — see `docs/testnet-handoff-catalyst-testnet.md`.
