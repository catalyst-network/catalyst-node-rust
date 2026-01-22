### ADR 0005 — Canonical transaction derivations (txid, sender, lock_time) (B1)

Status: **Accepted (public testnet MVP)**

Related issues:
- #137 (Milestone B — Transaction pipeline)

Spec references:
- Consensus v1.2 §4.2–§4.5: `https://catalystnet.org/media/CatalystConsensusPaper.pdf`

## Context / Problem

Multiple parts of the codebase need to derive the same values from the same transaction:
- tx id (dedupe / persistence key)
- sender identity (nonce sequencing)
- lock-time interpretation (not-before)

Historically this logic was duplicated across RPC + node code, which risks divergence.

## Decision (current MVP rules)

Canonical tx object:
- `catalyst_core::protocol::Transaction`

Canonical derivations:
- **txid**: `blake2b_256(bincode(Transaction))` via `catalyst_core::protocol::transaction_id`
- **sender pubkey** (single-sender rule): `catalyst_core::protocol::transaction_sender_pubkey`
  - registrations / smart contracts: `entries[0].public_key`
  - transfers: the (single) pubkey with a negative non-confidential amount
  - multi-sender is rejected for now (returns `None`)
- **lock_time**: interpreted as unix seconds “not before”
  - `transaction_is_unlocked(tx, now_secs)` and `validate_basic_and_unlocked(tx, now_secs)`

## Consequences

- RPC, node, and mempool reuse the same derivation rules.
- Future work (aggregated signatures, confidential transfers) can evolve behind these APIs without duplicating rules.

