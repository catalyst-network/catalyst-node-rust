# Security threat model (mainnet v1 baseline)

This document captures the security assumptions and threat model for mainnet readiness.
It is the canonical input for:

- `#262` epic: security threat model and adversarial test gate
- `#271`: publish mainnet threat model and attack assumptions
- `#272`: adversarial test execution evidence
- `#273`: external review scope and remediation checklist

## Security goals

- **Safety**: honest nodes reject invalid state transitions and preserve deterministic state convergence.
- **Liveness**: the network continues advancing applied cycles under WAN conditions and bounded adversarial pressure.
- **Resource bounds**: untrusted traffic cannot cause unbounded CPU, memory, disk, or connection growth.
- **Recoverability**: operators can restore/rejoin safely after failures without silent state drift.

## Assumptions and trust boundaries

### Assumptions

- At least one honest connectivity path exists between non-faulty nodes over time.
- Honest operators run released binaries with intended configuration.
- Private keys for validator/operator identities are not broadly compromised.
- Adversaries can control public internet traffic to their own endpoints but not global routing everywhere at once.

### Trust boundaries

- **Consensus/state boundary**: LSU validation and deterministic application path.
- **P2P boundary**: all inbound peers and gossip messages are untrusted by default.
- **RPC boundary**: all client requests are untrusted, including transaction submissions.
- **Storage boundary**: local RocksDB state/snapshots are trusted only with integrity checks and operational controls.
- **Operator boundary**: third-party RPC/indexer operators may be malicious or faulty.

### Protected assets

- state root continuity and LSU chain integrity
- account balances/nonces and replay safety
- node availability and cycle progression
- key material and launch configuration
- operator runbooks and recovery artifacts (snapshots/backups)

## Adversary classes

- **Remote internet attacker**: sends arbitrary high-rate traffic to public P2P/RPC surfaces.
- **Sybil attacker**: creates many identities/peers to bias connectivity or economic eligibility.
- **Eclipse attacker**: attempts to isolate target nodes behind attacker-controlled peers.
- **Partition attacker/event**: creates or exploits temporary network splits.
- **Malicious integrator/operator**: serves incorrect RPC/indexer views or withholds data.

## Threat matrix (v1)

### 1) P2P message flood / oversized payload DoS

- **Impact**: CPU/memory exhaustion, degraded liveness.
- **Current controls**:
  - message and payload limits
  - relay/dedup cache bounds
  - hop/TTL constraints and rate budgets
- **Verification path**: adversarial load scenarios in `#272`.

### 2) Rebroadcast amplification / cache-bloat DoS

- **Impact**: memory growth and excessive relay overhead.
- **Current controls**:
  - bounded relay cache and dedup tables
  - deterministic relay suppression
- **Verification path**: flood + amplification scenarios in `#272`.

### 3) Replay / downgrade / wire-compat confusion

- **Impact**: acceptance divergence, node instability.
- **Current controls**:
  - envelope versioning (`CENV` + protocol version)
  - identify protocol gating (`catalyst/1`)
  - nonce and signature domain checks on tx path
- **Verification path**: malformed/replay suites in `#272`.

### 4) Eclipse / peer-set capture

- **Impact**: liveness stall, delayed convergence, manipulated data view.
- **Current controls (partial)**:
  - bootstrap + DNS seed support
  - min-peer maintenance with backoff/jitter
- **Residual risk**:
  - stronger peer diversity and scoring still needed
- **Verification path**: eclipse scenarios in `#272`; external review in `#273`.

### 5) Sybil pressure on economic eligibility

- **Impact**: unfair reward capture and fee-credit abuse.
- **Current controls**:
  - waiting-pool eligibility gates (identity warmup + churn penalty)
- **Residual risk**:
  - this is baseline protection, not full economic Sybil immunity
- **Verification path**: adversarial eligibility abuse cases in `#272`.

### 6) Network partition and heal

- **Impact**: temporary liveness loss, reconciliation stress on rejoin.
- **Current controls**:
  - backfill/reliable-join continuity checks
  - bounded gossip behavior
- **Verification path**: partition/heal drills with evidence in `#272`.

### 7) Storage corruption / rollback hazards

- **Impact**: node failure, potential operator recovery mistakes.
- **Current controls**:
  - snapshot restore paths
  - pruning boundaries and maintenance tools
  - storage version markers
- **Verification path**: reset/recovery drills under `#275`, plus security review in `#273`.

### 8) RPC abuse and expensive query pressure

- **Impact**: degraded node responsiveness and potential liveness side-effects.
- **Current controls**:
  - baseline request limiting exists
- **Residual risk**:
  - endpoint-level shaping and stricter cost controls need continuous hardening
- **Verification path**: abuse workloads in `#272`.

## Residual risks accepted for v1 (explicit)

- Global internet-scale DDoS resistance is out of scope for protocol code alone.
- Economic Sybil resistance is improved but not considered fully solved.
- Public RPC/indexer trust remains best-effort; clients should use multiple sources and proofs where available.

## Mainnet security gate (definition of done)

To satisfy `#262` before launch:

1. Threat model and assumptions are published and reviewed (`#271`).
2. Adversarial test plan is executed with reproducible evidence (`#272`).
3. External review scope and remediation workflow are defined (`#273`).
4. Residual risks are explicitly accepted or mitigated with owners and timelines.

