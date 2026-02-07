#!/usr/bin/env bash
set -euo pipefail

# Create an issue using a body read from stdin (non-interactive).
#
# Usage:
#   cat body.md | scripts/gh/create_issue_from_stdin.sh \
#     --title "[Spec] ..." \
#     --label spec \
#     --milestone 1
#
# Env:
#   REPO (default catalyst-network/catalyst-node-rust)

REPO="${REPO:-catalyst-network/catalyst-node-rust}"

TITLE=""
LABELS=""
MILESTONE=""

while [ $# -gt 0 ]; do
  case "$1" in
    --title) TITLE="$2"; shift 2 ;;
    --label) LABELS="$LABELS $2"; shift 2 ;;
    --milestone) MILESTONE="$2"; shift 2 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

[ -n "$TITLE" ] || { echo "missing --title" >&2; exit 2; }

tmp="$(mktemp)"
cat > "$tmp"

args=(--repo "$REPO" --title "$TITLE" --body-file "$tmp")

if [ -n "$MILESTONE" ]; then
  args+=(--milestone "$MILESTONE")
fi

if [ -n "$LABELS" ]; then
  # shellcheck disable=SC2086
  for l in $LABELS; do
    args+=(--label "$l")
  done
fi

gh issue create "${args[@]}"
