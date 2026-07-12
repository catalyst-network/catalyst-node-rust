# catalyst-testnet genesis reset — 2026-07-12 (explorer/indexer action required)

Hand this file directly to whoever/whatever operates the block explorer at
https://explorer.catalystnet.org/. It explains why the explorer is showing the network as offline
and exactly what needs to change to bring it back.

## TL;DR

The network is **up and in consensus** — this is not a connectivity or liveness problem. The
network's **genesis hash changed** as of 2026-07-12 ~10:36 UTC. If the explorer validates chain
identity against the old genesis hash (which is the documented, correct thing for it to do — see
`docs/testnet-handoff-catalyst-testnet.md`), it will correctly conclude "this is not the chain I
was tracking" and should be updated to the new identity below, then treat this as indexing a new
chain from cycle 0 (not a reorg/continuation of its existing data).

## Live proof (verify yourself)

```bash
curl -s -X POST https://testnet-eu-rpc.catalystnet.org -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_head","params":[]}'
```

As of 2026-07-12 10:58 UTC this returned (call it again — `applied_cycle` should be higher than
this every time you run it, ~1 per 20s):

```json
{"applied_cycle":89192693,"applied_lsu_hash":"0x1616765c...","applied_state_root":"0x0d292470...","last_lsu_cid":"bafkreigndyw2dfkoi4hpirmoqthpu7fgatb7aprtg3vwzy6zsxnhf6knce"}
```

`catalyst_peerCount` on the same endpoint currently returns `3` (healthy 4-node mesh, self excluded).

## What changed

| Field | Old (until 2026-07-12) | New (since 2026-07-12 ~10:36 UTC) |
|---|---|---|
| `network_id` | `catalyst-testnet` | unchanged |
| `chain_id` | `200820092` / `0xbf8457c` | unchanged |
| `genesis_hash` | `0xeea16848e6b1d39d6b7a5e094ad9189d5382a6a4b19fb95342ef9846258fee5a` | **`0x32bceec02712a1184f788ce4aebf3472e98be2f09ffd5e356148e13a01f7ea9d`** |

**Everything before this reset is gone**: all historical blocks/cycles, all transactions, all
account balances (including any the explorer had indexed). The chain restarted at cycle 0 with a
fresh faucet-funded genesis. This is a full replacement, not a fork/reorg — there is no shared
history between the old and new chains to reconcile.

## Why this happened

Three of the network's four validators (`us`, `asia`, and the `rpc` node) had been stopped since
2026-07-04 — over a week. Cycle numbers are wall-clock-derived (~20s/cycle), and a node that's been
offline that long cannot catch up via normal replay (the reconcile path caps how many cycles it will
replay forward). The only way to bring the fleet back to a healthy, consistent state was a full
coordinated reset: stop all four, wipe data, restart together from a fresh genesis. Full technical
detail (an unrelated but related consensus bug fixed in the same session) is in
`docs/consensus-reliability-review-2026-07.md` and the `fix/consensus-observer-and-phase-timing`
branch history in this repo.

## What the explorer/indexer needs to do

1. Update its configured/expected chain identity to the **new** `genesis_hash` above (`chain_id`
   and `network_id` are unchanged, so signature/address logic doesn't need to change).
2. Treat this as a **new chain, not a reorg**: any indexed blocks/transactions/balances from before
   2026-07-12 ~10:36 UTC belong to a chain that no longer exists and should not be merged with new
   data. If your indexer supports it, this is cleanest handled as "wipe and re-index from genesis"
   rather than trying to reconcile against the old history.
3. Re-verify connectivity against the RPC endpoint(s) below (they did not change — if the explorer
   was actually failing to *connect* rather than rejecting on identity mismatch, something else is
   wrong and worth checking separately).
4. If the explorer has any hardcoded/cached reference to the old `genesis_hash` for signature
   verification or chain-identity display, update it too.

## Endpoints (unchanged)

- RPC: `https://testnet-eu-rpc.catalystnet.org` (public, HTTPS) or `http://45.32.177.248:8545`
  (direct; may be IP-restricted — see `docs/testnet-handoff-catalyst-testnet.md` for an SSH-tunnel
  fallback).
- Full RPC method reference, indexing loop, tx/receipt model: `docs/testnet-handoff-catalyst-testnet.md`
  and `docs/explorer-handoff.md` in this repo (both already updated with the new genesis hash).

## If something still looks wrong after updating the identity

Re-run the live-proof command above first — if `applied_cycle` is advancing and matches the new
genesis hash, the network itself is healthy and the remaining issue is explorer-side (indexer
config, cached identity, or a stale connection/session that needs restarting). If `applied_cycle` is
*not* advancing or the genesis hash doesn't match this document, stop and flag it — that would be a
new, different problem from the one this document explains.
