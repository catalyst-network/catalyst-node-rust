# catalyst-testnet genesis reset — 2026-07-17 (explorer/indexer action required)

Third reset in three days. Same pattern as `docs/explorer-genesis-reset-2026-07-16.md`: **genesis
hash unchanged**, only re-indexing from a new starting cycle is needed, no chain-identity update.

## TL;DR

`applied_cycle` at or above **`89215966`** is current. Anything indexed before that belongs to a
prior incarnation and should not be merged with new data.

## What happened

Two distinct, previously-undiscovered bugs, both now fixed:

1. **`persist_lsu_history` overwriting self-produced state (commit `0900a3a`).** On receipt of a
   peer's `LsuCidGossip` for a cycle this node had *already* self-applied and certified, the node
   unconditionally overwrote its own `consensus:lsu_state_root:{cycle}` bookkeeping record with
   whatever (unsigned, unverified) root the peer's message claimed — even when that peer had itself
   genuinely diverged. This surfaced as a fleet-wide false-positive halt: `asia` had a real,
   independent state-root divergence at cycle `89213234`; its bad gossip then silently overwrote
   `eu`'s and `us`'s bookkeeping (though not their real applied state) to match, so all three tripped
   the ADR 0002 self-produced-divergence circuit breaker together, freezing the network for ~17
   hours before this was diagnosed.

2. **Circuit-broken nodes still voting (commit `799c089`).** The reconcile circuit breaker
   (`should_block_self_produced_apply`) was only checked *after* a node completed the full
   four-phase Construction/vote round, gating just its own local apply — not its participation in
   the round itself. A circuit-broken node still broadcast a vote that counted toward *other*
   nodes' BFT quorum before discarding the result for itself. During the rolling (one-node-at-a-time)
   recovery from bug #1, this let freshly-repaired validators complete several cycles using votes
   from peers that were still circuit-broken and never applied those same cycles — permanently
   crediting the fresh nodes with producer compensation the broken peers never received. The result
   was a static, silent `accounts`-root divergence across `eu`/`us`/`asia` (three different
   compensation-balance totals, each a whole-cycle multiple apart) with no error logged anywhere,
   since nothing was technically wrong about any single node's own application — only their
   disagreement on which cycles had happened at all.

Bug #2's fix was verified with a real 3-node harness
(`scripts/consensus-circuit-breaker-participation-e2e.sh`) that reproduces the exact failure mode
and confirms red (bug present, isolated healthy node falsely advances) / green (fix present, it
correctly stalls) behavior.

Since bug #1's overwrite meant no single node's on-disk data could be trusted as "the" healthy
copy (a targeted db-restore needs a confirmed-clean source), and bug #2's divergence was a real,
accumulated difference in account balances rather than a metadata-only issue, the fleet was reset
fresh (`scripts/catalyst_fleet_reset.sh`, non-destructive rename mode) rather than attempting a
targeted repair. All 4 nodes were on both fixes before the reset; post-reset verification (three
lockstep samples plus continued monitoring) shows zero errors and identical `applied_state_root`
across all four nodes.

## What the explorer/indexer needs to do

Same as last time: no chain-identity update, just wipe and re-index from cycle `89215966` forward.

## Endpoints

Unchanged — see `docs/testnet-handoff-catalyst-testnet.md`.
