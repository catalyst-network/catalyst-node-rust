### Catalyst Node — implementation backlog (spec-aligned)

References:
- `CatalystConsensusPaper.pdf`: `https://catalystnet.org/media/CatalystConsensusPaper.pdf`
- `IntroductionToCatalystNetwork.pdf`: `https://catalystnet.org/media/IntroductionToCatalystNetwork.pdf`

This repo now **builds all crates** on Linux and has a **spec-shaped core** to build on (`crates/catalyst-core/src/protocol.rs`) plus a compiled 4‑phase consensus engine scaffold (`crates/catalyst-consensus/`).

The remaining work is to replace stubs/scaffolds with protocol-faithful behavior and harden it with tests.

## Public Testnet Readiness (MVP) — “stable + safe + observable”

This section turns the spec-aligned milestones below into a **public-testnet-ready** checklist. The local 3‑node harness (`make testnet`, `scripts/netctl.sh`, `scripts/smoke_testnet.sh`) is a great functional baseline, but public testnet needs stronger correctness, resilience, and ops defaults.

### Inputs we need from the spec (please confirm)

- **Consensus spec version(s)**: link + revision/date (paper references currently say “v1.2”).
- **Hashing / PRNG**: exact hash functions and any domain separation for \(r_{n+1}\) (producer selection) and Merkle roots.
- **Wire encoding**: canonical encoding/versioning (bincode/protobuf/custom) and hashing preimages.
- **Transaction semantics**: fees, lock-time semantics, signature scheme/aggregation timeline, confidential transfer validation hooks.
- **State semantics**: exact account model, nonce rules, LSU application ordering, and state root definition.

### MVP acceptance criteria (public testnet)

- **Safety**: nodes reject invalid txs/LSUs (signature/nonce/state-root continuity); no “accept anything” paths on the public surface.
- **Determinism**: for the same cycle membership + tx set, producers converge on the same `first_hash` and LSU hash.
- **Sync**: a fresh node can join, fetch recent LSUs via DFS/CID or peer fallback, and reach the same state root.
- **Persistence**: restart does not lose pending txs; applied head/state root persisted and served over RPC.
- **Observability**: logs + basic metrics for cycle/phase timings, peer count, mempool size, and apply failures.
- **Operational defaults**: safe RPC bind defaults, rate limits, sane config templates, and documented ports.

### Immediate “ticket-sized” work items (seed list)

- **Unify protocol transaction flow end-to-end**
  - **What**: ensure consensus consumes protocol-shaped txs (or a deterministic, spec-defined projection), not ad-hoc `TransactionEntry` lists.
  - **Where**: `crates/catalyst-core/src/protocol.rs`, `crates/catalyst-consensus/src/*`, `crates/catalyst-cli/src/tx.rs`, `crates/catalyst-cli/src/node.rs`, `crates/catalyst-rpc/src/lib.rs`
  - **Acceptance**: network gossips a canonical tx object; Construction uses a deterministic, spec-defined derivation of `dn` and sorted entries.

- **Replace placeholder fee/majority/witness logic with spec-faithful rules**
  - **What**: implement the paper’s exact definitions for fee extraction, majority conditions, `Ln(prod)`, `Ln(vote)`, and witness generation/verification.
  - **Where**: `crates/catalyst-consensus/src/phases.rs`
  - **Acceptance**: 3-node testnet converges without leader “batch crutch” (or leader mechanism is explicitly in spec as a temporary testnet rule).

- **DFS integration hardening**
  - **What**: make LSU storage/retrieval/content addressing match the DFS spec and be robust to packet loss/startup ordering.
  - **Where**: `crates/catalyst-dfs`, `crates/catalyst-cli/src/dfs_store.rs`, `crates/catalyst-cli/src/node.rs`
  - **Acceptance**: node joining late can fetch LSU by CID and verify hash/state-root continuity.

- **State root / proof semantics**
  - **What**: define/lock the state root algorithm and proof format; ensure `catalyst_getBalanceProof` is stable and verifiable.
  - **Where**: `crates/catalyst-storage/src/{merkle.rs,sparse_merkle.rs,manager.rs}`, `crates/catalyst-rpc/src/lib.rs`, `crates/catalyst-cli/src/commands.rs`
  - **Acceptance**: proof verifies client-side and matches the root returned by `catalyst_head`.

- **Public testnet configuration + runbook**
  - **What**: provide a minimal “public testnet node” config, documented ports, bootstrap procedure, and safe RPC defaults.
  - **Where**: `catalyst.toml`, `scripts/netctl.sh`, `README.md`, `crates/catalyst-config/configs/*`
  - **Acceptance**: a new operator can run a node, connect to bootstrap peers, and query head/peers via RPC.

## Milestone 0 — Protocol primitives and boundaries (foundation)

- **Protocol types (done starter)**
  - **What**: Spec-shaped types for transactions, ledger updates, producer selection inputs/outputs.
  - **Where**: `crates/catalyst-core/src/protocol.rs`
  - **Next**:
    - Replace placeholder PRNG (`sha2`) with the paper’s exact hash/PRNG scheme once `catalyst-crypto` defines it.
    - Add validation functions: signature checks, confidential amount verification hooks.

- **Message codec boundary**
  - **What**: Decide which structs are “wire types” vs “internal types”.
  - **Where**: `crates/catalyst-utils/src/network.rs`, `crates/catalyst-consensus/src/types.rs`
  - **Next**:
    - Define canonical encoding (bincode vs custom) and versioning strategy.
    - Add round-trip tests for each message type.

## Milestone 1 — Transaction pipeline (mempool → cycle input)

- **Mempool**
  - **What**: Accept transactions from RPC/network, validate basic rules, dedupe, apply freeze window.
  - **Where**: likely `crates/catalyst-core` (trait), `crates/catalyst-storage` (persistence), `crates/catalyst-rpc` (API)
  - **Spec**: Consensus paper §4.2–§4.5 and §5.2.1 (freeze window).
  - **Tickets**:
    - Create `Mempool` trait in `catalyst-core`.
    - Implement in `catalyst-storage` (RocksDB-backed).
    - Add `submit_tx`, `pending_txs` RPC endpoints in `catalyst-rpc`.

- **Transaction verification**
  - **What**: Implement signature verification, lock-time rules, fee accounting.
  - **Where**: `crates/catalyst-crypto`, `crates/catalyst-core/src/protocol.rs`
  - **Spec**: §4.4 and §4.5.
  - **Tickets**:
    - Implement aggregated signature verification (MuSig) for the `Transaction` core.
    - Add hooks for confidential transfer validation (Pedersen + range proofs).

## Milestone 2 — Worker pool + producer selection (Cn → Cn+1)

- **Worker registration / DHT pool**
  - **What**: Maintain worker pool partitioned by region & size, publish/refresh, remove stale.
  - **Where**: `crates/catalyst-network`, `crates/catalyst-core`, `crates/catalyst-storage`
  - **Spec**: Consensus paper §2.2.1 (worker pool / DHT queue).
  - **Tickets**:
    - Define `WorkerRecord` and persistence schema.
    - Networking messages: `WorkerRegistration`, `WorkerHeartbeat`, `WorkerSelection`.

- **Producer selection (done starter)**
  - **What**: Deterministically select producers via `Id_i XOR r_{n+1}` sorting.
  - **Where**: `crates/catalyst-core/src/protocol.rs` (`select_producers_for_next_cycle`)
  - **Spec**: §2.2.1
  - **Next**:
    - Feed PRNG seed from previous cycle LSU Merkle root (store & retrieve via storage).
    - Add property tests: determinism, stability, boundary conditions.

## Milestone 3 — 4‑phase collaborative consensus (protocol-faithful)

The current engine compiles and has the correct phase breakdown, but much of the logic is still placeholder.

- **Construction phase (ΔLn,j)**
  - **What**: From frozen mempool set, build partial LSU with sorted entries + tx-signature root.
  - **Where**: `crates/catalyst-consensus/src/phases.rs` (`ConstructionPhase`)
  - **Spec**: §5.2.1
  - **Tickets**:
    - Correct fee calculation and entry semantics (paper vs current placeholder).
    - Use real transaction format and derive `dn` from transaction signature hashes (paper’s structure).

- **Campaigning phase (cj)**
  - **What**: Identify majority `hj` and construct witness list `Ln(prod)` and hash tree witness `t(j)`.
  - **Where**: `crates/catalyst-consensus/src/phases.rs` (`CampaigningPhase`)
  - **Spec**: §5.2.2
  - **Tickets**:
    - Implement exact majority check and witness generation.
    - Ensure deterministic ordering and validation rules for received quantities.

- **Voting phase (vj / ΔLn)**
  - **What**: Build full LSU with compensation entries, vote, and produce `vj`.
  - **Where**: `crates/catalyst-consensus/src/phases.rs` (`VotingPhase`)
  - **Spec**: §5.2.3
  - **Tickets**:
    - Implement compensation rules (paper §4.3 / §5.2.3).
    - Implement validation of `cj` and LSU hash calculation.

- **Synchronisation phase (oj + DFS)**
  - **What**: Store LSU to DFS, broadcast content address, finalize `Ln(vote)`.
  - **Where**: `crates/catalyst-consensus/src/phases.rs` (`SynchronizationPhase`), `crates/catalyst-dfs`
  - **Spec**: §5.2.4 + §6
  - **Tickets**:
    - Replace placeholder DFS address derivation with `catalyst-dfs` integration (IPFS or equivalent).
    - Add LSU fetch/verify path for late joiners.

## Milestone 4 — State application + chain service surfaces

- **Apply LSU to account/state**
  - **What**: Convert LSU entries into state transitions, update Merkle roots, persist.
  - **Where**: `crates/catalyst-storage`
  - **Spec**: Ledger state semantics in both PDFs.

- **APIs**
  - **What**: RPC endpoints for status, peers, submit tx, get tx, get latest LSU / cycle info.
  - **Where**: `crates/catalyst-rpc`, `crates/catalyst-cli`

## Milestone 5 — Security + testing

- **Deterministic simulation tests**
  - **What**: Spin up N in-process nodes, run cycles, assert convergence and consistency.
  - **Where**: `crates/catalyst-consensus/tests.rs` + an integration test harness crate.

- **Fuzz/property tests**
  - **What**: Message parser fuzzing; adversarial inputs; partial network partitions.

- **Interop + versioning**
  - **What**: Wire format stability and protocol versioning policy.

