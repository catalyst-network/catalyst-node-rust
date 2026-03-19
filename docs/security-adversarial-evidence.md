# Security adversarial evidence (mainnet gate)

This file records reproducible execution evidence for `#272`.

Scope:

- CI-suitable adversarial checks
- status of WAN/chaos adversarial scenarios

## Run context

- Repository: `catalyst-network/catalyst-node-rust`
- Branch: `main`
- Environment: local Linux dev host
- Compiler override used for consistency with prior runs:
  - `CC=gcc-13`
  - `CXX=g++-13`

## Executed CI-suitable scenarios

### Track A refresh (latest local rerun)

- **Date**: 2026-03-08
- **Execution mode**: local scripted run (CI-safe)
- **Aggregate result**: all listed Track A scenarios passed again with the same deterministic outcomes.

### 1) Envelope wire version rejection

- **Command**
  - `CC=gcc-13 CXX=g++-13 cargo test -p catalyst-utils test_envelope_wire_version_mismatch_rejected`
- **Result**
  - pass (`1 passed, 0 failed`)
- **Coverage intent**
  - verifies unknown envelope protocol versions are rejected deterministically

### 2) Hop/loop bound behavior

- **Command**
  - `CC=gcc-13 CXX=g++-13 cargo test -p catalyst-utils test_routing_info`
- **Result**
  - pass (`1 passed, 0 failed`)
- **Coverage intent**
  - verifies routing hop accounting and loop detection semantics

### 3) Simple transport budget enforcement

- **Commands**
  - `CC=gcc-13 CXX=g++-13 cargo test -p catalyst-network simple::tests::conn_budget_enforces_msgs_and_bytes`
  - `CC=gcc-13 CXX=g++-13 cargo test -p catalyst-network simple::tests::backoff_and_jitter_are_bounded`
- **Result**
  - pass (`1 passed, 0 failed`) for each
- **Coverage intent**
  - verifies per-connection message/byte budget limits
  - verifies dial backoff/jitter clamp behavior

### 4) Service transport budget enforcement

- **Commands**
  - `CC=gcc-13 CXX=g++-13 cargo test -p catalyst-network service::tests::peer_budget_enforces_msgs_and_bytes`
  - `CC=gcc-13 CXX=g++-13 cargo test -p catalyst-network service::tests::backoff_and_jitter_are_bounded`
- **Result**
  - pass (`1 passed, 0 failed`) for each
- **Coverage intent**
  - verifies peer-level budget caps in service path
  - verifies service-side backoff/jitter bounds

## WAN/chaos scenarios status

These scenarios are defined in `docs/adversarial-test-plan.md` and remain required for full `#272` closure:

- eclipse attempt and recovery
- sybil pressure under constrained peer diversity
- partition + heal convergence validation
- high-QPS mixed-payload DoS flood with resource/liveness metrics

Status: **pending execution evidence**.

Operator decision for Track B:

- run WAN/chaos scenarios on the currently unannounced public test network (no external users observed)
- reset the network state after disruptive tests complete

## Evidence gap summary

- CI-suitable adversarial checks: initial baseline evidence captured.
- WAN adversarial checks: not yet executed in this artifact.
- `#272` should remain open until WAN evidence and metrics are attached.
