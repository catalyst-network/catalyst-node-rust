# Mainnet readiness roadmap (working)

This doc translates “what’s left before mainnet” into trackable epics/issues in this repo.
It’s intentionally pragmatic: **public testnet stability + developer usability first**, then economics + launch ceremony.

## Current status (as of this doc)

- Public testnet is running; wallet + explorer are live.
- Recent hardening work focused on WAN reliability, sync continuity, and backfill (see `Mainnet Milestone A` in GitHub milestones).

## Workstreams (mapped to your list)

### 1) Faucet sustainability (top-up + web UI + anti-abuse)

- **Hosted faucet service**: `#220`
- **Top-up + custody strategy**: `#233`
- **Public faucet web app + abuse controls**: `#234`
- Wallet integration notes: `docs/wallet-faucet-integration-catalyst-testnet.md`

### 2) “Install and run a node” should be easy (distribution)

- **Node installers/packages (Windows/macOS/Ubuntu)**: `#235`
- **Reproducible builds + upgrade policy**: `#207`
- Operator runbooks: `#206`, `#205` and `docs/node-operator-guide.md`

### 3) Indexing / subgraph path for developers

- **Indexer epic**: `#189`
- **Self-hosted subgraph compatibility plan**: `#236`
- **RPC completeness epic** (needed for indexing): `#187`
- Explorer compatibility notes: `docs/explorer-handoff.md`

### 4) Smart contract reliability (real dapps)

- **EVM ABI/logs/receipts improvements**: `#209`
- **Dapp compatibility test suite (Uniswap/Jeskei)**: `#237`

### 5) Web2 developer ergonomics (beyond raw RPC)

- **Web2-friendly APIs (service bus/webhooks/REST)**: `#238`
- Service bus crate: `crates/catalyst-service-bus/`

### 6) Economics + mainnet launch

- **Economics spec**: `#185`
- **Economics implementation**: `#184`

### 7) Token naming / identity freeze (product + client safety)

- **Token name/symbol decision + chain identity freeze**: `#239`
- Identity notes: `docs/network-identity.md`

### 8) Developer tooling + docs + training

- **DevRel package (SDKs/examples/docs)**: `#240`

### 9) Testing (beyond unit tests)

- **Fuzz/property tests**: `#202`
- **Threat model + adversarial plan**: `#201`
- **WAN soak/load/chaos harness**: `#241`
- Release gate milestone: GitHub milestone “Milestone G — Testing + release gate”

## Additions I recommend (not explicitly in the list, but mainnet-critical)

- **Security review/audit scope** (even a lightweight external review): start from `#201` and define a minimum audit plan.
- **Monitoring/alerting** for validators/RPC/indexers (SLOs): peer count, cycle liveness (“no LSU produced”), disk growth, RPC rate limits.
- **Genesis / launch runbook**: key ceremony, parameter freeze, rollback plan (partially covered by `#206`, but should be explicit for mainnet).

## Suggested ordering (high leverage)

1. Finish **reliable join / backfill / safety limits** (Mainnet Milestone A).
2. Faucet service + web UI (make testnet usable at scale).
3. RPC + indexing + devrel basics (make builders productive).
4. EVM compatibility suite + missing RPC pieces (enable real contracts).
5. Soak/load/chaos testing + release policy (prove stability).
6. Economics finalization + mainnet launch ceremony.

