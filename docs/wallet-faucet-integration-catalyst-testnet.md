# Wallet faucet integration (`catalyst-testnet`)

This document is intended for the wallet builder implementing a “Get testnet funds” experience for the **Catalyst public testnet**.

## Summary

On `catalyst-testnet`, the “faucet” is **not** a smart contract.

It is a deterministic, pre-funded **account** that exists in protocol state:
- faucet private key bytes = **`[0xFA; 32]`**
- on node startup, **if missing**, it is initialized with balance **`1_000_000`**

The wallet can fund accounts by sending a normal Catalyst transfer transaction **from the faucet account**.

## Live network parameters

- **RPC_URL**: `http://45.32.177.248:8545`
- **network_id**: `catalyst-testnet`
- **chain_id**: `200820092` (hex: `0xbf8457c`)
- **genesis_hash**: `0xeea16848e6b1d39d6b7a5e094ad9189d5382a6a4b19fb95342ef9846258fee5a`

Always bind signatures to \(`chain_id`, `genesis_hash`\) using the **v1 signing payload**.

## Security note (important)

Embedding a “shared faucet private key” into a production wallet is **not safe**.

Recommended approaches:

1) **Dev-only faucet mode** (acceptable for testnet/dev builds)
   - The wallet includes the deterministic faucet key in **debug/testnet builds only**.
   - The UI should clearly label this as *testnet-only*.

2) **Hosted faucet service** (recommended for public distribution)
   - Run a small service that holds the faucet key server-side.
   - The wallet requests funds by calling the service (service signs + submits tx).

3) **Copy/paste CLI flow**
   - The wallet shows the user a one-liner command to run with `catalyst-cli`.

This repo currently implements option (3) in docs; option (1) is easy for wallet builders; option (2) is what you’ll want long-term.

## Option A: dev-only built-in faucet (wallet signs + submits)

### High-level flow

1) **Confirm chain identity** (and refuse if mismatched):
   - `catalyst_chainId`
   - `catalyst_genesisHash`
   - `catalyst_networkId` (optional but recommended)

2) **Derive faucet pubkey** from the deterministic private key:
   - private key bytes: `fa fa fa ...` (32 bytes)
   - public key: 32-byte compressed Ristretto point
   - wallet “address” text form: `0x` + hex(pubkey32)

3) **Fetch faucet nonce**:
   - `catalyst_getNonce("0x<FAUCET_PUBKEY32>")` → `u64`
   - set tx nonce = `nonce + 1`

4) **Build a transfer transaction** to the user’s pubkey:
   - `tx_type = NonConfidentialTransfer`
   - entries:
     - faucet pubkey: `-amount`
     - user pubkey: `+amount`
   - `timestamp` = now (ms)
   - `lock_time` = now (seconds)
   - `fees`:
     - easiest: call `catalyst_estimateFee(...)` and set `fees` to that value

5) **Sign** using Catalyst v1 signing payload:
   - domain `"CATALYST_SIG_V1"`
   - `chain_id` (u64 le)
   - `genesis_hash` (32 bytes)
   - canonical serialized `TransactionCore`
   - `timestamp` (u64 le)

6) **Submit** as v1 wire bytes:
   - `"CTX1" || canonical_serialize(Transaction)`
   - `catalyst_sendRawTransaction("0x" + hex(bytes))`

7) **Track inclusion**:
   - poll `catalyst_getTransactionReceipt(tx_id)` until `status == "applied"` (or `"dropped"`)

### RPC snippets

Confirm chain identity:

```bash
curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_getSyncInfo","params":[]}'
```

Fetch nonce:

```bash
curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_getNonce","params":["0x<FAUCET_PUBKEY32>"]}'
```

Submit raw tx:

```bash
curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_sendRawTransaction","params":["0x<CTX1_WIRE_HEX>"]}'
```

Get receipt:

```bash
curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_getTransactionReceipt","params":["0x<TX_ID>"]}'
```

## Option B: hosted faucet service (recommended)

Run a minimal HTTPS service that:
- validates request (rate limit, captcha, allowlist, etc.)
- builds/signs/submits a faucet transfer tx
- returns the `tx_id`

Wallet UI:
- button “Get testnet funds”
- service returns `tx_id`
- wallet tracks receipt until applied

This keeps the faucet private key **off** client devices.

## Option C: CLI funding (documented)

This is the documented operator/user flow using `catalyst-cli`:

```bash
python3 -c 'print("fa"*32)' > faucet.key
FAUCET_PK="$(./target/release/catalyst-cli pubkey --key-file faucet.key | tail -n 1)"
./target/release/catalyst-cli send "0x<YOUR_WALLET_PUBKEY32>" 1000 \
  --key-file faucet.key --rpc-url http://45.32.177.248:8545
```

## References

- Wallet v1 encoding/signing rules: `docs/wallet-interop.md`
- Wallet builder handoff (full context): `docs/wallet-builder-handoff-catalyst-testnet.md`
- Signature implementation reference: `crates/catalyst-crypto/src/signatures.rs`

