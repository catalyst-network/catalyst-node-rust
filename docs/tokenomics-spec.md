# Tokenomics specification (proposed; editable)

This document is intended to be **modified by humans** and then used as the canonical source for implementing:
- **#185**: economics spec alignment (this doc)
- **#184**: economics implementation (fees, rewards, issuance, anti-spam)

It is structured as:
- **Parameters**: the values that must be chosen (with recommendations)
- **Mechanisms**: how fees/issuance/rewards work conceptually
- **Invariants + tests**: what must be true for determinism and safety

## Status quo (code today)

- Balances are integer amounts stored under `bal:<pubkey>` (unit is an unlabelled “atom”).
- Transfers are validated with a basic “no negative balances” rule.
- Transactions include a `fees` field and the node enforces a deterministic **minimum fee schedule**:
  - `crates/catalyst-core/src/protocol.rs`: `min_fee_for_core(...)`
- EVM executes with `gas_price = 0` and fee charging is handled (or will be handled) at the protocol layer.
- There is **no** issuance schedule, staking/rewards, inflation, or burn/treasury logic implemented yet.

## Goals

- **Deterministic**: starting from genesis, all honest nodes compute identical balances/supply.
- **Anti-spam**: sustained junk tx submission must be economically bounded.
- **Operationally safe**: parameters are bounded/validated; changes are treated as coordinated upgrades.
- **Wallet/dev UX**: fee estimation is straightforward and stable across upgrades.

## Terminology

- **ATOM**: the smallest indivisible unit of the native token (integer).
- **TOKEN**: human unit (e.g. \(10^9\) ATOM = 1 TOKEN). This doc proposes a decimal scheme below.
- **Burn**: removing tokens from circulation by sending to an unspendable sink.
- **Supply**:
  - **total supply**: sum of all balances + any accounted “burn” (depending on accounting choice)
  - **circulating supply**: total supply minus locked/unspendable accounts (if any)

## Parameter table (values to decide)

### 1) Token identity (product / ecosystem)

- **token_name**: **Kat**
- **token_symbol**: **KAT**
- **token_decimals**: **9** (1 KAT = 1,000,000,000 ATOM)

#### Display units (recommended)

These are UI-friendly subdivisions for wallets/explorers/SDKs. Only **ATOM** is the on-ledger integer unit.

| Unit | Name | Value (KAT) | Atoms |
|---|---|---:|---:|
| KAT | Kat | 1 | 1,000,000,000 |
| KIT | Kit | 0.001 | 1,000,000 |
| BYTE | Byte | 0.000001 | 1,000 |
| ATOM | Atom | 0.000000001 | 1 |

### 2) Supply model (economic policy)

Choose one of these as the **mainnet** policy:

**Option A — Fair launch constant emission (your stated preference)**  
- **genesis_supply_tokens**: **1 TOKEN** (only)
- **block_reward_tokens**: **1 TOKEN per block**
- **premine / treasury**: **none**
- **reward_recipients**: producers (details below)

Important implementation note: in current Catalyst code and docs, “block” corresponds to the **consensus cycle** / applied LSU head (`applied_cycle`). This spec therefore treats:
\[
\text{blocks per year} \approx \frac{365\cdot 24\cdot 60\cdot 60}{\text{cycle\_duration\_seconds}}
\]
and total emitted supply is deterministic from the number of applied cycles.

Concrete examples (with `block_reward_tokens = 1`):

| `cycle_duration_seconds` | blocks/year | new tokens/year |
|---:|---:|---:|
| 1 | 31,536,000 | 31,536,000 |
| 10 | 3,153,600 | 3,153,600 |
| 20 | 1,576,800 | 1,576,800 |
| 60 | 525,600 | 525,600 |

Pros: aligns with “earn by running nodes”; avoids premine distortions; extremely simple supply rule.  
Cons: **Sybil incentive** (many identities → more chances at rewards) unless producer selection has a strong anti-sybil gate.

**Option B — Fixed supply (no emission)**  
- **initial_total_supply_atoms**: `TBD`
- **block_reward_tokens**: `0`

Pros: minimal monetary governance.  
Cons: incentives become fee-only; weak early network bootstrapping.

**Option C — Low inflation / capped schedule**  
- Define issuance schedule (per-cycle or per-year) and deterministic recipients.

Pros: flexible incentives.  
Cons: more parameters and governance complexity.

**Option D — Adaptive issuance** (not recommended initially)  
Inflation reacts to participation/uptime.

### 3) Fee model (protocol-level anti-spam)

This repo already has a deterministic minimum fee schedule. The spec should decide:

- **fee_denomination**: ATOM
- **min_fee_schedule_v1**:
  - **base_fee_by_tx_type** (ATOM): `TBD`  
    - suggested starting point (close to code today):
      - transfer: `1`
      - smart_contract: `5`
      - worker_registration: `1`
  - **per_entry_fee** (ATOM): suggested `1`
  - **per_byte_fee** (ATOM/byte): suggested `0` for testnet, `>=1` for mainnet if large payloads are a concern
  - **max_tx_wire_bytes**: already bounded in code; keep aligned with networking limits

**Fee charging rule** (recommended):  
At apply-time, require `tx.fees >= min_fee(tx)` and then:
- debit `tx.fees` from sender (or from the transaction’s funding entries if multi-entry semantics)
- credit according to `fee_routing_policy` (below)

### 4) Fee routing policy (burn vs treasury vs rewards)

Decide a single deterministic policy for v1:

- **fee_burn_bps**: **recommended** `7000` (burn 70% of fees)
- **fee_to_reward_pool_bps**: **recommended** `3000` (route 30% of fees to node rewards)
- **fee_to_treasury_bps**: `0` (no treasury in v1)
- **invariant**: `fee_burn_bps + fee_to_reward_pool_bps + fee_to_treasury_bps = 10000`

Rationale: this keeps anti-spam pressure strong via burning while still making fees a meaningful long-term operator incentive as network usage grows. It also matches the project goal of no custodial treasury at launch.

#### Fee scaling and burn-risk guardrail

At higher adoption, fees can exceed fixed issuance, which is acceptable if intentional. Track:

\[
\text{net\_issuance\_per\_cycle} = \text{block\_reward\_atoms} - \text{fees\_burned\_atoms}
\]

- If `fees_burned_atoms` is below 1 KAT per cycle on average, supply remains net-inflationary.
- If `fees_burned_atoms` rises above 1 KAT per cycle for sustained periods, supply becomes net-deflationary.
- Recommended operational guardrail: monitor a rolling 90-day ratio; if burn persistently exceeds issuance and operator incentives weaken, lower `fee_burn_bps` and increase `fee_to_reward_pool_bps` via a coordinated protocol upgrade.

### 4.1) Fee credits (earn-to-spend; “everyone in the pool earns”)

If the network goal is “anyone can run a node” and participation grows faster than per-cycle rewards, **selection-based rewards alone** can become too small or too infrequent to feel meaningful for most users.

A practical v1 lever is to let contributors **earn fee credits** that can pay for **their own** transaction fees. This preserves anti-spam economics (fees still exist), but lets long-lived contributors avoid paying cash fees out of pocket.

**Key idea:** fee credits are **non-transferable**, **capped**, and **earned over time** (with warm-up + decay), so they can’t be farmed quickly or turned into a secondary currency.

#### Parameters (values to decide)

- **fee_credits_enabled**: `true`
- **fee_credits_unit**: ATOM (integer)
- **fee_credits_warmup_days**: `14`
  - Credits only begin accruing after sustained participation.
- **fee_credits_accrual_atoms_per_day**: `200`
  - A per-identity/day budget earned while eligible.
- **fee_credits_max_balance_atoms**: `6000`
  - Cap to prevent indefinite banking.
- **fee_credits_decay_bps_per_day**: `25` (0.25%/day)
- **fee_credits_daily_spend_cap_atoms**: `300`
  - Limits how much credit can be spent per day to control abuse.
- **fee_credits_eligibility_min_uptime**: `0.90` over a rolling 14-day window
- **fee_credits_churn_penalty_days**: `3`
  - If a node drops out, it must wait before accruing again (discourages “join only when I’m using the network”).

Interpretation of the recommended numbers:
- An eligible node can accumulate up to ~30 days of typical credit accrual (`6000 / 200 = 30`).
- Daily spend cap (`300`) allows normal use but constrains burst spam.
- Small decay keeps credits circulating rather than hoarded forever.

#### Eligibility (high level)

Define what it means to be “in the pool” for credit accrual. Examples:

- Registered worker identity in good standing (per whatever worker/producer admission rules exist)
- Meets uptime and behavior requirements (not rate-limited/banned)
- Maintains a stable identity for long enough (warm-up / aging)

The exact anti-sybil gate is a separate launch-blocking item, but fee credits should be designed to work with it (time-based aging, good-standing, churn penalties).

#### Spending rule (how credits pay fees)

At apply-time (or admission-time), interpret payment as:

1) Compute `min_fee(tx)` as usual (still required).
2) Determine `fee_due = tx.fees` (must be \(\ge\) `min_fee(tx)`).
3) If `fee_credits_enabled` and sender has credits:
   - `credit_spend = min(fee_due, sender_credits, daily_spend_cap_remaining)`
   - debit `credit_spend` from sender’s credit balance
   - remaining `fee_due - credit_spend` is paid from the sender’s token balance (if any)
4) Fee routing (burn/route) applies to the total paid fee as usual.

**Important constraint:** credits should only pay fees for transactions authorized by the same identity (no delegation/transfer).

#### Abuse controls (recommended)

- **Warm-up**: no instant benefits for new identities.
- **Churn penalty**: prevents joining only when needing to transact.
- **Cap + decay**: prevents long-term hoarding.
- **Daily spend cap**: prevents credits being used for sustained spam.
- **Good standing**: repeated rate limits / misbehavior disables accrual/spend.

#### Invariants

- Credits are **non-transferable**.
- Credits cannot make `min_fee(tx)` optional; they only change the source of payment.
- Credit spending is deterministic and replay-safe (same tx cannot be “paid twice”).
- Credit balances are bounded: `0 <= credits <= max_balance`.

### 5) Rewards (validators/producers/workers)

If **Option A (fair launch constant emission)** is chosen, define precisely:

- **block_reward_tokens**: `1 TOKEN` (fixed)
- **reward_event**: “on successful cycle application” (i.e. when a new LSU is applied and `applied_cycle` increments)
- **reward_recipients_rule_v1**: **Rule 2 (split equally among the cycle producer set)**
- **reward_split_bps_v1**:
  - `producer_set_reward_bps = 7000` (70% of block reward)
  - `eligible_waiting_pool_reward_bps = 3000` (30% of block reward)
  - invariant: `producer_set_reward_bps + eligible_waiting_pool_reward_bps = 10000`

Fees are **on top of** fixed issuance:
- The 1 TOKEN/cycle is minted deterministically every successful cycle.
- A configured fraction of transaction fees (`fee_to_reward_pool_bps`) is additionally routed into rewards.
- Therefore, total operator compensation per cycle is:
\[
\text{operator\_payout} = \text{issuance\_reward} + \text{routed\_fees}
\]
As network transaction volume grows, routed fees are expected to become a larger share of total node economics.

- **reward_recipients_rule options** (for future revisions):

**Rule 1 — Cycle leader only (simplest)**  
Pick a deterministic leader for cycle \(n\) (e.g. producer set index 0, or the one that finalizes the LSU) and credit `1 TOKEN` to that leader’s account.

Pros: simplest implementation.  
Cons: concentrates rewards if leadership is sticky; incentives for leader targeting.

**Rule 2 — Split equally among the cycle’s producer set (recommended)**  
If the protocol defines a producer set for cycle \(n\), split the `1 TOKEN` equally across the set (integer division rules must be specified; remainder burned).

Pros: aligns “anyone running nodes” with broad distribution; less leader centralization.  
Cons: requires a well-defined producer set at apply-time; needs careful rounding rules.

**Rule 3 — Split among witnessed contributors**  
Split among the witness list / participants that contributed to finalization.

Pros: closer to “pay for participation”.  
Cons: more complex; needs stable witness definitions.

Security note (critical): If rewards are paid based on being selected as a producer, then **producer selection must be Sybil-resistant** (otherwise attackers can flood the worker pool and capture emissions). This spec therefore needs a clear anti-sybil gate, such as:
- stake/bond (not compatible with “no premine” unless bonds are earned over time),
- proof-of-resource / proof-of-work style gating for worker registration,
- strict per-IP/subnet caps + reputation + long-lived identity requirements,
- permissioned validator sets for phase-0 mainnet (less ideal for your stated goals).

Important: reward rules must map cleanly onto the existing consensus cycle model to stay deterministic.

### 6) Genesis funding (no premine)

- **genesis_supply_tokens**: `1 TOKEN` total supply at genesis
- **genesis_recipient**: `TBD` (who gets the 1 token?)
  - recommendation: a well-known “genesis owner” key that can only be used for operational actions, or an unspendable sink if the 1 token is purely symbolic.
  - if the 1 token is intended to fund initial tx fees, it must belong to a real key.

Recommendation:
- **Testnet**: keep the faucet mechanism for UX.
- **Mainnet**: faucet disabled; all supply comes from block rewards.

### 7) Mempool admission (anti-spam enforcement points)

Define where fees are enforced:

- **mempool_min_fee_enforced**: recommended `true` (reject at admission if `tx.fees < min_fee`)
- **apply_time_fee_enforced**: required `true` (final enforcement)
- **rate limits**: handled separately (P2P/RPC safety limits)

### 8) EVM fee mapping (when EVM gas becomes meaningful)

Current approach is protocol fee charging with EVM `gas_price = 0`. For v1:

- **evm_fee_mode**: recommended `FlatFeeByTxType` (use `min_fee_schedule_v1`)

Future (v2):
- **GasToFee**: charge based on gas used:
  - `fee_atoms = gas_used * gas_price_atoms_per_gas`
  - optional basefee dynamics (EIP-1559-like) if desired

## Recommended baseline (what I would start with)

### Public testnet baseline (simple + safe)

- **Supply**: use the same fair-launch emission model or keep testnet relaxed; faucet can exist for UX.
- **Fees**: keep current deterministic minimum fee schedule (small but non-zero).
- **Fee routing**: burn fees (no treasury).
- **Rewards**: can be enabled if you want testnet to match mainnet; otherwise testnet may diverge from mainnet economics.

### Early mainnet baseline (v1 mainnet)

Per your goals, recommended v1 mainnet baseline:
- Choose **Option A (fair launch constant emission)**.
- Set `genesis_supply_tokens = 1` and `block_reward_tokens = 1`.
- Set average cycle duration target to **20 seconds** (about **1,576,800 blocks/year**).
- Enable fees with conservative floors (anti-spam); route `70%` to burn and `30%` to rewards.
- Use **reward rule 2** (split across producer set), with 70/30 split between producer set and eligible waiting pool.
- Enable fee credits with warm-up, cap, decay, and daily spend limits.
- Prioritize Sybil-resistance in worker registration / producer selection as a launch blocker (otherwise emissions can be captured cheaply).

## Invariants (must be test-covered)

- **No negative balances** after applying any cycle.
- **Deterministic supply**:
  - total supply at cycle \(n\) is a pure function of genesis + all applied LSUs up to \(n\)
- **Fee determinism**:
  - `min_fee(tx)` is deterministic and versioned (changing it requires an explicit protocol bump)
- **Conservation**:
  - for transfers (excluding fees/rewards/issuance), sums net to zero
  - fees are either burned or routed deterministically; never “disappear”
- **Replay safety**:
  - same tx cannot be applied twice (nonce rules + txid indexing)

## Implementation mapping (how this becomes code)

- **Protocol constants and fee schedule**: `crates/catalyst-core/src/protocol.rs`
- **Apply-time accounting**: LSU application path (storage/node apply)
- **RPC surfaces**:
  - fee parameters (for wallets to estimate)
  - supply/issuance stats (for explorers)

## Open questions checklist (fill these in)

1) Token name/symbol/decimals: **KAT / 9 decimals**  
2) Genesis recipient of the 1 token (or sink): **operator wallet (to be configured)**  
3) Reward recipients rule (leader-only vs producer-set split vs witness split): **producer-set split (Rule 2)**  
4) Sybil-resistance mechanism for worker registration / producer selection: `TBD` (launch blocker)  
5) Do we want per-byte fees at launch? `TBD`  
6) Fee routing at launch: **70% burn / 30% reward pool / 0% treasury**  
7) Fee credits v1 parameters: **set in section 4.1 (warm-up 14d, accrual 200/day, cap 6000, decay 25 bps/day, spend cap 300/day, uptime >= 90%, churn penalty 3d**

