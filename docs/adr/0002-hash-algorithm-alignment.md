### ADR 0002 — Hash algorithm alignment (consensus-critical) (A3)

Status: **Accepted (for public testnet MVP)**

Related issues:
- #127 (Milestone A — Spec pinning + canonical boundaries)

Spec references:
- Consensus v1.2 §1.2 “Choice of hashing algorithm”: `https://catalystnet.org/media/CatalystConsensusPaper.pdf`

## Context / Problem

The repo historically used multiple hash functions in different places (SHA-256, Blake2b, etc.).
For public testnet, consensus-critical artifacts must use a **single, spec-faithful** hash choice.

## Decision

### Canonical consensus-critical hash

For consensus-critical hashing, we use **Blake2b** with a 256-bit output.

Implementation convention used across the repo:
- `blake2b_256(x) := first_32_bytes(blake2b_512(x))`

This applies to:
- **Transaction id** (mempool/network dedupe): `blake2b_256(bincode(Transaction))`
- **Producer selection PRNG seed derivation** (temporary implementation): `blake2b_256(seed || domain_tag)`
- **LSU hash** and other consensus artifacts hashed by the consensus engine
- **Hash-tree roots** used by consensus (`dn`, list hashes, etc.)

### Domain separation

When the same hash function is used for different artifacts, a stable ASCII tag MUST be included
in the preimage (domain separation). The exact tags are versioned as part of Wire v1.

## Non-goals / Out of scope

- The **state root** hash algorithm (authenticated state) is handled in **Milestone E** and may use a different construction.
- IPFS/DFS content addressing may use multihash defaults that are not Blake2b; that’s not treated as a consensus hash.

## Consequences

Pros:
- Eliminates accidental SHA-256 usage in producer selection and other consensus-critical paths.
- Reduces “consensus split” risk due to mixed hash algorithms.

Cons:
- Requires follow-up work in A4 to implement the paper’s exact PRNG/seed rules beyond the placeholder.

