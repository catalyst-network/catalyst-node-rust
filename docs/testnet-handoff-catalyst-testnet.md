# Catalyst public testnet handoff (`catalyst-testnet`)

This file is intended to be handed to an external agent/tooling team building a block explorer/indexer.

## Network identity (MUST match)

- **network_id**: `catalyst-testnet`
- **chain_id (decimal)**: `200820092`
- **chain_id (hex)**: `0xbf8457c`
- **genesis_hash**: `0xeea16848e6b1d39d6b7a5e094ad9189d5382a6a4b19fb95342ef9846258fee5a`

If these don’t match what the explorer sees from RPC, it is indexing the wrong chain.

## Endpoints

### RPC (JSON-RPC over HTTP)

- **RPC_URL**: `http://45.32.177.248:8545` (EU node)

If RPC is IP-restricted, use an SSH tunnel:

```bash
ssh -N -L 8545:127.0.0.1:8545 root@45.32.177.248
export RPC_URL=http://127.0.0.1:8545
```

### P2P (FYI; explorers don’t need this)

- EU: `/ip4/45.32.177.248/tcp/30333`
- US: `/ip4/45.76.21.153/tcp/30333`
- Asia: `/ip4/207.148.126.35/tcp/30333`

## Quick verification commands

### 1) Confirm chain identity

```bash
curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_getSyncInfo","params":[]}'
```

Expected fields:
- `chain_id` == `0xbf8457c`
- `network_id` == `catalyst-testnet`
- `genesis_hash` == `0xeea16848e6b1d39d6b7a5e094ad9189d5382a6a4b19fb95342ef9846258fee5a`

### 2) Confirm the network is live

```bash
curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_blockNumber","params":[]}'
```

Call twice ~30–60 seconds apart; it should increase.

### 3) Peer count (best-effort)

```bash
curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_peerCount","params":[]}'
```

On a healthy 3-node mesh, the EU node should usually report `2`.

## Indexing model (Catalyst vs Ethereum)

- A Catalyst “block” is a **consensus cycle**.
- In RPC, `block_number` == **applied cycle**.
- The canonical per-cycle artifact is an LSU (`LedgerStateUpdate`) applied by the node.

Treat `"applied"` receipts as included in that node’s canonical head.

## RPC methods you will use (explorer/indexer)

### Head / chain identity

- `catalyst_chainId` → hex string
- `catalyst_networkId` → string
- `catalyst_genesisHash` → `0x` + 32-byte hex
- `catalyst_head` → `{ applied_cycle, applied_lsu_hash, applied_state_root, last_lsu_cid }`
- `catalyst_blockNumber` → `u64` (applied cycle)

### Blocks / history

- `catalyst_getBlocksByNumberRange(start, count, full_transactions)`
  - `full_transactions=false` for normal indexing (store tx hashes + summaries)
  - `full_transactions=true` if you need full tx objects

### Transactions / receipts

- `catalyst_getTransactionByHash(tx_id)`
- `catalyst_getTransactionReceipt(tx_id)`
  - `status`: `"pending" | "selected" | "applied" | "dropped"`
  - `selected_cycle`, `applied_cycle` (when known)
  - EVM best-effort execution info: `success`, `error`, `gas_used`, `return_data`
- `catalyst_getTransactionInclusionProof(tx_id)` (Merkle proof when applied)

### Accounts (optional explorer features)

- `catalyst_getBalance(0x<pubkey32>)` (decimal string)
- `catalyst_getAccount(0x<pubkey32>)` (balance + nonce)
- `catalyst_getBalanceProof(0x<pubkey32>)` (state root + Merkle proof)

### Rate limiting

RPC may rate-limit (error code `-32029`). Indexers should:
- back off with jitter
- reduce batch sizes
- avoid tight polling loops

## Practical indexing loop (recommended)

1) Read head:
   - `catalyst_blockNumber` or `catalyst_head`
2) Fetch blocks in ranges:
   - `catalyst_getBlocksByNumberRange(start, count, full_transactions=false)`
3) For each block:
   - store block header fields and tx hash list
   - for each tx hash, store `catalyst_getTransactionReceipt`

## Address format

- Catalyst “addresses” are **32-byte public keys**
- Canonical text form: **lowercase** `0x` + 64 hex chars

## Tx id and submission (FYI)

Tx id:

\[
\text{tx\_id} = \text{blake2b512}(\text{"CTX1"} \,\|\, \text{tx\_bytes})[0..32]
\]

Tx submission (`catalyst_sendRawTransaction`) accepts:
- v1: `0x` + hex(`"CTX1"` + canonical serialized `Transaction`)
- legacy: `0x` + hex(`bincode(Transaction)`)

## Fast sync / snapshots (optional)

If the operator publishes a snapshot:

- `catalyst_getSnapshotInfo` → latest snapshot ad:
  - `archive_url` (+ optional sha256/bytes)
  - `published_at_ms`, optional `expires_at_ms`

This is mainly for node operators, but explorers can use it to spin up a local archival node quickly.

## References in this repo (implementation details)

- RPC surface: `crates/catalyst-rpc/src/lib.rs`
- Explorer compatibility notes: `docs/explorer-handoff.md`
- Network identity scheme: `docs/network-identity.md`

