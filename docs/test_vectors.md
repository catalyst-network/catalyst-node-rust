### Catalyst test vectors (Wire v1 / public testnet MVP)

These vectors are used to **lock down determinism** for consensus-critical hashing and selection.
They are asserted in tests so changes to encoding/hashing/selection cannot slip in unnoticed.

Spec references:
- Consensus v1.2 §4.2–§4.5 (transaction structure/signature/validity): `https://catalystnet.org/media/CatalystConsensusPaper.pdf`
- Consensus v1.2 §2.2.1 (producer selection): `https://catalystnet.org/media/CatalystConsensusPaper.pdf`

## Vector 1 — Transaction signing payload

Definition (current implementation):
- `payload = bincode(TransactionCore) || timestamp_le`

Inputs:
- `tx_type = NonConfidentialTransfer`
- entries:
  - from: `0x01…01` (32 bytes of `0x01`), amount `-5`
  - to: `0x02…02` (32 bytes of `0x02`), amount `+5`
- `nonce = 1`
- `lock_time = 0`
- `fees = 0`
- `data = []`
- `timestamp = 123`

Expected:
- `tx_signing_payload_hex = 0x000000000200000000000000010101010101010101010101010101010101010101010101010101010101010100000000fbffffffffffffff0202020202020202020202020202020202020202020202020202020202020202000000000500000000000000010000000000000000000000000000000000000000000000000000007b00000000000000`

## Vector 2 — Transaction id (txid)

Definition (current implementation):
- `tx_id = blake2b_256(bincode(Transaction))`

Inputs:
- same as Vector 1, with signature bytes set to 64 zeros

Expected:
- `tx_id_hex = 0x40385854022fe17953c55fddd2fdd62a9ab0baa830282ec143460b64534e9177`

## Vector 3 — Producer selection ordering (Id_i XOR r_{n+1})

Definition (current implementation):
- `r_{n+1} = blake2b_256(seed || DOMAIN_TAG)` (see ADR 0003)
- `u_i = Id_i XOR r_{n+1}`
- sort by `u_i` ascending; choose first `P`

Inputs:
- `seed = 0x2a…2a` (32 bytes of `0x2a`)
- worker pool: 10 ids where `id[i] = [i, 0, 0, …]` for i=0..9
- `P = 4`

Expected:
- selected producers (in order):
  - `0x0800…00`
  - `0x0900…00`
  - `0x0200…00`
  - `0x0300…00`

