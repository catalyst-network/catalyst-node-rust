### ADR 0004 — Wire vs internal types (and convergence plan) (A5)

Status: **Accepted (for public testnet MVP)**

Related issues:
- #129 (Milestone A — Spec pinning + canonical boundaries)

Spec references:
- Consensus v1.2 §5.2.1–§5.2.4 (phase messages/values): `https://catalystnet.org/media/CatalystConsensusPaper.pdf`
- Consensus v1.2 §4.2–§4.5 (tx structure/signature/validity): `https://catalystnet.org/media/CatalystConsensusPaper.pdf`

## Problem

The repo currently has two overlapping type systems used in protocol-adjacent paths:
- `catalyst-core::protocol` — spec-shaped protocol artifacts (transactions, LSU structures, selection helpers)
- `catalyst-consensus::types` — consensus engine + network-facing message structs (ProducerQuantity/Candidate/Vote/Output)

Without an explicit boundary, it’s easy to accidentally:
- hash/sign different encodings of “the same” concept
- duplicate rules (validation, ordering, deterministic hashing)
- introduce incompatible wire encodings

## Decision: canonical wire boundary (Wire v1)

### 1) Canonical wire frame

The canonical network frame for peer-to-peer messaging is:
- `catalyst_utils::network::MessageEnvelope`

Wire encoding on the transport:
- **`bincode(MessageEnvelope)`** (see `catalyst-network` simple/libp2p services).

Wire versioning:
- `MessageEnvelope.version` (defaults to `PROTOCOL_VERSION`).

Envelope signing preimage:
- `bincode(MessageEnvelope-without-signature)` (see ADR 0001).

### 2) Canonical wire payload types

The bytes inside `MessageEnvelope.payload` are produced by:
- `NetworkMessage::serialize()` of the concrete message type.

Wire v1 rule:
- every network message type MUST have a stable round-trip test for its payload encoding.

### 3) Internal (non-wire) types

Internal types are those that are *not* a stable wire contract:
- consensus engine internal state (e.g., caches, collectors, timers)
- node-local plumbing types and channels
- `catalyst-network::messaging::NetworkMessage` wrapper (libp2p routing metadata), which is an internal transport convenience and not a consensus artifact

## Ownership

- `catalyst-utils` owns:
  - `MessageEnvelope`, `MessageType`, `NetworkMessage` trait
  - wire version constants
- `catalyst-core` owns:
  - protocol-shaped types intended to converge toward the spec (“single source of truth”)
  - shared consensus-critical hashing helpers (e.g., txid)
- `catalyst-consensus` owns:
  - the executable consensus engine
  - **temporary** wire payload structs for consensus phases until we converge them into `catalyst-core::protocol`

## Convergence plan (incremental, low risk)

1) **Document & test the current wire contract** (this ADR + round-trip tests).
2) For each consensus phase message:
   - introduce an equivalent spec-shaped type in `catalyst-core::protocol` (or a dedicated `catalyst-wire` module/crate if we want strict separation)
   - add conversion glue (old ↔ new) and keep both temporarily
3) Switch network/engine code to the new canonical type.
4) Remove the old `catalyst-consensus::types` duplicates once stable.

This keeps the system runnable while steadily reducing duplication and ambiguity.

