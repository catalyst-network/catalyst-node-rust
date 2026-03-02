## Prompt: scaffold `catalyst-contracts` (contracts repo) for Catalyst EVM deployments

You are building a new repository named **`catalyst-contracts`** intended to hold Catalyst’s public Solidity contracts (starting with CNS/name service), plus a ready-to-run workflow for compiling and deploying them to Catalyst.

This repo should be structured so it works well with:
- Foundry (primary)
- Hardhat (secondary)
- Catalyst-native deployment tooling (`catalyst-deploy` / `catalyst-cli deploy`)

### Goals (MVP)

- A clean contract workspace with:
  - `src/` Solidity sources
  - `test/` unit tests
  - `script/` deploy scripts (Foundry)
  - `artifacts/` ignored by git (optional)
- A minimal “hello contract” that can be deployed and called.
- A first “real” folder reserved for CNS contracts:
  - `src/cns/` (placeholders are fine)
  - Keep contracts modular (namehash, registry, resolver) so they can evolve.
- A documented deployment process that targets Catalyst RPC and Catalyst transaction format.

### Non-goals (for MVP)

- Do not implement ENS/CNS business logic unless explicitly provided.
- Do not require an Ethereum node; Catalyst has its own RPC.

### Repo layout (recommended)

- `foundry.toml`
- `package.json` (optional, only if Hardhat or tooling is included)
- `src/`
  - `examples/Counter.sol`
  - `cns/` (directory reserved for CNS suite)
- `test/`
  - `Counter.t.sol`
- `script/`
  - `DeployCounter.s.sol` (produces initcode + prints constructor args encoding)
- `docs/`
  - `deploy.md`

### Deployment workflow (must be easy)

Because Foundry cannot broadcast to Catalyst via Ethereum JSON-RPC, the workflow should be:

1. Compile with Foundry to produce an artifact JSON:
   - `forge build`
2. Deploy using Catalyst tooling:
   - `catalyst-deploy --rpc-url ... --key-file ... --artifact out/.../Counter.json --wait --verify-code`
   - OR `catalyst-cli deploy ...` if `catalyst-deploy` is not available.

Include this as copy/paste docs.

### Required documentation (`docs/deploy.md`)

- Prereqs: `forge`, `node` (if needed), and Catalyst deploy tool
- How to create a `wallet.key` (32-byte hex private key file)
- How to compile (`forge build`)
- How to deploy from Foundry artifact JSON
- How to verify deployment (`catalyst_getCode`)
- How deterministic contract addresses are computed on Catalyst (brief)

### Testing

- Unit tests should run with `forge test`.
- Add one optional “live deploy smoke test” script that is only run when env vars are set:
  - `CATALYST_RPC_URL`
  - `CATALYST_KEY_FILE`

### Deliverables checklist

- Contract workspace compiles with `forge build`
- Unit tests pass with `forge test`
- `docs/deploy.md` gives an end-to-end deploy walkthrough for Catalyst
- CNS directory exists and is ready to receive the real contract set

