### ADR 0001 — Canonical encoding and wire versioning (Wire v1)

Status: **Accepted (for public testnet MVP)**

Related issues:
- #126 (Milestone A — Spec pinning + canonical boundaries)

Spec references:
- Consensus v1.2 §4.2–§4.5 (transactions: structure/signature/validity): `https://catalystnet.org/media/CatalystConsensusPaper.pdf`
- Consensus v1.2 §5.2 (protocol messages across phases): `https://catalystnet.org/media/CatalystConsensusPaper.pdf`

## Context / Problem

This codebase currently uses multiple encodings at different boundaries:
- `bincode` for some protocol-shaped messages and RPC transports
- `catalyst-utils` `CatalystSerialize` for many internal/wire-ish structs
- JSON for some signing preimages (historical convenience)

Without a clear, written policy, nodes can disagree on:
- hashing preimages
- signature payloads
- wire message decoding/versioning

This is unacceptable for public testnet.

## Decision

### 1) Canonical encoding for signature/hashing preimages

**Wire v1** canonical rule:
- **Never** use JSON for consensus/security preimages.
- Use **binary, deterministic encodings** only.

Concrete choices:
- **Transaction signing payload**: defined in `catalyst-core` as `bincode(TransactionCore) || timestamp_le` (temporary, until aggregated signature scheme is fully implemented).
- **Transaction id**: defined in `catalyst-core` as `blake2b_256(bincode(Transaction))` (dev/testnet stable id for dedupe).
- **Message envelope signing preimage**: `bincode(MessageEnvelope-without-signature)` (implemented in `catalyst-utils`).

### 2) Canonical encoding for wire messages

Wire message structure:
- Outer envelope is `catalyst-utils::network::MessageEnvelope`
- Inner payload is `payload: Vec<u8>` produced by `NetworkMessage::serialize()`

**Wire v1** policy:
- The envelope signing preimage is **bincode**.
- Each `NetworkMessage` implementation must have a **stable round-trip encoding test**.
- New network message types SHOULD prefer a single encoding (default recommendation: `bincode`), unless there is a strong reason to use `CatalystSerialize`.

### 3) Wire versioning policy

Wire version is carried in `MessageEnvelope.version` and defaults to `PROTOCOL_VERSION`.

**Wire v1** compatibility policy:
- Nodes MUST drop envelopes with a `version` they don’t support.
- Backward/forward compatibility is not guaranteed until we implement explicit version negotiation in the handshake.
- Any breaking wire change must bump `PROTOCOL_VERSION` and update handshake negotiation.

## Consequences

Pros:
- Deterministic signing/hashing inputs
- Clear policy for future message additions
- Prevents accidental reintroduction of JSON-based preimages

Cons:
- Cross-version compatibility remains a future task (requires negotiated versions and/or typed versioned messages).

