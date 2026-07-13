#!/usr/bin/env bash
# Checklist §7.2: drop inbound P2P delivery to one follower (node2) for a bounded cycle window,
# then restore it, and assert the fleet never diverges (stall-or-recover only, never a fork).
#
# No dev-only fault-injection hook exists in crates/catalyst-network today (only the in-memory
# gossip_sim.rs used by the unit-level convergence harness); adding one would touch the
# production network stack, which this script deliberately avoids. Instead this blocks ALL
# inbound TCP to node2's P2P port (30334) via a loopback-scoped iptables rule — strictly a
# *stronger* fault than dropping just ProtocolTxBatch frames, but it still exercises exactly
# §7.2's invariant, so it's an acceptable substitute. Requires passwordless sudo for iptables
# (true on GitHub-hosted ubuntu-latest runners).
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
# shellcheck source=lib/testnet_common.sh
source scripts/lib/testnet_common.sh

# cycle is wall-clock-derived (unix_ms / cycle_ms), already in the tens-of-millions on any
# running fleet — absolute thresholds are meaningless. All *_CYCLES vars below are deltas from
# a baseline captured after the fleet is up.
DROP_AFTER_CYCLES="${DROP_AFTER_CYCLES:-2}"
DROP_DURATION_CYCLES="${DROP_DURATION_CYCLES:-3}"
CONVERGE_AFTER_CYCLES="${CONVERGE_AFTER_CYCLES:-3}"
CONVERGE_TIMEOUT_SECS="${CONVERGE_TIMEOUT_SECS:-240}"
CLEAN="${CLEAN:-true}"

cleanup() {
  sudo iptables -D INPUT -p tcp --dport "$TESTNET_P2P_PORT_2" -j DROP 2>/dev/null || true
  bash scripts/netctl.sh testnet down >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "==> consensus-follower-batch-drop-e2e: build"
cargo build --release -p catalyst-cli

echo "==> consensus-follower-batch-drop-e2e: start testnet (3 validators)"
if [ "$CLEAN" = "true" ]; then
  bash scripts/netctl.sh testnet up --clean
else
  bash scripts/netctl.sh testnet up
fi
bash scripts/netctl.sh testnet wait-rpc "$TESTNET_RPC1" 90
bash scripts/netctl.sh testnet wait-rpc "$TESTNET_RPC2" 90
bash scripts/netctl.sh testnet wait-rpc "$TESTNET_RPC3" 90

# applied_cycle reports 0 (sentinel) until the first LSU is actually applied, at which point it
# jumps straight to the real wall-clock cycle index (not 1) - wait past that jump before
# capturing the baseline, or "+N cycles" would be satisfied instantly by that same jump.
START_CYCLE="$(wait_for_cycle "$TESTNET_RPC1" 1 90)"
DROP_AT=$((START_CYCLE + DROP_AFTER_CYCLES))
echo "==> baseline cycle ${START_CYCLE}; waiting for cycle ${DROP_AT} before dropping inbound P2P to node2"
wait_for_cycle "$TESTNET_RPC2" "$DROP_AT" 180 >/dev/null

echo "==> blocking inbound TCP to node2's P2P port ($TESTNET_P2P_PORT_2) for ${DROP_DURATION_CYCLES} cycles"
sudo iptables -I INPUT -p tcp --dport "$TESTNET_P2P_PORT_2" -j DROP

RESUME_AT=$((DROP_AT + DROP_DURATION_CYCLES))
echo "==> asserting node1/node3 keep advancing on 2-of-3 quorum while node2 is cut off"
test_consensus_heads_subset "$RESUME_AT" 180 "$TESTNET_RPC1" "$TESTNET_RPC3"

echo "==> restoring inbound P2P to node2"
sudo iptables -D INPUT -p tcp --dport "$TESTNET_P2P_PORT_2" -j DROP

echo "==> asserting full 3-way convergence after node2 rejoins (timeout ${CONVERGE_TIMEOUT_SECS}s)"
bash scripts/netctl.sh testnet test-consensus-heads "$((RESUME_AT + CONVERGE_AFTER_CYCLES))" "$CONVERGE_TIMEOUT_SECS"

echo "==> DONE: fleet converged on an identical applied_state_root after a single-follower batch-delivery drop"
