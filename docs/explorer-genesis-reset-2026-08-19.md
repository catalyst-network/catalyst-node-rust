# catalyst-testnet genesis reset — 2026-08-19 (explorer/indexer action required)

Root cause found and fixed. `genesis_hash` **did not change**
(`0x32bceec02712a1184f788ce4aebf3472e98be2f09ffd5e356148e13a01f7ea9d`, same as every prior reset).

## TL;DR

`applied_cycle` at or above **`89356443`** is current (post-reset). Only re-indexing from the new
starting cycle is needed — no chain-identity change, no `state_root` formula change.

If your indexer already has data from earlier tonight (anywhere in the `~89354600`-`~89356440`
range), discard it: two resets happened tonight (~21:22 and ~08:41 UTC), and only the second one's
starting cycle above is current. Data at cycles in that range from before 08:41 UTC no longer
exists on any node.

## What happened

A fleet-wide liveness stall (not a state divergence — every node held identical `state_root` the
whole time) that began ~2026-08-18 17:38 UTC, spanning most of the night. Root-caused and fixed
across 9 commits (`1eeb8f5..34830bd`), all in this repo, none touching the vendored
`libp2p-gossipsub` dependency:

1. A codebase-wide dead logger (`catalyst_utils::logging`'s `log_info!`/`log_warn!` routed through
   a `CatalystLogger` `catalyst-cli` never initialized — silent no-op everywhere it was used).
2. Two independent dial-retry loops that both had the same bug: they gave up trying to reconnect a
   missing peer once `connected_peers >= min_peers`. With exactly 3 validators and `min_peers=2`, a
   node sitting at 2/3 connections looked "healthy enough" and would **permanently** stop trying to
   reconnect the third — confirmed live after a routine restart.
3. All consensus messages (`ProducerQuantity`/`Candidate`/`Vote`/`Output`, plus `ConsensusSync`)
   shared one gossipsub topic and send queue with high-volume routine traffic (tx relay/resync,
   state sync). Confirmed live: a `ProducerQuantity` took 40+ seconds to arrive — a `publish()` call
   returning success immediately but the message queuing behind routine traffic (head-of-line
   blocking, not loss). Fixed by giving consensus messages their own dedicated gossipsub topic.
4. Single-shot consensus broadcasts vs. the retry-until-landed pattern already used for tx-batch
   traffic elsewhere in the codebase — added periodic rebroadcast during each phase's collection
   window.
5. The rotating batch leader's tx-batch rebroadcast loop blocked its own entry into the consensus
   pipeline for ~1.6s every cycle it led — made it a background task instead.
6. The deepest fix: each of the four consensus phases' collection deadlines were relative timers
   reset fresh at the start of that phase, rather than anchored to a single fixed point. Because a
   phase's actual completion time is essentially random (whenever the pivotal peer message happens
   to land), this let cross-node phase windows drift out of alignment, compounding with each
   subsequent phase — quantities partially worked, candidates almost never did, and votes was never
   reached at all in one extended live sample. Fixed by anchoring every phase's deadline to one
   fixed pipeline-start instant, so finishing a phase early extends the *next* phase's window
   instead of starting a fresh, shorter one.

Full details: see this repo's session memory / commit messages for `1eeb8f5..34830bd`.

## Fleet reset performed

`scripts/catalyst_fleet_reset.sh --yes`, twice tonight (the first reset's fixes turned out to be
incomplete — the stall recurred ~70 minutes later; the fixes above were completed and the fleet
reset a second time). Only the **second** reset's starting point is current.

## Verification

Post-reset (second, current): single genesis hash (unchanged, see above), lockstep verified by the
reset script and independently via direct RPC on all 4 nodes (`eu`, `us`, `asia`, `rpc`) — all four
byte-identical `applied_state_root` at the same `applied_cycle`, sustained normal-speed production
confirmed afterward (not just a lucky one-off completion).

Note: the non-validator `rpc` node was inadvertently left on old code for several hours after the
validator fixes deployed (fix #3 above moved messages `rpc`'s observer path depends on onto a topic
old code didn't know to subscribe to, so it sat stuck at `applied_cycle=0` until updated) — if your
indexer points at `rpc` specifically and was still seeing no data as of ~08:40-09:05 UTC, that's
why; it's been updated and is now in sync with the validators.

## Still open

None of the above interacts with the earlier, separate, longer-running "same recipe, different
`state_root`" `bal`-only divergence class (see `docs/explorer-genesis-reset-2026-08-02.md` and
earlier) — that class was about disagreement, not stalling, and nothing in tonight's session
touched its fix (`a4f5397`, `5968afe`). If a `bal`-only divergence recurs, it's unrelated to
tonight's fixes.

## What the explorer/indexer needs to do

Same as every prior reset: no chain-identity update, just wipe and re-index from cycle `89356443`
forward. If your indexer has any cached/indexed data from the ~21:22-08:41 UTC window tonight,
discard it too — it's from the superseded first reset and no longer exists on any node.

## Endpoints

Unchanged — see `docs/testnet-handoff-catalyst-testnet.md`.
