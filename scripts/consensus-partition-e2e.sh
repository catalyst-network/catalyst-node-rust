#!/usr/bin/env bash
# Checklist §7.3: isolate one node (node3) from the rest of the fleet — bidirectionally, unlike
# the one-directional drop in consensus-follower-batch-drop-e2e.sh (§7.2) — for a bounded window,
# then reconnect it, and assert the fleet never diverges.
#
# Blocking inbound TCP to node3's P2P port is sufficient to fully isolate it: its outbound SYNs
# to node1/node2 get no reply, and its already-established connections' inbound ACKs are also
# dropped by the same rule, so it degrades to fully isolated within one TCP timeout. See
# consensus-follower-batch-drop-e2e.sh for why iptables (vs. a real protocol-level fault-injection
# hook, which doesn't exist yet) is used here.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"
# shellcheck source=lib/testnet_common.sh
source scripts/lib/testnet_common.sh

# cycle is wall-clock-derived (unix_ms / cycle_ms), already in the tens-of-millions on any
# running fleet — absolute thresholds are meaningless. All *_CYCLES vars below are deltas from
# a baseline captured after the fleet is up.
PARTITION_AFTER_CYCLES="${PARTITION_AFTER_CYCLES:-2}"
PARTITION_DURATION_CYCLES="${PARTITION_DURATION_CYCLES:-3}"
CONVERGE_AFTER_CYCLES="${CONVERGE_AFTER_CYCLES:-3}"
CONVERGE_TIMEOUT_SECS="${CONVERGE_TIMEOUT_SECS:-240}"
CLEAN="${CLEAN:-true}"

cleanup() {
  sudo iptables -D INPUT -p tcp --dport "$TESTNET_P2P_PORT_3" -j DROP 2>/dev/null || true
  bash scripts/netctl.sh testnet down >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "==> consensus-partition-e2e: build"
cargo build --release -p catalyst-cli

echo "==> consensus-partition-e2e: start testnet (3 validators)"
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
PARTITION_AT=$((START_CYCLE + PARTITION_AFTER_CYCLES))
echo "==> baseline cycle ${START_CYCLE}; waiting for cycle ${PARTITION_AT} before partitioning node3"
wait_for_cycle "$TESTNET_RPC3" "$PARTITION_AT" 180 >/dev/null

echo "==> isolating node3 (blocking inbound TCP to P2P port $TESTNET_P2P_PORT_3) for ${PARTITION_DURATION_CYCLES} cycles"
sudo iptables -I INPUT -p tcp --dport "$TESTNET_P2P_PORT_3" -j DROP

RESUME_AT=$((PARTITION_AT + PARTITION_DURATION_CYCLES))
echo "==> asserting node1/node2 keep converging with each other while node3 is partitioned"
test_consensus_heads_subset "$RESUME_AT" 180 "$TESTNET_RPC1" "$TESTNET_RPC2"

echo "==> reconnecting node3"
sudo iptables -D INPUT -p tcp --dport "$TESTNET_P2P_PORT_3" -j DROP

echo "==> asserting full 3-way convergence after node3 rejoins (timeout ${CONVERGE_TIMEOUT_SECS}s)"
bash scripts/netctl.sh testnet test-consensus-heads "$((RESUME_AT + CONVERGE_AFTER_CYCLES))" "$CONVERGE_TIMEOUT_SECS"

echo "==> DONE: fleet converged on an identical applied_state_root after an asymmetric partition"
