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

### Local/dev (explicit deterministic faucet example)

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

## Storage growth / history pruning (recommended for long-running networks)

By default, nodes retain historical block + transaction metadata indefinitely (useful for explorers),
which means disk usage grows without bound over time.

For “regular validator nodes” on long-running public networks, it’s recommended to run in **pruned**
mode and keep only a sliding window of recent history. This does **not** affect current balances or
EVM state (authenticated state lives in the RocksDB `accounts` column family); it only prunes old
RPC/indexer metadata such as:
- `consensus:lsu:*` per-cycle history
- `tx:*` per-cycle tx indices and raw/meta blobs

Example config (keep last 7 days at 1 cycle/sec):

```toml
[storage]
history_prune_enabled = true
history_keep_cycles = 604800                # 7 * 24 * 60 * 60
history_prune_interval_seconds = 300        # run at most every 5 minutes
history_prune_batch_cycles = 1000           # prune up to 1000 cycles per run
```

Notes:
- If you run a public explorer/indexer, keep at least one **archival** node (set `history_prune_enabled = false`).
- In pruned mode, very old `catalyst_getBlockByNumber` / `catalyst_getTransactionByHash` calls may return `null`.

### Useful maintenance commands

Inspect local DB growth hot-spots (key counts per column family + RocksDB stats):

```bash
./target/release/catalyst-cli db-stats --data-dir /var/lib/catalyst/<region>/data
```

Force a maintenance pass (flush + manual compaction + snapshot cleanup):

```bash
./target/release/catalyst-cli db-maintenance --data-dir /var/lib/catalyst/<region>/data
```

## Upgrades, backups, and rollback safety

For long-running public networks, treat upgrades as potentially stateful operations.

Recommended workflow:

- **Before upgrading**:
  - take a snapshot backup:

```bash
./target/release/catalyst-cli db-backup \
  --data-dir /var/lib/catalyst/<region>/data \
  --out-dir "/var/lib/catalyst/<region>/backup.$(date +%s)"
```

- **If an upgrade goes wrong**:
  - stop the service
  - restore from the backup directory:

```bash
./target/release/catalyst-cli db-restore \
  --data-dir /var/lib/catalyst/<region>/data \
  --from-dir "/var/lib/catalyst/<region>/backup.<ts>"
```

Notes:
- The storage layer records an informational on-disk version marker (`storage:version`) to help detect mismatches across upgrades.
- For public networks, prefer snapshot-based recovery rather than ad-hoc manual edits to the DB directory.

## Troubleshooting

### “Insufficient data collected: got 1, required 2”

This means validators are not receiving enough peer messages in a cycle. Common causes:
- `30333/tcp` blocked (cloud firewall or `ufw`)
- node not listening on `0.0.0.0:30333`
- not actually connected to other validators (bootstrap peers missing/wrong)
- large clock drift (ensure NTP/chrony; validators share a wall-clock cycle index). See also [`consensus-wall-clock-and-time.md`](./consensus-wall-clock-and-time.md).

### Pruning vs fork replay (archival vs pruned)

With `[storage].history_prune_enabled = true`, old `consensus:lsu:*` metadata is deleted. **Quorum fork replay** that replays from cycle `1` requires **archival** LSU metadata (or successful **P2P prefetch** within `CATALYST_RECONCILE_PREFETCH_MS`). Keep at least one archival node, or disable pruning, if you need self-healing from ancient forks. See [`consensus-quorum-and-fork-choice.md`](./consensus-quorum-and-fork-choice.md).

### Reconcile stuck: "no checkpoint or peer bridged" the gap

Every process now writes a checkpoint on its **first** successful apply after startup (in addition
to the periodic `CATALYST_CHECKPOINT_EVERY_CYCLES` interval, default 32 cycles), so a fresh restart
or reset should always have an anchor available for near-head reconcile. If you still see
`Quorum fork reconcile FAILED: missing stored LSU for cycle N and no checkpoint or peer bridged it`
in logs (tracing target `catalyst.consensus.reconcile`) and the
`consensus_fork_reconcile_no_checkpoint_total` counter incrementing, the node has correctly
refused to guess rather than silently apply divergent state — it needs operator help:

- Pull a `db-backup` from a healthy, caught-up peer and `db-restore` it onto this node (see
  [Upgrades, backups, and rollback safety](#upgrades-backups-and-rollback-safety) above), then
  restart.
- Or wait for peers to answer `LsuRangeRequest` (`CATALYST_RECONCILE_PREFETCH_MS`) if the gap is
  recoverable from gossip.
- Do **not** manually delete/edit `consensus:*` metadata to force past this — the check exists
  because the alternative is a silent, permanent state fork (see
  [`consensus-reliability-review-2026-07.md`](./consensus-reliability-review-2026-07.md)).

### Circuit breaker tripped: consensus halted, needs operator action

**Symptom:** a node's applied head stops advancing (the block explorer flatlines for that node, or
for the whole network if the diverged node was the one others were tracking) even though the
process is still running and connected.

This is the escalation of the "no checkpoint or peer bridged" case above: after
`CATALYST_RECONCILE_CIRCUIT_BREAK_THRESHOLD` (default `5`) consecutive reconcile failures for the
same applied head within `CATALYST_RECONCILE_CIRCUIT_BREAK_WINDOW_MS` (default `30000`), the node
deliberately **stops trying to heal itself** — both the reconcile path and, separately, its own
cycle production are gated shut for that head — rather than burning CPU on a doomed
purge/replay loop forever (the historical failure mode: 55,000+ failed reconcile attempts over 3
days with no operator signal). This is **correct, intentional behavior**: it trades "network keeps
producing empty-looking progress while quietly diverged" for "network visibly stops and waits for a
human." Look for:

- Log lines (tracing target `catalyst.consensus.reconcile`): `"Reconcile circuit-broken after
  repeated consecutive failures resolving a replay gap"` and/or `"Refusing to apply self-produced
  LSU for cycle N: reconcile circuit breaker is open for applied head M"`.
- Counters: `consensus_fork_reconcile_circuit_broken_total`,
  `consensus_self_produced_apply_blocked_circuit_broken_total`.

**A second, distinct trigger for this same breaker:** `"Self-produced state_root does not match
network-certified root (same recipe, different result)"` (target `catalyst.consensus.apply`,
counter `consensus_self_produced_state_root_mismatch_total`). This fires when a node's *own*
self-produced cycles agree with peers on the `lsu_hash` (recipe) but computed a different
`state_root` — i.e. the node's local execution has silently diverged, something the ADR 0002
state-root certificate now catches even though nothing self-produced was ever cross-checked
against it before. This is a different root cause from the reconcile-failure case above (that one
is triggered by a *peer's* gossiped LSU disagreeing; this one is caught by a periodic background
scan comparing this node's own applied history against certificates it already has), but the same
breaker and the same remediation apply once it trips.

Remediation (same as the checkpoint case above — this is the same underlying condition, just past
the point where the node will keep retrying on its own):

- Pull a `db-backup` from a healthy, caught-up peer and `db-restore` it onto this node, then restart
  (see [Upgrades, backups, and rollback safety](#upgrades-backups-and-rollback-safety) above).
- Investigate *why* the divergence happened before just restoring and moving on — a repeated trip on
  the same node points at a local bug or environment issue (clock skew, disk corruption, etc.), not
  bad luck.
- Do **not** manually delete/edit `consensus:*` metadata to force past this, for the same reason as
  above.

**Alerting:** set `CATALYST_ALERT_WEBHOOK_URL` to a Discord incoming-webhook URL and the node will
post a message there the moment either counter above trips, instead of relying on someone noticing
the explorer stalled. `CATALYST_ALERT_COOLDOWN_SECS` (default `900` = 15 min) limits repeat alerts
for the same failure so a node stuck for hours doesn't spam the channel. This is best-effort
(fire-and-forget, logged-not-propagated on failure) and does not replace watching the metrics/logs —
if `CATALYST_ALERT_WEBHOOK_URL` is unset, the process logs a startup warning
(`catalyst.alerting` target) saying so.

### `TX_BATCH_MISS_FATAL` / validators stop producing the same head

Multi-validator nodes agree on **per-cycle transaction batches** via a deterministic batch leader. If a follower cannot obtain the canonical batch before the end of the cycle window, it **skips producing** for that cycle (preferable to forking with an empty construction set).

Mitigations:

- Keep **NTP-synchronized** clocks and open `30333/tcp` between all validators.
- Optionally raise the wait budget (milliseconds): `CATALYST_TX_BATCH_WAIT_BUDGET_MS` (default scales from `cycle_duration` minus a safety margin, minimum 12s; see `tx_batch_follower_wait_budget_default` in `crates/catalyst-cli/src/consensus_limits.rs`).
- For **fork replay** gaps on pruned nodes, ensure peers answer `LsuRangeRequest` or raise `CATALYST_RECONCILE_PREFETCH_MS` (see [`protocol-params.md`](./protocol-params.md)).

**Removed 2026-07-22:** `CATALYST_ALLOW_LEGACY_AMBIGUOUS_EMPTY_TX_BATCH` used to let a follower proceed with an empty construction set whenever it couldn't confirm the canonical batch was genuinely agreed-empty. It was a direct contributor to that day's catalyst-testnet outage (see `docs/consensus-reliability-review-2026-07.md`): it let a node whose own consensus round failed to reach quorum quietly construct a plausible-looking empty batch instead of erroring loudly, masking a real cycle skip. An ambiguous empty batch is now always fatal for a follower (`TX_BATCH_MISS_FATAL`) — same as it already was for a leader. If you see this on a live network, that's a real liveness problem (batch propagation or peer connectivity) to fix, not something to paper over.

### LSU finality on apply (ADR 0001)

**Default:** `[consensus] require_lsu_finality = true` — validators only treat LSUs with a verified **`LsuFinalityCertificateV1`** as canonical for fork choice and apply. Migration testnets may set `require_lsu_finality = false` or `CATALYST_REQUIRE_LSU_FINALITY=0` until every peer gossips finality CIDs reliably.

The process logs a startup warning if you run **`--validator`** with finality disabled. **`CATALYST_REQUIRE_LSU_FINALITY`** overrides TOML when set to a non-empty value. **`CATALYST_ALLOW_CERT_EQUIVOCATION_TIEBREAK`** is for dev/test only (see [`consensus-quorum-and-fork-choice.md`](./consensus-quorum-and-fork-choice.md)).

### Canonical vs observed LSU metadata (indexers)

| Metadata | Meaning |
|----------|---------|
| `consensus:lsu:{cycle}`, `consensus:lsu_hash:{cycle}` | **Canonical** fork-choice winner at that cycle |
| `cycle_by_lsu_hash:{hash}` | **Observed** LSU (any hash seen; may not be canonical) |
| `consensus:certified_lsu_hash:{cycle}` | First verified certified hash at cycle (equivocation guard) |

Expose **canonical** head to applications; use observed keys only for debugging or explorer “fork” views.

### RPC is reachable but tx inclusion is slow

This is expected in the current implementation: RPC acceptance returns a tx id immediately; inclusion can take a few cycles depending on propagation and which validator is the tx batch leader.

