#!/usr/bin/env bash
# Create a RocksDB snapshot, upload to object storage, and publish RPC metadata.
# Run on the designated *canonical* validator (US recommended) via systemd timer or cron.
#
# Required env:
#   DATA_DIR          e.g. /var/lib/catalyst/us/data
#   SNAPSHOT_OUT      e.g. /var/lib/catalyst/us/snapshots
#   ARCHIVE_URL_BASE  e.g. https://pub-....r2.dev/catalysttestnet
#   PUBLIC_ARCHIVE    e.g. ${ARCHIVE_URL_BASE}/latest.tar  (stable URL for followers)
# Optional:
#   CLI               path to catalyst-cli (default: ./target/release/catalyst-cli)
#   R2_UPLOAD_CMD     shell snippet: aws s3 cp "$ARCHIVE" "s3://bucket/..." --profile r2 ...
#   FLEET_CHECK_CONFIG  if set, run catalyst_network_check.py before snapshot (abort on fork)
#   SYSTEMD_UNIT      default: catalyst
#   RETAIN            default: 3
#   TTL_SECONDS       default: 86400

set -euo pipefail

CLI="${CLI:-./target/release/catalyst-cli}"
DATA_DIR="${DATA_DIR:?set DATA_DIR}"
SNAPSHOT_OUT="${SNAPSHOT_OUT:?set SNAPSHOT_OUT}"
ARCHIVE_URL_BASE="${ARCHIVE_URL_BASE:?set ARCHIVE_URL_BASE}"
PUBLIC_ARCHIVE="${PUBLIC_ARCHIVE:-${ARCHIVE_URL_BASE%/}/latest.tar}"
SYSTEMD_UNIT="${SYSTEMD_UNIT:-catalyst}"
RETAIN="${RETAIN:-3}"
TTL_SECONDS="${TTL_SECONDS:-86400}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ -n "${FLEET_CHECK_CONFIG:-}" ]]; then
  echo "==> Fleet alignment check (${FLEET_CHECK_CONFIG})"
  python3 "${SCRIPT_DIR}/catalyst_network_check.py" \
    --config "${FLEET_CHECK_CONFIG}" \
    --max-cycle-diff 2 \
    --fail-state-file /var/lib/catalyst/fleet-alignment.failed
fi

echo "==> Stopping ${SYSTEMD_UNIT}"
sudo systemctl stop "${SYSTEMD_UNIT}"

echo "==> snapshot-make-latest"
OUT="$("${CLI}" snapshot-make-latest \
  --data-dir "${DATA_DIR}" \
  --out-base-dir "${SNAPSHOT_OUT}" \
  --archive-url-base "${ARCHIVE_URL_BASE}" \
  --retain "${RETAIN}" \
  --ttl-seconds "${TTL_SECONDS}" 2>&1 | tee /tmp/catalyst-snapshot-make.log)"

ARCHIVE="$(echo "${OUT}" | sed -n 's/^archive_path: //p' | tail -1)"
SNAPSHOT_DIR="$(echo "${OUT}" | sed -n 's/^snapshot_dir: //p' | tail -1)"
[[ -n "${ARCHIVE}" && -f "${ARCHIVE}" ]] || { echo "archive_path not found in CLI output"; exit 1; }

if [[ -n "${R2_UPLOAD_CMD:-}" ]]; then
  echo "==> Uploading to object storage"
  ARCHIVE="${ARCHIVE}" eval "${R2_UPLOAD_CMD}"
fi

echo "==> Publishing stable latest.tar metadata"
"${CLI}" snapshot-publish \
  --data-dir "${DATA_DIR}" \
  --snapshot-dir "${SNAPSHOT_DIR}" \
  --archive-path "${ARCHIVE}" \
  --archive-url "${PUBLIC_ARCHIVE}" \
  --ttl-seconds "${TTL_SECONDS}"

echo "==> Starting ${SYSTEMD_UNIT}"
sudo systemctl start "${SYSTEMD_UNIT}"

grep applied_cycle "${SNAPSHOT_DIR}/catalyst_snapshot.json" || true
echo "snapshot_ok archive=${PUBLIC_ARCHIVE}"
