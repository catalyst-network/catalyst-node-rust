# catalyst-testnet genesis reset — 2026-07-18 (explorer/indexer action required)

Fourth reset in four days. Same pattern as `docs/explorer-genesis-reset-2026-07-17.md`: **genesis
hash unchanged**, only re-indexing from a new starting cycle is needed, no chain-identity update.

## TL;DR

`applied_cycle` at or above **`89219239`** is current. Anything indexed before that belongs to a
prior incarnation and should not be merged with new data.

## What happened

Two distinct issues today, on top of the 2026-07-17 fixes:

1. **The 2026-07-17 bug recurred (commit `78275fa`).** The `persist_lsu_history` fix from
   2026-07-17 (`0900a3a`) added a check-then-write guard — skip overwriting a node's own
   `consensus:lsu_state_root:{cycle}` if it already has a settled `(lsu_hash, state_root)` pair for
   that cycle — but neither that check-and-write nor the self-produced apply's own unconditional
   write of the same keys held any lock. The guard's "already settled?" read was therefore not
   atomic with respect to the concurrent self-apply write it existed to defer to: a gossip relay's
   `persist_lsu_history` call could read "not yet settled" a moment before the self-apply's write
   landed, then write its own (unsigned, possibly wrong) claim a moment after, silently clobbering
   the correct value. This reproduced the exact 2026-07-17 fleet-wide false-positive halt again, on
   the already-fixed binary, the next day. Both write sites now serialize on the existing
   `lsu_apply_lock`, verified with a deterministic concurrency test.

2. **A separate, real divergence on `asia`.** After repairing the 2026-07-18 bookkeeping corruption
   and letting the fleet catch up, `eu`/`us`/`rpc` converged cleanly, but `asia` computed a
   genuinely different `state_root` at the very next cycle it processed — a persistent, real
   execution divergence, not a bookkeeping issue (its own circuit breaker correctly tripped and it
   correctly withheld consensus participation this time, rather than poisoning the other nodes —
   both of today's fixes worked as intended here). This echoes the still-unexplained `asia`-specific
   divergence pattern first flagged in `docs/explorer-genesis-reset-2026-07-16.md`; root cause not
   yet identified. With three nodes (`eu`/`us`/`rpc`) confirmed mutually consistent, a targeted
   `db-restore` of just `asia` from a healthy peer was available and offered, but a full fleet reset
   was chosen instead.

Post-reset verification (three lockstep samples, all four nodes including `asia`, plus continued
monitoring) shows zero errors and identical `applied_state_root` across all four nodes.

**Open follow-up:** why `asia` specifically keeps producing a different result from the same LSU
recipe is still not understood. If it recurs after this reset, that's the next thing to dig into —
possibly an environment/hardware difference on that host rather than a code bug, since the other
three nodes (including `rpc`, running different code paths as a non-validator) have never shown
this pattern.

## What the explorer/indexer needs to do

Same as last time: no chain-identity update, just wipe and re-index from cycle `89219239` forward.

## Endpoints

Unchanged — see `docs/testnet-handoff-catalyst-testnet.md`.
