# Smoke tests

## `smoke_testnet.sh`

Runs a short end-to-end test against a local 3-node testnet:
- starts node1/node2/node3
- waits for node1 RPC
- sends a faucet-funded tx to node1
- restarts node1
- verifies the tx is still included (mempool persistence + rehydrate + rebroadcast)
- stops the testnet

Run via make:

```bash
make smoke-testnet
```

Or directly:

```bash
bash scripts/smoke_testnet.sh
```

Environment variables:
- `AMOUNT` (default `3`)
- `TIMEOUT_SECS` (default `90`)
- `RPC_URL` (default `http://127.0.0.1:8545`)
- `RPC_HOSTPORT` (default `127.0.0.1:8545`)

## `netctl.sh`

A small network control script used by `make` targets.

Local testnet workflow:

```bash
make testnet-up
make testnet-status
make testnet-basic-test
make testnet-down
```

Tail logs:

```bash
make testnet-logs NODE=node1
```

Devnet workflow (single node, externally reachable RPC):

```bash
make devnet-up HOST=<public_ip_or_dns> P2P_PORT=30333 RPC_PORT=8545
make devnet-status
make devnet-down
```

## Fleet health, snapshots, and auto-heal (testnet ops)

See **[`docs/automated-ops.md`](../docs/automated-ops.md)** for the full runbook.

| Script | Purpose |
|--------|---------|
| `catalyst_network_check.py` | Compare `catalyst_head` across nodes; detect forks |
| `catalyst_auto_snapshot.sh` | Canonical validator: snapshot → R2 `latest.tar` → publish |
| `catalyst_heal_local.sh` | Outlier host: `sync-from-archive` + restart |
| `catalyst_rpc_sync.sh` | RPC: restore when lagging validators by N cycles |

CLI: `catalyst-cli sync-from-archive --archive-url ... --data-dir ...`

## `txgen_faucet.sh`

Generates a small, random number of faucet-funded transfers per block so
blocks don’t look empty (useful for public testnets + explorers).

Run:

```bash
RPC_URL="http://<your-rpc-host>:8545" bash scripts/txgen_faucet.sh
```

Safety knobs (environment variables):
- `MAX_TX_PER_BLOCK` (default `2`): each new block sends 0..MAX tiny transfers
- `AMOUNT_MIN` / `AMOUNT_MAX` (defaults `1`..`3`)
- `RECIP_COUNT` (default `25`): size of recipient pool
- `STOP_AFTER_BLOCKS` (unset => run forever)

State:
- stores `faucet.key` + recipient pool under `./txgen/` by default (override with `STATE_DIR`)
