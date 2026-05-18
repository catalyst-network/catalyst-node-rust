#!/usr/bin/env bash
# Checklist §7.1: multi-process three-validator test — identical applied head via RPC.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

MIN_CYCLE="${MIN_CYCLE:-3}"
TIMEOUT_SECS="${TIMEOUT_SECS:-240}"
CLEAN="${CLEAN:-true}"

cleanup() {
  bash scripts/netctl.sh testnet down >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "==> consensus-three-validator-e2e: build"
cargo build --release -p catalyst-cli

echo "==> consensus-three-validator-e2e: start testnet (3 validators)"
if [ "$CLEAN" = "true" ]; then
  bash scripts/netctl.sh testnet up --clean
else
  bash scripts/netctl.sh testnet up
fi

echo "==> consensus-three-validator-e2e: compare heads (min_cycle=$MIN_CYCLE)"
bash scripts/netctl.sh testnet test-consensus-heads "$MIN_CYCLE" "$TIMEOUT_SECS"

echo "==> DONE"
