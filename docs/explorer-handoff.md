## Catalyst V2 explorer handoff (compatibility notes)

This file is intended to be copied into a *fresh* Cursor workspace as context for building an external block explorer/indexer.
It focuses on the minimum you must know to be **compatible** with the current Catalyst testnet implementation.

### Mental model (Catalyst vs Ethereum)

- A Catalyst “block” is a **consensus cycle**. In RPC it is represented as a `RpcBlock` whose `number` equals the **applied cycle**.
- The canonical on-chain artifact per cycle is an **LSU** (`LedgerStateUpdate`), persisted by nodes under `consensus:lsu:<cycle>` (internal).
- Transaction inclusion is “best-effort” until applied by your target node; do not assume immediate finality from submit.

### Chain identity (required for correct signing)

Explorers should show these (and wallets must sign against them):

- `catalyst_chainId` → hex string (e.g. `"0x7a69"`)
- `catalyst_networkId` → string (e.g. `"tna_testnet"`)
- `catalyst_genesisHash` → `0x` + 32-byte hex

### Addresses (canonical)

- Address bytes are **32-byte public keys**.
- Text form is **lowercase** `0x`-prefixed hex (64 hex chars).

### Transaction ID (tx hash)

The tx id returned by RPC is:

\[
\text{tx\_id} = \text{blake2b512}(\text{"CTX1"} \,\|\, \text{tx\_bytes})[0..32]
\]

Returned as `0x` + 32-byte hex.

### Transaction submission (wire format)

`catalyst_sendRawTransaction(data)` accepts:

- **Preferred v1**: `0x` + hex(`"CTX1"` + canonical serialized `Transaction` bytes)
- **Legacy**: `0x` + hex(`bincode(Transaction)`)

### Receipts (how explorers should track state)

Use receipts as the primary truth for transaction state:

- `catalyst_getTransactionReceipt(tx_hash)` returns:
  - `status`: `"pending" | "selected" | "applied" | "dropped"`
  - `selected_cycle`, `applied_cycle`
  - optional execution info:
    - `success` (bool)
    - `error` (string)
    - `gas_used` (u64, EVM best-effort)
    - `return_data` (hex string, EVM best-effort)

If `applied_cycle` is present, you can also query:

- `catalyst_getTransactionInclusionProof(tx_hash)` → Merkle proof against the cycle’s tx Merkle root

### Blocks / history indexing (practical loop)

Recommended index loop:

1) Read chain head:
   - `catalyst_blockNumber` (applied cycle)
   - or `catalyst_head` (cycle + lsu_hash + state_root)

2) Fetch blocks in ranges:
   - `catalyst_getBlocksByNumberRange(start, count, full_transactions)`
   - Use `full_transactions=true` only when you need full objects; otherwise store summaries and fetch full txs on demand.

3) For each block:
   - persist block header fields + transaction summary list
   - for each tx hash, also persist `catalyst_getTransactionReceipt` output (this is where `success/error/gas_used` lives)

Notes:
- The server enforces caps/rate limiting; use paging and backoff.
- `catalyst_getTransactionsByAddress` exists but is best-effort and can be expensive; prefer block-range indexing.

### Rate limiting / anti-spam behavior

RPC may return a rate-limit error (code `-32029`). Your explorer should:

- back off with jitter
- reduce batch sizes
- avoid tight polling loops on receipts

### Known limitations (important for explorer UX)

- “Finality” is node-local and pragmatic; treat `"applied"` as included in that node’s head.
- EVM is REVM-backed and persists `getCode/getStorageAt/getLastReturn` as dev helpers; it is not a full Ethereum RPC surface.

### Where to look in this repo (reference)

- RPC surface: `crates/catalyst-rpc/src/lib.rs`
- Wallet / tx v1 spec: `docs/wallet-interop.md`
- General integration notes: `docs/builder-guide.md`

