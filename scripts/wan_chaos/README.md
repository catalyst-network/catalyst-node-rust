## WAN soak / chaos harness (Linux netns + tc/netem)

This harness runs a 3-node Catalyst network inside Linux network namespaces and applies
WAN-like conditions (latency/loss/jitter) and chaos events (partitions, restarts).

It is designed to be run on a dedicated Linux host (VM ok) with root privileges.

### Prerequisites

- Linux with `ip` (iproute2), `tc`, and `iptables` available
- Root privileges (namespace setup + tc + iptables)
- Rust toolchain (or prebuilt `catalyst-cli`)

### What it does

- Creates a bridge `catalystbr0` and 3 namespaces `catalyst-n1..n3`
- Assigns IPs: `10.70.0.1..3`
- Starts 3 nodes with P2P on `30333` and RPC on `8545` inside each namespace
- Periodically polls `catalyst-cli status` to check `applied_cycle` monotonicity
- Applies tc netem profiles and optional partitions
- Writes logs + a simple summary report under `./wan_chaos/out/<run_id>/`

### Quick start

From repo root:

```bash
sudo bash scripts/wan_chaos/run.sh
```

### Useful env vars

- `DURATION_SECS` (default `300`)
- `LOSS_PCT` (default `0`)
- `LATENCY_MS` (default `50`)
- `JITTER_MS` (default `10`)
- `PARTITION_AT_SECS` (default unset)
- `PARTITION_DURATION_SECS` (default `30`)
- `RESTART_AT_SECS` (default unset)
- `RESTART_NODE` (`n1|n2|n3`, default `n1`)

### Cleanup

The runner attempts to cleanup automatically. If it fails:

```bash
sudo bash scripts/wan_chaos/cleanup.sh
```

