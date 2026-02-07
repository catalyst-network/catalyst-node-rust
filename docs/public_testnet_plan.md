### Catalyst — Public Testnet Plan (branch: `network-upgrade`)

References:
- Consensus: [CatalystConsensusPaper.pdf (v1.2)](https://catalystnet.org/media/CatalystConsensusPaper.pdf)
- Network overview: [IntroductionToCatalystNetwork.pdf](https://catalystnet.org/media/IntroductionToCatalystNetwork.pdf)

This doc turns the spec into **implementation milestones and ticket-sized tasks** aimed at a **stable public test network**, while keeping the existing local dev/testnet harness working (`make testnet`, `make smoke-testnet`).

## Definitions

- **Public testnet**: long-running, externally reachable P2P + RPC endpoints, multiple independent operators, resilient to restarts and message loss, safe against invalid input.
- **Local devnet/testnet**: fast iteration harness for developers (can use shortcuts as long as they don’t leak into public defaults).

## Non-negotiable acceptance criteria (public testnet MVP)

- **Safety**:
  - Invalid transactions are rejected (signature + nonce + lock-time + balance/fees).
  - Invalid LSUs are rejected (hash mismatch, state-root continuity mismatch, invalid proof bundle, etc.).
  - No unauthenticated “accept anything” RPC paths on default config.
- **Determinism / convergence**:
  - For the same cycle membership + eligible tx set, producers converge on the same `first_hash` and LSU hash.
- **Sync**:
  - A fresh node can join and reach the same head/state root by fetching LSUs via DFS/CID (or peer fallback).
- **Persistence**:
  - Restart does not lose pending txs; head/state root are persisted and served over RPC.
- **Operational quality**:
  - Documented ports, bootstrap procedure, and safe RPC bind defaults; basic observability exists (logs/metrics).

## Milestone A — Spec pinning + canonical boundaries (must do first)

Goal: remove ambiguity around hashing preimages, encodings, and what is “wire vs internal”.

Tickets:
- **A1. Spec version pinning**
  - Capture spec revision/date and any known errata or amendments.
  - Add section references for producer selection, tx validity, and the 4 phases.
- **A2. Canonical encoding strategy**
  - Decide canonical encoding for hashing and network messages (and versioning rules).
  - Add round-trip tests for each wire message.
- **A3. Hashing + PRNG**
  - Implement the paper’s exact hash/PRNG scheme (domain separation included).
  - Replace placeholder PRNGs in producer selection and other consensus-critical hashing.

## Milestone B — Transaction pipeline (mempool → cycle input)

Goal: only valid, non-replayable transactions can influence consensus.

Tickets:
- **B1. Canonical transaction type**
  - Standardize on `catalyst-core::protocol::Transaction` for RPC/network submission.
  - Define how consensus derives “construction inputs” (entries, `dn`, fee accounting) from protocol txs.
- **B2. Mempool correctness**
  - Enforce: signature validity, nonce sequencing (per sender), lock-time, sufficient funds, fee rules.
  - Dedupe by txid (canonical hash), TTL/size limits, and persistence (RocksDB).
- **B3. Freeze window + inclusion rules**
  - Implement spec-faithful freeze window and deterministic tx selection order for a cycle.

## Milestone C — Membership / worker pool

Goal: producers are selected from a verifiable, spec-defined worker pool.

Tickets:
- **C1. Worker registration semantics**
  - Implement on-chain worker registration per spec (structure + validity rules).
  - Persist and query worker pool from state (not only config bootstrap).
- **C2. Producer selection**
  - Implement `Id_i XOR r_{n+1}` selection exactly as specified (seed source from previous LSU merkle/root as per spec).
  - Add determinism/property tests (same inputs → same producers; boundary conditions).

## Milestone D — 4-phase collaborative consensus (protocol-faithful)

Goal: replace placeholders in phases with spec-faithful logic and validation.

Tickets:
- **D1. Construction (ΔLn,j)**
  - Deterministic sorted entries; correct `dn` derivation and fee extraction per spec.
  - Ensure two honest producers with the same eligible tx set converge on identical `first_hash`.
- **D2. Campaigning (cj)**
  - Majority determination, witness generation, and validation rules per spec.
- **D3. Voting (vj / ΔLn)**
  - Compensation rules and deterministic ordering per spec.
  - Validate `cj` before producing vote.
- **D4. Synchronisation (oj + DFS)**
  - Store LSU to DFS, broadcast content address; support fetch/verify for late joiners.

## Milestone E — State application + authenticated queries

Goal: LSU application results in a stable authenticated state root; RPC proofs are verifiable.

Tickets:
- **E1. State root definition**
  - Lock the state root algorithm (and proof format) as the canonical chain commitment.
- **E2. Apply LSU**
  - Apply LSUs deterministically; persist head metadata; ensure nonce updates + balance updates are consistent.
- **E3. Proof-carrying RPC**
  - `catalyst_getBalanceProof` must verify client-side against `catalyst_head` state root.

## Milestone F — Public testnet operations + safety defaults

Goal: an operator can run a node safely and observe it.

Tickets:
- **F1. Config templates**
  - Provide public-testnet config templates and a minimal runbook (ports, bootstrap, persistence paths).
- **F2. RPC hardening**
  - Safe bind defaults; rate limit; CORS defaults; document exposure knobs.
- **F3. Observability**
  - Cycle/phase timings, peer counts, mempool size, apply failures logged/metrics exported.

## Milestone G — Testing + release gate

Goal: “public testnet ready” is a reproducible bar, not a feeling.

Tickets:
- **G1. Deterministic multi-node simulation tests**
  - In-process or harness-driven tests that assert convergence and state-root equality across nodes.
- **G2. Adversarial tests**
  - Invalid txs, wrong nonces, wrong roots, message reordering/loss, restart scenarios.
- **G3. Release gate**
  - A single command CI/local run that executes the public-testnet readiness suite.

