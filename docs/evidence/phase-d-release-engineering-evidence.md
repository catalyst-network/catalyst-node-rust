# Phase D — Release engineering + upgrade safety (issues **#264**, **#276**, **#277**)

This document supports closing checklist **Phase D** in [`mainnet-pre-deploy-checklist.md`](../mainnet-pre-deploy-checklist.md).

---

## D1 — Release build process + provenance (**#264**, **#276**)

### What is already defined in this repository

| Topic | Where / what |
|--------|----------------|
| **Release steps** | [`release-process.md`](../release-process.md) — tags `v*.*.*`, GitHub Release, artifact names |
| **CI gate before ship** | `.github/workflows/ci.yml` — `cargo test --workspace --locked` |
| **Release artifacts** | `.github/workflows/release.yml` — `cargo build -p catalyst-cli --release --locked`, tarball `catalyst-cli-<tag>-x86_64-unknown-linux-gnu.tar.gz`, **SHA256** sidecar |
| **Checksums** | Per-binary `SHA256SUMS` inside tarball + `.sha256` for the `.tar.gz` (see workflow) |
| **Wire compatibility** | [`release-process.md`](../release-process.md) — golden vectors in `crates/catalyst-core/tests/wire_vectors.rs` |
| **Version reporting** | `catalyst_version` RPC / `catalyst-cli --version` (per `release-process.md`) |

### Reproducible builds + SBOM (**#276**)

- **Bit-for-bit reproducible** Linux binaries are **not** asserted by automation in this repo today.
- **Provenance** that *is* available for operators:
  - **Git tag** + **commit** on `main`
  - **GitHub Actions** run for the tag (`release.yml`)
  - **SHA256** of the published tarball (verify after download)
  - **`--locked`** builds in CI/release for dependency determinism from `Cargo.lock`

_(Optional — add your org’s policy: e.g. “SBOM deferred to post-mainnet” or link to an external SBOM artifact.)_

### D1 sign-off (fill in)

- [ ] We accept the **documented release process** ([`release-process.md`](../release-process.md)) + **checksums + locked builds** as sufficient provenance for mainnet v1.
- [ ] **Release tag used for mainnet binaries:** `v_____` **Commit:** `__________`
- [ ] **Notes (optional):** …

| Name | Date |
|------|------|
| | |

---

## D2 — Upgrade matrix + rollback (**#277**)

### Documented rollback path (code + docs)

- **Before upgrade:** `catalyst-cli db-backup` — [`node-operator-guide.md`](../node-operator-guide.md) § *Upgrades, backups, and rollback safety*
- **If upgrade fails:** `catalyst-cli db-restore` from the backup directory
- **Chain identity:** do not change `chain_id` / `network_id` / genesis for a “running” network except as a **new chain** — [`protocol-params.md`](../protocol-params.md)
- **On-disk marker:** `storage:version` helps detect mismatches across upgrades (see node-operator guide)

### Suggested upgrade matrix (fill in what you actually tested)

| Scenario | Binary / version | Data dir | Tested? | Result / notes |
|----------|------------------|----------|---------|----------------|
| Same tag, restart only | … | … | ☐ | |
| New patch release, same `Cargo.lock` line / compatible | … | … | ☐ | |
| After `db-backup`, replace binary, start | … | … | ☐ | |
| Rollback: `db-restore` + previous binary | … | … | ☐ | |

**Local dev example (optional):** stop testnet → backup `testnet/node1/data` → upgrade binary → `testnet-up` → smoke `catalyst_status` / head advancing; rollback = restore backup + old binary.

### D2 sign-off (fill in)

- [ ] **Coordinated upgrade** assumption documented: operators agree on **tag**, **backup window**, and **rollback** using **db-backup / db-restore**.
- [ ] **Residual:** cross-version P2P / consensus incompatibility is handled by **release notes** + **not** mixing incompatible majors on one network.

| Name | Date |
|------|------|
| | |

---

## Close on GitHub

Comment on **#264**, **#276**, **#277** (or umbrella **#260**):  
*“Phase D evidence: `docs/evidence/phase-d-release-engineering-evidence.md` @ \<commit\>”*

---

## Related

- [`release-process.md`](../release-process.md)
- [`node-operator-guide.md`](../node-operator-guide.md) — upgrades, backups, rollback
- [`mainnet-roadmap.md`](../mainnet-roadmap.md) — epics **#264**, **#276**, **#277**
