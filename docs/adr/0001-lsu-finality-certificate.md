# ADR 0001: LSU finality certificate (cryptographic quorum evidence)

| Field | Value |
|--------|--------|
| **Status** | Proposed |
| **Date** | 2026-03-28 |
| **Related** | [`docs/consensus-quorum-and-fork-choice.md`](../consensus-quorum-and-fork-choice.md) |

## Context

### Requirements (from project docs)

[`consensus-quorum-and-fork-choice.md`](../consensus-quorum-and-fork-choice.md) states that a ledger state update (LSU) should be **final** only when supported by evidence that **≥ quorum** of the **current producer set** committed to it (“signatures, vote certificates, or equivalent—**format TBD**”). It also notes that today’s follower may infer quorum from **`vote_list` inside the LSU** without a **cryptographic finality certificate**.

### Current implementation (summary)

- **`LedgerStateUpdate`** carries `producer_list` and `vote_list` as identifiers; quorum is counted in software from those fields (see `lsu_has_bft_vote_quorum` in `crates/catalyst-cli/src/node.rs`).
- **`ProducerVote`** (`crates/catalyst-consensus/src/types.rs`) binds **`ledger_state_hash`**, **`vote_list_hash`**, and **`producer_id`**, but has **no signature field**.
- **`MessageEnvelope`** (`crates/catalyst-utils/src/network.rs`) may carry an optional signature over **JSON-serialized envelope bytes**. That is suitable for **transport authenticity**, not for a **canonical, stable commitment** to an LSU for consensus finality (field order, JSON, and cross-language parity are poor fits for a consensus digest).
- **`StateProofBundle`** (`crates/catalyst-cli/src/sync.rs`) proves **state-transition correctness** (Merkle proofs) for a given `lsu_hash` and roots. It does **not** prove **which producers finalized** the LSU.
- **`catalyst-crypto`** provides **Schnorr-style 64-byte signatures** and **MuSig** aggregation primitives suitable for per-vote or aggregated quorum proofs.

### Problem

Mainnet needs a **portable, verifiable artifact**: any node (or auditor) must be able to check **cryptographic** evidence that the same LSU was committed by a **BFT supermajority** of the **declared committee**, with **domain separation** so votes cannot be replayed across chains or certificate versions.

## Decision

Introduce a versioned **LSU finality certificate** type and a **canonical digest** `H_cert` that committee members sign. Treat **finality** as “LSU bytes + certificate verify successfully”; **execution** may still use **`StateProofBundle`** or apply-then-check where applicable.

This ADR defines **v1** semantics and wire fields. Exact Rust module paths are left to implementation; behavior must match this document.

## Definitions

### Existing hashes (unchanged)

- **`lsu_hash`**: 32-byte hash produced by existing `catalyst_consensus::types::hash_data(&lsu)` (BLAKE2b-512 over Catalyst-serialized LSU, truncated to 32 bytes — same as today).

### Producer identity for signing

- Each **`ProducerId`** used in certificates **must** be a **64-character hex string** encoding a **32-byte Ed25519/Ristretto public key** (same material as `parse_producer_id_pubkey` in `node.rs` expects). Implementations **must reject** certificates that reference non-hex or wrong-length ids for v1.

### Committee digest (`committee_hash`)

Let `P = [pk_1, …, pk_n]` be the **32-byte public keys** parsed **in order** from `LedgerStateUpdate.producer_list`.

```
committee_raw = pk_1 || pk_2 || … || pk_n   (32 * n bytes)
committee_hash = BLAKE2b-512(committee_raw)[0..32]
```

Use the same BLAKE2 output truncation rule as `hash_data` (first 32 bytes of BLAKE2b-512 output). If `n == 0`, `committee_hash` is 32 zero bytes (invalid certificate in practice; verifiers should reject).

### Vote-list digest (`vote_list_hash`)

Compute exactly as in the voting phase:

`vote_list_hash = hash_producer_list_merkle(&lsu.vote_list)`

using `hash_producer_list_merkle` from `crates/catalyst-consensus/src/phases.rs` (same value carried on **`ProducerVote.vote_list_hash`** when votes align with the finalized LSU). Verifiers **must** recompute from the **`LedgerStateUpdate`** they are certifying, not trust a detached field without checking.

### Chain domain

- **`chain_id`**: `u64` little-endian, from node/ledger metadata (same notion as transaction domain elsewhere).
- **`genesis_hash`**: 32 bytes, canonical genesis identifier for the network (same as chain identity docs).

### Finality digest `H_cert` (v1)

Single concatenation, then hash:

| Segment | Length | Description |
|---------|--------|-------------|
| `domain` | 16 bytes | UTF-8 bytes of `CATALYST_LSUFC` right-padded with `0x00` |
| `version` | 1 byte | `0x01` |
| `chain_id` | 8 bytes | `u64` LE |
| `genesis_hash` | 32 bytes | |
| `cycle` | 8 bytes | `u64` LE |
| `lsu_hash` | 32 bytes | |
| `committee_hash` | 32 bytes | |
| `vote_list_hash` | 32 bytes | |

```
H_cert = BLAKE2b-512(domain || version || chain_id || genesis_hash || cycle || lsu_hash || committee_hash || vote_list_hash)[0..32]
```

Sign **`H_cert`** (32 bytes) with the producer’s private key using the **same signature scheme** as `catalyst-crypto`’s `SignatureScheme` (64-byte Schnorr-style signatures) unless a later ADR upgrades the algorithm with a new `version`.

## Certificate payload (v1)

Two supported **evidence styles**; implementations may implement one or both.

### Style A — Quorum of individual signatures

```
LsuFinalityCertificateV1 {
    version: u8,                    // = 1
    style: u8,                      // = 1 (individual)
    chain_id: u64,
    genesis_hash: [u8; 32],
    cycle: u64,
    lsu_hash: [u8; 32],
    committee_hash: [u8; 32],
    vote_list_hash: [u8; 32],
    votes: Vec<ProducerVoteEntry>,
}

ProducerVoteEntry {
    producer_id: String,            // 64-hex-char pubkey
    signature: [u8; 64],            // signs H_cert
}
```

**Rules:**

- **`votes`** must contain **distinct** `producer_id` values (no duplicates).
- Each `producer_id` **must** be a member of **`LedgerStateUpdate.producer_list`** for this LSU.
- **`H_cert`** must be recomputed from the fields above and match the signed message for each signature.
- Valid signature count **≥ `bft_vote_threshold(n)`** where `n = producer_list.len()` (⌈2n/3⌉ distinct valid votes), matching [`consensus-quorum-and-fork-choice.md`](../consensus-quorum-and-fork-choice.md).

### Style B — Aggregated signature (optional in v1)

```
style: u8 = 2
aggregated_signature: [u8; 64]     // MuSig or scheme-defined aggregate
participants_bitmask: [u8; ...]   // or sorted list of producer indices — TBD in implementation
```

**Note:** Style B requires a **specified** aggregation and participant-encoding rule in code + test vectors. If Style B is not ready, ship **Style A** first; this ADR does not block mainnet on Style B.

## Verification (normative)

Given **`LedgerStateUpdate` bytes**, **`LsuFinalityCertificateV1`**, **`chain_id`**, **`genesis_hash`**:

1. Deserialize LSU; compute **`lsu_hash`**; ensure it matches the certificate.
2. Parse **`producer_list`**; compute **`committee_hash`**; ensure it matches the certificate.
3. Obtain **`vote_list_hash`** from the LSU / voting rules; ensure it matches the certificate (must be consistent with **`ProducerVote`** semantics for that cycle).
4. Recompute **`H_cert`**; verify each **individual** signature (Style A) under the claimed **`producer_id`** pubkey, or verify aggregate (Style B) per implemented spec.
5. Enforce **BFT threshold** on **distinct** valid producers.
6. **Optional but recommended:** verify **`StateProofBundle`** if present for fast verification of roots without re-execution; this step is **independent** of finality.

If any step fails, the LSU is **not** treated as **final** under this ADR.

## Distribution and storage

- **Wire:** extend **`LsuCidGossip`** (or adjacent message) with a **`finality_cid`** field (same pattern as **`proof_cid`** in `crates/catalyst-cli/src/sync.rs`), pointing to serialized **`LsuFinalityCertificateV1`** bytes in DFS / content addressing.
- **Persistence:** store under metadata keys such as `consensus:lsu_finality:<cycle>` or beside existing `consensus:lsu:<cycle>` blobs; exact key names are implementation details.

## Non-goals (v1)

- Slashing proofs or on-chain enforcement of equivocation (can be layered later using individual signatures from Style A).
- Light-client-optimized **checkpoint** certificates spanning many cycles (separate ADR).
- Replacing **`StateProofBundle`**; it remains the **execution** proof path.

## Consequences

### Positive

- Clear **verifier story** for mainnet: quorum is **cryptographic**, not “strings inside LSU.”
- **Domain separation** reduces cross-chain replay and ambiguous payloads.
- **Composable** with existing **`lsu_hash`**, **`ProducerVote`** hashes, and crypto crates.

### Negative / risks

- **Bandwidth and CPU** at catchup for Style A (mitigate later with Style B or checkpoints).
- **Committee derivation** must match **exact** producer list encoding; bugs here cause false rejects.
- Requires **disciplined canonical encoding** for `H_cert` (do **not** reuse `MessageEnvelope` JSON signing for this digest).

## Implementation checklist (engineering)

1. Add **`H_cert`** helper + **golden test vectors** (fixed hex inputs → fixed `H_cert`).
2. Add **`LsuFinalityCertificateV1`** + serde/bincode (or chosen codec) + size limits.
3. Extend voting / sync paths to **produce** certificates when producing an LSU (validators).
4. Extend follower apply path to **require** valid certificate when feature flag / network parameter enabled.
5. Wire **`finality_cid`** through **`LsuCidGossip`** and DFS put/get.
6. Document rollout: testnet first, then default-on for mainnet per [`consensus-quorum-and-fork-choice.md`](../consensus-quorum-and-fork-choice.md) testnet gate.

### Implemented in `catalyst-node-rust` (status)

- **`H_cert` / verify / certificate types:** `crates/catalyst-consensus/src/lsu_finality.rs` (exported from `catalyst-consensus` crate).
- **Wire messages:** `MessageType::LsuFinalityAttestation`, `MessageType::LsuFinalityCid`; payloads `LsuFinalityAttestationMsg`, `LsuFinalityCidMsg` in `crates/catalyst-cli/src/sync.rs`. **`LsuCidGossip`** includes **`finality_cid`** (extra serialized field — **coordinated upgrade** across peers).
- **Production:** Validators in **`vote_list`** sign **`H_cert`** and gossip **`LsuFinalityAttestationMsg`**; nodes merge attestations and publish **`LsuFinalityCertificateV1`** to DFS when **BFT quorum** of distinct voters is met, then persist **`consensus:lsu_finality_cid:<cycle>`** and gossip **`LsuFinalityCidMsg`**.
- **Verification on apply:** Environment variable **`CATALYST_REQUIRE_LSU_FINALITY`** (`1` / `true` / `yes`) requires a valid certificate (from gossip hint or metadata + DFS) before applying an LSU via CID / `FileResponse`. If unset, behavior is unchanged except optional **warn** when a present certificate fails verification.

## Open questions

- **Style B:** exact MuSig participant list encoding and bitmask layout (must be specified before enabling in production).
- **Epoch boundaries:** if `producer_list` can change at epoch edges, certificate verification must use the **committee for `cycle`** as defined by protocol state (single source of truth in code).

## References (code)

- `crates/catalyst-consensus/src/types.rs` — `LedgerStateUpdate`, `ProducerVote`
- `crates/catalyst-cli/src/node.rs` — `lsu_has_bft_vote_quorum`, apply / reconcile paths
- `crates/catalyst-cli/src/sync.rs` — `LsuCidGossip`, `StateProofBundle`
- `crates/catalyst-utils/src/network.rs` — `MessageEnvelope` (transport only)
- `crates/catalyst-crypto/src/signatures.rs` — signing primitives
