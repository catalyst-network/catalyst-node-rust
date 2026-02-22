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
- `catalyst_chainId`
- `catalyst_networkId`
- `catalyst_genesisHash`

## Chain identity (clients)

Clients should treat the tuple \(`chain_id`, `network_id`, `genesis_hash`\) as the network identity.

- `chain_id` is used for wallet v1 signing and EVM domain separation.
- `network_id` is a human-readable name.
- `genesis_hash` is used in the wallet v1 signing payload and must be stable.

See [`network-identity.md`](./network-identity.md) for the public numbering scheme used by this project.

Accounts:
- `catalyst_getBalance` (returns decimal string)
- `catalyst_getBalanceProof` (balance + state root + Merkle proof steps)
- `catalyst_getNonce`
- `catalyst_getAccount`

Tx submission:
- `catalyst_sendRawTransaction`
- payload is either:
  - **v1 canonical wire tx**: `0x` + hex(`"CTX1"` + canonical `Transaction` bytes)
  - legacy dev transport: `0x` + hex(`bincode(Transaction)`)
- returns a `tx_id` (32-byte hex): `blake2b512(CTX1||tx_bytes)[..32]`

Tx query / receipts:
- `catalyst_getTransactionByHash`
- `catalyst_getTransactionReceipt`
- `catalyst_getTransactionInclusionProof`
- `catalyst_getBlocksByNumberRange`
- `catalyst_getTransactionsByAddress`

EVM helpers (dev/test):
- `catalyst_getCode` (20-byte address)
- `catalyst_getLastReturn` (20-byte address)
- `catalyst_getStorageAt` (20-byte address, 32-byte slot)

## Transaction model (current)

The CLI constructs `catalyst_core::protocol::Transaction` objects and submits them as **v1 canonical wire bytes**.

Important behavior:
- RPC verifies Schnorr signature and rejects “nonce too low”.
- Inclusion happens via validator consensus cycles; acceptance at RPC does not guarantee immediate inclusion.

For external wallets/integrators, see [`wallet-interop.md`](./wallet-interop.md) for the v1 encoding + signing payload.

## Faucet model (dev/test)

The node supports an optional faucet account seeded at genesis for dev/test usage:
- **dev/local default**: deterministic faucet key (`[0xFA; 32]`) with a configurable initial balance
- **public networks**: disable the deterministic faucet and configure a real faucet/treasury pubkey
  and large initial balance in `protocol.*` config

The deterministic faucet key is **public** (embedded in the repo), so it must not be used as the
funding source for any long-lived public network.

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
- transaction `fees` are enforced via a deterministic minimum fee schedule
- EVM executes with `gas_price = 0` (fee charging is at the protocol layer for now)

There is no implemented issuance schedule, staking rewards, inflation, or burn mechanics in this repo snapshot.

## Known limitations

- Explorer-grade indexing is still best-effort (indexers should use block ranges + receipts).
- RPC peer count reflects the RPC node’s current network connections; it is not always a full picture of the validator mesh.
- Logging config includes a `file_path`, but the node primarily logs to stdout/stderr via `tracing` in the current setup.

