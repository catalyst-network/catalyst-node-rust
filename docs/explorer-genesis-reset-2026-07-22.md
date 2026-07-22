# catalyst-testnet genesis reset — 2026-07-22 (explorer/indexer action required)

Seventh reset. Root cause found, fixed, deployed, and verified in lockstep across all four nodes
same-day.

## TL;DR

`applied_cycle` at or above **`89236345`** is current (post-reset). Genesis hash **did not
change** (same `chain_id`/`network_id`/faucet config as every reset since 2026-07-15); only
re-indexing from the new starting cycle is needed. **Important for anyone independently verifying
state roots**: the `state_root` computation itself changed in this release (see below) — a
verifier built against the pre-2026-07-22 formula will not reproduce these roots.

## What happened

Investigation (full evidence and timeline in the incident record) found a real, root-caused
mechanism — not a recurrence of any previously-fixed bug class:

`us`'s own consensus round failed to reach quorum for a single cycle (`Insufficient data
collected`) while `eu`'s identical round succeeded. The depth-gate that is supposed to stop a node
from producing past a cycle it doesn't actually have was purely reactive to background gossip
timing — it had no way to actively verify anything — so `us`'s next tick silently treated the gap
as a legitimate network-wide skip and produced past it, permanently offsetting its cycle height
from `eu`'s by one. This went undetected instead of tripping the existing `prev_root`/`state_root`
mismatch safety net because `state_root` was never a function of the cycle number itself, only of
resulting account balances — and the skipped cycle happened to be an empty-tx-batch cycle with a
flat, cycle-invariant producer reward, so the wrongly-shifted node's `state_root` came out
byte-identical to the correct one. From that point, every subsequent cycle computed differently on
`us` than `eu`, cascading over the following ~30 hours into full mutual divergence across all four
nodes — the same class of event a prior reset (2026-07-20) observed once, couldn't explain, and
flagged to capture evidence for if it recurred. It recurred; evidence was captured live before this
reset, and the mechanism was fully traced from it.

## Fixes shipped in this reset (commit `cb3041a`)

1. **Depth-gate made semi-authoritative.** On a local round failure, the node now actively
   broadcasts a request for exactly that cycle and holds production of the next cycle for a bounded
   grace window (default 4s), waiting for a peer's positive confirmation before treating the gap as
   legitimate — instead of silently presuming it. Already observed firing correctly in production
   logs immediately after this reset (`"round failure: no peer confirmed holding it within 4000ms;
   treating as a legitimate network-wide skip"`), as an explicit, audited decision rather than a
   silent one.
2. **`state_root` bound to the cycle number.** A new reserved key in the account state is written
   on every applied cycle, making `state_root` unconditionally sensitive to which cycle produced
   it. This is *why* genesis needed a coordinated reset today even though `genesis_hash` itself
   (config-derived, unrelated to account state) didn't change — every node needed to start from the
   same cycle 1 under the new formula; it isn't something that can be rolled out gradually or mixed
   with old-code peers.
3. **`CATALYST_ALLOW_LEGACY_AMBIGUOUS_EMPTY_TX_BATCH` removed entirely** (was enabled fleet-wide;
   a direct contributor to this incident — it let a follower silently proceed with an empty batch
   instead of erroring loudly when it couldn't confirm the canonical one). Already observed
   correctly refusing an ambiguous batch post-reset (`TX_BATCH_MISS_FATAL ... refuse ambiguous
   empty construction`) rather than papering over it.

## Verification

Post-reset: single genesis hash across all four nodes, three lockstep samples with byte-identical
`applied_state_root` at identical cycles, and continued agreement observed manually afterward at
cycle `89236352`. See the fleet reset script's own output for the automated verification.

## Open, still-unresolved thread

The original, separate `asia`-specific execution divergence (first flagged 2026-07-16) remains
unexplained — this reset did not investigate or claim to fix it. If `asia` alone diverges again
going forward (as distinct from the mechanism above), that is the old open question, not a
regression of today's fix.

## What the explorer/indexer needs to do

Same as every prior reset: no chain-identity update, just wipe and re-index from cycle
`89236345` forward. If your indexer independently recomputes or verifies `state_root` (rather than
trusting the RPC-reported value), it must be updated to include the new reserved cycle-counter
account key in its Merkle computation, or roots will not match from this reset forward.

## Endpoints

Unchanged — see `docs/testnet-handoff-catalyst-testnet.md`.
