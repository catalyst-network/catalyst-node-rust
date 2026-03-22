# Mainnet pre-deploy checklist (executable)

Use this document to **drive execution** before cutting a public mainnet. It consolidates [`mainnet-roadmap.md`](./mainnet-roadmap.md) into **ordered gates** with pointers to evidence and runbooks.

**Umbrella tracker:** GitHub `#260` — **Milestone:** `Mainnet Launch Readiness`

---

## How to use

1. Work **top to bottom** unless a dependency blocks you (e.g. security findings before reliability sign-off).
2. For each gate, attach **evidence**: commit SHA, run logs, dates, operator names (or links to issues/PRs).
3. **Do not** change `chain_id`, `network_id`, or `genesis_hash` for an already-announced mainnet without treating it as a **new chain** — see [`network-identity.md`](./network-identity.md), [`protocol-params.md`](./protocol-params.md).

---

## Phase A — Economics & tokenomics

| # | Item | Evidence / doc | GitHub |
|---|------|----------------|--------|
| A1 | Tokenomics v1 parameters reviewed and frozen for launch (genesis `0 KAT`, issuance, splits, fee routing, fee credits). | [`tokenomics-model.md`](./tokenomics-model.md), [`tokenomics-spec.md`](./tokenomics-spec.md) | `#261`, `#268` |
| A2 | Deterministic tests / edge cases for rewards + fee credits acceptable. | Tests + `#269` closure | `#269` |
| A3 | Anti-sybil / waiting-worker eligibility acceptable for launch risk. | Code + `#270` closure | `#270` |
| A4 | Testnet or staging validation run completed (issuance RPC, progression). | [`tokenomics-testnet-validation.md`](./tokenomics-testnet-validation.md) | — |

**Sign-off:** Name / date: TheNewAutonomy 22nd March 2026

---

## Phase B — Security

| # | Item | Evidence / doc | GitHub |
|---|------|----------------|--------|
| B1 | Threat model read and gaps accepted or closed. | [`security-threat-model.md`](./security-threat-model.md) | `#271` |
| B2 | Adversarial / abuse tests executed; findings triaged per severity policy. | [`security-adversarial-evidence.md`](./security-adversarial-evidence.md), [`adversarial-test-plan.md`](./adversarial-test-plan.md) | `#272` |
| B3 | Review scope + remediation + disclosure workflow agreed. | [`security-external-review-scope.md`](./security-external-review-scope.md) | `#273` |
| B4 | **Critical** findings: fixed + re-verified. **High**: fixed or written acceptance + compensating controls. | Same | `#262` |

**Sign-off:** Name / date: TheNewAutonomy 22nd March 2026

---

## Phase C — Reliability & performance

| # | Item | Evidence / doc | GitHub |
|---|------|----------------|--------|
| C1 | WAN soak / load / chaos gate meets thresholds (or failures have follow-up issues). | [`wan-soak-load-chaos-gate.md`](./wan-soak-load-chaos-gate.md) | `#263`, `#274` |
| C2 | Reset / recovery / backfill reliability acceptable for operators. | [`evidence/track275-reset-recovery-evidence.md`](./evidence/track275-reset-recovery-evidence.md) (retrospective OK) | `#275` |

**Sign-off:** Name / date: TheNewAutonomy 22nd March 2026

**Note:** If you reset testnets many times but did not log each run, complete **C2** by filling in the retrospective sections in `evidence/track275-reset-recovery-evidence.md` and linking it when closing **#275**.

---

## Phase D — Release engineering

| # | Item | Evidence / doc | GitHub |
|---|------|----------------|--------|
| D1 | Release build process documented; binaries reproducible or provenance captured as required. | [`release-process.md`](./release-process.md), [`evidence/phase-d-release-engineering-evidence.md`](./evidence/phase-d-release-engineering-evidence.md) | `#264`, `#276` |
| D2 | Upgrade matrix + rollback path tested (coordinated upgrade assumptions documented). | [`evidence/phase-d-release-engineering-evidence.md`](./evidence/phase-d-release-engineering-evidence.md) (matrix + [`node-operator-guide.md`](./node-operator-guide.md)) | `#277` |

**Sign-off:** Name / date: TheNewAutonomy 22nd March 2026

**Note:** Full bit-reproducible builds / SBOM may be optional for v1; this repo documents **tag + CI + `--locked` + SHA256 artifacts** — see the Phase D evidence file. Fill D1/D2 sign-offs and the upgrade matrix there to close the phase.

---

## Phase E — Operations

| # | Item | Evidence / doc | GitHub |
|---|------|----------------|--------|
| E1 | Key management runbook (genesis keys, validator keys, backups, compromise response). | [`node-operator-guide.md`](./node-operator-guide.md), [`evidence/phase-e-operations-evidence.md`](./evidence/phase-e-operations-evidence.md) | `#278` |
| E2 | Genesis / launch ceremony runbook (ordering, abort criteria, comms). | [`evidence/phase-e-operations-evidence.md`](./evidence/phase-e-operations-evidence.md), [`network-identity.md`](./network-identity.md) | `#279` |
| E3 | Monitoring, alerting, on-call / incident response defined. | [`tester-guide.md`](./tester-guide.md), [`evidence/phase-e-operations-evidence.md`](./evidence/phase-e-operations-evidence.md) | `#280` |

**Sign-off:** Name / date: TheNewAutonomy 22nd March 2026

**Note:** Phase E combines **repo docs** with **org-specific** custody, on-call, and launch comms — complete the tables in [`evidence/phase-e-operations-evidence.md`](./evidence/phase-e-operations-evidence.md) to close **#278–#280**.

---

## Phase F — Integrations

| # | Item | Evidence / doc | GitHub |
|---|------|----------------|--------|
| F1 | RPC surfaces needed for wallets/explorers verified; consistency tests green. | [`builder-guide.md`](./builder-guide.md), [`wallet-interop.md`](./wallet-interop.md), [`evidence/phase-f-integrations-evidence.md`](./evidence/phase-f-integrations-evidence.md) | `#266`, `#281` |
| F2 | Indexer / explorer expectations documented for mainnet. | [`explorer-handoff.md`](./explorer-handoff.md), [`evidence/phase-f-integrations-evidence.md`](./evidence/phase-f-integrations-evidence.md) | `#282` |

**Sign-off:** Name / date: TheNewAutonomy 22nd March 2026

**Note:** Complete [`evidence/phase-f-integrations-evidence.md`](./evidence/phase-f-integrations-evidence.md) (mainnet RPC URL + identity table, F1/F2 checkboxes) to close **#266**, **#281**, **#282**.

---

## Phase G — EVM (if mainnet must support production dapps)

| # | Item | Evidence / doc | GitHub |
|---|------|----------------|--------|
| G1 | ABI / logs / receipt fidelity validated for target toolchains. | [`evm-deploy.md`](./evm-deploy.md), [`builder-guide.md`](./builder-guide.md), [`evidence/phase-g-evm-evidence.md`](./evidence/phase-g-evm-evidence.md) | `#267`, `#283` |
| G2 | Dapp smoke suite + compatibility matrix pass. | [`evidence/phase-g-evm-evidence.md`](./evidence/phase-g-evm-evidence.md) | `#284` |

**Sign-off:** Name / date: TheNewAutonomy 22nd March 2026

**Note:** If EVM dapps are **not** a v1 mainnet requirement, complete the **N/A** section in [`evidence/phase-g-evm-evidence.md`](./evidence/phase-g-evm-evidence.md) and defer **#267–#284** with rationale. Otherwise fill G1/G2 and the toolchain matrix.

---

## Phase H — Final launch

| # | Item | Evidence / doc | GitHub |
|---|------|----------------|--------|
| H1 | Mainnet `chain_id` / `network_id` / genesis ceremony executed once; hashes published. | [`network-identity.md`](./network-identity.md), [`evidence/phase-h-final-launch-evidence.md`](./evidence/phase-h-final-launch-evidence.md) | `#279` |
| H2 | Umbrella `#260` closed or explicitly deferred with recorded rationale. | [`evidence/phase-h-final-launch-evidence.md`](./evidence/phase-h-final-launch-evidence.md), GitHub **#260** | `#260` |

**Sign-off (launch authority):** Name / date: _______________

**Note:** Record **published** `genesis_hash`, RPC URLs, and release tag in [`evidence/phase-h-final-launch-evidence.md`](./evidence/phase-h-final-launch-evidence.md), then close **#260** using the template in §H2 (or defer with rationale).

---

## Dependency hygiene (ongoing)

- **Supply chain:** keep Dependabot / `cargo audit` process per [`security-dependency-updates.md`](./security-dependency-updates.md).

---

## Suggested order (same as `mainnet-roadmap.md`)

1. A + B  
2. C  
3. D + E  
4. F (+ G if required)  
5. H  

---

## Related

- Roadmap + issue mapping: [`mainnet-roadmap.md`](./mainnet-roadmap.md)
