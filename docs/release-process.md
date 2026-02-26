# Release process (checksums + compatibility gates)

This project uses Git tags and GitHub releases to produce deployable artifacts for operators.

## Goals

- Provide **checksummed** binaries for deployment.
- Ensure protocol/wire compatibility changes are **intentional** and reviewed.
- Keep upgrades operationally safe via snapshot backup/restore workflows.

## Versioning policy (current)

- **Tags**: `vX.Y.Z` (project release version).
- **Binary version**: reported by `catalyst_version` and `catalyst-cli --version`.
- **Protocol changes**:
  - must be accompanied by updated docs (e.g. `docs/wallet-interop.md`)
  - must update golden vectors (see below) in the same PR, with rationale in the PR description.

## CI

GitHub Actions runs:

- `cargo test --workspace --locked`

See `.github/workflows/ci.yml`.

## Wire-format compatibility gate

`crates/catalyst-core/tests/wire_vectors.rs` contains golden vectors for:
- `CTX2` wire encoding (`encode_wire_tx_v2`)
- `tx_id_v2`
- v2 signing payload (`CATALYST_SIG_V2`)

Any change to canonical serialization will fail CI until the vectors are updated intentionally.

## Network wire versioning (MessageEnvelope)

`MessageEnvelope` is encoded on the network using a **versioned** wrapper:

- **magic**: `CENV`
- **version**: `PROTOCOL_VERSION` (u32 LE)
- **payload**: `bincode(MessageEnvelope)`

Nodes reject unknown versions, and libp2p identify advertises a matching protocol string (`catalyst/1`).

Version bump policy (current):
- **Bump `PROTOCOL_VERSION`** only for intentionally breaking, network-wide changes to envelope decoding.
- Keep the identify protocol string in sync with the major version (`catalyst/<PROTOCOL_VERSION>`).

## Creating a release

1) Ensure `main` is green and merged.
2) Create and push a tag:

```bash
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin vX.Y.Z
```

3) The release workflow builds `catalyst-cli` and uploads:
- `catalyst-cli-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz`
- `catalyst-cli-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz.sha256`

See `.github/workflows/release.yml`.

## Upgrade / rollback safety

For production upgrades, always take a snapshot backup first and be prepared to restore.

See `docs/node-operator-guide.md` for `db-backup` / `db-restore` workflows.

