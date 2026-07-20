# catalyst-testnet genesis reset — 2026-07-20 (explorer/indexer action required)

Sixth reset in six days. Unlike every prior reset in this run, **the root-cause bug was found and
fixed today**, not just worked around.

## TL;DR

`applied_cycle` at or above **`89227690`** is current (post-reset). Genesis hash **did not change**
(same `chain_id`/`network_id`/faucet config); only re-indexing from the new starting cycle is
needed.

## What happened

Two distinct bugs, found and fixed in this order:

1. **A third, previously-unpatched call site for the "unsigned peer gossip clobbers a node's own
   correct `state_root` bookkeeping" bug class** (commits `05c8690`, `727368a`, `367f50d`). The
   2026-07-17/07-18 incidents fixed this in `persist_lsu_history` and the self-produced-apply write
   site (`78275fa`), both guarded by an "already settled, same recipe" check under a shared lock.
   A third site — the "Synced LSU via CID" handler for `LsuCidGossip` messages, specifically the
   branch taken when a cycle was `already_applied` at entry — had no such guard at all: it
   unconditionally overwrote `consensus:lsu_state_root:{cycle}` with the peer's unverified claim,
   with no lock and no already-settled check. `eu` and `us` had both correctly self-produced and
   BFT-attested `state_root=8a2b2c04...` for cycle `89224040`; this bug silently overwrote both
   nodes' own bookkeeping with a different, unsigned peer claim (`2876a13a...`), which is what
   `scan_for_self_produced_state_root_divergence` (itself hardened today, `05c8690`, to actually
   call `verify_lsu_state_root_certificate` before trusting a fetched cert — a related but separate
   gap) then correctly flagged as a mismatch and circuit-broke on two otherwise-healthy nodes. A
   first attempt at this fix (`727368a`) had a live bug of its own — the already-settled check read
   `consensus:lsu_hash:{cycle}` *after* the same code path's own unconditional write of that same
   key, making the check vacuously true against itself. Confirmed via live observation (`asia`
   accumulated 14 further corrupted cycles in the ~20 minutes after `727368a` before the ordering
   fix, `367f50d`, actually took effect) and corrected the same day.
2. **A new, more severe live divergence surfaced while validating the fix**: after repairing known
   bad cycles via `repair-self-produced-state-root` and restoring `asia`'s data directory
   byte-for-byte from `eu` (to eliminate `asia`'s long-standing, still-unexplained execution
   divergence as a variable), **all four nodes began computing mutually different `state_root`s
   from the identical `lsu_hash` on every single cycle** — not the previous pattern of one outlier
   agreeing with itself, but zero pairwise agreement at all. No `LsuStateRootCertificateV1` ever
   reached quorum during this window (nothing to sign onto), so the divergence scan never fired,
   and cycles kept producing on top of an ever-deepening fork with no circuit breaker engaged.
   **Root cause of this specific 3-way event is not identified.** It was not reproduced by the
   fresh genesis reset that followed (three post-reset lockstep samples plus ~4 minutes of
   continued monitoring show zero errors and byte-identical `applied_state_root` across all four
   nodes at every cycle), so it was not carried forward — but it was **not explained**, and it
   appeared specifically coincident with the `asia` restore-from-peer operation, not with any of the
   ordinary restarts that happened earlier in the same session. See **Open follow-up** below.

## Open follow-up (important — do not treat this reset as closing the investigation)

Two separate, still-unresolved threads, both predating today and not fixed by today's work:

- The long-standing "`asia` computes a different `state_root` than `eu`/`us` from the same recipe"
  pattern first flagged 2026-07-16, recurring 2026-07-17 and 2026-07-18. Today's fix closes the
  *bookkeeping-corruption* failure mode that made this pattern catastrophic (fleet-wide false-positive
  halts), but does not explain why `asia` diverges in the first place. Still suspected:
  environment/hardware difference on that host, per the 2026-07-18 note.
- **New today**: a brief but severe 4-way mutual divergence (see point 2 above), observed only once,
  immediately following a targeted `db-restore` of `asia` from `eu`'s backup while `eu`/`us` were
  live and producing. Whether this was triggered by the restore itself, by `asia` rejoining as a
  fully-agreeing third voter for the first time in this incident (changing which code path handles
  quorum), or something else entirely, is unknown. If a similar all-nodes-disagree pattern recurs,
  capture `consensus:lsu:*` / attestation logs from all nodes *before* resetting — today's reset
  destroyed the only evidence of it.

## What the explorer/indexer needs to do

Same as every prior reset: no chain-identity update, just wipe and re-index from cycle `89227690`
forward.

## Endpoints

Unchanged — see `docs/testnet-handoff-catalyst-testnet.md`.
