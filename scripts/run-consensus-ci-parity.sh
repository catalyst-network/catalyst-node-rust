#!/usr/bin/env bash
# Repeatable “Phase 0” slice from docs/consensus-implementation-completion-checklist.md.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
: "${CARGO_MANIFEST_PATH:=$ROOT/Cargo.toml}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
if command -v make >/dev/null 2>&1; then
  make -C "$ROOT" sync-gvfs-mirror
else
  echo "warning: make not found; skipping sync-gvfs-mirror" >&2
fi
RUST_LOG="${RUST_LOG:-debug}"
export RUST_LOG
cargo test -p catalyst-cli --manifest-path "$CARGO_MANIFEST_PATH"
cargo test -p catalyst-consensus --manifest-path "$CARGO_MANIFEST_PATH"
cargo test -p catalyst-consensus --test convergence_harness --manifest-path "$CARGO_MANIFEST_PATH"
cargo test -p catalyst-core protocol:: --manifest-path "$CARGO_MANIFEST_PATH"
cargo test -p catalyst-network --no-default-features --features test-hooks --test tcp_batch_drop --manifest-path "$CARGO_MANIFEST_PATH"
# Optional §7.1 gate (multi-process): run with RUN_CONSENSUS_E2E=1 after `cargo build --release -p catalyst-cli`
if [ "${RUN_CONSENSUS_E2E:-0}" = "1" ]; then
  bash "$ROOT/scripts/consensus-three-validator-e2e.sh"
fi
