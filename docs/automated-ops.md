# Automated snapshots, fleet health, and fork recovery

This guide replaces manual `tar` / `scp` cycles with timers, object storage, and scripts that already exist in the repo.

## Architecture (testnet)

```text
                    ┌─────────────────────────────────────┐
                    │  R2 / S3: .../catalysttestnet/      │
                    │           latest.tar (canonical)    │
                    └──────────────┬──────────────────────┘
                                   │
         snapshot timer (US)         │  sync-from-archive
              ┌────────────────────┼────────────────────┐
              ▼                    ▼                    ▼
         ┌─────────┐         ┌──────────┐        ┌──────────┐
         │ US val. │◄───────►│ Asia val.│        │ EU RPC   │
         │canonical│  P2P    │          │        │ follower │
         └─────────┘         └──────────┘        └──────────┘
              ▲
              │ heal if fork
         ┌─────────┐
         │ EU val. │  ← outlier: auto heal from latest.tar
         └─────────┘
```

| Component | Role |
|-----------|------|
| **`catalyst_network_check.py`** | Detect forks (same cycle, different `applied_state_root`) and lag |
| **`catalyst_auto_snapshot.sh`** | Stop → snapshot → upload `latest.tar` → publish metadata (canonical validator only) |
| **`catalyst_heal_local.sh`** | Stop → `sync-from-archive` → start (outlier validator or RPC) |
| **`catalyst_rpc_sync.sh`** | Restore RPC when lag > N cycles behind validators |
| **`catalyst-cli sync-from-archive`** | Download `latest.tar` and `db-restore` without manual extract |

### IPFS vs object storage

- **Block/LSU content** today uses **`dfs_cache/`** (local files) + **P2P `FileRequest`**, not the Kubo HTTP API (`config.toml` `ipfs_api_url` is reserved for future use).
- **Snapshots** are full RocksDB exports → best delivered by **R2/S3 `latest.tar`**, not IPFS DHT (large, operator-controlled).
- Optional later: `ipfs add latest.tar` for redundancy; not required for the workflow below.

## 1. Fleet health check (fork detector)

From your laptop (public validator URLs):

```bash
python3 scripts/catalyst_network_check.py \
  --config scripts/catalyst_network_check.testnet.json \
  --max-cycle-diff 2 \
  --fail-state-file /tmp/catalyst-fleet.failed
```

On a **single validator** (localhost only), copy `scripts/catalyst_network_check.validators.json` and set each machine’s name in a local override, or query via SSH tunnels.

**Fork** = log line like: `fork: applied_state_root differs at cycle N`.

**Machine-readable + heal hints:**

```bash
python3 scripts/catalyst_network_check.py \
  --config scripts/catalyst_network_check.testnet.json \
  --heal-plan \
  --archive-url-template "https://pub-....r2.dev/catalysttestnet/latest.tar"
```

**JSON (automation):**

```bash
python3 scripts/catalyst_network_check.py -c scripts/catalyst_network_check.testnet.json --json
```

## 2. Automated snapshot publish (canonical validator)

Run only on **US** (or whichever node is canonical after a fleet check). **Never** snapshot EU while it is a known outlier.

`/etc/catalyst/snapshot.env`:

```bash
DATA_DIR=/var/lib/catalyst/us/data
SNAPSHOT_OUT=/var/lib/catalyst/us/snapshots
ARCHIVE_URL_BASE=https://pub-9e7e1b5e3a264b2cb43c5fc723e05dd3.r2.dev/catalysttestnet
PUBLIC_ARCHIVE=https://pub-9e7e1b5e3a264b2cb43c5fc723e05dd3.r2.dev/catalysttestnet/latest.tar
CLI=/root/catalyst-node-rust/target/release/catalyst-cli
# Optional: abort snapshot if validators disagree (run check from ops host with public URLs)
# FLEET_CHECK_CONFIG=/path/to/catalyst_network_check.testnet.json
R2_UPLOAD_CMD='aws s3 cp "$ARCHIVE" "s3://${BUCKET}/catalysttestnet/latest.tar" --profile r2 --endpoint-url "https://${ACCOUNT_ID}.r2.cloudflarestorage.com"'
```

```bash
chmod +x scripts/catalyst_auto_snapshot.sh
sudo bash -c 'set -a; source /etc/catalyst/snapshot.env; set +a; scripts/catalyst_auto_snapshot.sh'
```

**systemd timer (US example)** `/etc/systemd/system/catalyst-snapshot.service`:

```ini
[Unit]
Description=Catalyst canonical snapshot publish
After=network-online.target

[Service]
Type=oneshot
EnvironmentFile=/etc/catalyst/snapshot.env
WorkingDirectory=/root/catalyst-node-rust
ExecStart=/root/catalyst-node-rust/scripts/catalyst_auto_snapshot.sh
```

`/etc/systemd/system/catalyst-snapshot.timer`:

```ini
[Unit]
Description=Hourly Catalyst snapshot

[Timer]
OnCalendar=hourly
Persistent=true

[Install]
WantedBy=timers.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now catalyst-snapshot.timer
```

Use **hourly** or **every 30 min** for testnet; avoid stopping EU alone for ad-hoc snapshots.

## 3. Auto-heal outlier validator (EU)

When US + Asia match and EU differs at the same cycle:

```bash
cd ~/catalyst-node-rust
git pull && cargo build --release -p catalyst-cli

sudo bash scripts/catalyst_heal_local.sh \
  --data-dir /var/lib/catalyst/eu/data \
  --archive-url "https://pub-9e7e1b5e3a264b2cb43c5fc723e05dd3.r2.dev/catalysttestnet/latest.tar"
```

Or CLI only:

```bash
sudo systemctl stop catalyst
sudo rm -rf /var/lib/catalyst/eu/data /tmp/catalyst-restore
sudo mkdir -p /var/lib/catalyst/eu/data

./target/release/catalyst-cli sync-from-archive \
  --archive-url "https://pub-....r2.dev/catalysttestnet/latest.tar" \
  --data-dir /var/lib/catalyst/eu/data \
  --work-dir /tmp/catalyst-restore

sudo systemctl start catalyst
```

Confirm `applied_state_root` matches US before publishing RPC snapshots from EU.

## 4. RPC follower auto-refresh

On **catalyst-1** (RPC, `validator=false`):

```bash
# /etc/catalyst/rpc-sync.env
RPC_DATA_DIR=/var/lib/catalyst/eu/data
ARCHIVE_URL=https://pub-....r2.dev/catalysttestnet/latest.tar
VALIDATOR_URLS="http://45.76.21.153:8545 http://207.148.126.35:8545"
MAX_LAG_CYCLES=300
CLI=/root/catalyst-node-rust/target/release/catalyst-cli
```

```bash
chmod +x scripts/catalyst_rpc_sync.sh
sudo bash -c 'set -a; source /etc/catalyst/rpc-sync.env; set +a; scripts/catalyst_rpc_sync.sh'
```

Recommended RPC settings:

```ini
# systemd: Environment=CATALYST_REQUIRE_LSU_FINALITY=0
# until follower apply is stable (see sync-guide.md)
```

Pair with **git pull** builds that include follower catch-up (`Follower advanced` in logs).

## 5. Rules that prevent forks

| Do | Don't |
|----|--------|
| Snapshot **US** (or aligned majority) on a timer | Stop **only EU** while US/Asia produce |
| Heal EU from **`latest.tar`** built from US | Upload EU snapshots while EU `state_root` ≠ US |
| Start all validators within **~10s** after a full wipe | Start EU, wait 60s, then start US/Asia |
| Use `catalyst_network_check.py` before/after maintenance | Trust `applied_cycle` alone (check `applied_state_root`) |
| RPC: `sync-from-archive` + `CATALYST_REQUIRE_LSU_FINALITY=0` | Rely on Kubo for Catalyst (not wired yet) |

## 6. Optional: central ops cron

From one ops machine with SSH to all hosts:

```bash
# 1) Detect
python3 scripts/catalyst_network_check.py -c scripts/catalyst_network_check.testnet.json --heal-plan

# 2) If EU in outliers list, SSH heal
ssh root@45.32.177.248 'bash -s' < scripts/catalyst_heal_local.sh --data-dir ...  # adapt
```

## 7. Determinism & automatic self-heal

The applied state root is a pure function of `(cycle, canonical tx batch, seed-selected producer set)`. Two classes of non-determinism that previously seeded silent single-node forks have been removed in code:

- **LSU content from the canonical committee.** `producer_list` / `vote_list` / `compensation_entries` are derived from the seed-selected producer set, not from each node's locally-observed votes. Locally-observed votes are used only as a liveness/quorum signal.
- **LSU-driven fee-credit settlement.** Fee-credit spend reimbursement is reconstructed from the applied LSU's transaction entries instead of the leader-only `cycle_txids` index. Previously only the batch leader settled fee credits, so any cycle that spent credits offset that node's state root by one cycle and forked it off-chain.

### Reconcile / backfill (already enabled)

| Env | Default | Purpose |
|-----|---------|---------|
| `CATALYST_RECONCILE_PREFETCH_MS` | `8000` | Time budget to backfill missing `consensus:lsu:*` from peers over P2P before a fork-reconcile attempt. `0` disables. |
| `CATALYST_CHECKPOINT_EVERY_CYCLES` | `32` | Cadence of local state checkpoints used to anchor a reconcile replay (avoids genesis replay). |
| `CATALYST_MAX_REPLAY_CYCLES` | (see config) | Caps reconcile replay depth. |

Prefetch is **on by default** — set non-zero only if it was explicitly disabled. For backfill to succeed, peers must be reachable (`bootstrap_peers` / libp2p connectivity) so the node can answer `LsuRangeRequest`s. Fork-reconcile now resolves a checkpoint prefix **first**, then aims the P2P backfill at the actual replay window (`checkpoint+1..target`) rather than always anchoring to the local head.

### Self-heal bounds (and when to restore from snapshot)

Automatic reconcile recovers a node that is **behind** or briefly divergent **within the checkpoint/replay horizon** and whose checkpoint lies on the canonical chain. It cannot safely recover a node that:

- has diverged beyond `CATALYST_MAX_REPLAY_CYCLES`, or
- is missing genesis-era LSU history with no on-chain checkpoint (logs: `Quorum fork reconcile skipped: missing stored LSU for cycle 1`), or
- repeatedly logs `Insufficient data collected: got 1, required 2` (its cycle/branch no longer aligns with the quorum — it has been cut out).

For those, re-seed from the canonical majority via `sync-from-archive` (sections 3–4). Verify with `catalyst_network_check.py` that `applied_state_root` matches the majority **at the same cycle** before resuming snapshot publication.

## Related docs

- [`sync-guide.md`](./sync-guide.md) — snapshot format and RPC metadata
- [`node-operator-guide.md`](./node-operator-guide.md) — ports, validator set, pruning
- [`consensus-quorum-and-fork-choice.md`](./consensus-quorum-and-fork-choice.md) — fork semantics
