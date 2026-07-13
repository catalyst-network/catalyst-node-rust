#!/usr/bin/env bash
# Checklist §7.4: real multi-process clock-skew harness, using the already-real, already-wired
# CATALYST_TEST_WALL_OFFSET_MS (crates/catalyst-utils/src/lib.rs::test_wall_offset_ms /
# wall_now_ms, dev-only - "do not set in production" per scripts/pre-deploy-consensus-gate.sh).
#
# Two sub-cases against testnet's 20s cycle_duration_seconds (see
# docs/consensus-wall-clock-and-time.md) and the default 3-cycle (60s)
# CATALYST_P2P_TX_BATCH_MAX_CYCLE_SLACK:
#   1. Mild skew (+30s, under the 60s slack): node2 should converge normally with the fleet -
#      proves the slack absorbs realistic drift.
#   2. Severe skew (+180s, 3 cycles ahead of real wall time): node2's clock now disagrees with
#      the rest of the fleet by more than the slack tolerates. The exact internal mechanism
#      (depth-gate defer, P2P wall-lead rejection at ingress, or quorum starvation on a cycle
#      number nobody else has reached) is not asserted here - what's asserted is the outcome
#      the checklist actually cares about: node2 must never end up applying a *different*
#      state_root than its peers, and once the skew is removed and node2 restarts (simulating
#      NTP resync), the fleet must fully reconverge.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
# shellcheck source=lib/testnet_common.sh
source scripts/lib/testnet_common.sh

# cycle is wall-clock-derived (unix_ms / cycle_ms), already in the tens-of-millions on any
# running fleet — absolute thresholds are meaningless. All *_CYCLES vars below are deltas from
# a baseline captured after the fleet is up.
MILD_SKEW_MS="${MILD_SKEW_MS:-30000}"
SEVERE_SKEW_MS="${SEVERE_SKEW_MS:-180000}"
SKEW_AFTER_CYCLES="${SKEW_AFTER_CYCLES:-2}"
SKEW_DURATION_CYCLES="${SKEW_DURATION_CYCLES:-3}"
CONVERGE_AFTER_CYCLES="${CONVERGE_AFTER_CYCLES:-3}"
CONVERGE_TIMEOUT_SECS="${CONVERGE_TIMEOUT_SECS:-240}"
CLEAN="${CLEAN:-true}"

cleanup() {
  bash scripts/netctl.sh testnet down >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "==> consensus-clock-skew-e2e: build"
cargo build --release -p catalyst-cli

echo "==> consensus-clock-skew-e2e: start testnet (3 validators)"
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
SKEW_AT=$((START_CYCLE + SKEW_AFTER_CYCLES))
echo "==> baseline cycle ${START_CYCLE}; waiting for cycle ${SKEW_AT} before restarting node2 with mild skew (+${MILD_SKEW_MS}ms)"
wait_for_cycle "$TESTNET_RPC1" "$SKEW_AT" 180 >/dev/null
stop_node 2
start_node 2 "CATALYST_TEST_WALL_OFFSET_MS=${MILD_SKEW_MS}"
bash scripts/netctl.sh testnet wait-rpc "$TESTNET_RPC2" 90

MILD_TARGET=$((SKEW_AT + SKEW_DURATION_CYCLES))
echo "==> asserting full 3-way convergence under mild (in-slack) skew (timeout ${CONVERGE_TIMEOUT_SECS}s)"
bash scripts/netctl.sh testnet test-consensus-heads "$MILD_TARGET" "$CONVERGE_TIMEOUT_SECS"

echo "==> escalating node2 to severe skew (+${SEVERE_SKEW_MS}ms, beyond slack)"
stop_node 2
start_node 2 "CATALYST_TEST_WALL_OFFSET_MS=${SEVERE_SKEW_MS}"
bash scripts/netctl.sh testnet wait-rpc "$TESTNET_RPC2" 90

SEVERE_TARGET=$((MILD_TARGET + SKEW_DURATION_CYCLES))
echo "==> asserting node1/node3 keep converging with each other while node2 is severely skewed"
test_consensus_heads_subset "$SEVERE_TARGET" 180 "$TESTNET_RPC1" "$TESTNET_RPC3"

echo "==> node2 status while severely skewed (informational - stall or lag is safe, divergence is not):"
echo "node2 cycle=$(rpc_field "$TESTNET_RPC2" applied_cycle) state_root=$(rpc_field "$TESTNET_RPC2" applied_state_root)"

echo "==> resyncing node2 (restart without the skew var, simulating NTP correction)"
stop_node 2
start_node 2
bash scripts/netctl.sh testnet wait-rpc "$TESTNET_RPC2" 90

echo "==> asserting full 3-way reconvergence after resync (timeout ${CONVERGE_TIMEOUT_SECS}s)"
bash scripts/netctl.sh testnet test-consensus-heads "$((SEVERE_TARGET + CONVERGE_AFTER_CYCLES))" "$CONVERGE_TIMEOUT_SECS"

echo "==> DONE: fleet reconverged on an identical applied_state_root after a severe clock-skew episode"
