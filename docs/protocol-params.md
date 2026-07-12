# Protocol parameters (safety + governance notes)

This document describes how Catalyst parameters are currently configured and validated, and what rules operators should follow when changing them.

## Parameter sources

Today, parameters come from three places:

- **Compile-time defaults**: `NodeConfig::default()` in `crates/catalyst-cli/src/config.rs`.
- **Config file**: `catalyst.toml` (or `--config <path>`), parsed into `NodeConfig`.
- **CLI flags**: e.g. `catalyst-cli start --validator --rpc ...`, which override config fields at runtime.

There is no on-chain governance or parameter voting in this implementation snapshot.

## Safety rule: chain identity is immutable

These identify a chain:
- `protocol.chain_id`
- `protocol.network_id`
- `protocol.genesis_hash` (persisted at genesis)

Changing `protocol.chain_id` or `protocol.network_id` for a running network creates a **new chain**. Do not change them except for intentional resets.

## Validation and safe bounds

On startup, the node validates the config via `NodeConfig::validate()` (called from `catalyst-cli start`).

Examples of enforced bounds:
- `protocol.chain_id > 0`
- `protocol.network_id` non-empty
- `network.max_peers >= network.min_peers`
- `network.listen_addresses` non-empty
- `consensus.cycle_duration_seconds >= 10`
- `consensus.min_producer_count >= 1`
- `consensus.max_producer_count >= consensus.min_producer_count`
- pruning knobs are validated when enabled (e.g. non-zero interval and batch size)

If validation fails, the node refuses to start with an actionable error.

## Version negotiation (libp2p Identify vs message envelopes)

Two independent version surfaces apply:

- **`MessageEnvelope.version`** on every gossip envelope: numeric wire revision (`PROTOCOL_VERSION` in `crates/catalyst-utils/src/network.rs`). Decode requires an **exact** match; a peer on a different number is rejected during envelope parsing.
- **libp2p Identify `protocol_version` string** (Identify behaviour in `catalyst-network`): a Catalyst family label, advertised as `catalyst/1.0` (`CATALYST_IDENTIFY_PROTOCOL_VERSION`). Peers are kept when the remote string shares the same **major** (e.g. `catalyst/1`, `catalyst/1.0`), via `catalyst_identify_protocol_major_ok` in `crates/catalyst-network/src/protocol_identify.rs`, so patch-level bumps do not drop `catalyst/1` builds, while incompatible majors still disconnect.

Changing operator TOML defaults for Identify does not change bincode payload layout; coordinated breaking upgrades must treat **envelope version** and **Identify string** separately.

## Consensus-related environment variables

These are read by `catalyst-cli` at runtime (not TOML). They affect **liveness**, **fork recovery**, and **finality policy**.

| Variable | Default | Meaning |
|----------|---------|---------|
| `CATALYST_TX_BATCH_WAIT_BUDGET_MS` | `max(12000, cycle_ms − 8000)` (approx.) | Max time a **non–batch-leader** validator waits for the canonical `ProtocolTxBatch` / recovery before skipping consensus for that cycle. |
| `CATALYST_P2P_TX_BATCH_MAX_CYCLE_SLACK` | `3` (max `128`) | Ingress gate for `MessageType::TransactionBatch` and `TxBatchControl::Commit`: accept only if the message’s `cycle` is within this many whole cycles of the wall-derived index `floor(now_ms / cycle_ms)`. Does **not** apply to `TxBatchControl::ResyncRequest`. |
| `CATALYST_P2P_LSU_MAX_WALL_LEAD` | `8192` (max `2000000`) | Ingress gate for LSU-related payloads (`LsuCidGossip`, `LsuGossip`, finality messages, `LsuRangeRequest` / response refs, `FileResponse` LSU completion): reject when `cycle > floor(now_ms / cycle_ms) + lead`. Does **not** reject historical cycles behind the wall index. |
| `CATALYST_ALLOW_LEGACY_AMBIGUOUS_EMPTY_TX_BATCH` | unset (strict) | If `1`/`true`/`yes`, allows the **unsafe** legacy path: proceed with an empty construction set when batch and commit are ambiguous (mixed-version testnets only). |
| `CATALYST_RECONCILE_PREFETCH_MS` | `8000` | Before aborting **quorum fork replay**, broadcast `LsuRangeRequest` and poll for missing `consensus:lsu:{i}` metadata. Set `0` to disable. |
| `CATALYST_MAX_REPLAY_CYCLES` | `1000000` | Upper bound on replay depth for fork reconcile; `0` disables reconcile replay. |
| `CATALYST_REQUIRE_LSU_FINALITY` | unset (use TOML) | When set to a **non-empty** value, overrides [`consensus.require_lsu_finality`](#consensus-toml-keys) for this process. `1`/`true`/`yes` enable; `0`/`false`/`no` disable. |
| `CATALYST_REQUIRE_STATE_ROOT_FINALITY` | unset (use TOML) | When set to a **non-empty** value, overrides [`consensus.require_state_root_finality`](#consensus-toml-keys) for this process. `1`/`true`/`yes` enable; `0`/`false`/`no` disable. |
| `CATALYST_ALLOW_CERT_EQUIVOCATION_TIEBREAK` | unset | If `1`/`true`/`yes`, allow deterministic `lsu_hash` tie-break when two **verified** certificates conflict at the same cycle (disables production stall). **Dev/test only.** |

`MessageType::TransactionRequest` gossip payloads for tx-batch control are **bincode-encoded** `TxBatchControl` (`Commit` / `ResyncRequest`) in `crates/catalyst-cli/src/tx.rs`; do not multiplex unrelated payloads on that message type across versions.

For a given cycle, **`TxBatchControl::Commit.tx_count` must equal** the number of transactions in the canonical `ProtocolTxBatch`, and **`batch_hash`** must match that batch’s hash; followers reject batches that disagree (metadata key `consensus:tx_batch_commit:{cycle}` stores 32-byte hash + 4-byte little-endian `tx_count`).

### Consensus TOML keys (`[consensus]` in `NodeConfig`)

| Key | Default | Meaning |
|-----|---------|---------|
| `require_lsu_finality` | `true` | If `true`, fork choice and apply require a verified **`LsuFinalityCertificateV1`** (ADR 0001); quorum-inferred LSUs do not compete for canonical slots. Set `false` on migration testnets only. Overridable by `CATALYST_REQUIRE_LSU_FINALITY`. |
| `require_state_root_finality` | `false` | If `true`, the trusted/catch-up apply path additionally requires a verified **`LsuStateRootCertificateV1`** (ADR 0002) whose certified `state_root` matches the peer's claimed root before trusting it. Defaults `false` while networks roll out ADR 0002 attestation gossip; plan to flip to `true` for mainnet once every peer gossips state-root CIDs reliably (same rollout shape ADR 0001 used). Overridable by `CATALYST_REQUIRE_STATE_ROOT_FINALITY`. |

Wall-clock behavior and NTP expectations: [`consensus-wall-clock-and-time.md`](./consensus-wall-clock-and-time.md).

## Recommended operational policy

- **Public networks**: treat changes to consensus parameters as coordinated upgrades.
- **Defaults**: keep conservative defaults, and use explicit overrides only when measured needs justify them.
- **Rollback**: before changing parameters, take a DB snapshot backup (see `docs/node-operator-guide.md`).

