# Builder guide (RPC surface, integration notes, current limitations)

This guide is for developers integrating with the current testnet implementation.

## Architecture (current)

- **P2P (`30333/tcp`)**: consensus + transaction gossip between nodes
- **RPC (`8545/tcp`)**: HTTP JSON-RPC for clients (submit tx, query balances/head, EVM helpers)

Consensus traffic is not carried over RPC.

## RPC methods implemented (as of `catalyst-rpc/0.1.0`)

Core:
- `catalyst_version`
- `catalyst_peerCount`
- `catalyst_head`
- `catalyst_blockNumber` (mapped to applied cycle)

Accounts:
- `catalyst_getBalance` (returns decimal string)
- `catalyst_getBalanceProof` (balance + state root + Merkle proof steps)
- `catalyst_getNonce`

Tx submission:
- `catalyst_sendRawTransaction`
  - payload is **hex-encoded bincode(Transaction)** (dev transport)
  - returns a `tx_id` (sha256 over tx bytes)

EVM helpers (dev/test):
- `catalyst_getCode` (20-byte address)
- `catalyst_getLastReturn` (20-byte address)
- `catalyst_getStorageAt` (20-byte address, 32-byte slot)

## Transaction model (current)

The CLI constructs `catalyst_core::protocol::Transaction` objects and submits them as bincode bytes.

Important behavior:
- RPC verifies Schnorr signature and rejects “nonce too low”.
- Inclusion happens via validator consensus cycles; acceptance at RPC does not guarantee immediate inclusion.

## Faucet model (dev/test)

The faucet is a deterministic, pre-funded account initialized by the node:
- private key bytes are `[0xFA; 32]`
- initial balance is `1_000_000` (if missing in state)

This is *not* a contract faucet (no ERC20, no mint function).

## EVM execution (current)

The node applies EVM deploy/call during LSU application using REVM and persists:
- runtime code at `evm:code:<addr20>` (enables `catalyst_getCode`)
- storage slots at `evm:storage:<addr20><slot>`
- last return data at `evm:last_return:<addr20>` (enables `catalyst_getLastReturn`)

### Practical tip: use a fresh funded key for EVM

In the current implementation, EVM tx nonce is derived from the **protocol nonce**.
If you send multiple “payment” txs from the faucet key and then try EVM deploys from the same key,
you can hit nonce mismatches. The recommended workflow is:

1) create a fresh wallet key
2) fund it from the faucet
3) deploy/call using the fresh wallet key

See [`user-guide.md`](./user-guide.md).

## Tokenomics / fees (current)

Current behavior is scaffolding-oriented:
- balances are integer values stored under `bal:<pubkey>`
- transfers are validated with a simple “no negative balances” rule
- transaction `fees` are set to `0` in the CLI
- EVM executes with `gas_price = 0` (no fee charging yet)

There is no implemented issuance schedule, staking rewards, inflation, or burn mechanics in this repo snapshot.

## Known limitations

- RPC does not currently return rich transaction/block data (e.g. `getTransactionByHash` returns `None`).
- RPC peer count reflects the RPC node’s current network connections; it is not always a full picture of the validator mesh.
- Logging config includes a `file_path`, but the node primarily logs to stdout/stderr via `tracing` in the current setup.

