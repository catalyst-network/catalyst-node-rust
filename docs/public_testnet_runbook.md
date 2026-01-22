### Catalyst public testnet runbook (minimal)

### Ports
- **P2P**: TCP `30333`
- **RPC (JSON-RPC)**: TCP `8545` (recommended: bind to `127.0.0.1` and put behind a reverse proxy / auth if exposing)

### Config
- Start from `crates/catalyst-config/configs/public_testnet.toml`.
- Set these fields before running:
  - **`[network].bootstrap_peers`**: at least one bootstrap multiaddr
  - **`[storage].data_dir`**: persistent disk path
  - **`[dfs].cache_dir`**: persistent disk path

### Safe RPC defaults
- Keep `rpc.enabled = false` unless you need it.
- If enabling RPC:
  - keep `rpc.address = "127.0.0.1"` by default
  - tighten `cors_origins` (do not use `"*"` on public infrastructure)

### Bootstrap procedure
- Obtain a bootstrap peer multiaddr (example placeholder):
  - `/ip4/<BOOTSTRAP_IP>/tcp/30333/p2p/<BOOTSTRAP_PEER_ID>`
- Add it to `bootstrap_peers`.

### Quick verification
- After startup, query head:
  - `catalyst_head`
- Query peers:
  - `catalyst_peerCount`
