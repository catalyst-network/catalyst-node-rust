# Catalyst Docs

These docs are written against the current `catalyst-node-rust` implementation (CLI: `catalyst-cli`).

## Mainnet readiness

- **Pre-deploy checklist (executable gates):** [`mainnet-pre-deploy-checklist.md`](./mainnet-pre-deploy-checklist.md)
- **Roadmap + GitHub epics (`#260`–`#284`):** [`mainnet-roadmap.md`](./mainnet-roadmap.md)
- **#275 / C2 reset–recovery evidence (retrospective template):** [`evidence/track275-reset-recovery-evidence.md`](./evidence/track275-reset-recovery-evidence.md)
- **Phase D / #264–#277 release + upgrade evidence:** [`evidence/phase-d-release-engineering-evidence.md`](./evidence/phase-d-release-engineering-evidence.md)
- **Phase E / #278–#280 operations (keys, launch, monitoring):** [`evidence/phase-e-operations-evidence.md`](./evidence/phase-e-operations-evidence.md)

## Guides

- **Node operators**: [`node-operator-guide.md`](./node-operator-guide.md)
  - Build + run a local node
  - Build + run a 3-node public testnet (Vultr-style) with EU as boot + RPC
  - Firewall ports and common pitfalls

- **Network identity**: [`network-identity.md`](./network-identity.md)
  - `chain_id` / `network_id` meaning and rules
  - Public numbering + naming scheme

- **Users**: [`user-guide.md`](./user-guide.md)
  - Using the dev/test faucet
  - Sending a simple payment transaction
  - Deploying + calling an EVM contract (REVM-backed)

- **Testers**: [`tester-guide.md`](./tester-guide.md)
  - Health checks (status/head advancing)
  - RPC checks, peer checks, and basic troubleshooting

- **Builders**: [`builder-guide.md`](./builder-guide.md)
  - RPC methods currently implemented
  - Contract/runtime notes and current limitations

- **Tokenomics model**: [`tokenomics-model.md`](./tokenomics-model.md)
  - Canonical v1 economics parameters
  - Locked `0 KAT` genesis policy + fixed issuance model
  - Implementation mapping for engineering and documentation agents

- **Tokenomics validation**: [`tokenomics-testnet-validation.md`](./tokenomics-testnet-validation.md)
  - Clean testnet reset workflow
  - Issuance/reward verification checks
  - Regression checks to confirm nothing else broke

- **Tokenomics website/docs handoff**: [`tokenomics-explainer-handoff.md`](./tokenomics-explainer-handoff.md)
  - Copy-ready explainer and FAQ points
  - Canonical parameter list + phrasing guidance for agents
  - Operator verification snippet using RPC

- **Wallets / Integrators**: [`wallet-interop.md`](./wallet-interop.md)
  - Canonical address format
  - v1 transaction signing payload + wire encoding

- **Wallet builder handoff (testnet)**: [`wallet-builder-handoff-catalyst-testnet.md`](./wallet-builder-handoff-catalyst-testnet.md)
  - Live `catalyst-testnet` endpoint + chain identity
  - Practical RPC flow for wallets (account/nonce/fees/send/receipts)

- **Explorer / Indexer handoff**: [`explorer-handoff.md`](./explorer-handoff.md)
  - Chain model + compatibility notes
  - Practical indexing loop (blocks-by-range + receipts)

- **Sync / Fast sync**: [`sync-guide.md`](./sync-guide.md)
  - Snapshot-based node restore workflow
  - Verifying chain identity + head

- **Security threat model**: [`security-threat-model.md`](./security-threat-model.md)
  - Mainnet attacker assumptions and trust boundaries
  - Threat matrix mapped to controls and test/review gates

- **Security external review scope**: [`security-external-review-scope.md`](./security-external-review-scope.md)
  - In-scope components for security review under the no-budget launch model
  - Severity policy and remediation evidence checklist

- **Security dependency updates**: [`security-dependency-updates.md`](./security-dependency-updates.md)
  - High/critical dependency bumps (libp2p, rustls/aws-lc, quinn, yamux) and known residuals

- **Security adversarial evidence**: [`security-adversarial-evidence.md`](./security-adversarial-evidence.md)
  - Reproducible command log for CI-suitable adversarial checks
  - WAN/chaos execution status and outcomes

- **WAN soak/load/chaos gate**: [`wan-soak-load-chaos-gate.md`](./wan-soak-load-chaos-gate.md)
  - Thresholds and execution checklist for `#274`
  - Evidence template for soak/load/chaos runs
  - Archived logs (zip): [`evidence/track274-wan-gate-evidence.zip`](./evidence/track274-wan-gate-evidence.zip) — [`evidence/README.md`](./evidence/README.md)

## Important implementation notes (current state)

- **Consensus traffic is P2P on `30333/tcp`**, not RPC. RPC is only for client interaction.
- **Faucet** is optional and disabled by default in protocol config (fair-launch / `0 KAT` genesis baseline).
- **Tokenomics** baseline is documented in `docs/tokenomics-model.md`.

