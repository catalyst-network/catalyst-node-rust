## Wallet interoperability (standards + v2 encoding)

This document defines **stable, wallet-facing conventions** for Catalyst V2 testnets.

### Address format (canonical)

- **Address bytes**: 32-byte public key.
- **Text form**: lowercase hex, **0x-prefixed**, 32 bytes (64 hex chars).

Example:

```text
0x26f4404048d8bef3a36cd38775f8139e031689cf1041d793687c3d906da98a76
```

### Chain identity (domain separation)

Wallets should bind signatures to the target chain using:

- `chain_id`: returned by RPC `catalyst_chainId` (hex string, e.g. `"0x7a69"`).
- `genesis_hash`: returned by RPC `catalyst_genesisHash` (32-byte hex string).

### Transaction signing payload (v2)

To sign a `Transaction` (Schnorr), wallets sign the **v2 signing payload**:

- `domain` = ASCII `"CATALYST_SIG_V2"`
- `chain_id` = u64 little-endian
- `genesis_hash` = 32 bytes
- `signature_scheme` = u8 (currently only `0` supported)
- `sender_pubkey` = Option<Vec<u8>> (currently must be `None`; reserved for PQ)
- `core_bytes` = canonical serialization of `TransactionCore` (see below)
- `timestamp` = u64 little-endian

### Transaction wire encoding (v2)

RPC accepts a **canonical wire encoding** for `catalyst_sendRawTransaction`:

- 4-byte magic prefix: ASCII `"CTX2"`
- followed by canonical serialization of `Transaction` (see below)

Notes:
- For backward compatibility, the node may also accept `"CTX1"` (v1) wire txs, but it will return
  the canonical tx id computed over the v2 form.

### Transaction hash / tx_id (v2)

The tx hash returned by the node is:

\[
\text{tx\_id} = \text{blake2b512}(\text{"CTX2"} \,\|\, \text{tx\_bytes})[0..32]
\]

Returned in RPC as a 0x-prefixed 32-byte hex string.

### Canonical serialization rules (v2)

All integers are **little-endian**.

`TransactionType` is encoded as a single u8 tag:

- `0`: NonConfidentialTransfer
- `1`: ConfidentialTransfer
- `2`: DataStorageRequest
- `3`: DataStorageRetrieve
- `4`: SmartContract
- `5`: WorkerRegistration

`EntryAmount` is encoded as:

- `0x00` + i64 (le): NonConfidential
- `0x01` + 32-byte commitment + (len:u32 le + range_proof bytes): Confidential

Vectors/bytes are encoded as:

- `len:u32 le` followed by `len` items/bytes.

### Legacy compatibility

For now, the node/RPC also accepts the legacy dev transport:

- hex-encoded `bincode(TransactionV1)`

but wallets **should** use v2 wire encoding + v2 signing payload going forward.

### Safety limits (anti-DoS)

The node enforces conservative caps:
- max raw tx bytes (hex-decoded, including magic): **64 KiB**
- max signature bytes: **8 KiB**
- max sender pubkey blob bytes: **4 KiB**
- unknown `signature_scheme` ids are rejected

