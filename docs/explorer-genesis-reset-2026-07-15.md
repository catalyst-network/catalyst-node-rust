# catalyst-testnet genesis reset — 2026-07-15 (explorer/indexer action required)

Hand this file directly to whoever/whatever operates the block explorer at
https://explorer.catalystnet.org/. It explains a second genesis reset since the 2026-07-12 one
(`docs/explorer-genesis-reset-2026-07-12.md`) and what's different about this one.

## TL;DR

The network is **up and in consensus** again after being frozen for ~2 hours. Unlike the 2026-07-12
reset, **the genesis hash has NOT changed** — chain-identity checks (`chain_id`/`network_id`/
`genesis_hash`) will still pass against your existing configuration. What changed is the **history**:
all balances/transactions from before this reset are gone, and the chain restarted from a fresh
genesis partway through the cycle sequence. Treat everything at or after cycle `89205458` as the
current chain; anything indexed before that cycle belongs to a state that no longer exists and
should not be merged with new data.

## Live proof (verify yourself)

```bash
curl -s -X POST https://testnet-eu-rpc.catalystnet.org -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_head","params":[]}'
```

`applied_cycle` should be at or above `89205458` and increasing by roughly 1 every ~20s.

## What happened

Three validators (`eu`, `us`, `asia`) each independently computed a **different `state_root`** for
the same cycle (`89204746`) after their local reconcile logic kicked in — a genuine 3-way state
fork, not a connectivity issue. Each node's circuit breaker correctly detected the unrecoverable
divergence and halted (this is working-as-designed safety behavior, not a new bug), which is why the
network stopped producing anything from **05:55:43 UTC** onward — roughly 2 hours before this was
first noticed.

Root cause: `crates/catalyst-storage/src/snapshot.rs`'s `clear_database()` — called at the start of
every checkpoint restore — was a stub that only logged a warning and deleted nothing. Every
"restore" was therefore an additive overlay onto whatever was currently live, not a real
point-in-time rollback, silently contaminating state on every reconcile. This is very likely the
root cause behind prior forking incidents too, not just this one. Fixed in commit `0beeea4`, with a
new regression test (`load_snapshot_removes_post_checkpoint_writes`) — there was previously zero
test coverage of the restore path.

Given no checkpoint restore that ever ran in production before this fix could be trusted, the
existing chain state across the fleet couldn't be trusted either, so a coordinated fresh-genesis
reset (`scripts/catalyst_fleet_reset.sh`, non-destructive rename mode — old data preserved under
`data.reset.<timestamp>` on each host, not deleted) was run across all 4 nodes (`eu`/`us`/`asia`/
`rpc`) once all were rebuilt on the fixed binary.

## What else changed in this deploy

- **ADR 0002 state-root finality is now enforced** (`require_state_root_finality = true` on all 4
  nodes) — the apply path now requires a BFT-certified `LsuStateRootCertificateV1`, not just a
  peer's claimed root, closing the "recipe agreed, result diverges" gap that state-root forks
  exploit.
- **Discord alerting** on circuit-breaker trips (`crates/catalyst-cli/src/alerting.rs`) — an operator
  now gets paged the moment a node halts, instead of the explorer flatlining silently for hours
  before anyone notices, as happened this time.

## What the explorer/indexer needs to do

1. **No chain-identity update needed** — `chain_id`, `network_id`, and `genesis_hash` are all
   unchanged from the 2026-07-12 values (genesis hash is deterministic over those fields plus the
   fixed faucet config, and none of them changed).
2. **Wipe and re-index from cycle `89205458` forward.** Any indexed block/transaction/balance data
   for cycles between the last known-good cycle (`89204746`) and the reset point belongs to a state
   that was never canonical (it was mid-fork when the network froze) and must not be merged with
   post-reset data.
3. Re-verify connectivity against the same RPC endpoints as before — they did not change.

## Endpoints (unchanged)

- RPC: `https://testnet-eu-rpc.catalystnet.org` (public, HTTPS) or `http://45.32.177.248:8545`
  (direct; may be IP-restricted).
- Full RPC method reference: `docs/testnet-handoff-catalyst-testnet.md`, `docs/explorer-handoff.md`.

## If something still looks wrong

Re-run the live-proof command above. If `applied_cycle` is advancing and at/above `89205458`, the
network is healthy and any remaining issue is explorer-side. If it's *not* advancing, stop and flag
it — that would be a new, different problem from the one this document explains.
