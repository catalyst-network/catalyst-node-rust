# catalyst-testnet genesis reset — 2026-07-25 (explorer/indexer action required)

Tenth-ish reset. Root cause found, fixed, deployed, and verified in lockstep across all four
nodes same-day. `genesis_hash` **did not change** (unchanged since the 2026-07-22 reset).

## TL;DR

`applied_cycle` at or above **`89250764`** is current (post-reset). Only re-indexing from the
new starting cycle is needed — no chain-identity change, and the `state_root` computation itself
is unchanged from the 2026-07-22 reset (that reset's cycle-bound `state_root` formula stands; this
one did not touch it).

## What happened

`asia`'s reconcile circuit breaker was already open going into this incident from an older,
still-unresolved self-produced-`state_root` divergence (same long-running open thread first
flagged 2026-07-16), which degraded the live validator set from 3 to 2 (`eu`, `us`) without
anyone reducing `required_majority` to match. With zero slack left, a transient round hiccup
made `us`'s local round come up one vote short at cycle `89249662` while `eu`'s identical round
succeeded — the same *kind* of gap the 2026-07-22 fix's round-failure confirmation grace window
was built to catch. This time the fix's own timing lost the race: `eu` needed longer than usual
to reach quorum with only two live validators, and didn't finish and broadcast the cycle until
after `us`'s fixed 4-second grace window had already expired. `us` concluded "no peer confirmed
holding it" and treated a cycle `eu` had actually just produced as a legitimate network-wide skip
— permanently diverging their `state_root` from that point forward, the same "identical CID,
different final root" signature as the 2026-07-23 incident.

## Fix shipped in this reset (commit `e31d407`)

**Round-failure confirmation grace window made schedule-aware.** The grace deadline was a fixed
constant (`CATALYST_ROUND_FAILURE_CONFIRMATION_GRACE_MS`, default 4000ms) racing against however
long a peer's round actually takes to complete — with an already-degraded validator set, that race
can be lost by construction. The deadline is now the *later* of the fixed grace period and the end
of the current cycle's own wall-clock slot (`round_failure_confirmation_deadline_ms` in
`consensus_limits.rs`), since the protocol never expects a new cycle to start before then anyway.
Replaying this incident's exact timestamps through the new function confirms it would have caught
`eu`'s real completion instead of falsely confirming a skip.

## Verification

Post-reset: single genesis hash across all four nodes
(`0x32bceec02712a1184f788ce4aebf3472e98be2f09ffd5e356148e13a01f7ea9d`, unchanged from 2026-07-23),
and lockstep samples with byte-identical `applied_state_root` at identical cycles confirmed
independently via direct RPC at cycle `89250769` on all four hosts.

## Still open, not addressed by this reset

- `asia`'s underlying self-produced-`state_root` divergence (first flagged 2026-07-16) is the thing
  that removed the network's quorum slack in the first place. Not investigated or fixed this
  session — still the deepest open thread.
- A circuit-broken validator still counts toward `expected_producers`/quorum denominator without
  `required_majority` being adjusted down. Not changed this session.

## What the explorer/indexer needs to do

Same as every prior reset: no chain-identity update, just wipe and re-index from cycle
`89250764` forward.

## Endpoints

Unchanged — see `docs/testnet-handoff-catalyst-testnet.md`.
