#!/usr/bin/env bash
# Run before deploying validators to a test network.
# Fast path: unit + harness tests. Full path: add RUN_CONSENSUS_E2E=1 for three-process RPC gate.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> pre-deploy-consensus-gate: unit + harness slice"
bash "$ROOT/scripts/run-consensus-ci-parity.sh"

if [ "${RUN_CONSENSUS_E2E:-0}" = "1" ]; then
  echo "==> pre-deploy-consensus-gate: multi-process three-validator E2E"
  bash "$ROOT/scripts/consensus-three-validator-e2e.sh"
else
  echo ""
  echo "Optional full gate (starts local testnet, ~3–5 min):"
  echo "  RUN_CONSENSUS_E2E=1 bash scripts/pre-deploy-consensus-gate.sh"
  echo ""
  echo "Testnet deploy (after gate passes):"
  echo "  - Build: cargo build --release -p catalyst-cli"
  echo "  - Per validator: --validator --storage --rpc"
  echo "  - Migration: CATALYST_REQUIRE_LSU_FINALITY=0 until fleet has finality CIDs"
  echo "  - Skew harness (dev): CATALYST_TEST_WALL_OFFSET_MS=<ms> (do not set in production)"
  echo "  - Checkpoint: CATALYST_CHECKPOINT_EVERY_CYCLES=32 (default)"
fi

echo "==> pre-deploy-consensus-gate: PASS (unit slice)"
