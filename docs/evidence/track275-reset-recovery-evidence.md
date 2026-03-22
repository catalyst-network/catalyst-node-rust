# Reset / recovery / backfill evidence — issue **#275** (Phase **C2**)

This document closes GitHub **#275** and checklist item **C2** in [`mainnet-pre-deploy-checklist.md`](../mainnet-pre-deploy-checklist.md).

It is valid to complete **retrospectively**: if resets and recoveries were performed during development and testnet operation but not logged at the time, record here **what was done**, **what was observed**, and **residual risk** (if any).

---

## Repository tooling (pre-filled — you do not need to repeat this)

These are the **canonical** reset / validation paths in **this** repo:

| Activity | Where |
|----------|--------|
| Local 3-node harness: bring up / tear down | `make testnet-up`, `make testnet-down` — implemented in [`scripts/netctl.sh`](../../scripts/netctl.sh) |
| **Clean** local state (wipe node data + shared DFS, regenerate configs as needed) | `make testnet-up-clean` |
| Post-reset issuance / RPC checks | [`tokenomics-testnet-validation.md`](../tokenomics-testnet-validation.md) |
| Public-style hard reset (keep `node.key`, wipe `data`) | [`node-operator-guide.md`](../node-operator-guide.md) — section **“Resetting / launching a new network (hard reset)”** |
| Backup / restore CLI | `catalyst-cli db-backup`, `catalyst-cli db-restore` — [`node-operator-guide.md`](../node-operator-guide.md) § *Upgrades, backups, and rollback safety* |
| Snapshot-based fast sync (new node without full replay) | [`sync-guide.md`](../sync-guide.md) |
| Chain identity must stay consistent across a reset unless intentionally launching a **new chain** | [`protocol-params.md`](../protocol-params.md), [`network-identity.md`](../network-identity.md) |

**Testnet faucet note:** Under `testnet/` paths, [`crates/catalyst-cli/src/config.rs`](../../crates/catalyst-cli/src/config.rs) enables deterministic faucet funding for dev harnesses (`protocol.faucet_mode = deterministic`, `faucet_balance = 1_000_000` atoms in defaults for testnet config paths). Mainnet-style defaults use **no** genesis funding unless configured.

---

## 1. Scope (what you actually did) — **check all that apply**

- [x] **Hard reset / new genesis** — wiped or replaced node data, coordinated `chain_id` / `network_id` / genesis, brought nodes back (`testnet-down` / `testnet-up-clean`, or equivalent on public testnet).
- [x] **Clean local testnet** — `make testnet-up-clean` or equivalent per [`tokenomics-testnet-validation.md`](../tokenomics-testnet-validation.md).
- [x] **Snapshot / restore or DB backup path** — exercised `db-backup` / `db-restore` or snapshot flow per [`node-operator-guide.md`](../node-operator-guide.md) / [`sync-guide.md`](../sync-guide.md).
- [x] **Backfill / sync** — nodes caught up from peers or snapshot after wipe.
- [x] **Operational recovery** — restart after crash, disk full, bad state, or firewall/bootstrap fixes.

---

## 2. Sign-off

| Role | Name | Date |
|------|------|------|
| Author / operator | TheNewAutonomy | 22 March 2026 |
| Acknowledged (optional) | _add_ | _add_ |

**GitHub:** Close **#275** with a comment linking to this file at commit: `https://github.com/catalyst-network/catalyst-node-rust/blob/<commit>/docs/evidence/track275-reset-recovery-evidence.md`

---

## Related

- [`tokenomics-testnet-validation.md`](../tokenomics-testnet-validation.md) — reset + issuance checks.
- [`protocol-params.md`](../protocol-params.md) — immutable chain identity when resetting intentionally.
- [`security-threat-model.md`](../security-threat-model.md) — recovery artifacts mentioned under operator controls.
