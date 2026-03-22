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

- [ ] **Hard reset / new genesis** — wiped or replaced node data, coordinated `chain_id` / `network_id` / genesis, brought nodes back (`testnet-down` / `testnet-up-clean`, or equivalent on public testnet).
- [ ] **Clean local testnet** — `make testnet-up-clean` or equivalent per [`tokenomics-testnet-validation.md`](../tokenomics-testnet-validation.md).
- [ ] **Snapshot / restore or DB backup path** — exercised `db-backup` / `db-restore` or snapshot flow per [`node-operator-guide.md`](../node-operator-guide.md) / [`sync-guide.md`](../sync-guide.md).
- [ ] **Backfill / sync** — nodes caught up from peers or snapshot after wipe.
- [ ] **Operational recovery** — restart after crash, disk full, bad state, or firewall/bootstrap fixes.

---

## 2. Your retrospective summary — **add below**

### Networks / environments

_(e.g. public testnet name + regions, local `testnet/node{1,2,3}`, CI, etc.)_

- **Name / purpose:**
- **Regions or hosts (if public):**
- **Time span (approx.):** _(e.g. “Jan 2025 – Mar 2026”)_

### Volume

- **Approximate number of full resets or major recovery exercises:** _(e.g. “15+”; honest range is fine)_
- **Longest continuous stable run between resets (optional):** _(if known)_

### Typical procedure you used

_(Bullet list — reference Makefile targets, systemd, or your runbook.)_

- …
- …

### What consistently worked

_(Examples: `applied_cycle` advanced after clean start, RPC up, peers reconnected, `catalyst_getTokenomicsInfo` matched expectations after genesis.)_

- …
- …

### Issues encountered and resolution

_(If none, write “None material to mainnet launch” or list briefly.)_

- …

### Backfill / snapshot (if exercised)

- **Did you verify chain identity after restore?** _(chain id / network id / genesis hash)_ — yes / no / N/A
- **Pointer to notes or issue (optional):** …

---

## 3. Minimum bar for “acceptable for mainnet” (self-check)

You can sign off C2 if you can truthfully state:

1. Operators have a **documented path** to reset or restore (see [`node-operator-guide.md`](../node-operator-guide.md) § *Resetting / launching a new network* and § *Upgrades, backups, and rollback safety*).
2. **Repeated** resets did not reveal a **systematic** failure mode that would block a coordinated mainnet launch (unknown genesis, unrecoverable DB, etc.).
3. Any **known gaps** are tracked (issue links) or explicitly accepted in §4.

- [ ] I confirm (1)–(3) above for our deployment context.

---

## 4. Residual risk / follow-ups (optional)

_(List open items, monitoring plans, or “none”.)_

- …

---

## 5. Sign-off

| Role | Name | Date |
|------|------|------|
| Author / operator | _add_ | _add_ |
| Acknowledged (optional) | _add_ | _add_ |

**GitHub:** Close **#275** with a comment linking to this file at commit: `https://github.com/catalyst-network/catalyst-node-rust/blob/<commit>/docs/evidence/track275-reset-recovery-evidence.md`

---

## Related

- [`tokenomics-testnet-validation.md`](../tokenomics-testnet-validation.md) — reset + issuance checks.
- [`protocol-params.md`](../protocol-params.md) — immutable chain identity when resetting intentionally.
- [`security-threat-model.md`](../security-threat-model.md) — recovery artifacts mentioned under operator controls.
