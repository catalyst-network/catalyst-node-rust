#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${CARGO_TARGET_DIR:-$ROOT_DIR/target}/release/catalyst-cli"

usage() {
  cat <<'EOF'
Usage:
  scripts/netctl.sh testnet up [--clean]
  scripts/netctl.sh testnet down
  scripts/netctl.sh testnet status
  scripts/netctl.sh testnet logs [node1|node2|node3]
  scripts/netctl.sh testnet wait-rpc [URL] [TIMEOUT_SECS]
  scripts/netctl.sh testnet test-basic
  scripts/netctl.sh testnet test-contract

  scripts/netctl.sh devnet up --host <PUBLIC_IP_OR_DNS> [--p2p-port 30333] [--rpc-port 8545]
  scripts/netctl.sh devnet down
  scripts/netctl.sh devnet status

Notes:
  - testnet uses ./testnet/{node1,node2,node3} and shared DFS at ./testnet/shared_dfs
  - devnet uses ./devnet/node1 and binds RPC to 0.0.0.0 so others can connect
EOF
}

die() { echo "ERROR: $*" >&2; exit 1; }

rpc_call() {
  local url="$1"
  local method="$2"
  local params_json="${3:-[]}"
  curl -s -X POST "$url" -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"${method}\",\"params\":${params_json}}"
}

wait_tcp() {
  local hostport="$1"
  local timeout="${2:-90}"
  local deadline=$(( $(date +%s) + timeout ))
  while ! (echo >"/dev/tcp/${hostport/:/\/}" 2>/dev/null); do
    if [ "$(date +%s)" -ge "$deadline" ]; then
      return 1
    fi
    sleep 1
  done
  return 0
}

testnet_up() {
  local clean="${1:-false}"
  if [ "$clean" = "true" ]; then
    rm -rf \
      testnet/node1/data testnet/node2/data testnet/node3/data \
      testnet/node1/dfs_cache testnet/node2/dfs_cache testnet/node3/dfs_cache \
      testnet/shared_dfs \
      testnet/node1/logs testnet/node2/logs testnet/node3/logs \
      testnet/validators.toml || true
  fi

  mkdir -p testnet/node1/{logs,data,dfs_cache} testnet/node2/{logs,data,dfs_cache} testnet/node3/{logs,data,dfs_cache}
  python3 -c 'print("fa"*32)' > testnet/faucet.key

  # identities
  for n in 1 2 3; do
    key="testnet/node${n}/node.key"
    if [ ! -f "$key" ]; then
      python3 -c 'import os,binascii; print(binascii.hexlify(os.urandom(32)).decode())' > "$key"
    fi
  done

  # validator set (node1 + node2)
  PK1="$("$BIN" --log-level error pubkey --key-file testnet/node1/node.key | tail -n 1)"
  PK2="$("$BIN" --log-level error pubkey --key-file testnet/node2/node.key | tail -n 1)"
  printf 'validator_worker_ids = ["%s", "%s"]\n' "$PK1" "$PK2" > testnet/validators.toml

  echo "Starting node1 (bootstrap + validator + rpc) ..."
  RUST_LOG=info stdbuf -oL -eL "$BIN" --config testnet/node1/config.toml start \
    --validator --storage --rpc --rpc-port 8545 \
    > testnet/node1/logs/stdout.log 2>&1 &
  echo $! > testnet/node1/node.pid

  sleep 2
  echo "Starting node2 (validator + storage) ..."
  RUST_LOG=info stdbuf -oL -eL "$BIN" --config testnet/node2/config.toml start \
    --validator --storage --rpc-port 8546 \
    --bootstrap-peers "/ip4/127.0.0.1/tcp/30333" \
    > testnet/node2/logs/stdout.log 2>&1 &
  echo $! > testnet/node2/node.pid

  sleep 2
  echo "Starting node3 (storage) ..."
  RUST_LOG=info stdbuf -oL -eL "$BIN" --config testnet/node3/config.toml start \
    --storage --rpc-port 8547 \
    --bootstrap-peers "/ip4/127.0.0.1/tcp/30333" \
    > testnet/node3/logs/stdout.log 2>&1 &
  echo $! > testnet/node3/node.pid

  echo "Testnet up."
  echo "  RPC: http://127.0.0.1:8545"
  echo "  Faucet key: testnet/faucet.key"
}

testnet_down() {
  make stop-testnet >/dev/null 2>&1 || true
  rm -f testnet/node1/node.pid testnet/node2/node.pid testnet/node3/node.pid || true
  echo "Testnet down."
}

testnet_status() {
  for n in 1 2 3; do
    pidfile="testnet/node${n}/node.pid"
    if [ -f "$pidfile" ]; then
      pid="$(cat "$pidfile" || true)"
      if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
        echo "node${n}: running (pid $pid)"
      else
        echo "node${n}: not running (stale pidfile)"
      fi
    else
      echo "node${n}: no pidfile"
    fi
  done
  local rpc="http://127.0.0.1:8545"
  if wait_tcp "127.0.0.1:8545" 1; then
    echo "rpc: reachable ($rpc)"
    echo "head: $(rpc_call "$rpc" catalyst_head '[]' | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"result\"))' 2>/dev/null || true)"
    echo "peers: $(rpc_call "$rpc" catalyst_peerCount '[]' | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"result\"))' 2>/dev/null || true)"
  else
    echo "rpc: NOT reachable ($rpc)"
  fi
}

testnet_logs() {
  local which="${1:-node1}"
  tail -n 200 -f "testnet/${which}/logs/stdout.log"
}

testnet_wait_rpc() {
  local url="${1:-http://127.0.0.1:8545}"
  local timeout="${2:-90}"
  local hostport
  hostport="$(python3 - "$url" <<'PY'
import sys,urllib.parse
u=urllib.parse.urlparse(sys.argv[1])
print(f"{u.hostname}:{u.port or 80}")
PY
)"
  echo "Waiting for RPC at $url (timeout ${timeout}s)"
  if ! wait_tcp "$hostport" "$timeout"; then
    die "RPC not reachable at $url"
  fi
  echo "RPC is reachable."
}

testnet_test_basic() {
  local rpc="http://127.0.0.1:8545"
  testnet_wait_rpc "$rpc" 90
  local node1_pubkey
  node1_pubkey="$(grep -a "Node ID:" -m1 testnet/node1/logs/stdout.log | awk '{print $NF}' || true)"
  [ -n "$node1_pubkey" ] || die "could not parse node1 pubkey from logs"
  echo "node1_pubkey=$node1_pubkey"

  echo "sending faucet -> node1 (amount 3)"
  cargo run -q -p catalyst-cli -- send "$node1_pubkey" 3 --key-file testnet/faucet.key --rpc-url "$rpc"

  echo "head: $(rpc_call "$rpc" catalyst_head '[]' | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"result\"))' 2>/dev/null || true)"
  echo "balance(node1): $(rpc_call "$rpc" catalyst_getBalance "[\"${node1_pubkey}\"]" | python3 -c 'import sys,json; print(json.load(sys.stdin).get(\"result\"))')"
}

testnet_test_contract() {
  local rpc="http://127.0.0.1:8545"
  testnet_wait_rpc "$rpc" 90

  echo "Deploying deterministic initcode contract (returns 0x2a)..."
  local out
  out="$(cargo run -q -p catalyst-cli -- deploy testdata/evm/return_2a_initcode.hex --key-file testnet/faucet.key --rpc-url "$rpc" --runtime evm)"
  echo "$out"
  local addr
  addr="$(echo "$out" | awk '/contract_address:/{print $2}')"
  [ -n "$addr" ] || die "could not parse contract_address from deploy output"

  echo "Waiting for getCode to be non-empty..."
  local deadline=$(( $(date +%s) + 90 ))
  while true; do
    local code
    code="$(rpc_call "$rpc" catalyst_getCode "[\"${addr}\"]" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("result",""))')"
    if [ "$code" != "0x" ] && [ -n "$code" ] && [ "$code" != "0x0" ]; then
      echo "code: $code"
      break
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
      die "contract code never appeared"
    fi
    sleep 2
  done

  echo "Calling contract (empty calldata)..."
  cargo run -q -p catalyst-cli -- call "$addr" 0x --key-file testnet/faucet.key --rpc-url "$rpc" --value 0

  echo "Waiting for last return to equal 32-byte 0x2a..."
  deadline=$(( $(date +%s) + 90 ))
  while true; do
    local ret
    ret="$(rpc_call "$rpc" catalyst_getLastReturn "[\"${addr}\"]" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("result",""))')"
    if [ "$ret" = "0x000000000000000000000000000000000000000000000000000000000000002a" ]; then
      echo "PASS: return=$ret"
      break
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
      die "expected return not observed; got=$ret"
    fi
    sleep 2
  done
}

devnet_write_config() {
  local host="$1"
  local p2p_port="$2"
  local rpc_port="$3"
  mkdir -p devnet/node1/{logs,data,dfs_cache}
  # Create a dedicated key so devnet identity is stable across restarts.
  if [ ! -f devnet/node1/node.key ]; then
    python3 -c 'import os,binascii; print(binascii.hexlify(os.urandom(32)).decode())' > devnet/node1/node.key
  fi
  cat > devnet/node1/config.toml <<EOF
validator = false

[node]
name = "catalyst-devnet-node-1"
private_key_file = "devnet/node1/node.key"
auto_generate_identity = true

[network]
listen_addresses = [
  "/ip4/0.0.0.0/tcp/${p2p_port}",
]
bootstrap_peers = []
max_peers = 200
min_peers = 0
protocol_version = "catalyst/1.0"
mdns_discovery = true
dht_enabled = true

[storage]
data_dir = "devnet/node1/data"
enabled = false
capacity_gb = 10
cache_size_mb = 256
write_buffer_size_mb = 64
max_open_files = 1000
compression_enabled = true

[consensus]
cycle_duration_seconds = 20
freeze_time_seconds = 1
max_transactions_per_block = 1000
min_producer_count = 1
max_producer_count = 100
validator_worker_ids = []

[consensus.phase_timeouts]
construction_timeout = 4
campaigning_timeout = 4
voting_timeout = 4
synchronization_timeout = 4

[dfs]
enabled = true
ipfs_api_url = "http://127.0.0.1:5001"
ipfs_gateway_url = "http://127.0.0.1:8080"
cache_dir = "devnet/shared_dfs"
cache_size_gb = 5
pinning_enabled = true
gc_enabled = true
gc_interval_hours = 24

[rpc]
enabled = false
address = "0.0.0.0"
port = ${rpc_port}
cors_enabled = true
cors_origins = ["*"]
auth_enabled = false
rate_limit = 100
request_timeout = 30

[logging]
level = "info"
format = "text"
file_enabled = true
file_path = "devnet/node1/logs/catalyst.log"
max_file_size_mb = 100
max_files = 10
console_enabled = true
EOF

  echo "Devnet config written: devnet/node1/config.toml"
  echo "Bootstrap multiaddr to share: /ip4/${host}/tcp/${p2p_port}"
  echo "RPC URL to share: http://${host}:${rpc_port}"
}

devnet_up() {
  local host=""
  local p2p_port="30333"
  local rpc_port="8545"
  while [ $# -gt 0 ]; do
    case "$1" in
      --host) host="$2"; shift 2 ;;
      --p2p-port) p2p_port="$2"; shift 2 ;;
      --rpc-port) rpc_port="$2"; shift 2 ;;
      *) die "unknown arg: $1" ;;
    esac
  done
  [ -n "$host" ] || die "--host is required"
  devnet_write_config "$host" "$p2p_port" "$rpc_port"
  mkdir -p devnet/shared_dfs

  echo "Starting devnet node1 (validator + rpc exposed) ..."
  RUST_LOG=info stdbuf -oL -eL "$BIN" --config devnet/node1/config.toml start \
    --validator --rpc --rpc-address 0.0.0.0 --rpc-port "$rpc_port" \
    > devnet/node1/logs/stdout.log 2>&1 &
  echo $! > devnet/node1/node.pid
  echo "Devnet up (pid $(cat devnet/node1/node.pid))."
}

devnet_down() {
  if [ -f devnet/node1/node.pid ]; then
    pid="$(cat devnet/node1/node.pid || true)"
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      sleep 1
      kill -9 "$pid" 2>/dev/null || true
    fi
  fi
  echo "Devnet down."
}

devnet_status() {
  if [ -f devnet/node1/node.pid ]; then
    pid="$(cat devnet/node1/node.pid || true)"
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
      echo "devnet node1: running (pid $pid)"
    else
      echo "devnet node1: not running"
    fi
  else
    echo "devnet node1: no pidfile"
  fi
}

main() {
  [ $# -ge 2 ] || { usage; exit 2; }
  local net="$1"; shift
  local cmd="$1"; shift
  case "$net:$cmd" in
    testnet:up)
      local clean="false"
      if [ "${1:-}" = "--clean" ]; then clean="true"; fi
      testnet_up "$clean"
      ;;
    testnet:down) testnet_down ;;
    testnet:status) testnet_status ;;
    testnet:logs) testnet_logs "${1:-node1}" ;;
    testnet:wait-rpc) testnet_wait_rpc "${1:-http://127.0.0.1:8545}" "${2:-90}" ;;
    testnet:test-basic) testnet_test_basic ;;
    testnet:test-contract) testnet_test_contract ;;
    devnet:up) devnet_up "$@" ;;
    devnet:down) devnet_down ;;
    devnet:status) devnet_status ;;
    *) usage; exit 2 ;;
  esac
}

main "$@"

