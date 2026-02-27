#!/usr/bin/env bash
set -euo pipefail

BR="catalystbr0"
NS_PREFIX="catalyst-n"

for i in 1 2 3; do
  ns="${NS_PREFIX}${i}"
  ip netns del "$ns" 2>/dev/null || true
done

ip link del "$BR" 2>/dev/null || true

echo "cleanup_ok: true"

