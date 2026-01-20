#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RPC_URL="${RPC_URL:-http://127.0.0.1:8545}"
RPC_HOSTPORT="${RPC_HOSTPORT:-127.0.0.1:8545}"

BIN="${CARGO_TARGET_DIR:-$ROOT_DIR/target}/release/catalyst-cli"

AMOUNT="${AMOUNT:-3}"
TIMEOUT_SECS="${TIMEOUT_SECS:-90}"

cleanup() {
  make stop-testnet >/dev/null 2>&1 || true
}
trap cleanup EXIT

rpc_get_balance() {
  local addr="$1"
  curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"catalyst_getBalance\",\"params\":[\"${addr}\"]}" \
    | python3 -c 'import sys,json; print(int(json.load(sys.stdin)["result"]))'
}

echo "==> smoke_testnet: stopping any existing testnet"
make stop-testnet >/dev/null 2>&1 || true

echo "==> smoke_testnet: wiping previous testnet state (data + shared dfs)"
rm -rf \
  testnet/node1/data testnet/node2/data testnet/node3/data \
  testnet/node1/dfs_cache testnet/node2/dfs_cache testnet/node3/dfs_cache \
  testnet/shared_dfs \
  testnet/node1/logs testnet/node2/logs testnet/node3/logs \
  testnet/validators.toml || true

echo "==> smoke_testnet: building release catalyst-cli"
cargo build --release -p catalyst-cli

echo "==> smoke_testnet: creating testnet dirs + faucet key"
mkdir -p testnet/node1/{logs,data,dfs_cache} testnet/node2/{logs,data,dfs_cache} testnet/node3/{logs,data,dfs_cache}
python3 -c 'print("fa"*32)' > testnet/faucet.key

# Pre-generate per-node identities so we can build validator set before startup.
for n in 1 2 3; do
  key="testnet/node${n}/node.key"
  if [ ! -f "$key" ]; then
    python3 -c 'import os,binascii; print(binascii.hexlify(os.urandom(32)).decode())' > "$key"
  fi
done

# Validator set (node1 + node2): protocol producer selection input.
PK1="$("$BIN" --log-level error pubkey --key-file testnet/node1/node.key | tail -n 1)"
PK2="$("$BIN" --log-level error pubkey --key-file testnet/node2/node.key | tail -n 1)"
printf 'validator_worker_ids = ["%s", "%s"]\n' "$PK1" "$PK2" > testnet/validators.toml

echo "==> smoke_testnet: starting node1 (validator + rpc) WITHOUT txgen"
RUST_LOG=info stdbuf -oL -eL "$BIN" --config testnet/node1/config.toml start \
  --validator --rpc --rpc-port 8545 \
  > testnet/node1/logs/stdout.log 2>&1 &
echo $! > testnet/node1/node.pid

sleep 2
echo "==> smoke_testnet: starting node2 (validator + storage)"
RUST_LOG=info stdbuf -oL -eL "$BIN" --config testnet/node2/config.toml start \
  --validator --storage --rpc-port 8546 \
  --bootstrap-peers "/ip4/127.0.0.1/tcp/30333" \
  > testnet/node2/logs/stdout.log 2>&1 &
echo $! > testnet/node2/node.pid

sleep 2
echo "==> smoke_testnet: starting node3 (storage)"
RUST_LOG=info stdbuf -oL -eL "$BIN" --config testnet/node3/config.toml start \
  --storage --rpc-port 8547 \
  --bootstrap-peers "/ip4/127.0.0.1/tcp/30333" \
  > testnet/node3/logs/stdout.log 2>&1 &
echo $! > testnet/node3/node.pid

echo "==> smoke_testnet: waiting for RPC $RPC_HOSTPORT (timeout ${TIMEOUT_SECS}s)"
deadline=$(( $(date +%s) + TIMEOUT_SECS ))
while ! (echo >"/dev/tcp/${RPC_HOSTPORT/:/\/}" 2>/dev/null); do
  if [ "$(date +%s)" -ge "$deadline" ]; then
    echo "ERROR: RPC not reachable at $RPC_URL"
    tail -n 80 testnet/node1/logs/stdout.log || true
    exit 1
  fi
  sleep 1
done

NODE1_PUBKEY="$(grep -a "Node ID:" -m1 testnet/node1/logs/stdout.log | awk '{print $NF}' || true)"
if [ -z "${NODE1_PUBKEY}" ]; then
  echo "ERROR: couldn't parse node1 pubkey from logs"
  tail -n 80 testnet/node1/logs/stdout.log || true
  exit 1
fi
echo "==> node1_pubkey=$NODE1_PUBKEY"

echo "==> smoke_testnet: baseline balance"
BAL_BEFORE="$(rpc_get_balance "$NODE1_PUBKEY" || echo 0)"
echo "==> balance_before=$BAL_BEFORE"

echo "==> smoke_testnet: sending tx (faucet -> node1) amount=$AMOUNT"
cargo run -q -p catalyst-cli -- send "$NODE1_PUBKEY" "$AMOUNT" --key-file testnet/faucet.key --rpc-url "$RPC_URL"

echo "==> smoke_testnet: allowing RPC->mempool persistence to settle"
sleep 4

echo "==> smoke_testnet: hard-restarting node1 quickly (tests mempool persistence + rehydrate)"
if [ -f testnet/node1/node.pid ]; then
  kill "$(cat testnet/node1/node.pid)" >/dev/null 2>&1 || true
fi
sleep 1
RUST_LOG=info stdbuf -oL -eL "$BIN" --config testnet/node1/config.toml start \
  --validator --rpc --rpc-port 8545 \
  > testnet/node1/logs/stdout.log 2>&1 &
echo $! > testnet/node1/node.pid

echo "==> smoke_testnet: waiting for node1 RPC to come back"
deadline=$(( $(date +%s) + TIMEOUT_SECS ))
while ! (echo >"/dev/tcp/${RPC_HOSTPORT/:/\/}" 2>/dev/null); do
  if [ "$(date +%s)" -ge "$deadline" ]; then
    echo "ERROR: RPC didn't come back after restart"
    tail -n 80 testnet/node1/logs/stdout.log || true
    exit 1
  fi
  sleep 1
done

echo "==> smoke_testnet: waiting for tx to be included (polling balance for up to ${TIMEOUT_SECS}s)"
deadline=$(( $(date +%s) + TIMEOUT_SECS ))
while true; do
  BAL_NOW="$(rpc_get_balance "$NODE1_PUBKEY" || echo 0)"
  if [ "${BAL_NOW}" != "${BAL_BEFORE}" ] && [ "$BAL_NOW" -gt "$BAL_BEFORE" ]; then
    echo "==> PASS: balance increased after restart: $BAL_BEFORE -> $BAL_NOW"
    break
  fi
  if [ "$(date +%s)" -ge "$deadline" ]; then
    echo "ERROR: balance did not increase within timeout (expected +$AMOUNT eventually)"
    echo "node1 stdout (tail):"
    tail -n 120 testnet/node1/logs/stdout.log || true
    exit 1
  fi
  sleep 2
done

echo "==> smoke_testnet: stopping testnet"
make stop-testnet >/dev/null 2>&1 || true
echo "==> DONE"

