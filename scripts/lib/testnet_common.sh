#!/usr/bin/env bash
# Shared helpers for the local 3-process testnet E2E scripts (scripts/consensus-*-e2e.sh).
# Source this after `set -euo pipefail` and after cd-ing to the repo root. Mirrors the
# conventions already established in scripts/consensus-restart-recovery-e2e.sh and
# scripts/netctl.sh (same RPC ports 8545/8546/8547, P2P ports 30333/30334/30335, pidfile
# locations) so all four E2E scripts behave consistently.

BIN="${CARGO_TARGET_DIR:-$ROOT_DIR/target}/release/catalyst-cli"

TESTNET_RPC1="http://127.0.0.1:8545"
TESTNET_RPC2="http://127.0.0.1:8546"
TESTNET_RPC3="http://127.0.0.1:8547"
TESTNET_P2P_PORT_1=30333
TESTNET_P2P_PORT_2=30334
TESTNET_P2P_PORT_3=30335

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

wait_for_cycle() {
  local url="$1" min_cycle="$2" timeout="$3"
  local deadline=$(($(date +%s) + timeout))
  while true; do
    local c
    c="$(rpc_field "$url" applied_cycle)"
    if [ -n "$c" ] && [ "$c" -ge "$min_cycle" ] 2>/dev/null; then
      echo "$c"
      return 0
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
      die "node at $url did not reach cycle ${min_cycle} within ${timeout}s"
    fi
    sleep 3
  done
}

# Stop node $1 (1/2/3) and wait for it to actually exit before returning.
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

# Start node $1 (2 or 3; node1 is the bootstrap and always started by `netctl.sh testnet up`).
# Extra env assignments (e.g. CATALYST_TEST_WALL_OFFSET_MS=...) may be passed as $2, $3, ...
# (each a "KEY=VALUE" string).
start_node() {
  local n="$1"; shift
  local rpc_port
  case "$n" in
    2) rpc_port=8546 ;;
    3) rpc_port=8547 ;;
    *) die "start_node: only node2/node3 are restartable follower roles (got $n)" ;;
  esac
  env RUST_LOG="${RUST_LOG:-info}" CATALYST_REQUIRE_LSU_FINALITY="${CATALYST_REQUIRE_LSU_FINALITY:-0}" "$@" \
    stdbuf -oL -eL "$BIN" --config "testnet/node${n}/config.toml" start \
    --validator --storage --rpc --rpc-port "$rpc_port" \
    --bootstrap-peers "/ip4/127.0.0.1/tcp/30333" \
    >>"testnet/node${n}/logs/stdout.log" 2>&1 &
  echo $! >"testnet/node${n}/node.pid"
}

# Checklist §7.1-style convergence check restricted to a subset of RPC URLs (e.g. two nodes
# still connected during a partition/drop window, while a third is deliberately isolated).
# Complements `netctl.sh testnet test-consensus-heads`, which requires all three.
test_consensus_heads_subset() {
  local min_cycle="$1" timeout="$2"; shift 2
  local urls=("$@")
  local deadline=$(($(date +%s) + timeout))
  local cycles=() roots=() hashes=()
  while true; do
    cycles=(); roots=(); hashes=()
    local all_ready=true
    for url in "${urls[@]}"; do
      local c r h
      c="$(rpc_field "$url" applied_cycle)"
      r="$(rpc_field "$url" applied_state_root)"
      h="$(rpc_field "$url" applied_lsu_hash)"
      cycles+=("$c"); roots+=("$r"); hashes+=("$h")
      if [ -z "$c" ] || ! [ "$c" -ge "$min_cycle" ] 2>/dev/null; then
        all_ready=false
      fi
    done
    if [ "$all_ready" = "true" ]; then
      break
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
      for i in "${!urls[@]}"; do
        echo "${urls[$i]} cycle=${cycles[$i]:-} state_root=${roots[$i]:-}"
      done
      die "subset did not reach cycle ${min_cycle} within ${timeout}s"
    fi
    sleep 3
  done
  local first_root="${roots[0]}" first_hash="${hashes[0]}"
  for i in "${!urls[@]}"; do
    echo "${urls[$i]} cycle=${cycles[$i]} state_root=${roots[$i]}"
    if [ "${roots[$i]}" != "$first_root" ]; then
      die "divergent applied_state_root within subset"
    fi
    if [ "${hashes[$i]}" != "$first_hash" ]; then
      die "divergent applied_lsu_hash within subset (state_root matched)"
    fi
  done
  echo "PASS: subset identical applied_state_root/applied_lsu_hash at cycle >= ${min_cycle}"
}
