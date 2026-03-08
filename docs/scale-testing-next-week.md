## Scale/load testing prep (next week)

This is a working runbook/checklist for setting up Catalyst scale tests on new hardware.

### Goals

- Validate **liveness** under WAN conditions (latency/loss/jitter/partitions) as node count grows.
- Validate **join reliability** (bootstrap, reconnect storms, churn).
- Validate **tx throughput + receipt latency** under load.
- Keep the test plan aligned with the design intent:
  - **Large worker pool** (many nodes exist and can verify/serve).
  - **Smaller producer set per cycle** (only the selected set participates in consensus for that cycle).

### Key parameters to treat as first-class knobs

- **N (network nodes)**: total nodes participating in the P2P network (non-producers + producers).
- **P (producers per cycle)**: producers selected for consensus in a cycle. This is the primary consensus scaling dimension.
- **Cycle duration** and **phase timeouts**: determine how sensitive the system is to WAN tail latency.
- **RPC limits + P2P safety limits**: impact how the system behaves under spam/abuse.
- **Tx load**: tx/s submitted + payload sizes.

### What is practical on a single machine

- **“Real nodes”** (full process, networking, storage, execution): target **10–200ish** depending on hardware.
- **“Thousands of nodes”**: requires simulation/hybrid (synthetic peers) rather than full nodes.

---

## Track A: Real-node WAN tests (10–200 nodes)

### Harness baseline

- Start from `scripts/wan_chaos/run.sh` (network namespaces + `tc netem` + optional partitions/restarts).
- Extend it to parameterize node count:
  - `NODES=3` → `NODES=10/25/50/...`
  - auto-generate per-node config + ports
  - auto-generate bootstrap peer lists

### Metrics to record (minimum viable)

- **Applied cycle progression**:
  - cycles/minute
  - longest stall
  - time-to-resume after partition
  - time-to-resume after restart
- **Connectivity**:
  - peer count stability
  - reconnect storms (spike in dial attempts)
- **RPC**:
  - rate-limit counts
  - tx receipt latency (p50/p95/p99)

### WAN profiles to test (starter set)

- **Baseline**: 30–80ms latency, 0.1% loss, small jitter
- **Bad WiFi**: 80–200ms, 1–3% loss, jitter 20–80ms
- **Partial partition**: isolate a subset for 30–120s then heal
- **Churn**: periodically restart 10–30% of nodes

### Test matrix (starter)

Run each profile for each cell:

- N = 10, 25, 50 (real nodes)
- P = 3, 5, 11, 31 (selected producers per cycle; if configurable)
- Phase timeouts = conservative vs aggressive

---

## Track B: Large-scale simulation (1k–10k nodes)

The goal is to test scaling properties without running full nodes.

### What to simulate (first)

- **Message propagation model** for the P2P layer:
  - fanout parameters
  - duplicate suppression
  - per-peer budgets / rate limits
  - topology (random, small-world, hub-and-spoke)
- **Consensus phase timing model**:
  - producers broadcast phase messages
  - each producer “collects” within timeout windows
  - delays drawn from latency distributions
  - loss/drop probabilities

### What not to simulate initially

- Full execution + RocksDB per node
- Full correctness proofs of ledger state

### Outputs

- Probability of successful phase completion for given P, latency tail, and loss
- Sensitivity curves for phase timeouts
- Predicted bandwidth + message rates vs P and fanout

---

## Hardware/OS prep checklist (new machine)

### Host OS

- Ubuntu/Debian preferred.
- Ensure enough disk (fast SSD) if running many real nodes.

### Kernel/user limits (for many processes/connections)

- `ulimit -n` high (e.g. 1,048,576) for the test user.
- `sysctl` tune as needed:
  - `net.core.somaxconn`
  - `net.ipv4.ip_local_port_range`
  - `net.ipv4.tcp_tw_reuse`
  - `net.core.rmem_max` / `wmem_max`

### Tooling prereqs

- `iproute2`, `iptables`, `tc`, `jq`, `curl`
- Rust toolchain only if building from source

---

## “Day 1 next week” plan (do this first)

1) Run `scripts/wan_chaos/run.sh` on the new machine (baseline 3 nodes) to ensure netns + tc works.
2) Extend the harness to `NODES=10` with stable reporting.
3) Add a tx-load generator (client-side) to submit txs at controlled QPS and record receipt latencies.
4) Run a small matrix:
   - N=10 with 2 WAN profiles
   - tx load ramp (1/s → 10/s → 50/s) and record p95 receipt time

---

## Notes on “large worker pool, smaller producer set”

If Catalyst selects **P producers per cycle** from a potentially unbounded pool of workers:

- Stress-testing consensus does **not** require P=1000+ unless you intend to select that many simultaneously.
- It does require testing:
  - large N for join/connectivity/churn behavior
  - a range of P for phase-timeout sensitivity and message fanout growth

