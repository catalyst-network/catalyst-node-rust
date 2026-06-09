#!/usr/bin/env bash
# Coordinated, whole-fleet GENESIS reset for the Catalyst testnet.
#
# Why this exists
# ---------------
# Cycle numbers are wall-clock-derived and the chain links by `prev_root`. If you
# reset ONE node while the rest keep running, the reset node boots at genesis
# (cycle 0) while the network is already at a huge wall-clock cycle, and it can
# NEVER rejoin (cold-start backfill of the genesis->first-cycle gap is not
# supported). A genesis reset must therefore be coordinated across the WHOLE
# fleet, and every follower/RPC MUST be online before the first produced cycle.
#
# This script enforces that ordering:
#   1. Stop catalyst on ALL nodes.
#   2. Wipe each node's data dir (rename by default; --purge to rm) and dfs_cache.
#      `node.key` and `config.toml` (outside the data dir) are always preserved.
#   3. Start the anchor node first, wait $ANCHOR_WAIT, then start ALL remaining
#      nodes together (validators AND rpc) so followers see the first cycle live.
#   4. Verify the fleet shares one genesis hash and advances in lockstep
#      (identical applied_state_root at identical cycles).
#
# For a SOFTWARE-ONLY update that does NOT change the on-disk format, do NOT use
# this script — do a rolling binary swap (stop -> replace binary -> start) one
# node at a time instead, which keeps the chain and needs no resync.
#
# Usage
# -----
#   scripts/catalyst_fleet_reset.sh [--yes] [--purge] [--keep-dfs-cache]
#                                   [--anchor <name>] [--wait <seconds>]
#                                   [--fleet <file>]
#
# Env / flags:
#   FLEET_FILE       path to a fleet definition file (overrides the built-in default)
#   ANCHOR           node name to start first (default: eu)
#   ANCHOR_WAIT      seconds to wait after the anchor before the cohort (default: 60)
#   SYSTEMD_UNIT     systemd unit name (default: catalyst.service)
#   RPC_PORT         local RPC port to query on each host (default: 8545)
#   SSH_USER_DEFAULT user to use when a node spec omits user@ (default: root)
#   --yes            skip the destructive-confirmation prompt
#   --purge          `rm -rf` the data dir instead of renaming it (irreversible)
#   --keep-dfs-cache do not delete dfs_cache
#
# Fleet definition format (one node per line, '#' comments allowed):
#   name | ssh_target | base_dir | role(validator|rpc) | wave(anchor|cohort)
# ssh_target may be "user@host" or just "host" (then SSH_USER_DEFAULT is used).
# base_dir is the parent of `data` / `dfs_cache` and the location of node.key.

set -euo pipefail

SYSTEMD_UNIT="${SYSTEMD_UNIT:-catalyst.service}"
RPC_PORT="${RPC_PORT:-8545}"
ANCHOR="${ANCHOR:-eu}"
ANCHOR_WAIT="${ANCHOR_WAIT:-60}"
SSH_USER_DEFAULT="${SSH_USER_DEFAULT:-root}"
SSH_OPTS=(-o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new)

ASSUME_YES=0
PURGE=0
KEEP_DFS=0
FLEET_FILE="${FLEET_FILE:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --yes) ASSUME_YES=1; shift ;;
    --purge) PURGE=1; shift ;;
    --keep-dfs-cache) KEEP_DFS=1; shift ;;
    --anchor) ANCHOR="$2"; shift 2 ;;
    --wait) ANCHOR_WAIT="$2"; shift 2 ;;
    --fleet) FLEET_FILE="$2"; shift 2 ;;
    -h|--help) sed -n '2,60p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

# ---- Default fleet (Catalyst testnet). Override with --fleet / FLEET_FILE. ----
DEFAULT_FLEET=$(cat <<'FLEET'
# name | ssh_target            | base_dir                | role      | wave
eu     | root@78.141.239.157   | /var/lib/catalyst/eu    | validator | anchor
us     | root@45.76.21.153     | /var/lib/catalyst/us    | validator | cohort
asia   | root@207.148.126.35   | /var/lib/catalyst/asia  | validator | cohort
rpc    | root@45.32.177.248    | /var/lib/catalyst/eu    | rpc       | cohort
FLEET
)

if [[ -n "${FLEET_FILE}" ]]; then
  [[ -f "${FLEET_FILE}" ]] || { echo "fleet file not found: ${FLEET_FILE}" >&2; exit 2; }
  FLEET_RAW="$(cat "${FLEET_FILE}")"
else
  FLEET_RAW="${DEFAULT_FLEET}"
fi

# Parse fleet into parallel arrays.
NAMES=(); TARGETS=(); BASES=(); ROLES=(); WAVES=()
while IFS= read -r line; do
  line="${line%%#*}"                      # strip comments
  line="$(echo "$line" | tr -d '[:space:]' )"
  [[ -z "$line" ]] && continue
  IFS='|' read -r n t b r w <<<"$line"
  [[ -z "$n" || -z "$t" || -z "$b" ]] && { echo "bad fleet line: $line" >&2; exit 2; }
  [[ "$t" != *"@"* ]] && t="${SSH_USER_DEFAULT}@${t}"
  NAMES+=("$n"); TARGETS+=("$t"); BASES+=("$b"); ROLES+=("${r:-validator}"); WAVES+=("${w:-cohort}")
done <<<"${FLEET_RAW}"

N=${#NAMES[@]}
[[ "$N" -ge 1 ]] || { echo "empty fleet" >&2; exit 2; }

idx_of() { local want="$1" i; for i in "${!NAMES[@]}"; do [[ "${NAMES[$i]}" == "$want" ]] && { echo "$i"; return 0; }; done; return 1; }
ssh_node() { local i="$1"; shift; ssh "${SSH_OPTS[@]}" "${TARGETS[$i]}" "$@"; }

rpc_call() {  # idx method -> raw json
  local i="$1" method="$2"
  ssh_node "$i" "curl -s -X POST -H 'Content-Type: application/json' \
    --data '{\"jsonrpc\":\"2.0\",\"method\":\"${method}\",\"params\":[],\"id\":1}' \
    http://127.0.0.1:${RPC_PORT}" 2>/dev/null
}

jget() { python3 -c "import sys,json
try:
  d=json.load(sys.stdin); r=d.get('result')
  print(r.get('$2') if isinstance(r,dict) else r)
except Exception:
  print('')" <<<"$1"; }

hr() { printf '%s\n' "------------------------------------------------------------"; }

echo "Fleet (${N} nodes), unit=${SYSTEMD_UNIT}, anchor=${ANCHOR}, wait=${ANCHOR_WAIT}s, purge=${PURGE}, keep_dfs=${KEEP_DFS}"
hr
printf "%-6s %-26s %-26s %-10s %s\n" NAME SSH BASE_DIR ROLE WAVE
for i in "${!NAMES[@]}"; do
  printf "%-6s %-26s %-26s %-10s %s\n" "${NAMES[$i]}" "${TARGETS[$i]}" "${BASES[$i]}" "${ROLES[$i]}" "${WAVES[$i]}"
done
hr

# ---- 0) Preflight: reachability + identity/config present ----
echo "==> Preflight"
PRE_FAIL=0
for i in "${!NAMES[@]}"; do
  out="$(ssh_node "$i" "b='${BASES[$i]}'; \
    echo nodekey=\$( [ -f \"\$b/node.key\" ] && echo yes || echo NO ) \
    config=\$( [ -f \"\$b/config.toml\" ] && echo yes || echo NO ) \
    unit=\$(systemctl is-enabled ${SYSTEMD_UNIT} 2>/dev/null || echo unknown)" 2>&1)" || { echo "  ${NAMES[$i]}: UNREACHABLE"; PRE_FAIL=1; continue; }
  echo "  ${NAMES[$i]}: ${out}"
  [[ "$out" == *"node.key"* && "$out" == *"nodekey=NO"* ]] && PRE_FAIL=1
  [[ "$out" == *"config=NO"* ]] && { echo "    WARNING: config.toml missing under ${BASES[$i]}"; }
  [[ "$out" == *"nodekey=NO"* ]] && echo "    ERROR: node.key missing — wiping would lose validator identity"
done
[[ "$PRE_FAIL" -eq 0 ]] || { echo "Preflight failed; aborting."; exit 1; }
idx_of "$ANCHOR" >/dev/null || { echo "anchor '${ANCHOR}' not in fleet"; exit 2; }

# ---- Confirm (destructive) ----
if [[ "$ASSUME_YES" -ne 1 ]]; then
  hr
  if [[ "$PURGE" -eq 1 ]]; then
    echo "This will rm -rf each node's data dir (IRREVERSIBLE) and reset from genesis."
  else
    echo "This will rename each node's data dir to data.reset.<ts> and reset from genesis."
  fi
  read -r -p "Type 'RESET' to proceed: " ans
  [[ "$ans" == "RESET" ]] || { echo "aborted."; exit 1; }
fi

TS="$(date +%s)"

# ---- 1) Stop all ----
echo "==> Stopping ${SYSTEMD_UNIT} on all nodes"
for i in "${!NAMES[@]}"; do
  ssh_node "$i" "systemctl stop ${SYSTEMD_UNIT}" || true
  printf "  %-6s stopped (is-active=%s)\n" "${NAMES[$i]}" "$(ssh_node "$i" "systemctl is-active ${SYSTEMD_UNIT}" 2>/dev/null || echo '?')"
done

# ---- 2) Wipe all ----
echo "==> Wiping data dirs (preserving node.key + config.toml)"
for i in "${!NAMES[@]}"; do
  b="${BASES[$i]}"
  if [[ "$PURGE" -eq 1 ]]; then
    wipe_cmd="rm -rf '$b/data' && echo 'data removed'"
  else
    wipe_cmd="[ -d '$b/data' ] && mv '$b/data' '$b/data.reset.${TS}' && echo 'data -> data.reset.${TS}' || echo 'no live data dir'"
  fi
  dfs_cmd="true"
  [[ "$KEEP_DFS" -ne 1 ]] && dfs_cmd="rm -rf '$b/dfs_cache' && echo 'dfs_cache removed' || true"
  printf "  %-6s " "${NAMES[$i]}"
  ssh_node "$i" "set -e; ${wipe_cmd}; ${dfs_cmd}" 2>&1 | paste -sd'; '
done

# ---- 3) Staged start: anchor first, then the rest together ----
ai="$(idx_of "$ANCHOR")"
echo "==> Starting anchor '${ANCHOR}' first"
ssh_node "$ai" "systemctl start ${SYSTEMD_UNIT}"
printf "  %-6s is-active=%s\n" "${ANCHOR}" "$(ssh_node "$ai" "systemctl is-active ${SYSTEMD_UNIT}" 2>/dev/null || echo '?')"
echo "  waiting ${ANCHOR_WAIT}s for anchor to come up..."
sleep "${ANCHOR_WAIT}"

echo "==> Starting remaining nodes together (validators + rpc)"
pids=()
for i in "${!NAMES[@]}"; do
  [[ "$i" -eq "$ai" ]] && continue
  ( ssh_node "$i" "systemctl start ${SYSTEMD_UNIT}" >/dev/null 2>&1; \
    printf "  %-6s is-active=%s\n" "${NAMES[$i]}" "$(ssh_node "$i" "systemctl is-active ${SYSTEMD_UNIT}" 2>/dev/null || echo '?')" ) &
  pids+=($!)
done
wait "${pids[@]}" 2>/dev/null || true

# ---- 4) Verify: genesis hash + lockstep ----
echo "==> Verifying (allow ~$((ANCHOR_WAIT))s for first quorum cycles)"
sleep 45

echo "  genesis hash:"
genset=""
for i in "${!NAMES[@]}"; do
  g="$(jget "$(rpc_call "$i" catalyst_genesisHash)" "")"
  printf "    %-6s %s\n" "${NAMES[$i]}" "${g:-<no response>}"
  genset+="${g}\n"
done
uniqgen="$(printf "%b" "$genset" | sed '/^$/d' | sort -u | wc -l)"
[[ "$uniqgen" -le 1 ]] && echo "  genesis: OK (single hash)" || echo "  genesis: MISMATCH ($uniqgen distinct) !!"

FORK=0
for s in 1 2 3; do
  echo "  lockstep sample $s ($(date -u +%H:%M:%S)):"
  declare -A ROOT_AT=()
  for i in "${!NAMES[@]}"; do
    r="$(rpc_call "$i" catalyst_head)"
    c="$(jget "$r" applied_cycle)"; sr="$(jget "$r" applied_state_root)"
    printf "    %-6s cycle=%-12s root=%s\n" "${NAMES[$i]}" "${c:-?}" "${sr:0:18}"
    if [[ -n "$c" && "$c" != "0" && -n "$sr" ]]; then
      if [[ -n "${ROOT_AT[$c]:-}" && "${ROOT_AT[$c]}" != "$sr" ]]; then
        echo "    !! FORK: differing root at cycle $c"; FORK=1
      fi
      ROOT_AT[$c]="$sr"
    fi
  done
  [[ "$s" -lt 3 ]] && sleep 22
done

hr
if [[ "$FORK" -eq 0 && "$uniqgen" -le 1 ]]; then
  echo "RESULT: OK — single genesis, no fork observed. Fleet reset complete."
else
  echo "RESULT: ATTENTION — genesis mismatch or fork detected above. Investigate before use."
  exit 1
fi
