# Phase F — Integrations (**#266**, **#281**, **#282**)

This document supports closing checklist **Phase F** in [`mainnet-pre-deploy-checklist.md`](../mainnet-pre-deploy-checklist.md).

---

## F1 — RPC for wallets & explorers; consistency (**#266**, **#281**)

### Canonical references (already in repo)

| Doc | Purpose |
|-----|---------|
| [`builder-guide.md`](../builder-guide.md) | Full **RPC method list**, wire tx formats, nonce/receipt behavior |
| [`wallet-interop.md`](../wallet-interop.md) | **Address format**, v2 **signing payload**, **CTX2** wire encoding |
| [`network-identity.md`](../network-identity.md) | **`chain_id` / `network_id`** scheme for mainnet vs testnet |
| `.github/workflows/ci.yml` | **`cargo test --workspace --locked`** — regression gate on RPC/consensus crates |

### Minimum RPC surface for wallets (summary)

Operators should verify these against **mainnet** RPC before launch:

| Method | Use |
|--------|-----|
| `catalyst_chainId`, `catalyst_networkId`, `catalyst_genesisHash` | Domain separation / wrong-chain detection |
| `catalyst_getNonce`, `catalyst_getBalance`, `catalyst_getAccount` | Account state |
| `catalyst_sendRawTransaction` | Submit **CTX2** (or accepted legacy) wire tx |
| `catalyst_getTransactionReceipt` | Inclusion / applied state |

See full list in [`builder-guide.md`](../builder-guide.md).

### Quick verification commands (copy; replace host)

```bash
RPC=https://<MAINNET_RPC_HOST>:8545
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_getSyncInfo","params":[]}'
curl -s -X POST "$RPC" -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_getTokenomicsInfo","params":[]}'
```

### F1 — Sign-off (fill in)

- [x] **CI green** on release commit: `cargo test --workspace --locked` (or link to Actions run).
- [x] **Manual smoke** on mainnet/staging RPC: `catalyst_getSyncInfo` matches published **chain_id**, **network_id**, **genesis_hash**.
- [x] **Wallet** (or test harness): send + receipt path tested — yes / no — notes: …

| Name | Date |
|------|------|
| | |

---

## F2 — Indexer / explorer expectations (**#282**)

### Canonical reference

- [`explorer-handoff.md`](../explorer-handoff.md) — cycle = block model, LSU, receipts, `catalyst_getBlocksByNumberRange`, address/tx id rules.

### Mainnet-specific publication (fill in)

Record what integrators must use for **this** launch:

| Field | Published value / URL |
|-------|------------------------|
| **HTTPS RPC base** | … |
| **chain_id** (decimal + hex echo from RPC) | … |
| **network_id** | … |
| **genesis_hash** | … |
| **Docs / changelog** for integrators | … |

### F2 — Sign-off (fill in)

- [ ] Explorer/indexer team (or owner) has **`explorer-handoff.md`** + mainnet table above.
- [ ] **No breaking RPC changes** vs tested release without release notes — yes / N/A.

| Name | Date |
|------|------|
| | |

---

## Close on GitHub

Comment on **#266**, **#281**, **#282** (or **#260**):

*“Phase F evidence: `docs/evidence/phase-f-integrations-evidence.md` @ \<commit\>”*

---

## Related

- [`wallet-builder-handoff-catalyst-testnet.md`](../wallet-builder-handoff-catalyst-testnet.md) — example testnet handoff pattern (adapt for mainnet)
