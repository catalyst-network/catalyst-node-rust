## Wallet interoperability (standards + v1 encoding)

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

### Transaction signing payload (v1)

To sign a `Transaction` (Schnorr), wallets sign the **v1 signing payload**:

- `domain` = ASCII `"CATALYST_SIG_V1"`
- `chain_id` = u64 little-endian
- `genesis_hash` = 32 bytes
- `core_bytes` = canonical serialization of `TransactionCore` (see below)
- `timestamp` = u64 little-endian

### Transaction wire encoding (v1)

RPC accepts a **canonical wire encoding** for `catalyst_sendRawTransaction`:

- 4-byte magic prefix: ASCII `"CTX1"`
- followed by canonical serialization of `Transaction` (see below)

### Transaction hash / tx_id (v1)

The tx hash returned by the node is:

\[
\text{tx\_id} = \text{blake2b512}(\text{"CTX1"} \,\|\, \text{tx\_bytes})[0..32]
\]

Returned in RPC as a 0x-prefixed 32-byte hex string.

### Canonical serialization rules (v1)

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

- hex-encoded `bincode(Transaction)`

but wallets **should** use v1 wire encoding + v1 signing payload going forward.

