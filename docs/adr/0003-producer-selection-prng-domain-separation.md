### ADR 0003 — Producer selection PRNG + domain separation (A4)

Status: **Accepted (for public testnet MVP)**

Related issues:
- #128 (Milestone A — Spec pinning + canonical boundaries)

Spec references:
- Consensus v1.2 §2.2.1 “Producer nodes selection”: `https://catalystnet.org/media/CatalystConsensusPaper.pdf`

## Context / Problem

Producer selection requires all honest nodes to compute the same pseudo-random value \(r_{n+1}\)
from a previous-cycle commitment, then sort workers by `Id_i XOR r_{n+1}`.

If the seed derivation or hashing is ambiguous, producer selection can diverge across nodes.

## Decision

### 1) Seed source (current implementation)

The paper describes deriving \(r_{n+1}\) from a previous cycle commitment (it mentions a Merkle root).

In the current codebase, the most direct 32-byte commitment carried by the LSU is the
**transaction signatures hash root** (`LedgerStateUpdate.partial_update.transaction_signatures_hash`),
persisted by the node (see metadata keys like `consensus:last_applied_tx_sigs_root`).

Therefore, for public testnet MVP we define:
- `seed := prev_cycle_commitment_32`, currently the **last applied tx signatures root** (32 bytes)

This is a **placeholder mapping** until we implement the paper’s exact commitment (e.g., LSU Merkle root)
as a first-class stored value. For backwards compatibility, the node may fall back to the last applied LSU hash
if the new seed key is not present.

### 2) Domain-separated derivation of \(r_{n+1}\)

We define the producer selection randomness as:

- `r_{n+1} = blake2b_256(seed || DOMAIN_TAG)`
- `DOMAIN_TAG = "catalyst:producer_selection:r_n_plus_1:v1"`

Where:
- `blake2b_256(x) := first_32_bytes(blake2b_512(x))` (consistent with ADR 0002)

## Consequences

- All nodes given the same seed compute the same \(r_{n+1}\) deterministically.
- Domain separation prevents accidental collisions with other Blake2b-based hashes/PRNGs.
- When we move to the paper’s exact commitment, only the **seed input** changes; the domain-separated derivation remains stable (or is revved via the `v1` suffix).

