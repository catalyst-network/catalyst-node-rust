# Reset / recovery / backfill evidence — issue **#275** (Phase **C2**)

This document closes GitHub **#275** and checklist item **C2** in [`mainnet-pre-deploy-checklist.md`](../mainnet-pre-deploy-checklist.md).

It is valid to complete **retrospectively**: if resets and recoveries were performed during development and testnet operation but not logged at the time, record here **what was done**, **what was observed**, and **residual risk** (if any).

---

## 1. Scope (what “reset / recovery” means here)

Check the boxes that apply to activity you actually performed:

- [ ] **Hard reset / new genesis** — wiped or replaced node data, coordinated `chain_id` / `network_id` / genesis, brought nodes back (`testnet-down` / `testnet-up-clean`, or equivalent on public testnet).
- [ ] **Clean local testnet** — `make testnet-up-clean` or equivalent per [`tokenomics-testnet-validation.md`](../tokenomics-testnet-validation.md).
- [ ] **Snapshot / restore or DB backup path** — exercised per [`node-operator-guide.md`](../node-operator-guide.md) (if applicable to your deployment).
- [ ] **Backfill / sync** — nodes caught up from peers or snapshot after wipe (describe briefly).
- [ ] **Operational recovery** — restart after crash, disk full, or bad state (describe).

---

## 2. Retrospective summary (fill in)

**Networks / environments:** (e.g. public testnet name, local 3-node testnet, region list)

**Approximate number of full resets or major recovery exercises:** (e.g. “10+ over 6 months” — honest range is fine)

**Typical procedure used:** (bullet list — point to Makefile targets, scripts, or runbook sections)

- …

**What consistently worked:** (e.g. chain progresses after clean start, RPC responsive, peers reconnect)

- …

**Issues encountered (if any) and resolution:** (e.g. stuck state → fixed by X; open follow-ups: link issues)

- …

---

## 3. Minimum bar for “acceptable for mainnet” (self-check)

You can sign off C2 if you can truthfully state:

1. Operators have a **documented path** to reset or restore (see [`node-operator-guide.md`](../node-operator-guide.md) §Resetting / launching a new network).
2. **Repeated** resets did not reveal a **systematic** failure mode that would block a coordinated mainnet launch (unknown genesis, unrecoverable DB, etc.).
3. Any **known gaps** are tracked (issue links) or explicitly accepted below.

---

## 4. Residual risk / follow-ups (optional)

- …

---

## 5. Sign-off

| Role | Name | Date |
|------|------|------|
| Author / operator | | |
| Acknowledged (optional) | | |

**GitHub:** Close **#275** with a comment linking to this file (commit URL after merge).

---

## Related

- [`tokenomics-testnet-validation.md`](../tokenomics-testnet-validation.md) — reset + issuance checks.
- [`protocol-params.md`](../protocol-params.md) — immutable chain identity when resetting intentionally.
