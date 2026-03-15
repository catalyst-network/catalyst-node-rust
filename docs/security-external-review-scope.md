# External security review scope and remediation checklist (mainnet)

This document defines the minimum external security review package required for mainnet launch readiness.

Tracking:

- Epic: `#262`
- Child ticket: `#273`

## Objective

Define a clear, auditable external review scope and a deterministic remediation workflow so findings can be triaged, fixed, verified, and signed off before launch.

## In-scope components for external review

Reviewers should focus on code paths that can cause consensus safety failures, liveness failures, or critical asset compromise.

### Protocol and consensus safety

- transaction validation and signature-domain handling
- nonce/replay enforcement
- deterministic consensus phase outputs and LSU construction
- LSU apply path and state-root continuity assumptions

Primary areas:

- `crates/catalyst-core`
- `crates/catalyst-consensus`
- `crates/catalyst-cli/src/node.rs` (LSU apply and runtime integration)

### Network and peer-layer security

- P2P message validation, limits, dedup/relay behavior
- peer lifecycle, bootstrap, and anti-eclipse posture
- malformed input and flood handling paths

Primary areas:

- `crates/catalyst-network`
- envelope/wire utilities in `crates/catalyst-utils`

### RPC and integrator-facing attack surface

- tx submission validation and abuse controls
- expensive query surfaces and endpoint-level throttling assumptions
- response integrity assumptions for explorers/indexers/wallets

Primary areas:

- `crates/catalyst-rpc`

### Storage, durability, and recovery

- RocksDB state persistence assumptions
- snapshot/restore and pruning safety boundaries
- rollback/recovery consistency risks

Primary areas:

- `crates/catalyst-storage`
- operational docs and runbooks in `docs/`

### Key management and operational controls

- key separation assumptions for validators/operators/faucet
- backup/rotation and compromise response expectations
- launch ceremony fail/abort criteria dependencies

Primary references:

- `docs/node-operator-guide.md`
- tickets `#278`, `#279`, `#280`

## Out-of-scope for this review pass

- deep cryptographic primitive design proofs beyond implemented usage checks
- enterprise perimeter/cloud hardening specific to one deployer
- legal/compliance review

Out-of-scope items can be tracked separately, but must not block launch-gate sign-off unless reclassified as critical.

## Severity model and launch policy

### Severity definitions

- **Critical**: consensus safety break, key compromise path, remote unauthenticated state corruption, or reliable chain-halting DoS.
- **High**: realistic exploitation with major liveness/economic integrity impact.
- **Medium**: meaningful weakness requiring non-trivial preconditions or limited blast radius.
- **Low**: hard-to-exploit or defense-in-depth improvements.

### Launch decision policy

- **Critical**: must be fixed and re-verified before mainnet launch.
- **High**: must be fixed, or explicitly accepted by leadership with written rationale and compensating controls.
- **Medium/Low**: may be scheduled post-launch with owner and deadline.

## Required review deliverables

The external reviewer package must include:

1. Scope and methodology summary.
2. Finding list with severity and exploit preconditions.
3. Reproduction steps or PoC details for each finding.
4. Suggested remediation direction.
5. Residual-risk statement after retest.

## Internal remediation workflow (checklist)

For each finding:

1. Create/associate a GitHub issue with severity label and owner.
2. Link affected code paths and commit references.
3. Implement fix in a reviewable PR.
4. Add/extend deterministic regression tests.
5. Run relevant package tests and checks.
6. Document behavior changes in `docs/` when externally visible.
7. Request reviewer retest or internal adversarial confirmation.
8. Mark as resolved only with evidence attached.

## Evidence requirements before closing `#273`

- reviewer scope document attached/linked
- findings table with statuses (open/fixed/accepted)
- all Critical findings resolved
- High findings resolved or explicitly accepted with mitigation notes
- remediation commits and test evidence linked per finding

## Handoff to `#272`

Any finding that requires adversarial validation must be mapped into executable scenarios in `docs/adversarial-test-plan.md` and tracked in `#272`.
