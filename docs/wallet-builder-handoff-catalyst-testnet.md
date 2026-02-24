# Wallet builder handoff (`catalyst-testnet`)

This document is intended for teams implementing a wallet against the current Catalyst V2 Rust node (`catalyst-node-rust`).

It combines:
- the **stable wallet v1 encoding/signing rules**
- the **practical RPC flow** (chain identity, balances, nonce, fee estimate, submission, receipts)
- the **live `catalyst-testnet` parameters**

## Live network parameters (as deployed)

### Chain identity (MUST match)

- **network_id**: `catalyst-testnet`
- **chain_id (decimal)**: `200820092`
- **chain_id (hex)**: `0xbf8457c`
- **genesis_hash**: `0xeea16848e6b1d39d6b7a5e094ad9189d5382a6a4b19fb95342ef9846258fee5a`

Wallets must bind signatures to \(`chain_id`, `genesis_hash`\) using the v1 signing payload.

### RPC endpoint

- **RPC_URL**: `http://45.32.177.248:8545`

If RPC is IP-restricted, use an SSH tunnel:

```bash
ssh -N -L 8545:127.0.0.1:8545 root@45.32.177.248
export RPC_URL=http://127.0.0.1:8545
```

## Key material and address format

### Private key

- 32 bytes (hex-encoded in tooling).
- Interpreted as a Curve25519/Ristretto scalar via `Scalar::from_bytes_mod_order(bytes)` (i.e. reduced mod group order).

### Public key (wallet “address”)

- 32 bytes = **compressed Ristretto** point.
- Canonical text form: **lowercase** `0x` + 64 hex chars.

Example:

```text
0x26f4404048d8bef3a36cd38775f8139e031689cf1041d793687c3d906da98a76
```

## Signature scheme (single-sig)

The node currently verifies a **Schnorr signature over Ristretto**:

- nonce \(k\) is random (wallet must use a CSPRNG)
- \(R = kG\)
- challenge \(e = H(R || P || message)\)
- \(s = k + e \cdot x\)
- signature bytes = `R_compressed (32) || s (32)`
- hash \(H\) = Blake2b (implementation helper: `blake2b_hash`)

Reference implementation: `crates/catalyst-crypto/src/signatures.rs`.

## Wallet transaction format (v2)

This is the stable wallet-facing format used by:
- `catalyst_sendRawTransaction`
- `tx_id` computation
- signing payload domain separation

See also: `docs/wallet-interop.md`.

### 1) Canonical signing payload (v2)

Wallets sign:

- `domain` = ASCII `"CATALYST_SIG_V2"`
- `chain_id` = u64 little-endian
- `genesis_hash` = 32 bytes
- `signature_scheme` = u8 (currently only `0` supported)
- `sender_pubkey` = Option<Vec<u8>> (currently must be `None`; reserved for PQ)
- `core_bytes` = canonical serialization of `TransactionCore`
- `timestamp` = u64 little-endian (milliseconds since epoch; same value used in `Transaction.timestamp`)

### 2) Canonical wire encoding (v2)

RPC expects `data` for `catalyst_sendRawTransaction`:

- bytes = `"CTX2" || canonical_serialize(Transaction)`
- hex string = `0x` + hex(bytes)

### 3) Transaction id (tx hash) (v2)

\[
\text{tx\_id} = \text{blake2b512}(\text{"CTX2"} \,\|\, \text{tx\_bytes})[0..32]
\]

Returned as `0x` + 32-byte hex.

### 4) Canonical serialization rules (v2)

Canonical serialization is defined in `docs/wallet-interop.md` and implemented in:
- `crates/catalyst-core/src/protocol.rs`

Wallet test vectors are in:
- `testdata/wallet/v1_vectors.json` (legacy; update pending)

## Building a simple transfer (recommended wallet MVP)

### TransactionType

Use `TransactionType::NonConfidentialTransfer` (tag `0` in canonical serialization).

### Entries (balance deltas)

`TransactionCore.entries` is a list of \(`public_key`, `amount`\) where:
- sender entry amount is **negative** (debit)
- recipient entry amount is **positive** (credit)

Constraints in current node implementation:
- entries must be non-empty
- sum(entries) must be \( \le 0 \)
- only one sender key should be net-negative (single-sender model)

Recommended pattern:
- entries sum to **0**
- use `core.fees` to pay fees (node debits fees separately at apply time)

Example:
- sender: `-7`
- recipient: `+7`

### Nonce

Fetch current nonce:
- `catalyst_getNonce("0x<pubkey32>")` → `u64`

Wallet should set:
- `tx.core.nonce = nonce + 1`

(The node expects a strictly increasing per-sender counter.)

### Timestamp and lock_time

Wallet sets:
- `tx.timestamp` = current time in **milliseconds**
- `tx.core.lock_time` = current time in **seconds** (best-effort anti-replay window in this scaffold)

### Fees

Wallet can compute a deterministic minimum fee, but the simplest approach is:

- call `catalyst_estimateFee` (returns a decimal string)
- set `tx.core.fees` to that value (as `u64`)

For a transfer, use a request like:

```json
{
  "from": "0x<SENDER_PUBKEY32>",
  "to": "0x<RECIP_PUBKEY32>",
  "value": "<amount_as_decimal_string>",
  "data": null,
  "gas_limit": null,
  "gas_price": null
}
```

Notes:
- `catalyst_estimateFee` is a convenience; the actual transaction submitted is the Catalyst v2 wire tx described above.

## RPC flow summary (wallet)

### 0) Confirm chain identity

- `catalyst_getSyncInfo` (preferred convenience)
- or `catalyst_chainId`, `catalyst_genesisHash`, `catalyst_networkId`

### 1) Read account

- `catalyst_getAccount("0x<pubkey32>")` → `{ balance, nonce }` (if known)
- `catalyst_getBalance("0x<pubkey32>")` → decimal string
- `catalyst_getNonce("0x<pubkey32>")` → `u64`

### 2) Estimate fee (optional but recommended)

- `catalyst_estimateFee(RpcTransactionRequest)` → decimal string

### 3) Sign and submit

- build `Transaction` (v2 rules)
- compute v2 signing payload using `chain_id` + `genesis_hash`
- sign using Schnorr scheme above (64 bytes)
- set `tx.signature` to the 64-byte signature
- send:
  - `catalyst_sendRawTransaction("0x" + hex("CTX2" + canonical_serialize(tx)))`

### 4) Track status

- `catalyst_getTransactionReceipt(tx_id)` returns:
  - `status`: `"pending" | "selected" | "applied" | "dropped"`
  - `selected_cycle`, `applied_cycle` (when known)

If applied:
- `catalyst_getTransactionInclusionProof(tx_id)` provides a Merkle proof for inclusion in that cycle.

## Smart contracts / EVM (current status)

The repo supports basic EVM deploy/call via `TransactionType::SmartContract`, but **this is dev/test** and not yet a stable wallet standard:
- the tx `data` field contains an internal `bincode`-encoded `EvmTxKind` payload
- the RPC surface is *not* Ethereum JSON-RPC compatible beyond a few helpers (`getCode`, `getStorageAt`, `getLastReturn`)

If you want a wallet MVP, implement:
- key management (32-byte privkeys)
- balances/nonces
- transfers + receipts

## References

- Wallet v1 standards: `docs/wallet-interop.md`
- Wallet v1 vectors: `testdata/wallet/v1_vectors.json`
- RPC surface: `crates/catalyst-rpc/src/lib.rs`
- Signature algorithm: `crates/catalyst-crypto/src/signatures.rs`

