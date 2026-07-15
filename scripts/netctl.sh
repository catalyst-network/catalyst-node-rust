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
  scripts/netctl.sh testnet test-consensus-heads

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

testnet_write_config() {
  local n="$1" p2p_port="$2" rpc_port="$3" ws_port="$4" min_producer_count="$5"
  local pk1="$6" pk2="$7" pk3="$8"
  cat > "testnet/node${n}/config.toml" <<EOF
validator = false

[protocol]
chain_id = 31337
network_id = "tna_testnet"
faucet_mode = "deterministic"
allow_deterministic_faucet = true
faucet_balance = 1000000

[node]
name = "catalyst-node-${n}"
private_key_file = "testnet/node${n}/node.key"
auto_generate_identity = true

[network]
listen_addresses = [
    "/ip4/0.0.0.0/tcp/${p2p_port}",
    "/ip6/::/tcp/${p2p_port}",
]
bootstrap_peers = []
dns_seeds = []
max_peers = 50
min_peers = 5
protocol_version = "catalyst/1.0"
mdns_discovery = true
dht_enabled = true

[network.timeouts]
connection_timeout = 30
request_timeout = 10
keep_alive_interval = 30

[network.safety_limits]
max_gossip_message_bytes = 8388608
per_peer_max_msgs_per_sec = 200
per_peer_max_bytes_per_sec = 8388608
max_tcp_frame_bytes = 8388608
per_conn_max_msgs_per_sec = 200
per_conn_max_bytes_per_sec = 8388608
max_hops = 10
dedup_cache_max_entries = 20000
dial_jitter_max_ms = 250
dial_backoff_max_ms = 60000

[network.relay_cache]
max_entries = 5000
target_entries = 4000
retention_seconds = 600

[storage]
data_dir = "testnet/node${n}/data"
enabled = false
capacity_gb = 10
cache_size_mb = 256
write_buffer_size_mb = 64
max_open_files = 1000
compression_enabled = true
history_prune_enabled = false
history_keep_cycles = 604800
history_prune_interval_seconds = 300
history_prune_batch_cycles = 1000

[consensus]
cycle_duration_seconds = 20
freeze_time_seconds = 1
max_transactions_per_block = 1000
min_producer_count = ${min_producer_count}
max_producer_count = 100
validator_worker_ids = [
    "${pk1}",
    "${pk2}",
    "${pk3}",
]
require_lsu_finality = true
require_state_root_finality = true

[consensus.phase_timeouts]
construction_timeout = 4
campaigning_timeout = 4
voting_timeout = 4
synchronization_timeout = 4

[runtimes.evm]
enabled = true
gas_limit = 8000000
gas_price = 1000000000
max_code_size = 24576
debug_enabled = false

[runtimes.svm]
enabled = false
compute_unit_limit = 200000
max_account_data_size = 10485760

[runtimes.wasm]
enabled = false
max_memory_pages = 1024
execution_timeout_ms = 5000

[service_bus]
enabled = true
websocket_address = "0.0.0.0"
websocket_port = ${ws_port}
max_connections = 1000
event_buffer_size = 10000
webhooks_enabled = true
webhook_timeout = 30
webhook_max_retries = 3

[dfs]
enabled = true
ipfs_api_url = "http://127.0.0.1:5001"
ipfs_gateway_url = "http://127.0.0.1:8080"
cache_dir = "testnet/shared_dfs"
cache_size_gb = 5
pinning_enabled = true
gc_enabled = true
gc_interval_hours = 24

[rpc]
enabled = false
address = "127.0.0.1"
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
file_path = "testnet/node${n}/logs/catalyst.log"
max_file_size_mb = 100
max_files = 10
console_enabled = true
EOF
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
    rm -f testnet/node1/config.toml testnet/node2/config.toml testnet/node3/config.toml || true
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

  # validator set (all three local validators — checklist §7.1)
  PK1="$("$BIN" --log-level error pubkey --key-file testnet/node1/node.key | tail -n 1)"
  PK2="$("$BIN" --log-level error pubkey --key-file testnet/node2/node.key | tail -n 1)"
  PK3="$("$BIN" --log-level error pubkey --key-file testnet/node3/node.key | tail -n 1)"
  printf 'validator_worker_ids = ["%s", "%s", "%s"]\n' "$PK1" "$PK2" "$PK3" > testnet/validators.toml

  # config.toml is gitignored (local runtime state) and not generated anywhere else, so a fresh
  # checkout (e.g. CI) has none — generate one per node if missing, wired to this run's freshly
  # derived validator ids. Never overwrites an existing config.toml (preserves local customization
  # on machines that already have one from prior manual setup); --clean removes them first so a
  # clean run always regenerates with the current node.key-derived ids.
  [ -f testnet/node1/config.toml ] || testnet_write_config 1 30333 8545 8546 2 "$PK1" "$PK2" "$PK3"
  [ -f testnet/node2/config.toml ] || testnet_write_config 2 30334 8546 8547 3 "$PK1" "$PK2" "$PK3"
  [ -f testnet/node3/config.toml ] || testnet_write_config 3 30335 8547 8548 3 "$PK1" "$PK2" "$PK3"

  local finality_env=()
  if [ -n "${CATALYST_REQUIRE_LSU_FINALITY:-}" ]; then
    finality_env=( "CATALYST_REQUIRE_LSU_FINALITY=${CATALYST_REQUIRE_LSU_FINALITY}" )
  else
    # Local harness: DFS/finality gossip may lag; disable until fleet-ready.
    finality_env=( "CATALYST_REQUIRE_LSU_FINALITY=0" )
  fi

  echo "Starting node1 (bootstrap + validator + rpc) ..."
  env RUST_LOG="${RUST_LOG:-info}" "${finality_env[@]}" stdbuf -oL -eL "$BIN" --config testnet/node1/config.toml start \
    --validator --storage --rpc --rpc-port 8545 \
    > testnet/node1/logs/stdout.log 2>&1 &
  echo $! > testnet/node1/node.pid

  sleep 2
  echo "Starting node2 (validator + storage + rpc) ..."
  env RUST_LOG="${RUST_LOG:-info}" "${finality_env[@]}" stdbuf -oL -eL "$BIN" --config testnet/node2/config.toml start \
    --validator --storage --rpc --rpc-port 8546 \
    --bootstrap-peers "/ip4/127.0.0.1/tcp/30333" \
    > testnet/node2/logs/stdout.log 2>&1 &
  echo $! > testnet/node2/node.pid

  sleep 2
  echo "Starting node3 (validator + storage + rpc) ..."
  env RUST_LOG="${RUST_LOG:-info}" "${finality_env[@]}" stdbuf -oL -eL "$BIN" --config testnet/node3/config.toml start \
    --validator --storage --rpc --rpc-port 8547 \
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
    echo "head: $(rpc_call "$rpc" catalyst_head '[]' | python3 -c 'import sys,json; print(json.load(sys.stdin).get("result"))' 2>/dev/null || true)"
    echo "peers: $(rpc_call "$rpc" catalyst_peerCount '[]' | python3 -c 'import sys,json; print(json.load(sys.stdin).get("result"))' 2>/dev/null || true)"
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
  local fund_out tx_id
  fund_out="$("$BIN" send "$node1_pubkey" 3 --key-file testnet/faucet.key --rpc-url "$rpc" 2>&1)" || true
  echo "$fund_out"
  tx_id="$(echo "$fund_out" | awk '/^tx_id:/{print $2}')"
  if [ -n "$tx_id" ]; then
    wait_for_tx_applied "$rpc" "$tx_id" 180 || echo "WARN: faucet->node1 tx not applied within timeout" >&2
  fi

  echo "head: $(rpc_call "$rpc" catalyst_head '[]' | python3 -c 'import sys,json; print(json.load(sys.stdin).get("result"))' 2>/dev/null || true)"
  echo "balance(node1): $(rpc_balance_atoms "$rpc" "$node1_pubkey")"
}

# Parse JSON-RPC result for catalyst_getBalance (handles null/error responses).
rpc_balance_atoms() {
  local rpc="$1"
  local addr="$2"
  rpc_call "$rpc" catalyst_getBalance "[\"${addr}\"]" | python3 -c '
import sys, json
try:
    d = json.load(sys.stdin)
    r = d.get("result")
    if r is None:
        err = d.get("error")
        if err:
            sys.stderr.write("getBalance error: %s\n" % err)
        print("0")
    else:
        print(r)
except Exception as e:
    sys.stderr.write("getBalance parse error: %s\n" % e)
    print("0")
'
}

wait_for_balance_at_least() {
  local rpc="$1"
  local addr="$2"
  local min_balance="$3"
  local timeout_secs="${4:-90}"
  local deadline=$(( $(date +%s) + timeout_secs ))
  while true; do
    local bal
    bal="$(rpc_balance_atoms "$rpc" "$addr")"
    if [ -n "${bal:-}" ] && [ "${bal}" -eq "${bal}" ] 2>/dev/null && [ "${bal}" -ge "$min_balance" ]; then
      return 0
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
      return 1
    fi
    sleep 2
  done
}

# Wait until tx is applied (or dropped/failed). send() returns before inclusion; balance polls need this.
wait_for_tx_applied() {
  local rpc="$1"
  local tx_hash="$2"
  local timeout_secs="${3:-180}"
  local deadline=$(( $(date +%s) + timeout_secs ))
  [ -n "$tx_hash" ] || return 1
  while true; do
    local status success err
    status="$(rpc_call "$rpc" catalyst_getTransactionReceipt "[\"${tx_hash}\"]" | python3 -c '
import sys, json
try:
    d = json.load(sys.stdin)
    r = d.get("result")
    if r is None:
        print("none")
    else:
        st = r.get("status", "")
        if st == "applied" and r.get("success") is False:
            print("failed:" + str(r.get("error", "unknown")))
        else:
            print(st)
except Exception as e:
    print("parse_err", file=sys.stderr)
    print("none")
')"
    case "$status" in
      applied)
        return 0
        ;;
      failed:*)
        die "transaction applied with failure: ${status#failed:}"
        ;;
      dropped)
        die "transaction was dropped (tx_id=${tx_hash})"
        ;;
      none|pending|selected|"")
        ;;
      *)
        ;;
    esac
    if [ "$(date +%s)" -ge "$deadline" ]; then
      echo "Last receipt status for ${tx_hash}: ${status:-unknown}" >&2
      return 1
    fi
    sleep 1
  done
}

rpc_head_json() {
  local url="$1"
  rpc_call "$url" catalyst_head '[]'
}

rpc_head_field() {
  local url="$1"
  local field="$2"
  rpc_head_json "$url" | python3 -c "import sys,json; r=json.load(sys.stdin).get('result') or {}; print(r.get('${field}',''))"
}

# Checklist §7.1: three multi-process validators must share applied_state_root at the same cycle.
testnet_test_consensus_heads() {
  local min_cycle="${1:-3}"
  local timeout="${2:-180}"
  local rpc1="http://127.0.0.1:8545"
  local rpc2="http://127.0.0.1:8546"
  local rpc3="http://127.0.0.1:8547"

  testnet_wait_rpc "$rpc1" 90
  testnet_wait_rpc "$rpc2" 90
  testnet_wait_rpc "$rpc3" 90

  local deadline=$(( $(date +%s) + timeout ))
  echo "Waiting for applied_cycle >= ${min_cycle} on all validators (timeout ${timeout}s) ..."
  while true; do
    local c1 c2 c3
    c1="$(rpc_head_field "$rpc1" applied_cycle)"
    c2="$(rpc_head_field "$rpc2" applied_cycle)"
    c3="$(rpc_head_field "$rpc3" applied_cycle)"
    if [ -n "$c1" ] && [ -n "$c2" ] && [ -n "$c3" ] \
      && [ "$c1" -ge "$min_cycle" ] 2>/dev/null \
      && [ "$c2" -ge "$min_cycle" ] 2>/dev/null \
      && [ "$c3" -ge "$min_cycle" ] 2>/dev/null; then
      break
    fi
    if [ "$(date +%s)" -ge "$deadline" ]; then
      echo "node1 head: $(rpc_head_json "$rpc1")"
      echo "node2 head: $(rpc_head_json "$rpc2")"
      echo "node3 head: $(rpc_head_json "$rpc3")"
      die "validators did not reach cycle ${min_cycle} within timeout"
    fi
    sleep 3
  done

  local r1 r2 r3 h1 h2 h3
  r1="$(rpc_head_field "$rpc1" applied_state_root)"
  r2="$(rpc_head_field "$rpc2" applied_state_root)"
  r3="$(rpc_head_field "$rpc3" applied_state_root)"
  h1="$(rpc_head_field "$rpc1" applied_lsu_hash)"
  h2="$(rpc_head_field "$rpc2" applied_lsu_hash)"
  h3="$(rpc_head_field "$rpc3" applied_lsu_hash)"
  c1="$(rpc_head_field "$rpc1" applied_cycle)"
  c2="$(rpc_head_field "$rpc2" applied_cycle)"
  c3="$(rpc_head_field "$rpc3" applied_cycle)"

  echo "node1 cycle=$c1 state_root=$r1"
  echo "node2 cycle=$c2 state_root=$r2"
  echo "node3 cycle=$c3 state_root=$r3"

  if [ "$r1" != "$r2" ] || [ "$r1" != "$r3" ]; then
    die "divergent applied_state_root across validators"
  fi
  if [ "$h1" != "$h2" ] || [ "$h1" != "$h3" ]; then
    die "divergent applied_lsu_hash across validators (state_root matched)"
  fi
  echo "PASS: identical applied_state_root and applied_lsu_hash at cycle >= ${min_cycle}"
}

testnet_test_contract() {
  local rpc="http://127.0.0.1:8545"
  testnet_wait_rpc "$rpc" 90

  # Use a fresh wallet for EVM deploy/call to avoid faucet nonce interaction pitfalls.
  local evm_wallet="testnet/evm_wallet.key"
  python3 -c 'import os,binascii; print(binascii.hexlify(os.urandom(32)).decode())' > "$evm_wallet"
  local evm_pk
  evm_pk="$("$BIN" --log-level error pubkey --key-file "$evm_wallet" | tail -n 1)"
  [ -n "$evm_pk" ] || die "could not derive evm wallet pubkey"

  echo "Funding fresh EVM wallet from faucet..."
  local fund_out tx_id
  fund_out="$("$BIN" send "$evm_pk" 100 --key-file testnet/faucet.key --rpc-url "$rpc" 2>&1)" || true
  echo "$fund_out"
  tx_id="$(echo "$fund_out" | awk '/^tx_id:/{print $2}')"
  [ -n "$tx_id" ] || die "send did not return tx_id (check faucet key and RPC)"
  wait_for_tx_applied "$rpc" "$tx_id" 180 || die "faucet funding tx did not apply (tx_id=${tx_id})"
  wait_for_balance_at_least "$rpc" "$evm_pk" 1 90 || die "evm wallet funding not observed after applied tx (check getBalance / execution)"

  echo "Deploying deterministic initcode contract (returns 0x2a)..."
  local out
  out="$(cargo run -q -p catalyst-cli -- deploy testdata/evm/return_2a_initcode.hex --key-file "$evm_wallet" --rpc-url "$rpc" --runtime evm --wait --verify-code --timeout-secs 240 --poll-ms 1000)"
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
  cargo run -q -p catalyst-cli -- call "$addr" 0x --key-file "$evm_wallet" --rpc-url "$rpc" --value 0

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
    testnet:test-consensus-heads) testnet_test_consensus_heads "$@" ;;
    devnet:up) devnet_up "$@" ;;
    devnet:down) devnet_down ;;
    devnet:status) devnet_status ;;
    *) usage; exit 2 ;;
  esac
}

main "$@"

