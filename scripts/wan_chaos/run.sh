#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

DURATION_SECS="${DURATION_SECS:-300}"
LOSS_PCT="${LOSS_PCT:-0}"
LATENCY_MS="${LATENCY_MS:-50}"
JITTER_MS="${JITTER_MS:-10}"

PARTITION_AT_SECS="${PARTITION_AT_SECS:-}"
PARTITION_DURATION_SECS="${PARTITION_DURATION_SECS:-30}"

RESTART_AT_SECS="${RESTART_AT_SECS:-}"
RESTART_NODE="${RESTART_NODE:-n1}"

RUN_ID="$(date +%s)"
OUT_DIR="${ROOT_DIR}/scripts/wan_chaos/out/${RUN_ID}"
mkdir -p "${OUT_DIR}"

BR="catalystbr0"
NS_PREFIX="catalyst-n"

CLI="${ROOT_DIR}/target/release/catalyst-cli"
if [[ ! -x "${CLI}" ]]; then
  echo "building catalyst-cli..."
  (cd "${ROOT_DIR}" && cargo build -p catalyst-cli --release --locked)
fi

cleanup() {
  set +e
  sudo bash "${ROOT_DIR}/scripts/wan_chaos/cleanup.sh" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "run_id: ${RUN_ID}" | tee "${OUT_DIR}/run.meta"
echo "duration_secs: ${DURATION_SECS}" | tee -a "${OUT_DIR}/run.meta"
echo "netem: latency_ms=${LATENCY_MS} jitter_ms=${JITTER_MS} loss_pct=${LOSS_PCT}" | tee -a "${OUT_DIR}/run.meta"

sudo bash "${ROOT_DIR}/scripts/wan_chaos/cleanup.sh" >/dev/null 2>&1 || true

sudo ip link add name "${BR}" type bridge
sudo ip link set "${BR}" up

for i in 1 2 3; do
  ns="${NS_PREFIX}${i}"
  veth="veth${i}"
  peer="veth${i}p"
  ip="10.70.0.${i}"

  sudo ip netns add "${ns}"
  sudo ip link add "${veth}" type veth peer name "${peer}"
  sudo ip link set "${veth}" master "${BR}"
  sudo ip link set "${veth}" up
  sudo ip link set "${peer}" netns "${ns}"

  sudo ip netns exec "${ns}" ip link set lo up
  sudo ip netns exec "${ns}" ip addr add "${ip}/24" dev "${peer}"
  sudo ip netns exec "${ns}" ip link set "${peer}" up

  # Default route via bridge is not needed for same-subnet comms.

  # Apply netem on the namespace-facing veth.
  sudo ip netns exec "${ns}" tc qdisc add dev "${peer}" root netem \
    delay "${LATENCY_MS}ms" "${JITTER_MS}ms" distribution normal \
    loss "${LOSS_PCT}%"
done

make_cfg() {
  local ip="$1"
  local dir="$2"
  mkdir -p "$dir"
  cat > "${dir}/config.toml" <<EOF
[network]
listen_addresses = ["\/ip4\/${ip}\/tcp\/30333"]
bootstrap_peers = [
  "\/ip4\/10.70.0.1\/tcp\/30333",
  "\/ip4\/10.70.0.2\/tcp\/30333",
  "\/ip4\/10.70.0.3\/tcp\/30333",
]
dns_seeds = []
max_peers = 50
min_peers = 2
protocol_version = "catalyst/1"
mdns_discovery = false
dht_enabled = false

[network.timeouts]
connection_timeout = 10
request_timeout = 10
keep_alive_interval = 30

[rpc]
enabled = true
address = "${ip}"
port = 8545

[storage]
enabled = true
data_dir = "${dir}/data"
capacity_gb = 10
cache_size_mb = 256
write_buffer_size_mb = 64
max_open_files = 1000
compression_enabled = true
history_prune_enabled = false
history_keep_cycles = 604800
history_prune_interval_seconds = 300
history_prune_batch_cycles = 1000
EOF
}

N1_DIR="${OUT_DIR}/n1"
N2_DIR="${OUT_DIR}/n2"
N3_DIR="${OUT_DIR}/n3"
make_cfg "10.70.0.1" "${N1_DIR}"
make_cfg "10.70.0.2" "${N2_DIR}"
make_cfg "10.70.0.3" "${N3_DIR}"

start_node() {
  local ns="$1"
  local dir="$2"
  sudo ip netns exec "${ns}" bash -lc "cd \"${ROOT_DIR}\" && RUST_LOG=info \"${CLI}\" --config \"${dir}/config.toml\" start --validator" \
    >"${dir}/node.log" 2>&1 &
  echo $! > "${dir}/node.pid"
}

start_node "${NS_PREFIX}1" "${N1_DIR}"
start_node "${NS_PREFIX}2" "${N2_DIR}"
start_node "${NS_PREFIX}3" "${N3_DIR}"

sleep 2

status_json() {
  local ns="$1"
  local ip="$2"
  sudo ip netns exec "${ns}" bash -lc "\"${CLI}\" status --rpc-url \"http://${ip}:8545\" 2>/dev/null || true"
}

echo "t=0 starting poll loop" | tee -a "${OUT_DIR}/run.meta"

START_TS="$(date +%s)"
END_TS="$((START_TS + DURATION_SECS))"

partition_on() {
  # Partition: isolate n3 from n1/n2 by dropping traffic between 10.70.0.3 and 10.70.0.{1,2}
  sudo ip netns exec "${NS_PREFIX}3" iptables -I OUTPUT -d 10.70.0.1 -j DROP
  sudo ip netns exec "${NS_PREFIX}3" iptables -I OUTPUT -d 10.70.0.2 -j DROP
  sudo ip netns exec "${NS_PREFIX}3" iptables -I INPUT -s 10.70.0.1 -j DROP
  sudo ip netns exec "${NS_PREFIX}3" iptables -I INPUT -s 10.70.0.2 -j DROP
  echo "partition: on" | tee -a "${OUT_DIR}/events.log"
}

partition_off() {
  sudo ip netns exec "${NS_PREFIX}3" iptables -D OUTPUT -d 10.70.0.1 -j DROP 2>/dev/null || true
  sudo ip netns exec "${NS_PREFIX}3" iptables -D OUTPUT -d 10.70.0.2 -j DROP 2>/dev/null || true
  sudo ip netns exec "${NS_PREFIX}3" iptables -D INPUT -s 10.70.0.1 -j DROP 2>/dev/null || true
  sudo ip netns exec "${NS_PREFIX}3" iptables -D INPUT -s 10.70.0.2 -j DROP 2>/dev/null || true
  echo "partition: off" | tee -a "${OUT_DIR}/events.log"
}

restart_node() {
  local which="$1"
  local dir="${OUT_DIR}/${which}"
  if [[ -f "${dir}/node.pid" ]]; then
    kill "$(cat "${dir}/node.pid")" 2>/dev/null || true
    sleep 1
  fi
  local ns="${NS_PREFIX}${which#n}"
  start_node "${ns}" "${dir}"
  echo "restart: ${which}" | tee -a "${OUT_DIR}/events.log"
}

PARTITION_DONE=0
RESTART_DONE=0
PARTITION_OFF_AT=0

LAST_N1=""
LAST_N2=""
LAST_N3=""
GAPS=0

while [[ "$(date +%s)" -lt "${END_TS}" ]]; do
  NOW="$(date +%s)"
  ELAPSED="$((NOW - START_TS))"

  if [[ -n "${PARTITION_AT_SECS}" && "${PARTITION_DONE}" -eq 0 && "${ELAPSED}" -ge "${PARTITION_AT_SECS}" ]]; then
    partition_on
    PARTITION_DONE=1
    PARTITION_OFF_AT="$((ELAPSED + PARTITION_DURATION_SECS))"
  fi

  if [[ "${PARTITION_DONE}" -eq 1 && "${PARTITION_OFF_AT}" -gt 0 && "${ELAPSED}" -ge "${PARTITION_OFF_AT}" ]]; then
    partition_off
    PARTITION_OFF_AT=0
  fi

  if [[ -n "${RESTART_AT_SECS}" && "${RESTART_DONE}" -eq 0 && "${ELAPSED}" -ge "${RESTART_AT_SECS}" ]]; then
    restart_node "${RESTART_NODE}"
    RESTART_DONE=1
  fi

  s1="$(status_json "${NS_PREFIX}1" "10.70.0.1" | tr '\n' ' ')"
  s2="$(status_json "${NS_PREFIX}2" "10.70.0.2" | tr '\n' ' ')"
  s3="$(status_json "${NS_PREFIX}3" "10.70.0.3" | tr '\n' ' ')"

  echo "t=${ELAPSED} n1=${s1} n2=${s2} n3=${s3}" >> "${OUT_DIR}/status.log"

  # Extremely simple liveness heuristic: if status output is identical for >20s across all nodes,
  # count as a “gap” (likely stalled).
  if [[ "${s1}" == "${LAST_N1}" && "${s2}" == "${LAST_N2}" && "${s3}" == "${LAST_N3}" ]]; then
    GAPS=$((GAPS + 1))
  fi
  LAST_N1="${s1}"
  LAST_N2="${s2}"
  LAST_N3="${s3}"

  sleep 2
done

echo "gaps_observed: ${GAPS}" | tee "${OUT_DIR}/report.txt"
echo "out_dir: ${OUT_DIR}" | tee -a "${OUT_DIR}/report.txt"
echo "report_ok: true" | tee -a "${OUT_DIR}/report.txt"

