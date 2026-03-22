# Phase H — Final launch (**#279**, **#260**)

This document supports closing checklist **Phase H** in [`mainnet-pre-deploy-checklist.md`](../mainnet-pre-deploy-checklist.md).

**H1** overlaps with the genesis / launch material in [`phase-e-operations-evidence.md`](./phase-e-operations-evidence.md) §E2 — use **one** published source of truth (this file or E2); link between them.

---

## H1 — Mainnet identity published; ceremony complete (**#279**)

### Recommended identifiers (from [`network-identity.md`](../network-identity.md))

| Field | Mainnet (proposed) |
|-------|---------------------|
| `chain_id` (decimal) | `200820091` |
| `network_id` | `catalyst-mainnet` |

**Replace** if your governance chose different values **before** genesis.

### Published mainnet record (fill in at launch)

| Item | Value / URL |
|------|-------------|
| **Release binary tag** | `v_____` |
| **Git commit** | `__________` |
| **`genesis_hash`** (from RPC `catalyst_genesisHash`) | `0x...` |
| **Public RPC HTTPS** (read / broadcast) | … |
| **Wallets / explorers** notified (links) | … |
| **Date / time (UTC) genesis / go-live** | … |

### Ceremony checklist

- [ ] All validators / operators used **identical** `chain_id`, `network_id` (and resulting **genesis**) for this network.
- [ ] **`catalyst_getSyncInfo`** (or equivalent) matches published **`genesis_hash`** on at least one public RPC.
- [ ] No accidental **fork** (two different genesis hashes advertised).

### H1 — Sign-off

| Role | Name | Date |
|------|------|------|
| Launch authority | | |

---

## H2 — Close umbrella **#260** (mainnet readiness program)

### Option A — Program complete

- [ ] Phases **A–G** satisfied or **explicitly N/A** with linked evidence docs.
- [ ] Remaining Dependabot / `cargo audit` items **accepted** or **tracked** per [`security-dependency-updates.md`](../security-dependency-updates.md).

**Closing comment for #260** (paste):

```text
Mainnet launch readiness program complete. Checklist: docs/mainnet-pre-deploy-checklist.md @ <commit>.
Evidence index: docs/evidence/README.md
```

### Option B — Defer part of program

| Deferred item | Issue # | Rationale | New target |
|---------------|---------|-----------|------------|
| | | | |

**Closing or comment for #260** (paste):

```text
Closing #260 as <complete with exceptions | deferred>. Rationale: <short>. Follow-ups: #...
```

### H2 — Sign-off

| Role | Name | Date |
|------|------|------|
| Program owner | | |

---

## Related

- [`network-identity.md`](../network-identity.md)
- [`mainnet-roadmap.md`](../mainnet-roadmap.md) — umbrella **#260**
- [`phase-e-operations-evidence.md`](./phase-e-operations-evidence.md) — E2 genesis ceremony
