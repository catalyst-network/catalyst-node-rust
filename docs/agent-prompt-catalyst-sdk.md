## Prompt: build `catalyst-sdk` (TypeScript) + deploy tooling

You are building a new public repository named **`catalyst-sdk`** that makes it easy for developers to deploy and interact with EVM smart contracts on **Catalyst**.

Catalyst is **not Ethereum**: it does not accept Ethereum transactions via `eth_sendRawTransaction`, and it uses **Catalyst CTX2 transactions** submitted via `catalyst_sendRawTransaction`. The SDK must hide these differences behind a familiar DX.

### Goals (MVP)

- Provide a **TypeScript SDK** that can:
  - Fetch the **transaction signing domain** from RPC using **one call**: `catalyst_getTxDomain`.
  - Build a **CTX2 SmartContract** transaction for EVM `CREATE` (deploy) and `CALL`.
  - Sign the transaction using a **32-byte private key** (hex) compatible with `wallet.key`.
  - Submit via `catalyst_sendRawTransaction`.
  - Poll `catalyst_getTransactionReceipt` until `applied`/`failed` with timeout.
  - Verify deploy by calling `catalyst_getCode` and ensuring it is non-empty.
- Provide a **CLI** (npm bin) `catalyst-deploy` that:
  - Accepts Foundry and Hardhat **artifact JSON** paths and/or raw bytecode files.
  - Deploys with `--rpc-url`, `--key-file`, `--args`, `--wait`, `--verify-code`.
  - Prints `tx_id` and deterministic `contract_address`.
- Provide a **Hardhat plugin** (`hardhat-catalyst`) that uses the SDK to deploy artifacts and run basic calls.

### Non-goals (for MVP)

- Do not try to make Foundry “broadcast” work by emulating Ethereum JSON-RPC.
- Do not implement Solidity compilation from scratch; rely on Foundry/Hardhat outputs.
- Do not design new on-chain standards in the SDK; just tooling.

### Compatibility constraints (must follow)

- RPC methods used:
  - `catalyst_getTxDomain` → returns `{ chain_id, network_id, genesis_hash, protocol_version, tx_wire_version }`
  - `catalyst_getNonce(0x<pubkey32>)` → returns `u64`
  - `catalyst_sendRawTransaction(0x<bytes>)` → returns `0x<txid32>`
  - `catalyst_getTransactionReceipt(0x<txid32>)` → receipt with `status` and `success`
  - `catalyst_getCode(0x<addr20>)` → `0x...` code
- Transaction type for EVM interactions is `TransactionType::SmartContract`.
- EVM deploy “bytecode” is **EVM initcode**. Constructor args are appended to initcode.
- Deterministic deployed address uses Ethereum CREATE derivation:
  - `evm_sender = last20(pubkey32)`
  - `evm_nonce = protocol_nonce - 1` (protocol nonce starts at 1; EVM nonce starts at 0)

### Repo structure (recommended)

- `packages/sdk/`
  - `src/client.ts` (RPC client)
  - `src/domain.ts` (TxDomain fetch + parsing)
  - `src/tx/` (builders, signing, encoding, txid)
  - `src/evm/` (deploy/call helpers; address derivation)
  - `src/index.ts`
- `packages/cli/`
  - `src/index.ts` (commander or yargs)
  - `src/artifacts.ts` (Foundry/Hardhat artifact parsing)
- `packages/hardhat-plugin/`
  - Hardhat tasks: `catalyst:deploy`, `catalyst:call`
- `packages/examples/` (optional)
  - Minimal deploy + call example

### Artifact parsing requirements

Support these common shapes:

- Foundry:
  - `out/<File>.sol/<Contract>.json`
  - `bytecode.object` is a hex string
- Hardhat:
  - `artifacts/contracts/.../<Contract>.json`
  - `bytecode` may be a hex string

If the artifact lacks bytecode, fail with a clear error.

### Testing (must include)

- Unit tests:
  - Domain parsing
  - Artifact bytecode extraction
  - Address derivation correctness
- Integration test (can be optional behind env var):
  - Requires `CATALYST_RPC_URL` and `CATALYST_KEY_HEX`
  - Deploy a tiny contract and assert `getCode != 0x`

### Deliverables checklist

- `README.md` with:
  - Quickstart: deploy from Foundry artifact
  - Quickstart: deploy from Hardhat artifact
  - Explanation of `wallet.key` format
  - How deterministic contract addresses are computed
- Published packages (local build is fine for now):
  - `@catalyst/sdk`
  - `catalyst-deploy` (bin)
  - `hardhat-catalyst`

