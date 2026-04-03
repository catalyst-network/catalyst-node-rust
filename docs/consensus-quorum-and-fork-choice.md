# Consensus quorum, finality, and fork choice (requirements)

This document captures **protocol intent** and **implementation gaps** for Catalyst validator networks. **Partial implementation (MVP):** follower apply paths in `crates/catalyst-cli/src/node.rs` now attempt **BFT vote quorum** on `LedgerStateUpdate.vote_list` vs `producer_list` and, when `prev_state_root` mismatches, may **purge mutable state (preserving `protocol:*` and stored per-cycle LSU metadata), replay** from `consensus:lsu:1..C-1`, then **apply** the quorum LSU—bounded by `CATALYST_MAX_REPLAY_CYCLES` (default `1000000`, `0` disables). This is **not** a substitute for checkpoints or state sync at very large heights; certified signature artifacts and full fork-choice remain future work.

## Problem statement

Operators have observed that validators can **diverge** (same logical cycle/height, different `state_root`) and **do not self-heal**. Follower logic today largely **skips** LSUs when `prev_state_root` does not match local storage (`prev_root mismatch`), without **reorganizing** to a **quorum-finalized** history.

For production and for growing validator sets, the network must:

1. **Finalize** ledger updates only when a **defined quorum** of block producers agrees.
2. **Treat quorum-finalized history as authoritative** for all nodes (including lagging or partitioned replicas).
3. **Reconcile** (reorg or state reset to a safe checkpoint) when local state **conflicts** with quorum-finalized updates—not **skip indefinitely**.

Snapshot-based manual restore remains a **break-glass** operational tool, not the liveness mechanism.

## Quorum size (scaling producer count)

We target **Byzantine fault tolerance** semantics: tolerate up to **f** faulty producers among **n**, with **n ≥ 3f + 1**, using a quorum of **⌈2n/3⌉** agreeing votes (equivalent to **2f + 1** in the classical 3f+1 layout).

Examples (quorum = **⌈2n/3⌉**):

| Producers (n) | Quorum (⌈2n/3⌉) | Notes |
|---------------|------------------|--------|
| 3 | 2 | Minimal BFT committee; 1 fault |
| 4 | 3 | |
| 5 | 4 | Stricter than bare majority (3); matches BFT supermajority |
| 6 | 4 | |
| 7 | 5 | |

**Not** the rule: fixed “50% + 1” only, which for n=5 would be 3 and would **not** match the intended BFT threshold.

The **exact** quorum formula, how **inactive** or **jailed** producers affect **n**, and how **epoch boundaries** rotate the set must be specified in protocol code and kept consistent with this table.

## Finality and fork choice (normative intent)

- **Finality**: A ledger state update (LSU) at a given cycle/height is **final** only if accompanied by (or derivable from) evidence that **≥ quorum** of the **current producer set** committed to it (signatures, vote certificates, or equivalent—format TBD in implementation).

- **Fork choice**: Each node’s **canonical chain** is the **prefix** of LSUs that is **compatible** with the **highest** (or **most recently finalized**) **quorum-certified** head, not merely the longest locally applied prefix that skips conflicting gossip.

- **Follower reconciliation**: If the node receives a **quorum-finalized** LSU whose **`prev_state_root`** does not match local storage:

  - The node **must not** permanently ignore that update solely because of mismatch.
  - The node **must** either:
    - **Roll back** local state to the certified parent and **apply** the finalized LSU (and any subsequent finalized updates), or
    - **Fetch and apply** a **contiguous certified prefix** from peers / archive that bridges local head to the finalized head, or
    - In extreme cases, **reset** from a **checkpoint** that is **at or before** the last mutually agreed finalized state, then fast-forward (implementation-defined, but **must** converge to the same finalized history as honest quorum members).

- **Safety**: Two **conflicting** LSUs at the **same** cycle/height must **not** both be final unless the protocol is broken; quorum logic must make **equivocation** by &lt; quorum producers **non-finalizing**.

## Relationship to current implementation (gap analysis)

Today (as of the discussions that motivated this doc):

- **Cycle** is partly driven by **wall-clock** (`current_timestamp_ms() / cycle_ms`), which interacts with producer selection and leader batching; this needs **audit** for skew and restart edge cases.
- **Followers** previously **skipped** applies on `prev_root mismatch`; they now **may** auto-reconcile via quorum + bounded replay as above when history is present locally (still **no** cryptographic finality certificate—quorum is inferred from LSU fields only).
- **Operational recovery** (snapshot, `db-restore`, copying `data/`) is required to realign forked validators—acceptable as **ops**, not as **the** liveness story.

Implementation work should:

1. Define **certified finality** artifacts for LSUs (what is signed, by whom, over what payload).
2. Wire **apply path** to **prefer** or **require** finality evidence before treating a branch as canonical.
3. Implement **rollback / reorg / checkpoint sync** bounded by safety rules.
4. Add **tests**: partition, clock skew, minority equivocation, and **post-rejoin convergence** to one `state_root` at each height.

## Testnet deployment gate

Changes implementing this document should be:

- Covered by **automated** multi-node tests where possible.
- Deployed to **testnet first**; operators verify **no sustained `prev_root mismatch` split** under injected faults and that **reconvergence** occurs without manual snapshot restore for **defined** scenarios.

## References (internal)

- `crates/catalyst-cli/src/node.rs` — consensus loop, `prev_root mismatch`, `apply_lsu_to_storage`, wall-clock `cycle`.
- `docs/node-operator-guide.md` — validator set bootstrap and operations.

---

*Document status: **requirements / roadmap**. Implementation and commit timing are at project discretion.*
