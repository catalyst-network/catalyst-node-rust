## Sync guide (snapshot-based fast sync)

This guide describes a pragmatic way to bring up a **new node** without replaying from genesis:
use a **snapshot export** from an existing healthy node, verify chain identity + head, then start normally.

### Prerequisites

- You have at least one healthy node with storage enabled (validator or storage-enabled node).
- You can copy a directory from the source node to the destination node (e.g. `rsync`/SCP/object storage).

### 1) Confirm chain identity on the source node (RPC)

On the source node RPC:

- `catalyst_chainId`
- `catalyst_networkId`
- `catalyst_genesisHash`
- `catalyst_head`
- `catalyst_getSyncInfo` (convenience bundle)

Record these values. Your destination node should match them after restore.

### 2) Create a snapshot export on the source node

Run on the source node (adjust data dir path to your config):

```bash
./target/release/catalyst-cli db-backup \
  --data-dir /var/lib/catalyst/eu/data \
  --out-dir /tmp/catalyst-snapshot \
  --archive /tmp/catalyst-snapshot.tar
```

This writes:

- a snapshot export (RocksDB snapshot directory structure)
- `catalyst_snapshot.json` containing:
  - chain identity (`chain_id`, `network_id`, `genesis_hash`)
  - applied head (`applied_cycle`, `applied_lsu_hash`, `applied_state_root`)
- optionally, a tar archive (recommended for transport)

### 3) Transfer the snapshot directory to the destination node

Copy the full directory (including `catalyst_snapshot.json`) to the destination node, e.g.:

```bash
rsync -av /tmp/catalyst-snapshot/ root@DEST:/tmp/catalyst-snapshot/
```

### 3b) (Optional) Publish snapshot info for automation

If your RPC node should advertise a downloadable snapshot to new nodes, publish it:

```bash
./target/release/catalyst-cli snapshot-publish \
  --data-dir /var/lib/catalyst/eu/data \
  --snapshot-dir /tmp/catalyst-snapshot \
  --archive-path /tmp/catalyst-snapshot.tar \
  --archive-url https://<your-host>/catalyst-snapshot.tar
```

Then clients can query `catalyst_getSnapshotInfo` from RPC.

### 4) Restore on the destination node

Stop the node if it is running, then restore:

```bash
./target/release/catalyst-cli db-restore \
  --data-dir /var/lib/catalyst/us/data \
  --from-dir /tmp/catalyst-snapshot
```

If `catalyst_snapshot.json` is present, the CLI verifies `chain_id` and `genesis_hash` after restore.

### 4b) (Optional) Automated restore from RPC-published snapshot

On a new node, you can download and restore the published snapshot archive:

```bash
./target/release/catalyst-cli sync-from-snapshot \
  --rpc-url http://<rpc-host>:8545 \
  --data-dir /var/lib/catalyst/us/data
```

### 5) Start the node normally and verify it is on the same chain

Start the node with the same `protocol.chain_id` / `protocol.network_id` as the rest of the testnet.

Verify:

- `catalyst_getSyncInfo` matches the source chain id + genesis hash
- `catalyst_head.applied_cycle` is close to the source node’s head (it will advance as the node continues syncing/participating)

### Notes / limitations

- This is snapshot-style sync, not yet a full “trust-minimized” state sync. It is intended for **testnet ops**.
- For mainnet readiness, fast sync should be extended to verify state roots/proofs and/or multiple peers.

