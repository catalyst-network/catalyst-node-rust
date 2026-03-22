# Mainnet readiness roadmap (working)

**Executable checklist:** [`mainnet-pre-deploy-checklist.md`](./mainnet-pre-deploy-checklist.md)

This doc translates “what’s left before mainnet” into trackable epics/issues in this repo.
It’s intentionally pragmatic: **public testnet stability + developer usability first**, then economics + launch ceremony.

Canonical tracking:

- **Umbrella**: `#260` (mainnet launch readiness program tracker)
- **Milestone**: `Mainnet Launch Readiness`

## Current status (as of this doc)

- Public testnet is running; wallet + explorer are live.
- Recent hardening work focused on WAN reliability, sync continuity, and backfill (see `Mainnet Milestone A` in GitHub milestones).

## Workstreams (mapped to your list)

### 1) Tokenomics + economics hardening

- **Epic**: `#261`
- **Fee routing + parameter surfaces**: `#268`
- **Deterministic tokenomics tests**: `#269`
- **Anti-sybil eligibility controls**: `#270`

### 2) Security gate

- **Epic**: `#262`
- **Threat model**: `#271`
- **Adversarial tests**: `#272`
- **Security review scope/remediation + disclosure policy**: `#273`

### 3) Reliability/performance gate

- **Epic**: `#263`
- **WAN soak/load/chaos thresholds**: `#274`
- **Repeated reset/recovery reliability report**: `#275`

### 4) Release engineering + upgrade safety

- **Epic**: `#264`
- **Reproducible builds + provenance/SBOM**: `#276`
- **Upgrade matrix + rollback validation**: `#277`

### 5) Operations + launch ceremony

- **Epic**: `#265`
- **Key management runbook**: `#278`
- **Genesis/launch ceremony runbook**: `#279`
- **Monitoring/alerting + incident response**: `#280`

### 6) RPC/indexer/wallet compatibility

- **Epic**: `#266`
- **RPC coverage + consistency tests**: `#281`
- **Indexer/subgraph integration reference**: `#282`

### 7) EVM production compatibility

- **Epic**: `#267`
- **ABI/log/receipt fidelity**: `#283`
- **Dapp smoke suite + compatibility matrix**: `#284`

## Additions I recommend (not explicitly in the list, but mainnet-critical)

- **No-budget security model**: launch can proceed without paid audit if reproducible adversarial and reliability evidence gates are complete.
- **Security review/disclosure scope** is tracked in `#273` (community reporting + triage workflow).
- **Monitoring/alerting** is tracked in `#280`.
- **Genesis / launch runbook** is tracked in `#279`.

## Suggested ordering (high leverage)

1. Tokenomics hardening (`#261`) + security gate kickoff (`#262`).
2. Reliability/performance gate execution (`#263`).
3. Release engineering + ops runbooks (`#264`, `#265`).
4. RPC/indexing compatibility completion (`#266`).
5. EVM compatibility gate (`#267`).
6. Final launch ceremony sign-off (`#279`) and umbrella close (`#260`).

