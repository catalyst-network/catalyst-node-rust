# Archived test evidence

## `track274-wan-gate-evidence.zip`

WAN soak / load / chaos gate for GitHub issue **#274** (epic **#263**).

- **Size:** ~4 MiB compressed (~53 MiB uncompressed).
- **Layout after unzip:** top-level `evidence/` with:
  - `evidence/{eu,us,asia}/catalyst-journal-24h-*.log` — 24h soak journals
  - `evidence/{eu,us,asia}/track274/load/` — load run (`block.log`, `peers.log`, `proc.log`, `head.log`, `catalyst-journal-load.log`, …)
  - `evidence/{eu,us,asia}/track274/chaos/` — chaos run (same capture files + `events.log` on ASIA)

**Summary and pass/fail:** see [`../wan-soak-load-chaos-gate.md`](../wan-soak-load-chaos-gate.md).
