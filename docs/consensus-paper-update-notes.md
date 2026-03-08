## Consensus paper update notes (inputs for a refresh)

These notes are meant to help update the older PDFs:

- `CatalystConsensusPaper.pdf` (v1.2 “under review”, ~61 pages)
- `CatalystNetworkSecurityConsiderations.pdf` (~13 pages)

Goal: align the written spec with what is implemented today, and with the practical lessons learned since the paper was written.

---

## What is implemented today (high-signal summary)

### 1) Consensus is a 4-phase cycle protocol (in code)

The current Rust implementation models:

- Construction → Campaigning → Voting → Synchronization
- Producers broadcast one message per phase and collect peer messages within phase windows/timeouts.

Concrete message types in code:

- `ProducerQuantity`
- `ProducerCandidate`
- `ProducerVote`
- `ProducerOutput`

See: `crates/catalyst-consensus/src/consensus.rs` and `crates/catalyst-consensus/src/types.rs`.

### 2) Network realities are explicitly handled

Recent work in the Rust node repo includes:

- Dial backoff + jitter (WAN join reliability)
- Multi-hop rebroadcast + deduplication bounds (simple transport)
- Versioned message envelope wire format with explicit protocol versioning
- RPC and P2P safety limits (rate limits, message size caps)

This matters because the original paper assumes “sufficient message propagation” but does not specify the modern operational hardening required for WAN-scale networks.

### 3) Transaction and signing behavior has evolved

Modern tooling uses:

- CTX2 wire transactions
- Explicit chain domain separation for signature verification (`chain_id`, `genesis_hash`)
- A single-call domain fetch RPC (`catalyst_getTxDomain`) to avoid load-balancer skew

These details are essential for builders and wallet implementations, and should be reflected in an updated spec.

---

## Where the paper likely diverges from the current implementation

### A) Producer/worker selection details

The papers emphasize a large worker pool `N` and a selected producer set `P` per cycle, and derive security properties (hypergeometric distribution / probability of majority malicious producers).

What to refresh:

- Precisely define the *current* selection algorithm (inputs, randomness source, anti-sybil assumptions).
- Clarify the intended scaling model:
  - **unbounded pool** of potential workers
  - **bounded producer set per cycle** for consensus execution

### B) Threshold parameters (Cmin/Vmin/Umin, etc.)

The security considerations paper enumerates threshold parameters per phase and discusses confidence intervals and bloom filters.

What to refresh:

- Identify which thresholds are implemented as real, enforceable rules today.
- For thresholds not implemented, either:
  - mark as “future work”, or
  - update the paper to match the actual simplified rules used now (and why).

### C) Bloom filters / compressed producer lists

The paper discusses bloom filters for compact producer/voter lists.

What to refresh:

- Confirm whether current messages use bloom filters or simple hashes / lists.
- If only hashes are used, document how correctness is established and how reward eligibility is computed.

### D) DFS / storage details for ledger state updates

The paper ties consensus output to DFS content addresses and LSU distribution.

What to refresh:

- Current on-disk storage behavior, pruning, and operational maintenance commands.
- Snapshot/fast-sync strategy and how it relates to LSU availability.

---

## Suggested new sections for an updated 2026 paper

### 1) “Scaling model” section (explicit)

Define two scaling dimensions separately:

- **N: total network nodes** (anyone can run a node; join and serve/verify)
- **P: producers per cycle** (consensus participants selected algorithmically)

Explain why:

- consensus message complexity grows quickly with P
- but N can be very large if the P2P layer is robust and the producer set per cycle is bounded

### 2) “WAN hardening requirements”

Include the practical requirements for real networks:

- backoff/jitter dial logic
- message size caps, rate limits, dedup windows
- protocol version negotiation and wire compatibility
- handling partitions and churn

### 3) “Builder and wallet integration requirements”

The old paper is heavy on protocol internals but light on concrete modern integration surfaces.

Add:

- the canonical RPC transaction lifecycle
- chain domain separation and why a single-call domain fetch matters
- tx receipt statuses and client retry/backoff guidance

### 4) “Threat model refresh”

Add/refresh:

- DoS (RPC and P2P) and mitigation knobs
- eclipse/sybil realities for open networks
- operational security for node operators (backups/rollback)

---

## Concrete contribution you can paste into a “paper update” prompt

1) Update the paper’s “Implementation status” to reflect the Rust codebase realities:
   - 4-phase consensus exists, but production-scale behavior depends on WAN hardening and safety limits.
2) Add a “Scaling model” that distinguishes N from P and explicitly states that stress testing focuses on P (chosen producers per cycle) while N can be very large.
3) Add a “Transaction signing domain” subsection:
   - signatures are verified with chain domain separation, and tooling must use a single-call domain fetch to avoid backend skew.
4) Add a “Parameter governance” subsection:
   - timeouts and safety limits are configuration-bound and validated; changes should be coordinated upgrades.
5) Mark any paper sections not implemented as “future work” (bloom filters, some threshold derivations), or update them to match the current protocol.

