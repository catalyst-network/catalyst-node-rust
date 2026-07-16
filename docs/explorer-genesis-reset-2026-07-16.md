# catalyst-testnet genesis reset — 2026-07-16 (explorer/indexer action required)

Second reset in two days. Same pattern as `docs/explorer-genesis-reset-2026-07-15.md`: **genesis hash
unchanged**, only re-indexing from a new starting cycle is needed, no chain-identity update.

## TL;DR

`applied_cycle` at or above **`89210491`** is current. Anything indexed before that belongs to a
prior incarnation and should not be merged with new data.

## What happened

The 2026-07-15 reset fixed the `clear_database` bug and a memory leak, and shipped a new detector
(`scan_for_self_produced_state_root_divergence`, commit `07e08de`) that cross-checks a node's own
applied `state_root` against the network's ADR 0002 certified root for the same `lsu_hash`. On
deploying that detector, it correctly caught **live corruption in progress**: `eu` and `us` had been
silently accumulating wrong historical `state_root` records via a recurring
`"Quorum fork reconcile failed, restoring snapshot: replay prev root mismatch"` cycle — present
continuously in the background even while their *current* head stayed in perfect agreement, which is
why it went unnoticed until a detector existed that checks history, not just the live head.

Root-caused to the `asia` validator: it was left running on pre-fix code (deliberately, mid-incident)
still self-producing from already-diverged state and continuously gossiping bad LSUs/attestations.
Stopping `asia`'s service immediately stopped all new reconcile-failure occurrences on `eu`/`us`/`rpc`
(confirmed: zero in the following minutes, versus one every ~3 minutes before). The exact mechanism
by which a *failed* reconcile attempt left behind a wrong historical `state_root` (rather than
harmlessly no-op'ing) is not yet fully understood — worth a follow-up investigation — but removing
the trigger (asia) stopped it from recurring.

With no node confirmed fully clean and the corruption mechanism not yet pinned down, the fleet was
reset fresh (`scripts/catalyst_fleet_reset.sh`, non-destructive rename mode) rather than attempting a
targeted repair. All 4 nodes were already on the fixed binary before the reset, so the new detector
is live from cycle 1 of this chain onward — any recurrence will be caught immediately rather than
silently accumulating again.

## What the explorer/indexer needs to do

Same as last time: no chain-identity update, just wipe and re-index from cycle `89210491` forward.

## Endpoints

Unchanged — see `docs/testnet-handoff-catalyst-testnet.md`.
