#!/usr/bin/env bash
# Realign this host's chain DB from the canonical snapshot URL (outlier recovery).
#
# Usage (on EU after US+Asia agree):
#   sudo bash scripts/catalyst_heal_local.sh \
#     --data-dir /var/lib/catalyst/eu/data \
#     --archive-url https://pub-....r2.dev/catalysttestnet/latest.tar
#
# Options:
#   --data-dir PATH
#   --archive-url URL
#   --work-dir PATH     default /tmp/catalyst-restore
#   --cli PATH          default ./target/release/catalyst-cli
#   --no-start          stop + restore only (do not systemctl start)
#   --systemd-unit NAME default catalyst

set -euo pipefail

DATA_DIR=""
ARCHIVE_URL=""
WORK_DIR="/tmp/catalyst-restore"
CLI="./target/release/catalyst-cli"
NO_START=0
SYSTEMD_UNIT="catalyst"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --data-dir) DATA_DIR="$2"; shift 2 ;;
    --archive-url) ARCHIVE_URL="$2"; shift 2 ;;
    --work-dir) WORK_DIR="$2"; shift 2 ;;
    --cli) CLI="$2"; shift 2 ;;
    --no-start) NO_START=1; shift ;;
    --systemd-unit) SYSTEMD_UNIT="$2"; shift 2 ;;
    *) echo "unknown arg: $1"; exit 2 ;;
  esac
done

[[ -n "${DATA_DIR}" && -n "${ARCHIVE_URL}" ]] || {
  echo "requires --data-dir and --archive-url"
  exit 2
}

echo "==> stop ${SYSTEMD_UNIT}"
sudo systemctl stop "${SYSTEMD_UNIT}" || true

echo "==> wipe data + extract dir"
sudo rm -rf "${DATA_DIR}" "${WORK_DIR}"
sudo mkdir -p "${DATA_DIR}" "${WORK_DIR}/extract"

echo "==> sync-from-archive"
"${CLI}" sync-from-archive \
  --archive-url "${ARCHIVE_URL}" \
  --data-dir "${DATA_DIR}" \
  --work-dir "${WORK_DIR}"

if [[ "${NO_START}" -eq 0 ]]; then
  echo "==> start ${SYSTEMD_UNIT}"
  sudo systemctl start "${SYSTEMD_UNIT}"
  sleep 3
  curl -s http://127.0.0.1:8545 -H 'content-type: application/json' \
    -d '{"jsonrpc":"2.0","method":"catalyst_head","params":[],"id":1}' || true
  echo
fi

echo "heal_local_ok"
