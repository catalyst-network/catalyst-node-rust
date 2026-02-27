# Security threat model (working)

This document is a living threat model for Catalyst mainnet readiness. It focuses on realistic adversaries and measurable failure modes.

## Security goals

- **Safety**: nodes should not accept or produce invalid state transitions.
- **Liveness**: the network should continue producing cycles under WAN conditions and moderate adversarial pressure.
- **Bounded resource usage**: hostile peers should not cause unbounded CPU/memory/disk growth.
- **Operator recoverability**: clear runbooks for rollback/restore and for key compromise response.

## Non-goals (for now)

- Perfect privacy / traffic analysis resistance (see `#198`).
- Full economic security analysis (see `#184` / `#185`).

## Trust boundaries + assets

- **Consensus / state**: LSU application, `prev_state_root` continuity, account state root.
- **P2P network**: peer connections, message relay/rebroadcast, discovery.
- **RPC surface**: public read APIs, tx submission, snapshot info.
- **Storage**: RocksDB data directory, snapshots, pruning.
- **Keys**: node identity key, validator key material, faucet custody keys.

## Adversary model

- **Remote internet attacker**: can connect to public P2P/RPC endpoints and send arbitrary messages at scale.
- **Sybil**: can create many peers/identities and attempt to dominate connectivity.
- **Eclipse attacker**: attempts to isolate a victim node by controlling its peer set.
- **Partition**: network splits due to BGP/routing/NAT/firewall or intentional interference.
- **Malicious operator**: runs a public RPC/indexer with modified code, logs, or data access.

## Major threats and current mitigations

### DoS via oversized or high-rate messages

- **Threat**: send huge payloads or many small payloads to exhaust CPU/memory/bandwidth.
- **Mitigations**:
  - per-peer/per-connection **rate budgets** and **payload caps** in networking layer
  - configurable safety limits in `[network.safety_limits]` (see `#246`)

### DoS via unbounded rebroadcast/dedup state

- **Threat**: force multi-hop rebroadcast caches to grow without bound.
- **Mitigations**:
  - relay/dedup caches are bounded and configurable (`[network.relay_cache]`, `[network.safety_limits.dedup_cache_max_entries]`)

### Replay / downgrade / incompatible wire payloads

- **Threat**: replay old messages or send incompatible wire payloads to cause confusion/crashes.
- **Mitigations**:
  - versioned envelope wire wrapper (`CENV` + `PROTOCOL_VERSION`)
  - libp2p identify protocol gating (`catalyst/1`)

### Eclipse / Sybil

- **Threat**: attacker controls victim’s peer set, preventing honest connectivity.
- **Mitigations (partial, current)**:
  - bootstrap peer + DNS seed support
  - min peer maintenance with dial backoff + jitter (`#200`)
- **Gaps**:
  - stronger peer selection diversity (IP/subnet caps, scoring, verified bootstrap sets)
  - explicit capability/feature negotiation beyond identify string

### Network partition / delayed delivery

- **Threat**: consensus stalls or forks during partitions; rejoin causes state divergence.
- **Mitigations (partial, current)**:
  - “reliable join” work: backfill + continuity checks
  - bounded message TTL/hops and dedup
- **Gaps**:
  - explicit partition testing harness and recovery procedures (`#241`)

### Storage durability / corruption / rollback hazards

- **Threat**: disk fills or DB corruption causes node failure or silent divergence.
- **Mitigations**:
  - history pruning (opt-in) + maintenance tools (`db-stats`, `db-maintenance`)
  - snapshot backup/restore runbooks (`docs/node-operator-guide.md`)
  - storage version marker (`storage:version`)

### RPC abuse

- **Threat**: high-rate RPC calls or expensive queries degrade node liveness.
- **Mitigations (partial)**:
  - P2P-side bounding exists; RPC-side needs explicit rate limiting and request shaping (future work).

## What “done” looks like (mainnet bar)

- Threats tracked to mitigations and tests.
- At least one **adversarial CI suite** exists and is run per PR.
- WAN soak/chaos testing is run before releases (`#241`).

