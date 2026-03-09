# Tokenomics model (v1 baseline)

This is the canonical short-form tokenomics reference for the current implementation.
It is intentionally explicit and machine-readable so it can be used as input for:

- implementation work
- issue tracking
- user-facing long-form documentation authored later

## Scope

This document defines the v1 baseline for:

- genesis supply policy
- fixed issuance schedule
- reward split between selected producers and waiting workers
- fee routing
- fee credits

It does **not** define future adaptive/dynamic monetary policy.

## Policy decisions (locked)

- **Genesis supply**: `0 KAT`
- **No premine / no sale**: `true`
- **Issuance start**: first successful cycle after genesis
- **Cycle target**: `20 seconds`
- **Fixed block reward**: `1 KAT` per successful cycle

Rationale:

- deterministic and easy to audit
- avoids governance complexity of dynamic inflation in early network phases
- aligns with fair-launch objective

## Reward model

Per successful cycle:

1. Mint fixed issuance (`1 KAT`).
2. Add fee-routed reward share from that cycle's total fees.
3. Split reward pool across:
   - producer set (selected for the cycle)
   - eligible waiting worker pool (registered workers not selected that cycle)

### Parameters

- `block_reward_atoms = 1`
- `fee_to_reward_pool_bps = 3000` (30% of fees)
- `producer_set_reward_bps = 7000`
- `waiting_pool_reward_bps = 3000`

Notes:

- `producer_set_reward_bps` and `waiting_pool_reward_bps` are applied to the reward pool.
- Any integer remainders are distributed deterministically.

## Fee routing

v1 fee routing intent:

- fees remain mandatory for anti-spam
- a portion of fees are routed back to rewards (`fee_to_reward_pool_bps`)
- remaining fee-routing policy (e.g. burn share) is deterministic and parameterized in implementation/spec

## Fee credits (waiting-node utility)

Fee credits are non-transferable credits used only to pay the sender's own transaction fees.

### Parameters

- `fee_credits_enabled = true`
- `fee_credits_warmup_days = 14`
- `fee_credits_accrual_atoms_per_day = 200`
- `fee_credits_max_balance_atoms = 6000`
- `fee_credits_daily_spend_cap_atoms = 300`
- cycle model uses `20s` to derive day buckets

### Behavioral rules

- Credits accrue only for eligible waiting workers (not selected producers) after warmup.
- Credits are bounded by max balance.
- Daily spend cap limits burst usage.
- Credits are non-transferable and cannot be delegated.

## Genesis behavior and config

Default protocol config is now set for zero-genesis funding:

- `protocol.faucet_mode = disabled`
- `protocol.faucet_balance = 0`

This ensures default startup has no pre-funded genesis wallet.

Dev/test networks can still opt into faucet funding by setting:

- `protocol.faucet_mode = deterministic` (or `configured`)
- `protocol.faucet_balance > 0`

## Implementation map

- Node defaults / genesis funding mode:
  - `crates/catalyst-cli/src/config.rs`
  - `crates/catalyst-cli/src/node.rs` (`ensure_chain_identity_and_genesis`)
- Reward split + producer compensation:
  - `crates/catalyst-consensus/src/phases.rs`
  - `crates/catalyst-cli/src/node.rs` (LSU apply path)
- Fee credits:
  - `crates/catalyst-cli/src/node.rs`

## Out of scope (future)

- dynamic inflation schedules
- adaptive issuance tied to utilization
- governance process for runtime parameter changes

These can be introduced in a later protocol version once operational telemetry and governance controls are mature.
