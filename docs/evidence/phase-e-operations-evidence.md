# Phase E — Operations (**#278**, **#279**, **#280**)

This document supports closing checklist **Phase E** in [`mainnet-pre-deploy-checklist.md`](../mainnet-pre-deploy-checklist.md). Fill sign-offs and org-specific details below; cross-links point to existing repo docs.

---

## E1 — Key management runbook (**#278**)

### What the codebase + docs already specify

| Topic | Reference |
|--------|-----------|
| **Chain / wallet domain** | [`network-identity.md`](../network-identity.md) — `chain_id`, `network_id`, `genesis_hash` must not change for a live network |
| **Node identity key** | `node.key` (32-byte hex) — path in `config.toml` (`private_key_file`). On hard reset, node-operator guide says **keep** `node.key`, **wipe** `data` when starting a new chain intentionally |
| **Validator / worker keys** | Workers register on-chain; validator set from config / `validators.toml` (see [`node-operator-guide.md`](../node-operator-guide.md)) |
| **Faucet / treasury (if any)** | [`node-operator-guide.md`](../node-operator-guide.md) § *Faucet genesis funding* — **public** networks: `faucet_mode = "configured"`, `faucet_pubkey_hex`, **never** publish the private key; use hot wallet pattern for user-facing faucet |
| **State backups** | `catalyst-cli db-backup` / `db-restore` — not a substitute for **key** backup; backup **`node.key`** and any treasury keys out-of-band (encrypted, access-controlled) |
| **Key compromise (operational)** | No on-chain “revoke” for ed25519 identity in the same way as smart-contract keys; response is **operational**: rotate bootstrap lists, take node offline, **new chain** if consensus keys are fully broken — document your escalation path |

### E1 — Your org checklist (fill in)

- [x] **Validator / operator keys** stored: _(HSM / file / vault — where)_
- [x] **Treasury / faucet keys** (if used): _(custody model)_
- [x] **Backup**: keys encrypted at rest; recovery tested at least once (yes / no)
- [x] **Incident**: named contact for key leak; steps: _(bullet list)_

| Sign-off | Name | Date |
|----------|------|------|
| | TheNewAutonomy | 22 March 2026|

---

## E2 — Genesis / launch ceremony (**#279**)

### Canonical identity choices

From [`network-identity.md`](../network-identity.md):

| Network | chain_id | network_id |
|---------|----------|------------|
| Mainnet (proposed) | `200820091` | `catalyst-mainnet` |
| Public testnet | `200820092` | `catalyst-testnet` |

Adjust only if your governance chooses different values; **never** reuse identity for a different genesis.

### Suggested ceremony order (template)

1. **Freeze** `chain_id`, `network_id`, genesis policy (faucet off vs configured), and **binary tag** (`release-process.md`).
2. **First node** (or designated “genesis coordinator”): empty `data/`, start once, confirm **`catalyst_getSyncInfo`** / `genesis_hash` matches expectation after genesis init.
3. **Publish** `genesis_hash`, `chain_id`, `network_id` to wallets/explorers (see [`wallet-builder-handoff-*.md`](../wallet-builder-handoff-catalyst-testnet.md) pattern for testnet).
4. **Bring up** remaining validators/storage nodes with **same** identity; verify peers and `applied_cycle` advancing.
5. **Abort / do not launch** if: RPC shows wrong `genesis_hash`, nodes cannot peer, or consensus errors — capture logs, fix config, **new chain** only if you must change identity.

### E2 — Your launch record (fill in)

- **Mainnet tag / commit:** …
- **Final `genesis_hash` (published):** …
- **Order of node bring-up:** …
- **Comms channel for operators:** …

| Sign-off | Name | Date |
|----------|------|------|
| | TheNewAutonomy | 22 March 2026|

---

## E3 — Monitoring, alerting, incident response (**#280**)

### What exists in-repo (baseline observability)

| Signal | How |
|--------|-----|
| **Liveness** | [`tester-guide.md`](../tester-guide.md) — `catalyst-cli status` (head advances), `catalyst_head` |
| **Chain identity** | `catalyst_getSyncInfo` — drift vs published mainnet values = wrong network |
| **Peers** | `catalyst_peerCount` / `catalyst-cli peers` |
| **Logs** | Node log file from `config.toml` `[logging]` |

Optional builds may expose **Prometheus** metrics (`catalyst-utils` / crates with `metrics` feature); treat as **supplementary** unless your deployment enables them.

### E3 — Minimum operational package (fill in)

Define at least:

1. **Who** is on-call / notified when RPC is down or `applied_cycle` stalls.
2. **How** alerts fire: _(e.g. HTTP check on `:8545`, cron + `status`, external uptime, log-based errors)_.
3. **Runbook** for: P2P blocked (`30333`), RPC down (`8545`), “insufficient data collected”, disk full (`db-stats` / pruning in node-operator guide).

- [x] On-call / owner: …
- [x] Alert channels: …
- [ ] Link to internal incident template (optional): …

| Sign-off | Name | Date |
|----------|------|------|
| | TheNewAutonomy | 22 March 2026|

---

## Close on GitHub

Comment on **#278**, **#279**, **#280** (or **#260**):

*“Phase E evidence: `docs/evidence/phase-e-operations-evidence.md` @ \<commit\>”*

---

## Related

- [`node-operator-guide.md`](../node-operator-guide.md)
- [`network-identity.md`](../network-identity.md)
- [`protocol-params.md`](../protocol-params.md)
- [`security-external-review-scope.md`](../security-external-review-scope.md) — key / launch dependencies
