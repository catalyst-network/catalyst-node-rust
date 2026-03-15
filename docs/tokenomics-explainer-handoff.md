# Tokenomics explainer handoff (website + docs agents)

This file is a handoff brief for AI agents maintaining:

- the Catalyst website explainer content
- long-form/public documentation

It summarizes the current v1 tokenomics decisions in plain language and provides copy-ready points.

## Canonical source of truth

When writing user-facing content, treat these as canonical:

- `docs/tokenomics-model.md` (policy and rationale)
- `docs/tokenomics-spec.md` (implementation-facing alignment)
- RPC `catalyst_getTokenomicsInfo` (operator verification surface)

## One-paragraph explainer (copy-ready)

Catalyst v1 uses a fair-launch model with `0 KAT` at genesis and fixed issuance of `1 KAT` per successful cycle (target ~20 seconds). Transaction fees are mandatory and split deterministically: `70%` is burned, `30%` goes into the reward pool, and `0%` goes to treasury. Each cycle, rewards are shared between selected producers and eligible waiting workers, with the larger share to active producers and a meaningful share to waiting participants. Waiting workers can also accrue non-transferable fee credits (after warmup) that offset their own transaction fees, improving utility for builders and operators while preserving anti-spam constraints.

## Key parameters (publish exactly)

- `genesis_supply = 0 KAT`
- `fixed_block_reward = 1 KAT` per successful cycle
- `atoms_per_kat = 1_000_000_000`
- `cycle_target = 20s`
- `fee_burn_bps = 7000` (70%)
- `fee_to_reward_pool_bps = 3000` (30%)
- `fee_to_treasury_bps = 0`
- `producer_set_reward_bps = 7000`
- `waiting_pool_reward_bps = 3000`
- fee credits:
  - `enabled = true`
  - `warmup_days = 14`
  - `accrual_atoms_per_day = 200`
  - `max_balance_atoms = 6000`
  - `daily_spend_cap_atoms = 300`

## Decision rationale (plain language)

### Why `0 KAT` at genesis

- reinforces fair launch (no premine/no sale)
- aligns issuance with ongoing participation
- keeps initial economics simple and auditable

### Why fixed `1 KAT` issuance per cycle

- predictable and easy to reason about
- avoids early complexity of dynamic inflation controls
- creates a clear baseline for operators, wallets, and explorers

### Why burn 70% of fees

- imposes a non-recoverable economic cost on spam/abuse
- avoids treasury governance/custody complexity at launch
- keeps fee routing simple and deterministic for verification
- still leaves 30% of fees to strengthen participant rewards

## Reward flow summary

Per successful cycle:

1. Mint fixed issuance (`1 KAT`).
2. Route 30% of cycle fees into reward pool (70% burned, 0% treasury).
3. Split reward pool between:
   - selected producer set (larger share)
   - eligible waiting worker pool (smaller share)
4. For eligible waiting workers, accrue fee credits (bounded and capped by day).

## Fee credits: correct wording for public docs

Use this wording:

- "Fee credits accrue to eligible waiting workers (not selected producers for that cycle)."
- "Fee credits are non-transferable and only pay the sender's own fees."
- "Credits have warmup, max balance, daily spend caps, and a churn-penalty eligibility window."

Avoid this wording:

- "All participants always accrue fee credits."
- "Fee credits are transferable."
- "Fee credits remove the need for transaction fees." (fees remain mandatory)

## Suggested FAQ entries

### Is Catalyst deflationary?

Not necessarily at all times. v1 has fixed issuance (`1 KAT` per successful cycle) and also burns 70% of fees. Net supply direction depends on activity levels and total fees over time.

### Why not send fees to a treasury?

v1 intentionally keeps treasury routing at zero to avoid launch-time custody/governance complexity. This can be revisited in a future protocol upgrade if needed.

### Do waiting nodes receive anything?

Yes. Eligible waiting workers receive a share of cycle rewards and can accrue fee credits that offset their own transaction fees.

### Are fee credits transferable or tradable?

No. Fee credits are non-transferable and scoped to the same sender identity.

## Verification snippet for operators

Use this JSON-RPC call to display live tokenomics parameters:

```bash
curl -s -X POST http://127.0.0.1:8545 -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"catalyst_getTokenomicsInfo","params":[]}'
```

Check that response includes:

- `block_reward_atoms`
- `fee_burn_bps`
- `fee_to_reward_pool_bps`
- `fee_to_treasury_bps`
- `producer_set_reward_bps`
- `waiting_pool_reward_bps`

## Audience framing guidance

- **Builders**: emphasize low-friction participation and fee-credit utility for ongoing app usage.
- **Node operators**: emphasize deterministic economics and clear validation surfaces.
- **General users**: emphasize fair launch, predictable issuance, and anti-spam fee policy.

## Change control note

This describes the active v1 baseline. Any changes to issuance, fee routing, reward splits, or fee-credit behavior should be treated as explicit protocol-version upgrade work, then reflected in:

1. `docs/tokenomics-model.md`
2. `docs/tokenomics-spec.md`
3. RPC validation expectations
