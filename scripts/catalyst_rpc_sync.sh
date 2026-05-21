#!/usr/bin/env bash
# Refresh RPC follower DB when lagging behind validators (snapshot pull from R2).
#
# Env:
#   RPC_DATA_DIR     e.g. /var/lib/catalyst/eu/data
#   ARCHIVE_URL      stable latest.tar URL
#   VALIDATOR_URLS   space-separated RPC URLs to compare head (optional)
#   MAX_LAG_CYCLES   default 500 — restore if RPC behind max(validator) by this much
#   CLI              default ./target/release/catalyst-cli
#   SYSTEMD_UNIT     default catalyst

set -euo pipefail

RPC_DATA_DIR="${RPC_DATA_DIR:?set RPC_DATA_DIR}"
ARCHIVE_URL="${ARCHIVE_URL:?set ARCHIVE_URL}"
VALIDATOR_URLS="${VALIDATOR_URLS:-}"
MAX_LAG_CYCLES="${MAX_LAG_CYCLES:-500}"
CLI="${CLI:-./target/release/catalyst-cli}"
WORK_DIR="${WORK_DIR:-/tmp/catalyst-restore}"
SYSTEMD_UNIT="${SYSTEMD_UNIT:-catalyst}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

rpc_num() {
  curl -sf "$1" -H 'content-type: application/json' \
    -d '{"jsonrpc":"2.0","method":"catalyst_blockNumber","params":[],"id":1}' \
    | python3 -c "import sys,json; print(json.load(sys.stdin)['result'])"
}

RPC_CYCLE="$(rpc_num http://127.0.0.1:8545)"
MAX_VAL="${RPC_CYCLE}"
for u in ${VALIDATOR_URLS}; do
  v="$(rpc_num "${u}")" || continue
  [[ "${v}" -gt "${MAX_VAL}" ]] && MAX_VAL="${v}"
done

LAG=$((MAX_VAL - RPC_CYCLE))
echo "rpc_cycle=${RPC_CYCLE} validator_max=${MAX_VAL} lag=${LAG}"

if [[ "${LAG}" -le "${MAX_LAG_CYCLES}" ]]; then
  echo "rpc_sync_skip: lag within threshold (${MAX_LAG_CYCLES})"
  exit 0
fi

echo "==> RPC lag ${LAG} > ${MAX_LAG_CYCLES}; restoring from ${ARCHIVE_URL}"
bash "${SCRIPT_DIR}/catalyst_heal_local.sh" \
  --data-dir "${RPC_DATA_DIR}" \
  --archive-url "${ARCHIVE_URL}" \
  --work-dir "${WORK_DIR}" \
  --cli "${CLI}" \
  --systemd-unit "${SYSTEMD_UNIT}"

echo "rpc_sync_ok"
