#!/usr/bin/env bash
# Checklist §7.3: isolate one node (node3) from the rest of the fleet for a bounded window, then
# reconnect it, and assert the fleet never diverges.
#
# node3 is restarted under a dedicated OS user (start_node_as_user) so both directions of its
# traffic can be blocked: an INPUT dport rule for inbound, plus `iptables -m owner --uid-owner`
# on OUTPUT for everything node3 itself sends. A port-only rule is not enough on its own — node3
# dials out to node1 as a client using an ephemeral source port (not its own listening port), and
# all three testnet processes share one loopback network stack (no netns), so a plain
# port-based OUTPUT rule can't distinguish node3's outbound traffic from node2's. See
# consensus-follower-batch-drop-e2e.sh for the live failure this was caught by (one-way leakage
# let an "isolated" node's self-produced LSUs reach and desync healthy peers).
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
ISOLATION_USER="testnet-node3-iso"

cleanup() {
  sudo iptables -D INPUT -p tcp --dport "$TESTNET_P2P_PORT_3" -j DROP 2>/dev/null || true
  sudo iptables -D OUTPUT -m owner --uid-owner "$ISOLATION_USER" -j DROP 2>/dev/null || true
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

echo "==> restarting node3 under isolation user ${ISOLATION_USER} (so its outbound traffic can be blocked)"
stop_node 3
start_node_as_user 3 "$ISOLATION_USER"
bash scripts/netctl.sh testnet wait-rpc "$TESTNET_RPC3" 90

echo "==> isolating node3 (inbound to P2P port $TESTNET_P2P_PORT_3, all outbound) for ${PARTITION_DURATION_CYCLES} cycles"
sudo iptables -I INPUT -p tcp --dport "$TESTNET_P2P_PORT_3" -j DROP
sudo iptables -I OUTPUT -m owner --uid-owner "$ISOLATION_USER" -j DROP

RESUME_AT=$((PARTITION_AT + PARTITION_DURATION_CYCLES))
echo "==> asserting node1/node2 keep converging with each other while node3 is partitioned"
test_consensus_heads_subset "$RESUME_AT" 180 "$TESTNET_RPC1" "$TESTNET_RPC2"

echo "==> reconnecting node3"
sudo iptables -D INPUT -p tcp --dport "$TESTNET_P2P_PORT_3" -j DROP
sudo iptables -D OUTPUT -m owner --uid-owner "$ISOLATION_USER" -j DROP

echo "==> asserting full 3-way convergence after node3 rejoins (timeout ${CONVERGE_TIMEOUT_SECS}s)"
bash scripts/netctl.sh testnet test-consensus-heads "$((RESUME_AT + CONVERGE_AFTER_CYCLES))" "$CONVERGE_TIMEOUT_SECS"

echo "==> DONE: fleet converged on an identical applied_state_root after an asymmetric partition"
