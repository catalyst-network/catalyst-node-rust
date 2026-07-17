#!/usr/bin/env bash
# Regression test for the 2026-07-17 rolling-recovery incident on catalyst-testnet: a node whose
# reconcile circuit breaker is open (self-produced apply blocked, "needs operator action") could
# still fully complete the four-phase Construction/vote round for NEW cycles and broadcast its own
# vote to the network -- which counted toward *other* nodes' BFT quorum (2-of-3) -- before bailing
# out at the final apply step and discarding the result for itself. During a rolling (one-node-at-
# a-time) recovery this let a freshly-repaired node complete cycles using a vote from a peer that
# was still circuit-broken; that peer never applied (and had no way to later backfill) those same
# cycles once repaired, permanently crediting the fresh node with producer compensation the broken
# peer never received -- a silent, discrete `accounts`-root divergence with no error logged
# anywhere. See the fix in `node.rs` (search "Withholding consensus participation").
#
# Reproduction: node2 is driven into the exact same circuit-broken state as the original incident
# via the same trigger (`scan_for_self_produced_state_root_divergence`, ADR 0002 self-produced
# state_root vs certificate) -- not by racing real execution divergence (which we don't have a
# reliable way to force), but by directly corrupting node2's own already-certified
# `consensus:lsu_state_root:{cycle}` record with `corrupt-self-produced-state-root-for-test`
# (TEST-ONLY tooling) while it's stopped, then restarting it. Its own scan (runs every ~2s) detects
# the self-disagreement within seconds and trips the breaker deterministically, with node2's
# process staying up/connected throughout -- this is the "circuit-broken but still connected" state
# the bug depends on (a node that's actually offline can't cast a vote at all).
#
# node3 is stopped entirely for the critical window, so node1 + node2 are the only two possible
# voters. Before the fix: node2 still contributes a vote despite being circuit-broken, so node1
# alone can complete cycles (2-of-3 satisfied by node1 + node2's vote) and applied_cycle keeps
# advancing. After the fix: node2 withholds participation entirely, leaving only node1 (1-of-3,
# below the BFT threshold), so cycles fail closed ("Insufficient data collected") and node1's
# applied_cycle must NOT advance while node2 is circuit-broken and node3 is offline.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${CARGO_TARGET_DIR:-$ROOT_DIR/target}/release/catalyst-cli"
RPC1="http://127.0.0.1:8545"
RPC2="http://127.0.0.1:8546"

# Cycle node1/node2/node3 must jointly reach before the corruption/isolation phase begins. Needs
# to be a few cycles in so the corrupted cycle already has a published ADR 0002 certificate.
BASELINE_CYCLE="${BASELINE_CYCLE:-5}"
# How long to observe node1 while isolated with only a circuit-broken node2 as a possible second
# voter. Must comfortably exceed one cycle_duration_seconds (20s in testnet/*/config.toml) so a
# false-pass (node1 silently keeps advancing) would be caught.
ISOLATION_WINDOW_SECS="${ISOLATION_WINDOW_SECS:-60}"
CONVERGE_TIMEOUT_SECS="${CONVERGE_TIMEOUT_SECS:-240}"
CLEAN="${CLEAN:-true}"

cleanup() {
  bash scripts/netctl.sh testnet down >/dev/null 2>&1 || true
}
trap cleanup EXIT

die() {
  echo "ERROR: $*" >&2
  exit 1
}

rpc_field() {
  local url="$1" field="$2"
  curl -s -X POST "$url" -H 'content-type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_head","params":[]}' \
    | python3 -c "import sys,json; r=json.load(sys.stdin).get('result') or {}; print(r.get('${field}',''))"
}

stop_node() {
  local n="$1"
  [ -f "testnet/node${n}/node.pid" ] || return 0
  local pid
  pid="$(cat "testnet/node${n}/node.pid")"
  kill "$pid" 2>/dev/null || true
  local deadline=$(($(date +%s) + 30))
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$(date +%s)" -ge "$deadline" ]; then
      kill -9 "$pid" 2>/dev/null || true
      break
    fi
    sleep 1
  done
  rm -f "testnet/node${n}/node.pid"
}

start_node2() {
  env RUST_LOG="${RUST_LOG:-info}" CATALYST_REQUIRE_LSU_FINALITY="${CATALYST_REQUIRE_LSU_FINALITY:-0}" \
    stdbuf -oL -eL "$BIN" --config testnet/node2/config.toml start \
    --validator --storage --rpc --rpc-port 8546 \
    --bootstrap-peers "/ip4/127.0.0.1/tcp/30333" \
    >>testnet/node2/logs/stdout.log 2>&1 &
  echo $! >testnet/node2/node.pid
}

start_node3() {
  env RUST_LOG="${RUST_LOG:-info}" CATALYST_REQUIRE_LSU_FINALITY="${CATALYST_REQUIRE_LSU_FINALITY:-0}" \
    stdbuf -oL -eL "$BIN" --config testnet/node3/config.toml start \
    --validator --storage --rpc --rpc-port 8547 \
    --bootstrap-peers "/ip4/127.0.0.1/tcp/30333" \
    >>testnet/node3/logs/stdout.log 2>&1 &
  echo $! >testnet/node3/node.pid
}

echo "==> consensus-circuit-breaker-participation-e2e: build"
cargo build --release -p catalyst-cli

echo "==> starting testnet (3 validators)"
if [ "$CLEAN" = "true" ]; then
  bash scripts/netctl.sh testnet up --clean
else
  bash scripts/netctl.sh testnet up
fi
bash scripts/netctl.sh testnet wait-rpc "$RPC1" 90
bash scripts/netctl.sh testnet wait-rpc "$RPC2" 90

# Sanity-check regression found during development: `test-consensus-heads` only checks
# applied_cycle/state_root convergence, which stays trivially true even while a node's own
# WorkerRegistration transaction hasn't landed on-chain yet (membership=bootstrap, or
# membership=onchain with a worker count that excludes itself) -- during that window the node
# doesn't consider itself a valid producer and silently never attempts construction/voting at
# all, for reasons that have nothing to do with the circuit breaker. That masked the fix in an
# earlier version of this script: reverting the fix and rerunning still "passed" (node1 stalled),
# but for the wrong reason. Block here until all 3 nodes' logs show the full on-chain producer
# set before ever touching node2/node3, so the later isolation window isolates only the variable
# this test actually cares about.
wait_for_full_onchain_membership() {
  local deadline=$(($(date +%s) + 180))
  while true; do
    local ok=true
    for n in 1 2 3; do
      grep -aq "membership=onchain workers=3" "testnet/node${n}/logs/stdout.log" 2>/dev/null || ok=false
    done
    if [ "$ok" = "true" ]; then
      return 0
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
      die "not all 3 nodes reached membership=onchain workers=3 within timeout -- worker registration never fully landed"
    fi
    sleep 3
  done
}
echo "==> waiting for all 3 nodes' own WorkerRegistration to land on-chain (membership=onchain workers=3 on each)"
wait_for_full_onchain_membership
echo "==> confirmed: all 3 nodes see the full on-chain producer set"

echo "==> waiting for baseline cycle ${BASELINE_CYCLE} on all 3 validators"
bash scripts/netctl.sh testnet test-consensus-heads "$BASELINE_CYCLE" 180

HEAD_AT_BASELINE="$(rpc_field "$RPC2" applied_cycle)"
[ -n "$HEAD_AT_BASELINE" ] || die "could not read node2 applied_cycle"
# Give attestation gossip a few extra seconds (and one more cycle) to actually publish the ADR
# 0002 certificate for HEAD_AT_BASELINE before we act on it -- the scan can only detect corruption
# against a cycle that already has a published `consensus:lsu_state_root_cid:{cycle}` pointer.
echo "==> waiting 25s for node2's cycle ${HEAD_AT_BASELINE} state-root certificate to publish"
sleep 25

echo "==> taking a healthy db-backup of node2 (used for the real repair at the end)"
stop_node 2
GOOD_BACKUP="testnet/node2/backup_good_$(date +%s)"
"$BIN" db-backup --data-dir testnet/node2/data --out-dir "$GOOD_BACKUP"

echo "==> stopping node3 entirely (offline for the rest of the isolation window)"
stop_node 3

# Cycle numbers can have gaps (a wall-clock slot with no quorum produces nothing, so
# consensus:lsu_state_root:{cycle} is never written for it) -- scan backward from
# HEAD_AT_BASELINE for the nearest cycle that actually has a recorded state_root to corrupt.
CORRUPT_CYCLE=""
for offset in 0 1 2 3 4 5; do
  candidate=$((HEAD_AT_BASELINE - offset))
  if "$BIN" corrupt-self-produced-state-root-for-test --data-dir testnet/node2/data --cycle "$candidate" 2>/tmp/corrupt_err.log; then
    CORRUPT_CYCLE="$candidate"
    break
  fi
done
[ -n "$CORRUPT_CYCLE" ] || die "could not find an applied cycle near ${HEAD_AT_BASELINE} to corrupt: $(cat /tmp/corrupt_err.log)"
echo "==> corrupted node2's own already-applied state_root for cycle ${CORRUPT_CYCLE} (TEST-ONLY tooling)"

echo "==> restarting node2 (process stays connected -- this is the 'circuit-broken but still voting' state)"
start_node2
bash scripts/netctl.sh testnet wait-rpc "$RPC2" 90

echo "==> waiting up to 60s for node2's reconcile circuit breaker to trip"
BREAKER_DEADLINE=$(($(date +%s) + 60))
BROKEN=false
while [ "$(date +%s)" -lt "$BREAKER_DEADLINE" ]; do
  if grep -aq "Self-produced state_root does not match network-certified root" testnet/node2/logs/stdout.log 2>/dev/null; then
    BROKEN=true
    break
  fi
  sleep 2
done
[ "$BROKEN" = "true" ] || die "node2's circuit breaker never tripped -- test setup is broken, not the fix"
echo "==> confirmed: node2 is circuit-broken (still connected)"

echo "==> observing node1 for ${ISOLATION_WINDOW_SECS}s: only node1 (healthy) + node2 (circuit-broken) can vote, node3 is offline"
C_START="$(rpc_field "$RPC1" applied_cycle)"
[ -n "$C_START" ] || die "could not read node1 applied_cycle at isolation start"
echo "    node1 applied_cycle at isolation start: $C_START"
sleep "$ISOLATION_WINDOW_SECS"
C_END="$(rpc_field "$RPC1" applied_cycle)"
[ -n "$C_END" ] || die "could not read node1 applied_cycle at isolation end"
echo "    node1 applied_cycle after ${ISOLATION_WINDOW_SECS}s isolation: $C_END"

if [ "$C_END" -gt "$C_START" ] 2>/dev/null; then
  die "REGRESSION: node1's applied_cycle advanced ($C_START -> $C_END) using only itself + a circuit-broken node2's vote, with node3 offline -- circuit-broken nodes must withhold consensus participation, not just their own apply"
fi
echo "==> PASS: node1 correctly stalled (no false 2-of-3 quorum from a circuit-broken node2) while isolated"

echo "==> repairing node2 for real (db-restore from the healthy backup) and bringing node3 back online"
stop_node 2
"$BIN" db-restore --data-dir testnet/node2/data --from-dir "$GOOD_BACKUP"
start_node2
start_node3

echo "==> asserting full fleet reconverges on an identical applied_state_root"
bash scripts/netctl.sh testnet test-consensus-heads "$((BASELINE_CYCLE + 3))" "$CONVERGE_TIMEOUT_SECS"

echo "==> DONE: circuit-broken nodes correctly withhold consensus participation"
