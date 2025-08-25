# Contributing
- Toolchain: see `rust-toolchain.toml`.
- Lints: `cargo clippy --all --all-features -D warnings`
- Format: `cargo fmt --all`
- Test tiers:
  - `unit`: within crates
  - `property`: `proptests/*` in each crate
  - `integration`: `tests/*` in top-level crates
  - `conformance`: `conformance/*`
- Devnet: `cargo xtask devnet up` / `down` (see below)
