#!/usr/bin/env bash
# Checklist Phase 10 (consensus-implementation-completion-checklist.md): multi-process
# restart-recovery gate. Stops a live validator, takes an offline db-backup, resumes it
# normally for several more cycles, then stops it again and restores the now-STALE backup
# before restarting — asserting the fleet still reconverges on an identical
# applied_state_root / applied_lsu_hash afterward.
#
# This reproduces the trigger pattern identified in docs/consensus-reliability-review-2026-07.md
# for the 2026-07-03 production fork: "restoring liveness after a freeze is exactly the moment a
# stale/divergent node resumes independently accruing rewards on top of whatever state it had when
# it froze". Unlike scripts/consensus-three-validator-e2e.sh (happy-path convergence only) or
# scripts/wan_chaos/run.sh (restarts with the SAME data dir, and needs sudo netns/iptables for its
# partition features), this gate specifically exercises "resume on stale data" without any special
# privileges, so it's runnable in ordinary CI.
#
# Note: `db-backup`/`db-restore` open the RocksDB data directory directly and cannot run against a
# LIVE node's data dir (RocksDB's own LOCK file rejects a second opener) — so node2 is stopped
# before each backup/restore, not snapshotted while running.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${CARGO_TARGET_DIR:-$ROOT_DIR/target}/release/catalyst-cli"

# Cycle at which node2 is stopped and backed up (the "stale" point later restored).
BACKUP_CYCLE="${BACKUP_CYCLE:-4}"
# How many further cycles node2 must apply live (after resuming normally) before it's stopped
# again and rolled back — this gap is what makes the restored backup genuinely stale.
RESUME_AFTER_CYCLES="${RESUME_AFTER_CYCLES:-4}"
CONVERGE_TIMEOUT_SECS="${CONVERGE_TIMEOUT_SECS:-240}"
CLEAN="${CLEAN:-true}"

RPC2="http://127.0.0.1:8546"

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

# Stop node2 and wait for it to actually exit (releasing the RocksDB LOCK file) before returning.
stop_node2() {
  [ -f testnet/node2/node.pid ] || return 0
  local pid
  pid="$(cat testnet/node2/node.pid)"
  kill "$pid" 2>/dev/null || true
  local deadline=$(($(date +%s) + 30))
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$(date +%s)" -ge "$deadline" ]; then
      kill -9 "$pid" 2>/dev/null || true
      break
    fi
    sleep 1
  done
  rm -f testnet/node2/node.pid
}

start_node2() {
  env RUST_LOG="${RUST_LOG:-info}" CATALYST_REQUIRE_LSU_FINALITY="${CATALYST_REQUIRE_LSU_FINALITY:-0}" \
    stdbuf -oL -eL "$BIN" --config testnet/node2/config.toml start \
    --validator --storage --rpc --rpc-port 8546 \
    --bootstrap-peers "/ip4/127.0.0.1/tcp/30333" \
    >>testnet/node2/logs/stdout.log 2>&1 &
  echo $! >testnet/node2/node.pid
}

echo "==> consensus-restart-recovery-e2e: build"
cargo build --release -p catalyst-cli

echo "==> consensus-restart-recovery-e2e: start testnet (3 validators)"
if [ "$CLEAN" = "true" ]; then
  bash scripts/netctl.sh testnet up --clean
else
  bash scripts/netctl.sh testnet up
fi

bash scripts/netctl.sh testnet wait-rpc "$RPC2" 90

echo "==> waiting for node2 to reach cycle ${BACKUP_CYCLE} before backing up"
wait_for_cycle "$RPC2" "$BACKUP_CYCLE" 180 >/dev/null

echo "==> stopping node2 to take an offline db-backup (this becomes the STALE data restored later)"
stop_node2
BACKUP_DIR="testnet/node2/backup_$(date +%s)"
"$BIN" db-backup --data-dir testnet/node2/data --out-dir "$BACKUP_DIR"

echo "==> resuming node2 normally"
start_node2
bash scripts/netctl.sh testnet wait-rpc "$RPC2" 90

RESUME_AT=$((BACKUP_CYCLE + RESUME_AFTER_CYCLES))
echo "==> waiting for node2 to advance to cycle ${RESUME_AT} (making the backup above stale) before rolling it back"
wait_for_cycle "$RPC2" "$RESUME_AT" 180 >/dev/null

echo "==> stopping node2 again and restoring the now-stale backup (simulates 'freeze then resume on rolled-back data')"
stop_node2
"$BIN" db-restore --data-dir testnet/node2/data --from-dir "$BACKUP_DIR"

echo "==> restarting node2 on the stale data directory"
start_node2

echo "==> asserting fleet reconverges after node2's stale-data restart (timeout ${CONVERGE_TIMEOUT_SECS}s)"
bash scripts/netctl.sh testnet test-consensus-heads "$((RESUME_AT + 2))" "$CONVERGE_TIMEOUT_SECS"

echo "==> DONE: fleet converged on an identical applied_state_root after restart-on-stale-data"
