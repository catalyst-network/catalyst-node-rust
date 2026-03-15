# Tokenomics specification (v1 active)

This file is the implementation-facing specification for tokenomics v1.
It is aligned with `docs/tokenomics-model.md` and current code on `main`.

If you are writing long-form/public docs, use `docs/tokenomics-model.md` as the canonical narrative source and this file as the engineering checklist.

## Locked v1 policy

- Genesis supply: `0 KAT`
- No premine / no token sale
- Issuance starts on successful post-genesis cycles
- Fixed issuance: `1 KAT` per successful cycle
- Reward split includes:
  - selected producer set
  - eligible waiting worker pool
- Fee credits are enabled for eligible waiting workers and are non-transferable

## Implementation alignment

### Config and genesis behavior

- `crates/catalyst-cli/src/config.rs`
  - default `protocol.faucet_mode = disabled`
  - default `protocol.faucet_balance = 0`
  - local `testnet/*` config path keeps deterministic faucet convenience for dev harnesses
- `crates/catalyst-cli/src/node.rs`
  - genesis initialization respects configured faucet mode/balance

### Reward distribution and fee routing

- `crates/catalyst-consensus/src/phases.rs`
  - `RewardConfig` defaults:
    - `block_reward = 1`
    - `fee_to_reward_pool_bps = 3000`
    - `producer_set_reward_bps = 7000`
    - `waiting_pool_reward_bps = 3000`
- `crates/catalyst-cli/src/node.rs`
  - fee-routing constants:
    - `TOKENOMICS_FEE_BURN_BPS = 7000`
    - `TOKENOMICS_FEE_TO_REWARD_POOL_BPS = 3000`
    - `TOKENOMICS_FEE_TO_TREASURY_BPS = 0`
  - applies compensation entries
  - computes waiting pool share and fee-credit accrual/spend paths

### Fee credits

- `crates/catalyst-cli/src/node.rs` constants:
  - `TOKENOMICS_FEE_CREDITS_ENABLED = true`
  - `TOKENOMICS_FEE_CREDITS_WARMUP_DAYS = 14`
  - `TOKENOMICS_FEE_CREDITS_ACCRUAL_ATOMS_PER_DAY = 200`
  - `TOKENOMICS_FEE_CREDITS_MAX_BALANCE_ATOMS = 6000`
  - `TOKENOMICS_FEE_CREDITS_DAILY_SPEND_CAP_ATOMS = 300`
  - `TOKENOMICS_WAITING_ELIGIBILITY_CHURN_PENALTY_DAYS = 3`
  - cycle reference `20s` for day-bucket math
- waiting-pool reward + fee-credit eligibility is gated by:
  - first-seen identity warmup
  - churn-penalty window based on recent registration cycle metadata

## Determinism and safety invariants

- No negative balances after LSU apply
- Issuance/reward outcomes are deterministic from chain state
- Reward splits are deterministic, including integer remainder handling
- Fee credits are non-transferable, bounded, and daily-cap enforced
- Same tx cannot be applied twice (nonce/replay rules)

## Validation path before/after deployment

Use `docs/tokenomics-testnet-validation.md` for reset + issuance validation + regression checks.

Current issuance observability surface:

- RPC `catalyst_getTokenomicsInfo` includes:
  - issuance summary (`applied_cycle`, `block_reward_atoms`, `estimated_issued_atoms`)
  - fee-routing params (`fee_burn_bps`, `fee_to_reward_pool_bps`, `fee_to_treasury_bps`)
  - reward-split params (`producer_set_reward_bps`, `waiting_pool_reward_bps`)

## Remaining work tracked in GitHub

- `#261` tracks tokenomics v1 hardening for mainnet.
- `#268` tracks fee-routing semantics + operator-facing parameter surfaces.
- `#269` tracks deterministic reward-split + fee-credit edge-case tests.
- `#270` tracks anti-sybil eligibility controls for waiting-pool rewards/fee credits.
- `#262` / `#271` / `#272` track security gates that tokenomics depends on before launch.
