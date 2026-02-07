#!/usr/bin/env bash
set -euo pipefail

REPO="${REPO:-catalyst-network/catalyst-node-rust}"

create() {
  local title="$1"
  gh api -X POST "repos/$REPO/milestones" -f "title=$title" >/dev/null
  echo "created milestone: $title"
}

create "Milestone A — Spec pinning + canonical boundaries"
create "Milestone B — Transaction pipeline"
create "Milestone C — Membership / worker pool"
create "Milestone D — 4-phase consensus (protocol-faithful)"
create "Milestone E — State application + authenticated queries"
create "Milestone F — Public testnet ops + safety defaults"
create "Milestone G — Testing + release gate"

