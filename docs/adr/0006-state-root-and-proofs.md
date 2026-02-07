# ADR 0006: Canonical state root + proof format

- **Status**: Accepted
- **Date**: 2026-01-22

## Context
Catalyst needs an authenticated commitment to chain state so that:
- nodes can verify LSU continuity during synchronization
- RPC clients can verify proofs (e.g. balances) against the head

We already maintain an authenticated root over the `accounts` column-family using a Sparse Merkle Tree (SMT), but the hashing algorithm, key-path derivation, and proof verification rules must be explicitly canonical and consistent across the codebase.

## Decision
### State root
The **state root** is the SMT root over all key/value pairs in the RocksDB column-family `accounts`.

- **Leaf hash** (for present keys):
  - `leaf = blake2b_256( 0x00 || u64_le(len(key)) || key || u64_le(len(value)) || value )`
- **Empty leaf hash** (for absent keys):
  - `empty_leaf = blake2b_256( 0x02 )`
- **Internal node hash**:
  - `node = blake2b_256( 0x01 || left || right )`
- **Key path**:
  - `path = blake2b_256( 0x03 || key )` (256-bit)

The SMT is a full binary tree of depth 256.

### Proof format
A proof for a key is:
- `steps`: an array of **256** sibling hashes, ordered from leaf → root.

RPC currently returns each step as a string like `"L:0x..."` or `"R:0x..."`.
- **These tags are informational only.**
- Canonical verification derives left/right ordering from the **key path bits**, not from untrusted tags.

### Verification
To verify a proof for `(key, value)`:
- compute `cur = leaf_hash(key,value)` (or `empty_leaf` for absence)
- compute `path = blake2b_256(0x03 || key)`
- for each step `i` from 0..255:
  - let `bit = path_lsb(path)` (starting from LSB and shifting right each step)
  - if `bit == 1` (current is right child): `cur = node(sibling[i], cur)`
  - else: `cur = node(cur, sibling[i])`
- accept iff `cur == state_root`

## Consequences
- State roots and proofs become consensus-critical and MUST use Blake2b-256.
- Proof verification MUST be key-derived (do not trust provided left/right direction).
- RPC can provide verifiable absence proofs (balance=0) without returning a fake root.
