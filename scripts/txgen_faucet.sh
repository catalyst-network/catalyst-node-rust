#!/usr/bin/env bash
set -euo pipefail

# txgen_faucet.sh
#
# Generates a small, random number of faucet-funded transfers per block so
# blocks don’t look empty.
#
# Designed for public testnets: runs against a single RPC endpoint and relies
# on the network to propagate txs to the deterministic batch leader.
#
# Safety: keep MAX_TX_PER_BLOCK and AMOUNT_MAX low so you don’t drain faucet.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

RPC_URL="${RPC_URL:-http://127.0.0.1:8545}"
BIN="${BIN:-${CARGO_TARGET_DIR:-$ROOT_DIR/target}/release/catalyst-cli}"

STATE_DIR="${STATE_DIR:-$ROOT_DIR/txgen}"
FAUCET_KEY_FILE="${FAUCET_KEY_FILE:-$STATE_DIR/faucet.key}"
RECIPS_FILE="${RECIPS_FILE:-$STATE_DIR/recipients.txt}"

RECIP_COUNT="${RECIP_COUNT:-25}"
MAX_TX_PER_BLOCK="${MAX_TX_PER_BLOCK:-2}"   # 0..MAX per block
AMOUNT_MIN="${AMOUNT_MIN:-1}"
AMOUNT_MAX="${AMOUNT_MAX:-3}"
POLL_SECS="${POLL_SECS:-2}"
JITTER_MS_MAX="${JITTER_MS_MAX:-600}"       # delay between txs in a block

# Optional: stop after N blocks (unset => run forever)
STOP_AFTER_BLOCKS="${STOP_AFTER_BLOCKS:-}"

need_bin() {
  if [ ! -x "$BIN" ]; then
    echo "ERROR: catalyst-cli not found/executable at BIN=$BIN"
    echo "Build it with: cargo build --release -p catalyst-cli"
    exit 1
  fi
}

rpc_applied_cycle() {
  # NOTE: do NOT use `python3 - <<EOF` in a pipeline: that consumes stdin for the program,
  # causing `curl` to error with "failed writing body" (exit 23) under `pipefail`.
  local body
  body="$(curl -s -X POST "$RPC_URL" -H 'content-type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_getSyncInfo","params":[]}' 2>/dev/null || true)"
  python3 -c 'import sys, json
try:
  obj = json.load(sys.stdin)
  head = (obj.get("result") or {}).get("head") or {}
  print(int(head.get("applied_cycle", 0)))
except Exception:
  print(0)
' <<<"$body" 2>/dev/null || echo 0
}

ensure_state() {
  mkdir -p "$STATE_DIR"
  if [ ! -f "$FAUCET_KEY_FILE" ]; then
    # Dev-only deterministic faucet key (matches node genesis logic)
    python3 -c 'print("fa"*32)' > "$FAUCET_KEY_FILE"
  fi

  if [ ! -f "$RECIPS_FILE" ]; then
    : > "$RECIPS_FILE"
  fi

  # Ensure we have at least RECIP_COUNT recipients. Each line is a 32-byte pubkey hex.
  local cur
  cur="$(grep -c '^[0-9a-f]\{64\}$' "$RECIPS_FILE" 2>/dev/null || true)"
  if [ "${cur}" -lt "${RECIP_COUNT}" ]; then
    local to_make=$((RECIP_COUNT - cur))
    for _ in $(seq 1 "$to_make"); do
      # We only need pubkeys; recipients never sign.
      # Generate a random 32-byte key file and derive its pubkey.
      local kf="$STATE_DIR/recip_$(date +%s%N)_$RANDOM.key"
      python3 -c 'import os,binascii; print(binascii.hexlify(os.urandom(32)).decode())' > "$kf"
      "$BIN" --log-level error pubkey --key-file "$kf" | tail -n 1 >> "$RECIPS_FILE"
      rm -f "$kf" || true
    done
  fi
}

rand_int() {
  # inclusive range [lo, hi]
  local lo="$1"
  local hi="$2"
  if [ "$hi" -lt "$lo" ]; then
    echo "$lo"
    return
  fi
  local span=$((hi - lo + 1))
  echo $((lo + (RANDOM % span)))
}

pick_recipient() {
  # pick a random line from recipients file (only valid 64-hex lines)
  local idx
  idx="$(rand_int 1 "$RECIP_COUNT")"
  awk 'BEGIN{n=0} /^[0-9a-f]{64}$/ {n++; if(n=='"$idx"'){print; exit}}' "$RECIPS_FILE"
}

send_one() {
  local to_pk="$1"
  local amt="$2"
  local out
  out="$("$BIN" send "$to_pk" "$amt" --key-file "$FAUCET_KEY_FILE" --rpc-url "$RPC_URL" 2>/dev/null || true)"
  local txid
  txid="$(printf "%s\n" "$out" | awk '/tx_id:/ {print $2}' | tail -n 1)"
  if [ -z "$txid" ]; then
    echo "WARN: send failed (amt=$amt to=$to_pk). Output:"
    printf "%s\n" "$out" | tail -n 20 || true
    return 1
  fi
  echo "$txid"
}

main() {
  need_bin
  ensure_state

  echo "==> txgen: RPC_URL=$RPC_URL"
  echo "==> txgen: MAX_TX_PER_BLOCK=$MAX_TX_PER_BLOCK AMOUNT_RANGE=[$AMOUNT_MIN,$AMOUNT_MAX] RECIP_COUNT=$RECIP_COUNT"
  echo "==> txgen: state dir: $STATE_DIR"
  echo "==> txgen: faucet key: $FAUCET_KEY_FILE"
  echo "==> txgen: recipients: $RECIPS_FILE"

  local last
  last="$(rpc_applied_cycle)"
  echo "==> txgen: starting at applied_cycle=$last"

  local blocks=0
  while true; do
    local cur
    cur="$(rpc_applied_cycle)"
    if [ "$cur" -le "$last" ]; then
      sleep "$POLL_SECS"
      continue
    fi

    last="$cur"
    blocks=$((blocks + 1))

    local n
    n="$(rand_int 0 "$MAX_TX_PER_BLOCK")"
    if [ "$n" -eq 0 ]; then
      echo "cycle=$cur txs=0"
    else
      echo "cycle=$cur txs=$n"
    fi

    for _ in $(seq 1 "$n"); do
      local to_pk amt txid
      to_pk="$(pick_recipient)"
      amt="$(rand_int "$AMOUNT_MIN" "$AMOUNT_MAX")"
      txid="$(send_one "$to_pk" "$amt" || true)"
      if [ -n "$txid" ]; then
        echo "  sent tx_id=$txid amount=$amt to=$to_pk"
      fi
      # jitter to avoid sending all txs at same instant
      local jms
      jms="$(rand_int 0 "$JITTER_MS_MAX")"
      python3 - <<PY
import time
time.sleep(${jms}/1000.0)
PY
    done

    if [ -n "$STOP_AFTER_BLOCKS" ] && [ "$blocks" -ge "$STOP_AFTER_BLOCKS" ]; then
      echo "==> txgen: stopping after $blocks blocks"
      break
    fi
  done
}

main "$@"

