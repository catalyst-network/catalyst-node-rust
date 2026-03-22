# Phase G — EVM production compatibility (**#267**, **#283**, **#284**)

This document supports closing checklist **Phase G** in [`mainnet-pre-deploy-checklist.md`](../mainnet-pre-deploy-checklist.md).

**Scope:** Phase G applies if **mainnet must support production-style EVM dapps**. If mainnet is **not** EVM-critical for v1, you may mark **N/A** below and defer **#267–#284** with rationale on **#260**.

---

## G1 — ABI / logs / receipt fidelity (**#267**, **#283**)

### Implementation references (repo)

| Topic | Doc / code path |
|--------|------------------|
| Deploy / call via CLI | [`evm-deploy.md`](../evm-deploy.md) — `catalyst-cli deploy` / `call`, CREATE address rule, **`catalyst_getTxDomain`** for tooling |
| RPC (EVM helpers) | [`builder-guide.md`](../builder-guide.md) — `catalyst_getCode`, `catalyst_getLastReturn`, `catalyst_getStorageAt`, receipts |
| Receipt fields | [`explorer-handoff.md`](../explorer-handoff.md) — `catalyst_getTransactionReceipt` (`status`, `applied_cycle`, `success`, `gas_used`, `return_data`, …) |
| Regression harness | `make testnet-contract-test` — deploys [`testdata/evm/return_2a_initcode.hex`](../../testdata/evm/return_2a_initcode.hex), verifies code + return data |

### G1 — Verification checklist (fill in)

- [ ] **`catalyst_getTxDomain`** returns consistent `chain_id` / `genesis_hash` / `network_id` on mainnet RPC (matches published identity).
- [ ] **Receipt** after EVM tx: `status` progresses to `applied`; `success` / `error` / `gas_used` / `return_data` match expectation for a known contract (smoke).
- [ ] **Logs / topics** (if you rely on them): tested with target toolchain — notes: …

| Name | Date |
|------|------|
| | |

---

## G2 — Dapp smoke + compatibility matrix (**#284**)

### Suggested matrix (fill rows you care about)

| Toolchain | Action tested | Commit / tag | Result | Notes |
|-----------|---------------|----------------|--------|--------|
| `catalyst-cli` + local initcode | deploy + call | … | ☐ pass | default CI path |
| Foundry artifact JSON → `deploy` | deploy | … | ☐ | see [`evm-deploy.md`](../evm-deploy.md) |
| Hardhat artifact JSON → `deploy` | deploy | … | ☐ | same pattern |
| Custom wallet | CTX2 + EVM | … | ☐ | |

### G2 — Sign-off

- [ ] At least one **end-to-end** path (deploy + call or equivalent) on **mainnet-equivalent** RPC (staging or mainnet).
- [ ] **Known limitations** documented for integrators (link issue or doc).

| Name | Date |
|------|------|
| | |

---

## N/A — EVM not a launch gate

If Phase G is **out of scope** for v1:

- **Reason:** …
- **Tracked follow-up issue:** …

| Name | Date |
|------|------|
| | |

---

## Close on GitHub

Comment on **#267**, **#283**, **#284** (or **#260**):

*“Phase G evidence: `docs/evidence/phase-g-evm-evidence.md` @ \<commit\>”*  
(or *“Phase G N/A: …”*)

---

## Related

- [`user-guide.md`](../user-guide.md) — EVM examples if present
- [`builder-guide.md`](../builder-guide.md) — full RPC list
