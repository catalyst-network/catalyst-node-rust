# catalyst-testnet genesis reset — 2026-08-02 (explorer/indexer action required)

Root cause found and fixed. `genesis_hash` **did not change** (unchanged since the 2026-07-23 reset).

## TL;DR

`applied_cycle` at or above **`89284014`** is current (post-reset). Only re-indexing from the new
starting cycle is needed — no chain-identity change, no `state_root` formula change.

## What happened

The recurring "same recipe, different `state_root`" `bal`-only divergence (previously narrowed but
not closed — see `docs/explorer-genesis-reset-2026-07-27.md` / `-2026-07-28.md`) recurred, this
time on `us` (45.76.21.153) rather than `asia`. `eu` and `us` kept producing and certifying
normally together (identical `lsu_hash`, identical `compensation_entries`) while silently disagreeing
on the resulting `bal` fingerprint and `state_root`; `asia`/`rpc` were correctly deferred (the
2026-07-27 gap-cap fix working as designed — safely stuck, not corrupting).

Unlike prior occurrences, this was closed to an exact, quantified, code-level cause: live balance
queries (`catalyst-cli balance <pubkey>`) against all three validator identities showed the *same*
deficit on `us` versus `eu` for every producer — about 1174 cycles' worth of missing
per-cycle compensation. `us`'s journal showed four separate "Reconcile circuit-broken" trips over
the prior 40 hours with no restart in between, meaning `us` kept advancing instead of staying
stuck like a circuit-broken node should.

Root cause: `try_apply_stored_lsu_at_cycle` (the background catch-up loop that runs independently
of the reconcile machinery, roughly every 2 seconds) had two gaps —

1. Its prev-root continuity check fell back to comparing the local state root against *itself*
   whenever `consensus:lsu_state_root:{cycle-1}` metadata wasn't recorded, silently disabling the
   only check that would have caught applying onto already-wrong state.
2. It was never gated by the reconcile circuit breaker at all (only self-produced apply was), so a
   node stuck on an unresolved gap could still creep forward one cycle at a time through this path,
   which also had the side effect of silently invalidating the breaker's own stuck-state tracking.

## Fix shipped in this reset (commit `a4f5397`)

- The prev-root check now fails closed when no canonical root is recorded for the previous cycle,
  except for genesis bootstrap (no prior applied state, nothing recorded yet — the one legitimate
  case).
- The same catch-up path now also refuses to advance once the reconcile circuit breaker is open for
  the current applied head, matching the guarantee `should_block_self_produced_apply` already gave
  self-produced apply.
- Two new regression tests added; full suite 100/100 passing.

## Verification

Post-reset: single genesis hash
(`0x32bceec02712a1184f788ce4aebf3472e98be2f09ffd5e356148e13a01f7ea9d`, unchanged), lockstep
verified by the reset script and independently via direct RPC and live per-producer balance
queries — all four nodes byte-identical at cycle `89284014` and beyond, clean logs, no errors.

## Still open

This closes a demonstrated, concrete hole, not just a hardening guess, and fully explains the
observed deficit mechanism. It is not proven to be the *only* path into this bug class — the
2026-07 architectural gap remains true in general (BFT certifies the LSU recipe, not the execution
result — see `docs/consensus-reliability-review-2026-07.md`). If a `bal`-only divergence recurs,
check first whether it went through a different apply path than `try_apply_stored_lsu_at_cycle`
before assuming this fix regressed.

## What the explorer/indexer needs to do

Same as every prior reset: no chain-identity update, just wipe and re-index from cycle `89284014`
forward.

## Endpoints

Unchanged — see `docs/testnet-handoff-catalyst-testnet.md`.
