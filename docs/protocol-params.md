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

## Recommended operational policy

- **Public networks**: treat changes to consensus parameters as coordinated upgrades.
- **Defaults**: keep conservative defaults, and use explicit overrides only when measured needs justify them.
- **Rollback**: before changing parameters, take a DB snapshot backup (see `docs/node-operator-guide.md`).

