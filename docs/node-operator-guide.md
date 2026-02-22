# Node operator guide (local + public testnet)

This guide covers running the current Rust node implementation (`catalyst-cli`).

## Quick glossary

- **P2P**: peer-to-peer networking for consensus + gossip (`30333/tcp`)
- **RPC**: HTTP JSON-RPC for clients (default `8545/tcp`)
- **Validator**: runs the consensus loop (`catalyst-cli start --validator`)

## Chain identity (required)

Each network has a stable identity:
- `protocol.chain_id` (u64): signing/execution domain separation (wallet v1 + EVM `chainId`)
- `protocol.network_id` (string): human-readable name

See [`network-identity.md`](./network-identity.md) for the numbering scheme and rules.

In each node’s `config.toml`, ensure:

```toml
[protocol]
chain_id = 200820092
network_id = "catalyst-testnet"
```

Important:
- **Do not change** these values for a running network. Changing them creates a **new chain**.

## Faucet genesis funding (dev vs public testnet)

This node can optionally seed a faucet/treasury account at genesis (fresh DB only). For **local/dev**
networks you can use the deterministic faucet key, but for any **public** testnet you must configure
a real pubkey and disable the deterministic faucet.

### Local/dev (default)

```toml
[protocol]
chain_id = 31337
network_id = "local"
faucet_mode = "deterministic"
allow_deterministic_faucet = true
faucet_balance = 1000000
```

### Public testnet (recommended)

Pick a private treasury key (never publish it), derive its 32-byte pubkey hex, then configure:

```toml
[protocol]
chain_id = 200820092
network_id = "catalyst-testnet"
faucet_mode = "configured"
allow_deterministic_faucet = false
faucet_pubkey_hex = "0x<32-byte-pubkey-hex>"
faucet_balance = 1000000000000
```

Notes:
- `faucet_balance` is an `i64`; choose a very large value for long-lived testnets.
- Your public faucet web app/service should spend from a **hot wallet**, topped up from this
  treasury pubkey as needed.

## Build

From the repo root:

```bash
make build
```

The binary you run is:

```bash
./target/release/catalyst-cli
```

## Local single node

```bash
./target/release/catalyst-cli start --rpc
```

Notes:
- If you pass `--config /some/path/config.toml` and it doesn’t exist, the node will **generate** it and create sibling dirs (data/dfs_cache/logs/node.key) near that config path.

## Public 3-node testnet (Vultr-style)

Topology:
- **EU**: boot + RPC + validator
- **US**: validator
- **Asia**: validator

### Example deployment values (reference)

These are example values from a working 3-node deployment (use your own IPs/keys).

| Instance | Location | Public IP | Worker id (pubkey hex) |
|---|---|---:|---|
| `catalyst-1` | London (EU) | `45.32.177.248` | `cc88b786e388e7bfc8406f569458beb6c9238f8800c0b2511c4366d40808b919` |
| `catalyst-2` | Chicago (US) | `45.76.21.153` | `26f4404048d8bef3a36cd38775f8139e031689cf1041d793687c3d906da98a76` |
| `catalyst-3` | Singapore (Asia) | `207.148.126.35` | `20facef46a9495c43d342eeb4a80ccc7a1c02c228277d3f277cee0cfabfd544f` |

### 1) Network ports / firewall

On **all 3 servers**:
- Allow inbound **`30333/tcp`** (P2P)

On **EU only**:
- Allow inbound **`8545/tcp`** (RPC). Recommended: restrict to your IP, or use SSH tunneling.

If using `ufw`:

```bash
# all servers
sudo ufw allow 30333/tcp

# EU only
sudo ufw allow 8545/tcp

sudo ufw reload
sudo ufw status numbered
```

Verify the process is listening:

```bash
sudo ss -ltnp | egrep ':30333|:8545' || true
```

Verify connectivity (run on each server, to the other two):

```bash
nc -vz <OTHER_PUBLIC_IP> 30333
```

### 2) Configure bootstrap peers

This codebase uses plain multiaddrs without `/p2p/<peerid>`:

```toml
[network]
bootstrap_peers = [
  "/ip4/<EU_PUBLIC_IP>/tcp/30333",
  "/ip4/<US_PUBLIC_IP>/tcp/30333",
  "/ip4/<ASIA_PUBLIC_IP>/tcp/30333",
]
listen_addresses = ["/ip4/0.0.0.0/tcp/30333"]
```

### 3) Bootstrap the validator set (critical)

For a brand-new network, the on-chain worker set starts empty. This implementation uses
`[consensus].validator_worker_ids` as the initial “membership bootstrap” for producer selection.

On each server, ensure a stable node key exists (starting once will auto-create it next to the config):

```bash
./target/release/catalyst-cli --config /var/lib/catalyst/<region>/config.toml start
# Ctrl+C once it’s started; this creates /var/lib/catalyst/<region>/node.key
```

Derive each node’s worker id (public key hex):

```bash
./target/release/catalyst-cli pubkey --key-file /var/lib/catalyst/eu/node.key
./target/release/catalyst-cli pubkey --key-file /var/lib/catalyst/us/node.key
./target/release/catalyst-cli pubkey --key-file /var/lib/catalyst/asia/node.key
```

Then set, on **all three** configs:

```toml
[consensus]
min_producer_count = 2
validator_worker_ids = [
  "<EU_PUBKEY_HEX>",
  "<US_PUBKEY_HEX>",
  "<ASIA_PUBKEY_HEX>",
]
```

Why `min_producer_count = 2`?
- With 3 validators, liveness is **2-of-3**. One validator can be down without halting progress.

### 4) Start nodes (recommended order)

Start EU first:

```bash
./target/release/catalyst-cli --config /var/lib/catalyst/eu/config.toml start \
  --validator --rpc --rpc-address 0.0.0.0 --rpc-port 8545
```

Start US and Asia:

```bash
./target/release/catalyst-cli --config /var/lib/catalyst/us/config.toml start --validator
./target/release/catalyst-cli --config /var/lib/catalyst/asia/config.toml start --validator
```

Note: CLI flags override config values at runtime. For example, `--validator` makes the node a validator even if `validator = false` is present in the file.

### 5) Health check

From any machine that can reach EU RPC:

```bash
./target/release/catalyst-cli status --rpc-url http://<EU_PUBLIC_IP>:8545
sleep 30
./target/release/catalyst-cli status --rpc-url http://<EU_PUBLIC_IP>:8545
```

`applied_cycle` should advance.

## Resetting / launching a new network (hard reset)

If you change `protocol.chain_id` / `protocol.network_id` (or intentionally restart from fresh genesis), you must treat it as a **new chain**:

- **Keep**: `/var/lib/catalyst/<region>/node.key` (stable P2P identity)
- **Wipe**: `/var/lib/catalyst/<region>/data` (chain DB/state)

Recommended safe “wipe” (rename) per node:

```bash
sudo systemctl stop catalyst || true
ts=$(date +%s)
mv /var/lib/catalyst/<region>/data "/var/lib/catalyst/<region>/data.old.$ts"
```

Then start EU first, then the other validators.

## Troubleshooting

### “Insufficient data collected: got 1, required 2”

This means validators are not receiving enough peer messages in a cycle. Common causes:
- `30333/tcp` blocked (cloud firewall or `ufw`)
- node not listening on `0.0.0.0:30333`
- not actually connected to other validators (bootstrap peers missing/wrong)
- large clock drift (ensure NTP/chrony)

### RPC is reachable but tx inclusion is slow

This is expected in the current implementation: RPC acceptance returns a tx id immediately; inclusion can take a few cycles depending on propagation and which validator is the tx batch leader.

