# Security-related dependency updates

This note tracks **high-severity Dependabot-class** dependency work (crypto, P2P, QUIC, TLS).

## Updates applied (workspace)

- **`rustls` / `aws-lc-rs` / `aws-lc-sys`:** bumped via compatible upgrades to address AWS-LC advisories (PKCS7 / X.509 / timing issues tied to `aws-lc-sys`).
- **`quinn-proto`:** `0.11.13` → **`0.11.14`** (GHSA-6xvm-j4wr-6v98 / QUIC transport parameter parsing DoS).
- **`libp2p`:** **`0.53` → `0.56`** to pull patched **`libp2p-gossipsub`**, **`libp2p-quic`**, **`libp2p-yamux`**, and related crates.
- **`yamux` (0.13 line):** **`0.13.8` → `0.13.10`** (GHSA-vxx9-2994-q338 / GHSA-4w32-2493-32g7).

## Code changes

- `catalyst-network`: `identify::Event::Received` match updated for libp2p 0.56 (`connection_id` field — use `..`).

## Known residual: `yamux` **0.12.1**

`libp2p-yamux` **0.47.0** declares **two** `yamux` semver ranges (`^0.12.1` and `^0.13.3`). Cargo therefore resolves **both** `yamux 0.12.1` and `yamux 0.13.10`. There is **no** `yamux 0.12.2+` release; the GitHub advisory range is fixed only at **`0.13.10`**, while the **0.12** line cannot be bumped to 0.13 without breaking `^0.12.1` requirements.

**Action:** track upstream `rust-libp2p` / `libp2p-yamux` releases that drop the legacy `yamux` 0.12 dependency. Re-run Dependabot after upgrades.

## Verification

```bash
cargo build --workspace
cargo test --workspace
```

Optional local scan:

```bash
cargo audit
```
