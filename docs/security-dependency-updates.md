# Security-related dependency updates

This note tracks **high-severity Dependabot-class** dependency work (crypto, P2P, QUIC, TLS).

## Updates applied (workspace)

- **`rustls` / `aws-lc-rs` / `aws-lc-sys`:** bumped via compatible upgrades to address AWS-LC advisories (PKCS7 / X.509 / timing issues tied to `aws-lc-sys`).
- **`quinn-proto`:** `0.11.13` → **`0.11.14`** (GHSA-6xvm-j4wr-6v98 / QUIC transport parameter parsing DoS).
- **`libp2p`:** **`0.53` → `0.56`** to pull patched **`libp2p-gossipsub`**, **`libp2p-quic`**, **`libp2p-yamux`**, and related crates.
- **`yamux` (0.13 line):** **`0.13.8` → `0.13.10`** (GHSA-vxx9-2994-q338 / GHSA-4w32-2493-32g7).
- **`rustls-webpki`:** **`0.103.8` → `0.103.10`** (align with patched webpki line used by `rustls` 0.23).
- **`time`:** **`0.3.45` → `0.3.47`** (crate advisories).
- **`tar`:** **`0.4.44` → `0.4.45`**.
- **`jsonrpsee`:** **`0.21` → `0.26`** (`catalyst-cli` HTTP client, `catalyst-rpc` server).
- **`prometheus`:** **`0.13` → `0.14`** (pulls **`protobuf` 3.x** in the lockfile).
- **`wasmtime`:** **`15` → `24.0.x`** (`catalyst-runtime-svm`; addresses multiple RUSTSEC items on older JIT/runtime lines).
- **`keccak`:** lockfile **`0.1.5` → `0.1.6`** (RUSTSEC-2026-0012 / yanked 0.1.5).
- **`catalyst-service-bus`:** removed unused **`reqwest` 0.11** and dev-dependency **`wiremock`** (shrinks the graph; `rustls-pemfile` / `instant` warnings tied to those paths drop when unused).
- **`jsonwebtoken`:** **`9.x` → `10.3`** (`catalyst-service-bus` auth; addresses Dependabot-class JWT issues on older lines).

## Code changes

- `catalyst-network`: `identify::Event::Received` match updated for libp2p 0.56 (`connection_id` field — use `..`).

## Known residual: `yamux` **0.12.1**

`libp2p-yamux` **0.47.0** declares **two** `yamux` semver ranges (`^0.12.1` and `^0.13.3`). Cargo therefore resolves **both** `yamux 0.12.1` and `yamux 0.13.10`. There is **no** `yamux 0.12.2+` release; the GitHub advisory range is fixed only at **`0.13.10`**, while the **0.12** line cannot be bumped to 0.13 without breaking `^0.12.1` requirements.

**Action:** track upstream `rust-libp2p` / `libp2p-yamux` releases that drop the legacy `yamux` 0.12 dependency. Re-run Dependabot after upgrades.

## Known residual: `tracing-subscriber` **0.2.x** (RUSTSEC-2025-0055)

`revm-precompile` → `ark-bn254` → `ark-relations` pulls **`tracing-subscriber` 0.2.25** when optional tracing features are enabled in that stack. Fixing it requires **`tracing-subscriber` ≥ 0.3.20**, which is not semver-compatible with **`ark-relations` 0.5.1**’s **`tracing-subscriber` 0.2** bound.

**Action:** track **`revm` / `arkworks`** releases that move to **`tracing-subscriber` 0.3+**, or upstream changes that drop the dependency. Until then, `cargo audit` may still report **1** vulnerability for this path.

## Verification

```bash
cargo build --workspace
cargo test --workspace
```

Optional local scan:

```bash
cargo audit
```
