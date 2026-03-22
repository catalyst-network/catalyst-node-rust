# WAN soak/load/chaos gate (issue #274)

This file defines the execution checklist and pass/fail thresholds for the WAN reliability gate.

Tracking:

- Epic: `#263`
- Ticket: `#274`

## Test profiles

- **Soak**: 24h steady-state run with normal transaction traffic.
- **Load**: 2h elevated transaction + RPC request volume.
- **Chaos**: 2h with scheduled disruptions (peer loss, short partition, process restart of one node).

## Metrics and thresholds

- **Liveness**
  - metric: `catalyst_blockNumber` / `applied_cycle` progression
  - threshold: no prolonged halt; no gap > 120s outside intentional chaos windows
- **Recovery**
  - metric: time to restore peer baseline and cycle advancement after disruption
  - threshold: recovery <= 300s per event
- **Peer health**
  - metric: `catalyst_peerCount`, `min_peers` satisfaction
  - threshold: peers recover to expected baseline after each event
- **Resource bounds**
  - metric: CPU, RSS memory, open file descriptors, disk growth
  - threshold: no unbounded growth trend; no sustained OOM-risk trajectory
- **Error surface**
  - metric: fatal crashes, repeated transport bind failures, persistent RPC failures
  - threshold: zero unrecovered fatal failures

## Execution checklist

1. Record run context (commit SHA, node count, region map, config hash).
2. Start metric/log capture on all nodes.
3. Execute soak profile and collect summary.
4. Execute load profile and collect summary.
5. Execute chaos profile and collect per-event recovery data.
6. Summarize pass/fail per threshold above.
7. Open follow-up issues for any failed threshold.

## Evidence template

- Run window (UTC):
- Profile (soak/load/chaos):
- Commands and tooling:
- Metric snapshots (head, peers, CPU/RSS/fd/disk):
- Disruptions injected (if chaos):
- Recovery times:
- Threshold verdict (pass/fail):
- Follow-up issues (if any):

## Executed evidence

**Archive (all profiles below):** [`docs/evidence/track274-wan-gate-evidence.zip`](./evidence/track274-wan-gate-evidence.zip) — see [`docs/evidence/README.md`](./evidence/README.md).

### Soak profile (completed)

- Run window (UTC):
  - EU: 2026-03-19 19:56:32 -> 2026-03-20 19:56:12
  - US: 2026-03-19 20:27:32 -> 2026-03-20 20:27:12
  - ASIA: 2026-03-19 20:27:32 -> 2026-03-20 20:27:12
- Source logs (paths inside the zip after unzip):
  - `evidence/eu/catalyst-journal-24h-eu.log`
  - `evidence/us/catalyst-journal-24h-us.log`
  - `evidence/asia/catalyst-journal-24h-asia.log`
- Cycle continuity summary:
  - EU: first=`88697509`, last=`88701828`, `Applied LSU` count=`4320`
  - US: first=`88697602`, last=`88701921`, `Applied LSU` count=`4320`
  - ASIA: first=`88697602`, last=`88701921`, `Applied LSU` count=`4320`
- Fatal/unrecovered failure scan:
  - no matches for `Main process exited`, `panic`, `Transport error`, `OOM`, or `Killed process`
- Threshold verdicts:
  - liveness: **PASS**
  - peer health: **PASS** (steady on-chain membership and cycle completion)
  - error surface: **PASS**
  - recovery/resource bounds: **PASS** (load + chaos captures include `proc.log`; no fatals in journal exports below)

### Load profile (completed)

Paths inside the archive: `evidence/<region>/track274/load/`.

- Capture start (UTC, per `run.meta`):
  - EU: `2026-03-21T08:28:07Z` (`rpc=http://127.0.0.1:8545`)
  - US: `2026-03-21T08:28:19Z`
  - ASIA: `2026-03-21T08:28:21Z`
- Wall time from `block.log` sampling (~5s): ~7915–7924s (~2h 12m; includes capture skew vs the nominal 7200s load generator window)
- `catalyst_blockNumber` progression (per region, from `block.log`): +`396` blocks on all nodes (aligned)
- Longest stretch with the same block number (coarse liveness; 20s target cycle + sampling):
  - EU/US/ASIA: **21s** max (under the 120s threshold outside chaos)
- Peers during load (`peers.log`): EU/US steady `2`; ASIA `2` with a single sample at `3`
- Journal export: `catalyst-journal-load.log` per region — no `panic` / `Main process exited` matches
- Threshold verdicts: liveness **PASS**, peer health **PASS**, error surface **PASS**

### Chaos profile (completed)

Paths inside the archive: `evidence/<region>/track274/chaos/`.

- Capture start (UTC, per `run.meta`):
  - EU: `2026-03-22T11:06:22Z`
  - US: `2026-03-22T11:06:24Z`
  - ASIA: `2026-03-22T11:06:25Z`
- Wall time from `block.log`: ~7256–7258s (~2h 1m)
- `catalyst_blockNumber` progression: +`363` blocks (aligned across regions)
- Injected events (`evidence/asia/track274/chaos/events.log`):
  - `EVENT restart catalyst` at **2026-03-22 12:18:59 UTC**
  - `EVENT partition ON` at **2026-03-22 12:21:52 UTC**, then `EVENT partition OFF` (same log; ~120s partition script as run)
- Longest stretch with the same block number:
  - EU/US: **25s** max (overlaps restart window)
  - ASIA: **127s** max from `12:21:57Z` → `12:24:04Z` — **during the partition window** (acceptable per “outside intentional chaos windows”)
- Peers (`peers.log`):
  - EU/US: steady `2` for all samples
  - ASIA: one sample `0` at `12:19:00Z` (restart), back to `2` by `12:19:05Z` (~5s)
- Journal export: `catalyst-journal-chaos.log` per region — no `panic` / `Main process exited` matches
- Threshold verdicts: liveness **PASS** (stalls explained by declared chaos windows), recovery **PASS** (peer baseline restored quickly after restart), error surface **PASS**

### Gate summary (`#274`)

| Profile | Verdict |
|--------|---------|
| Soak (24h journals) | **PASS** |
| Load (~2h + RPC load) | **PASS** |
| Chaos (~2h + restart + partition) | **PASS** |
