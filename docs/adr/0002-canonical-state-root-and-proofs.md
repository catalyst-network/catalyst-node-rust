# ADR 0002: Canonical state root, proofs, and state-root finality certificate

| Field | Value |
|--------|--------|
| **Status** | Accepted |
| **Date** | 2026-07-12 |
| **Related** | [`docs/adr/0001-lsu-finality-certificate.md`](./0001-lsu-finality-certificate.md), [`docs/consensus-reliability-review-2026-07.md`](../consensus-reliability-review-2026-07.md), [`docs/consensus-quorum-and-fork-choice.md`](../consensus-quorum-and-fork-choice.md) |
| **Supersedes/restores** | Original ADR 0006 "Canonical state root + proof format" (Accepted 2026-01-22), deleted in `a3b2d06` along with six other ADRs during a testnet simplification pass. Recoverable via `git show 2f121b8:docs/adr/0006-state-root-and-proofs.md`. This document restores its intent, corrects it against actual code, and adds the state-root finality certificate that did not exist when the original was written. |

## Context

[`consensus-reliability-review-2026-07.md`](../consensus-reliability-review-2026-07.md) diagnosed the
architectural gap this ADR closes: ADR 0001's `LsuFinalityCertificateV1` gives BFT-quorum cryptographic
backing to the LSU **recipe** (`lsu_hash`) — but nothing gave equivalent backing to the **result**
(`state_root`). Each node computed `state_root` independently, post-apply, with no consensus mechanism
forcing two nodes' results to be provably identical. On 2026-07-03, two active validators agreed on
`lsu_hash` at the same cycle but produced different `state_root`s — a live, silent correctness fork the
protocol had no way to detect as a consensus fact.

Separately, the original ADR 0006 that defined the canonical SMT hash construction was deleted without a
replacement, so the code that still implements it (`catalyst-storage/src/sparse_merkle.rs`,
`catalyst-storage/src/merkle.rs`) has had no governing document, no test-vector pinning, and — as this
review found — has drifted from what ADR 0006 specified.

## Part 1: Canonical state root (restored, corrected)

The **state root** is a Sparse Merkle Tree (SMT) root over all key/value pairs in the RocksDB
column-family `accounts`, depth 256, key path = hash of the key.

**Correction from the original ADR 0006:** that document mandated Blake2b-256. Actual code
(`crates/catalyst-storage/src/sparse_merkle.rs`, `crates/catalyst-storage/src/merkle.rs`) has always used
**SHA-256**, not Blake2b-256, and — critically — this has been the case since the hashing was implemented,
not a regression. Migrating the live SMT hash algorithm now would be a consensus-breaking, mainnet-blocking
change with zero correctness benefit (SHA-256 is not a weaker choice here; there was never a security
reason for the mismatch, only a documentation one). This ADR documents **reality** rather than force a
migration:

- **Leaf hash** (present key): `leaf = SHA256( 0x00 || u64_le(len(key)) || key || u64_le(len(value)) || value )` (`merkle.rs::h_leaf`).
- **Empty leaf hash**: `empty_leaf = SHA256( 0x02 )` (`sparse_merkle.rs::h_empty_leaf`).
- **Internal node hash**: `node = SHA256( 0x01 || left || right )` (`merkle.rs::h_node`, `sparse_merkle.rs::h_node` — identical construction, defined twice; a future cleanup could deduplicate but this is not consensus-relevant).
- **Key path**: `path = SHA256(key)`, 256-bit (`sparse_merkle.rs::key_path`).
- **Tree**: full binary SMT of depth 256 over `path`.

Full-recompute cost: `StorageManager::compute_state_root()` (`catalyst-storage/src/manager.rs`) iterates
the entire `accounts` column family on every call — O(n) in account count, not incremental. This is
adequate at current testnet scale; a future ADR should revisit if account count growth makes full
recompute a bottleneck.

### Proof format

A proof for a key (`MerkleProof` in `catalyst-storage/src/merkle.rs`) is:

- `leaf`: the leaf hash being proven.
- `steps: Vec<MerkleStep>`, each `{ sibling_is_left: bool, sibling: Hash }`, leaf → root order.

Verification (`merkle::verify_proof`): starting from `leaf`, for each step compute
`H(sibling, cur)` if `sibling_is_left` else `H(cur, sibling)`; accept iff the final value equals the
claimed root.

### Known gap: proof direction is self-declared, not key-derived (flagging, not fixing, in this ADR)

The original ADR 0006 required: *"Canonical verification derives left/right ordering from the key path
bits, not from untrusted tags."* **Actual code does not do this.** `merkle::verify_proof` trusts the
`sibling_is_left` flag carried in the proof itself; it never independently recomputes the expected
direction from `key_path(key)` the way `sparse_merkle::compute_root_and_proof_from_iter` does when
*generating* a proof (it derives `is_right` from `idx_lsb(&idx)` there, but the verifier doesn't repeat
that derivation). This is the standard SMT hardening every production sparse-Merkle-tree implementation
applies specifically to prevent proof-position confusion (a proof legitimately computed for one key path
being replayed, with self-declared directions, as if it applied to a different key). SHA-256's
one-wayness means this is **not** a trivial break — an attacker cannot forge an arbitrary root from
scratch — but it is a real deviation from the documented and generally-expected security model for this
proof format, and it governs every consumer of `StateProofBundle`/`KeyProofChange` (`catalyst-cli/src/sync.rs`),
including the ADR 0002 state-root certificate's "optional but recommended" proof-bundle cross-check
described below.

**This ADR does not fix `verify_proof` to derive direction from the key path** — that is a separable,
code-level hardening change whose blast radius (the generic `merkle.rs` module is also used for
non-keyed Merkle roots, e.g. transaction-list roots, where key-derived direction does not apply) needs
its own review before mainnet. It is recorded here so it is not lost: **flagged as a required fix before
`StateProofBundle` verification can be treated as a hard security boundary against a malicious peer**,
tracked as a follow-up to this ADR.

## Part 2: State-root finality certificate (new)

### Problem

Mainnet needs cryptographic, portable evidence that a BFT supermajority of the committee independently
computed and agrees on the **same** `state_root` after applying an LSU — not just that they agreed on
the LSU's *bytes* (ADR 0001 already provides that). Without this, `state_root` divergence between honest
nodes is undetectable as a consensus fact and, once introduced by any means (a bug, a trusted-catch-up
path caching an unverified peer claim, a node starting from already-divergent local state), compounds
silently forever.

### Decision

Add `LsuStateRootCertificateV1`: a second, additive certificate, structurally parallel to ADR 0001's
`LsuFinalityCertificateV1` but binding `state_root` instead of `committee_hash`/`vote_list_hash`, with
its own domain-separated digest so signatures over one certificate type can never be replayed as
evidence for the other.

**Why a separate certificate rather than a new field on `ProducerVote`/`H_cert`:** `ProducerVote` is
cast during the Voting phase, **before** any producer has executed the LSU
(`ProducerVote.ledger_state_hash = hash_data(&lsu)`, computed pre-apply — see `phases.rs`). Folding
`state_root` into that vote would require restructuring four-phase timing so voting happens after
execution, which this ADR deliberately avoids touching. Instead, state-root attestation is a second,
later round: after a producer applies the LSU (during/after Synchronization) and computes its own
`state_root`, it signs and gossips an attestation; once a BFT quorum of *distinct* producers attest to
the *same* `state_root` for the same `(cycle, lsu_hash)`, that agreement is assembled into a certificate,
exactly mirroring how ADR 0001 attestations aggregate into `LsuFinalityCertificateV1`.

### `H_root` (v1)

```
domain          16 bytes   UTF-8 "CATALYST_LSUROOT" right-padded with 0x00 (distinct from ADR 0001's "CATALYST_LSUFC")
version         1 byte     0x01
chain_id        8 bytes    u64 LE
genesis_hash    32 bytes
cycle           8 bytes    u64 LE
lsu_hash        32 bytes
committee_hash  32 bytes   BLAKE2b-512(pk_1 || ... || pk_n)[0..32], same construction as ADR 0001
state_root      32 bytes

H_root = BLAKE2b-512(domain || version || chain_id || genesis_hash || cycle || lsu_hash || committee_hash || state_root)[0..32]
```

Signed with the same Schnorr-style 64-byte signature scheme (`catalyst-crypto::SignatureScheme`) ADR
0001 uses.

### Certificate payload (v1)

```
LsuStateRootCertificateV1 {
    version: u8,             // = 1
    chain_id: u64,
    genesis_hash: [u8; 32],
    cycle: u64,
    lsu_hash: [u8; 32],
    state_root: [u8; 32],
    committee_hash: [u8; 32],
    attestations: Vec<StateRootAttestation>,
}

StateRootAttestation {
    producer_id: String,     // 64-hex-char pubkey
    signature: Vec<u8>,      // 64 bytes, signs H_root
}
```

**Rules** (mirrors ADR 0001 Style A): distinct `producer_id` values only; each must be a member of
`LedgerStateUpdate.producer_list`; `H_root` recomputed and matched per signature; valid distinct-signer
count `>= bft_vote_threshold(n)` (`⌈2n/3⌉`, same formula as ADR 0001).

### Verification (normative)

1. Deserialize the LSU; compute `lsu_hash`; must match the certificate.
2. Parse `producer_list`; compute `committee_hash`; must match the certificate.
3. Recompute `H_root`; verify each attestation signature under its claimed `producer_id` pubkey.
4. Enforce BFT threshold on distinct valid attesters.
5. **Callers gating apply on this certificate must additionally check `cert.state_root` equals whatever
   root they are about to trust.** A certificate that verifies but certifies a *different* root than the
   one about to be applied/cached must be rejected — otherwise a peer could present genuine quorum
   backing for one root while feeding the apply path a different claimed root, silently defeating the
   certificate.

### Where this plugs in

- **Production**: every producer in `producer_list` (not just the LSU's leader) independently computes
  `state_root` after applying and signs/gossips a `StateRootAttestationMsg`
  (`MessageType::StateRootAttestation`, `crates/catalyst-cli/src/sync.rs`); nodes aggregate matching
  attestations (keyed by `(cycle, lsu_hash, state_root)` so a divergent minority can never contaminate a
  majority bucket) and publish the assembled certificate to DFS, gossiping its CID via
  `StateRootFinalityCidMsg` (`MessageType::StateRootFinalityCid`) and the `state_root_cid` field on
  `LsuCidGossip`.
- **Apply-path enforcement**: `[consensus] require_state_root_finality` (default `false`, env override
  `CATALYST_REQUIRE_STATE_ROOT_FINALITY`) gates whether the trusted/catch-up apply path
  (`apply_lsu_to_storage_without_root_check`) requires a verified certificate matching the claimed root
  before trusting it, via `verify_state_root_finality_for_apply` (`crates/catalyst-cli/src/node.rs`).
  Same two-step migration shape as ADR 0001's `require_lsu_finality`: ship disabled, roll out attestation
  gossip fleet-wide, then flip to `true` for mainnet.
- **Fork choice**: `ForkChoiceCandidate.state_root_certified` (`crates/catalyst-cli/src/fork_choice.rs`)
  is a same-tier tie-break signal — when two candidates are otherwise tied (both `Certified` on the LSU
  recipe, different `lsu_hash`), the one additionally backed by a verified state-root certificate wins,
  before falling back to the smaller-hash tie-break. This never promotes a candidate above a higher tier;
  `tier` remains defined purely over LSU-recipe evidence.

## Non-goals (v1)

- Fixing `verify_proof`'s self-declared-direction gap (Part 1, flagged above) — separate follow-up.
- Aggregated/threshold signatures (MuSig) for the state-root certificate — ADR 0001's Style B note
  applies equally here; ship individual signatures first.
- Folding this certificate into `ProducerVote`/four-phase timing — explicitly avoided, see Decision above.
- Slashing or economic enforcement for equivocating attestations (same non-goal as ADR 0001).

## Consequences

### Positive

- `state_root` becomes a fact a BFT quorum cryptographically vouches for, not a single peer's claim —
  closes the core gap the 2026-07 reliability review identified.
- Additive: no breaking change to ADR 0001's `H_cert`/`LsuFinalityCertificateV1` wire format or golden
  test vector; old nodes tolerate the new optional `state_root_cid` field being absent during rollout.
- Uses the exact aggregation/rollout pattern already proven in production for ADR 0001, minimizing new
  protocol machinery.

### Negative / risks

- Additional gossip round (one attestation message per producer per cycle) and bandwidth for the
  certificate itself — same cost class ADR 0001 already accepted.
- `require_state_root_finality=false` (the default during rollout) means the certificate exists but does
  not yet close the gap it was built for until fleets coordinate the flip to `true` — same migration
  window risk ADR 0001 had for its own rollout.

## References (code)

- `crates/catalyst-consensus/src/state_root_finality.rs` — `H_root`, `LsuStateRootCertificateV1`, verify.
- `crates/catalyst-consensus/src/types.rs` — `ProducerVote` (confirms pre-apply vote timing).
- `crates/catalyst-cli/src/sync.rs` — `StateRootAttestationMsg`, `StateRootFinalityCidMsg`, `LsuCidGossip.state_root_cid`.
- `crates/catalyst-cli/src/node.rs` — attestation aggregation (`ingest_state_root_attestation`,
  `try_publish_state_root_certificate_from_bucket`), apply-path gate (`verify_state_root_finality_for_apply`).
- `crates/catalyst-cli/src/fork_choice.rs` — `ForkChoiceCandidate.state_root_certified`.
- `crates/catalyst-cli/src/consensus_limits.rs` — `effective_require_state_root_finality`.
- `crates/catalyst-storage/src/sparse_merkle.rs`, `crates/catalyst-storage/src/merkle.rs` — SMT/proof implementation (Part 1).
