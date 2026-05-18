# Consensus implementation completion checklist

This document is the **engineering gate** for calling the Catalyst **multi-validator** consensus path *feature-complete* for serious testnets and eventual scale-up. It assumes the collaborative four-phase model described in the Catalyst consensus paper (v1.2) and tracks **gaps between that intent and this repository**.

**Strategy (agreed):** complete and verify the implementation against this checklist first; **reopen protocol design** only if honest-network failures remain after these items are satisfied.

**Related docs**

- Requirements and gaps: [`consensus-quorum-and-fork-choice.md`](./consensus-quorum-and-fork-choice.md)
- Paper vs code drift: [`consensus-paper-update-notes.md`](./consensus-paper-update-notes.md)
- Cryptographic finality (target): [`adr/0001-lsu-finality-certificate.md`](./adr/0001-lsu-finality-certificate.md)
- WAN / chaos gate (operational): [`wan-soak-load-chaos-gate.md`](./wan-soak-load-chaos-gate.md)
- Wall clock & time sync: [`consensus-wall-clock-and-time.md`](./consensus-wall-clock-and-time.md)
- Consensus env knobs (runtime): [`protocol-params.md`](./protocol-params.md)

### Repo map (where to look)

| Topic | Primary location |
|-------|------------------|
| Tx batch leader / follower, `TxBatchControl`, reconcile prefetch | `crates/catalyst-cli/src/node.rs` |
| `TxBatchControl` wire encoding | `crates/catalyst-cli/src/tx.rs` |
| Env limits, wall-clock, tx-batch wait budget, commit blob vs batch, LSU P2P wall-lead cap | `crates/catalyst-cli/src/consensus_limits.rs` |
| Pruning + metadata keys (`tx_batch_commit`, `META_PRUNED_UP_TO_CYCLE`) | `crates/catalyst-cli/src/pruning.rs` |
| Four-phase collection, BFT vote quorum | `crates/catalyst-consensus/src/consensus.rs` |
| In-process multi-validator sim (LSU convergence + `produce_single_cycle_converged_lsu`) | `crates/catalyst-consensus/src/convergence_test_support.rs`, `crates/catalyst-consensus/tests/convergence_harness.rs` |
| `LsuFinalityCertificateV1` verify + unit tests | `crates/catalyst-consensus/src/lsu_finality.rs` |
| Operator runbook (batch loss, pruning vs replay) | `docs/node-operator-guide.md` |
| Phase 0 consensus test slice (CI parity helper) | `make consensus-checklist-tests`, `scripts/run-consensus-ci-parity.sh` |

---

## Phased execution plan (multi-session / long runs)

Use this when chipping away at the checklist over days or across agents. **Each phase should end with `make consensus-checklist-tests` (or `scripts/run-consensus-ci-parity.sh`) green** before starting dependent work.

| Phase | Goal | Primary deliverables | Depends on |
|-------|------|----------------------|------------|
| **0 — Baseline** | Repeatable “known good” test slice for consensus work | `make consensus-checklist-tests` (runs `catalyst-cli`, `catalyst-consensus` lib tests, **`--test convergence_harness`**, `catalyst-core` `protocol::`); `scripts/run-consensus-ci-parity.sh` | — |
| **1 — §7.2 core** | Prove follower **stall vs recover** logic without full libp2p | Pure helpers in `consensus_limits.rs`; **`node::tx_batch_follower_wait_tests`** (`node.rs`): follower returns **`None`** when `Commit` has `tx_count > 0` but no batch/recovery, **`Some([])`** when `tx_count == 0`, leader miss **`None`**; mid-wait batch arrival returns **`Some(entries)`**; `wait_for_tx_construction_entries(..., network: Option<…>)` skips resync when `None` (tests only). **Still TBD:** multi-node libp2p / gossipsim that drops `ProtocolTxBatch` for one follower only. | Phase 0 |
| **2 — §7.1 storage** | Same head **applied** `state_root` across nodes | **Partial:** `node::storage_state_root_convergence_tests` — three isolated RocksDB dirs, same genesis + `apply_lsu_to_storage` on identical converged LSUs per cycle (`produce_single_cycle_converged_lsu`). **Still TBD:** end-to-end multi-process node + protocol tx pipeline. | Phase 0–1 |
| **3 — §7.3–7.4** | Partition + clock skew | Libp2p testnet fixture or fault injection; skew = override `current_timestamp_ms` in test build or DI clock behind `cfg(test)` | Phase 1 |
| **4 — §4.1–4.2** | Fork choice + pruned replay without archival | Short ADR update; checkpoint or state-sync design; implement minimal checkpoint replay before quorum reconcile | Human/protocol sign-off for fork-choice rule |
| **5 — §2.2** | Wrong-cycle buffer/drop | **Partial:** four-phase collect paths use `phase_body_matches_current_cycle` in `consensus.rs` (doc + `phase_body_cycle_gate_rejects_wrong_cycle` unit test); wrong **type** → `pending_envelopes`; wrong **cycle** same type → drop. **`node.rs`:** tx-batch slack + LSU far-future wall-lead cap on P2P ingress (see `consensus_limits.rs`). **Still TBD:** exhaustive audit for remaining `MessageType`s (e.g. `Transaction` cycle coupling). | Phase 0 |
| **6 — §6.1 metrics** | Counters/histograms alongside tracing targets | **Partial:** P2P tx-batch ingress counters registered at CLI start + increments on cycle reject / commit disagree (`node.rs`, `main.rs`). Full histograms / reconcile / prev_root metrics TBD | Phase 0 |
| **7 — §8.3 rollout** | Default `require_lsu_finality = true` for validators | **Done (2026-05-16):** `NodeConfig` default `true`; testnets set `false` or `CATALYST_REQUIRE_LSU_FINALITY=0` for migration | Phases 1–3 |

**Parallel tracks:** Phase **5** and **6** can run beside **1**; **4** needs design before coding; **7** is last.

**Agents:** One agent can execute phases **0→1→2→3** sequentially. Use **multiple agents** only to parallelize **independent** tracks (e.g. one on Phase 6 metrics, one on Phase 5 audit) and merge behind the same CI gate.

---

## How to use this checklist

- Each major section has **must-have** items (blocking) and **should-have** items (strongly recommended before mainnet-scale trust).
- Tick boxes in your issue tracker or a private copy; keep this file as the **canonical list** in git.
- **Small committees (e.g. P = 3)** validate **liveness and implementation correctness**, not the large-*N* statistics in Chapter 6 of the paper. Plan a later phase with larger *P* and *N* for economic/security margins.

---

## 1. Transaction availability and canonical ordering (blocking)

The paper assumes producers ultimately reason over a **shared** (or majority-consistent) view of which user transactions enter the cycle. Today the node uses a **batch leader** plus a **short follower wait**; a follower that misses gossip can run Construction with an **empty** tx list while peers do not, which forks state.

| # | Criterion | Notes / pointers |
|---|-----------|------------------|
| 1.1 | **No silent empty batch:** A validator that is a selected producer for cycle *C* must **not** apply a different tx input set than the rest of the committee without an explicit, observable **failure mode** (stall, round retry, or fetch) — never “timeout → empty → proceed.” | `crates/catalyst-cli/src/node.rs` (batch leader / follower wait / `ProtocolTxBatch`) |
| 1.2 | **Canonical source of txs:** Define one normative rule: e.g. leader proposes + **hash commitment**, followers **fetch** full batch from any peer that has it, or use a **small committee broadcast** (RBC-style) before Construction commits state. | Wire types: `ProtocolTxBatch`, `MessageEnvelope` |
| 1.3 | **Same ordering rule everywhere:** Lexicographic / hash-ordered entries (paper §5.2.1) must match **exactly** between mempool validation, batch serialization, and `TransactionEntry` construction. | `validate_and_select_protocol_txs_for_construction`, `phases.rs` |
| 1.4 | **Timeouts are part of the protocol:** Document and implement what happens when the canonical batch is unavailable (retry same cycle, empty block **only if all honest nodes agree** on empty, or leader rotation). | Phase timeouts in config; follower batch wait: `tx_batch_follower_wait_budget_default` / `CATALYST_TX_BATCH_WAIT_BUDGET_MS` in `consensus_limits.rs` |

---

## 2. Four-phase protocol and networking (blocking)

| # | Criterion | Notes / pointers |
|---|-----------|------------------|
| 2.1 | **Peer messages drive phases:** All selected producers **collect** peer `ProducerQuantity`, `ProducerCandidate`, `ProducerVote`, and `ProducerOutput` messages within phase windows; local-only fallbacks are **test-only** or explicitly flagged. | `crates/catalyst-consensus/src/consensus.rs` (`collect_producer_*`, `run_four_phase_consensus`) |
| 2.2 | **Phase/cycle alignment:** Inbound messages for **wrong** `cycle_number` are dropped or buffered per policy; no cross-cycle bleed that could commit wrong state. | **`consensus.rs`:** `phase_body_matches_current_cycle` gates all `collect_producer_*` bodies; wrong-cycle phase messages **dropped** (not buffered); other types buffered to `pending_envelopes`. Unit test `phase_body_cycle_gate_rejects_wrong_cycle`. **`node.rs`:** `TransactionBatch` + `TxBatchControl::Commit` gated vs wall cycle index with `CATALYST_P2P_TX_BATCH_MAX_CYCLE_SLACK` (`ResyncRequest` not gated); LSU-related types gated vs far-future wall lead with `CATALYST_P2P_LSU_MAX_WALL_LEAD`; further envelope/cycle policy per message family (ongoing). |
| 2.3 | **WAN-realistic windows:** Phase durations and batch delivery must exceed **p99 RTT + margin** for your target topology (multi-region). | `ConsensusEngineConfig`, `crates/catalyst-config` |
| 2.4 | **Remove or quarantine stale “WIP” assumptions:** Comments/TODOs that imply single-producer behavior must be false for release builds or guarded by explicit dev flags. | `consensus.rs` comments vs actual `network_receiver` wiring |

---

## 3. Finality evidence and apply path (blocking for “trust it”)

| # | Criterion | Notes / pointers |
|---|-----------|------------------|
| 3.1 | **Single apply rule:** A node applies an LSU to authenticated state **only if** it satisfies **continuity** (`prev_state_root`) **and** **finality policy** (quorum from signed certificate or equivalent — not “string lists only” long term). | `apply_lsu_to_storage`, `try_reconcile_fork_from_quorum_lsu`, ADR 0001 |
| 3.2 | **Implement ADR 0001 (or supersede it in writing):** Producers emit verifiable votes over **`H_cert`**; followers verify `LsuFinalityCertificateV1` before treating an LSU as canonical. **Config:** `[consensus] require_lsu_finality` + env override (`crates/catalyst-cli/src/consensus_limits.rs`); apply path uses `verify_lsu_finality_for_apply`. **`LsuGossip`:** when finality is required, apply and relay require DFS + verified certificate (metadata `consensus:lsu_finality_cid:{cycle}` or CID path); otherwise gossip-only LSU is rejected for apply. | `crates/catalyst-consensus/src/lsu_finality.rs`, `crates/catalyst-cli/src/node.rs` |
| 3.3 | **Transport signatures ≠ consensus finality:** Envelope-level signing must not be mistaken for committee finality on the LSU. | ADR 0001 context |
| 3.4 | **BFT threshold consistent:** One documented formula (e.g. ⌈2*n*/3⌉) for “enough distinct producers”; producer **construction** majority vs **vote** quorum must be defined and consistent with the paper where intended. | `lsu_has_bft_vote_quorum`, `bft_vote_threshold` in `node.rs` |

---

## 4. Fork choice, reorg, and replay (blocking)

| # | Criterion | Notes / pointers |
|---|-----------|------------------|
| 4.1 | **Defined fork choice:** Given two competing LSUs at the same cycle, honest code follows a **single rule** (e.g. certified quorum + hash tie-break), not “longest skipped chain.” | **Partial:** `fork_choice.rs` + `node::fork_choice_integration_tests` (8 tests: persist tier/tie-break, apply gate, equivocation stall/tiebreak, reconcile/reorg to stronger certified LSU). Wired in P2P apply paths. Cross-cycle checkpoint policy still TBD. |
| 4.2 | **Replay/reorg does not assume genesis archive:** If metadata pruning is enabled, reconciliation must use a **checkpointed prefix**, **state sync**, or **fetch missing LSUs from peers** — not require `consensus:lsu:1..C-1` all local forever. **Current code:** optional P2P prefetch window (`CATALYST_RECONCILE_PREFETCH_MS`, `LsuRangeRequest`) before giving up; **full** archival replay without peers remains an operator/archival-node concern. | `try_reconcile_fork_from_quorum_lsu`, `reconcile_prefetch_lsu_metadata_if_missing`, `crates/catalyst-cli/src/pruning.rs` |
| 4.3 | **Pruning policy documented:** Operators know whether a node is **archival** (full LSU metadata) vs **pruned** (limited replay). | `node-operator-guide.md`, storage metadata keys |
| 4.4 | **`CATALYST_MAX_REPLAY_CYCLES` behavior documented** and tested at boundaries. | Env / `max_replay_cycles_from_env` in `crates/catalyst-cli/src/consensus_limits.rs` (unit tests for `0`, large values, invalid parse → default) |

---

## 5. Time, cycles, and membership (blocking)

| # | Criterion | Notes / pointers |
|---|-----------|------------------|
| 5.1 | **Wall-clock cycle documented:** `cycle = current_timestamp_ms() / cycle_ms` risks skew, sleep, and leap adjustments. Document **NTP requirements**, max skew, and behavior when local cycle jumps. | [`consensus-wall-clock-and-time.md`](./consensus-wall-clock-and-time.md); `ledger_cycle_index` tests in `crates/catalyst-cli/src/consensus_limits.rs`; `node.rs` consensus loop |
| 5.2 | **Producer set derivation:** `load_workers_from_state` vs bootstrap `validator_worker_ids` is deterministic and tested across **restart** and **empty worker state**. | `node.rs`; **`node::worker_registry_load_tests`** (empty Rocks state → no workers; `workers:<pk>=[1]` → one pubkey). |
| 5.3 | **Seed for selection:** `prev_seed` / last LSU hash chain is consistent across validators after reconciliation. | `select_producers_for_next_cycle` in `crates/catalyst-core/src/protocol.rs` (`producer_selection_order_depends_on_prev_cycle_seed` test); `prev_seed` updates in `node.rs` |

---

## 6. Observability and operations (should-have before long soak)

| # | Criterion | Notes / pointers |
|---|-----------|------------------|
| 6.1 | **Structured diagnostics:** Logs/metrics distinguish **batch miss**, **phase timeout**, **prev_root mismatch**, **reconcile skip** (missing LSU vs verify fail). | tracing targets: `catalyst.consensus.tx_batch`, `reconcile`, `prev_root`, `apply`, `policy`; counters registered in `ensure_consensus_p2p_cli_metrics_registered`: tx-batch ingress, LSU cycle gate, **`consensus_fork_reconcile_{succeeded,aborted,error}_total`**, **`consensus_lsu_apply_skipped_prev_root_total`**, **`consensus_lsu_gossip_apply_rejected_finality_total`**, etc. (`node.rs`) |
| 6.2 | **Runbook:** Operator steps for “head stuck” / “split root” align with implemented recovery (not only snapshot). | `node-operator-guide.md` |
| 6.3 | **Version negotiation:** Mixed-version clusters fail safe. | **Partial:** Identify `protocol_version` major-family check + advertise `catalyst/1.0` (`catalyst-network` `protocol_identify.rs`); envelope `MessageEnvelope.version` / strict `PROTOCOL_VERSION` (`catalyst-utils` `network.rs`). See [`protocol-params.md`](./protocol-params.md#version-negotiation-libp2p-identify-vs-message-envelopes). |
| 6.4 | **Wire payload discipline:** Reused message types (e.g. `MessageType::TransactionRequest` carrying `TxBatchControl`) must document **exact** serialization; old binaries sending different payloads to the same type risk mis-decode. | `tx.rs`, network codec docs |

---

## 7. Automated tests and CI gates (blocking)

| # | Criterion | Notes / pointers |
|---|-----------|------------------|
| 7.1 | **Multi-node happy path:** Three validators, uniform txs, assert identical **applied** `state_root` per cycle for *T* cycles. | **Partial (strong):** `convergence_test_support` + `tests/convergence_harness.rs` — identical **LSU hash** per cycle across *N* in-memory nodes. **`node::storage_state_root_convergence_tests`** — three **isolated** `StorageManager` instances (same genesis), each applies the same converged LSU per cycle via `apply_lsu_to_storage`; asserts identical **committed** `state_root` (and metadata vs engine cache). **Still TBD:** full multi-process nodes + real protocol txs / mempools. |
| 7.2 | **Batch loss:** Drop or delay `ProtocolTxBatch` for one follower; assert **no fork** (stall or recovery), never divergent roots. | **Partial:** `protocol_tx_batch_disagrees_with_commit`, commit blob parse/encode, inbound + recover enforce **hash + tx_count** vs `TxBatchControl::Commit` (`consensus_limits.rs`, `node.rs`). **`node::tx_batch_follower_wait_tests`** — stall when pinned non-zero `Commit` but no batch; agreed-empty; leader miss; RAM short-circuit; **`follower_batch_arrives_mid_wait_returns_entries`** (batch appears mid-wait → `Some(entries)`). Full multi-node P2P drop harness still TBD. |
| 7.3 | **Partition / asymmetric delivery:** Libp2p or harness simulates one slow link; network must converge or **halt safely** per spec. | `convergence_harness` `multi_node_convergence_slow_straggler_link` (extra delay to one straggler); **`SimNetConfig.skip_dest_message`** + `sim_net_should_deliver` for selective per-dest drop (harness building block). Libp2p-level partition tests + WAN gate still TBD. |
| 7.4 | **Clock skew harness:** Offset one node’s notion of time (or inject cycle jump); behavior matches §5 documentation. | **Partial:** `ledger_cycle_index` + **`ledger_cycle_skew_vs_wall`** unit tests; P2P ingress uses `p2p_tx_batch_max_cycle_slack` / `p2p_tx_batch_cycle_allows_ingress` vs wall index. Full multi-node skew harness TBD. |
| 7.5 | **Finality verification tests:** Unit + integration tests for `verify_lsu_finality_certificate` on good and bad certificates (wrong `lsu_hash`, insufficient votes / `InsufficientQuorum`, etc.). | `lsu_finality.rs` (`#[cfg(test)]` module) |

---

## 8. Exit criteria (“feature complete” for consensus)

Treat consensus as **feature-complete** for *trusted multi-region testnets* when **all** of the following hold:

1. **§1** canonical tx availability is implemented and covered by **§7.2** (no divergent roots on batch loss).
2. **§2** multi-producer phase collection is the **only** supported validator path in release configuration.
3. **§3** finality verification is **required** on the apply path (ADR 0001 or documented successor).
4. **§4** fork choice and replay/pruning story is **consistent**; no hidden dependency on full genesis LSU archive unless explicitly “archival node only.”
5. **§5** time and membership behavior is **documented** and covered by at least one automated skew or restart test.
6. **§7** CI runs the multi-node and adversarial tests on every merge (or nightly if resource-bound, with merge on green).

**After exit:** run extended soak + [`wan-soak-load-chaos-gate.md`](./wan-soak-load-chaos-gate.md). If failures persist **without** Byzantine nodes, capture traces and only then escalate to a **protocol design review** (paper-level changes).

---

## Revision history

| Date | Change |
|------|--------|
| 2026-05-13 | Initial checklist from maintainer consensus completion plan |
| 2026-05-13 | **Progress (batch 1):** §1 partial — `TxBatchControl` (`Commit` / `ResyncRequest` on `MessageType::TransactionRequest`), WAN-scoped batch wait + P2P resync + metadata recovery; strict follower path (no silent empty batch; optional legacy env). §4 partial — prune-aware fork-reconcile diagnostics; prune deletes `consensus:tx_batch_commit:*`. §2 partial — consensus.rs comments aligned with networked phase collection. |
| 2026-05-13 | **Progress (batch 2):** §4 — `CATALYST_RECONCILE_PREFETCH_MS` + `reconcile_prefetch_lsu_metadata_if_missing` (P2P LSU metadata prefetch before aborting quorum replay). §5 — `docs/consensus-wall-clock-and-time.md`; protocol env table in `protocol-params.md`; operator guide pruning vs replay + TX_BATCH mitigations. §3 — `lsu_finality.rs` negative tests (wrong cert hash, insufficient quorum). `node.rs` comment clarifying construction **simple majority** vs LSU vote **BFT** threshold. **Still not done:** flip default to require ADR 0001 finality on apply fleet-wide; §7 multi-node / batch-loss integration harness in CI; automated clock-skew harness (§7.4); checkpointed state-sync for replay-from-pruned-genesis without archival peers. **Build note:** workspace `rocksdb` **0.24** (`librocksdb-sys` 0.17 + bundled 10.4.x) avoids C++ compile failures when **system** `librocksdb-dev` headers (e.g. 9.x) shadow older bundled 8.10 headers; linking with `ROCKSDB_LIB_DIR` requires a RocksDB version compatible with the crate’s generated C bindings. |
| 2026-05-13 | **Progress (batch 3):** `[consensus] require_lsu_finality` + `effective_require_lsu_finality` (env non-empty overrides TOML); validator startup policy warning; tracing target `catalyst.consensus.apply` / `catalyst.consensus.policy`. `consensus_limits` module (replay cap + finality precedence unit tests). `convergence_harness`: slow-straggler + eight-cycle / three-validator tests. Docs: protocol TOML table, operator LSU finality subsection. **Still outstanding:** opt-in default for finality (flip when fleet-ready); §7.2 **node-level** `ProtocolTxBatch` loss harness; §7.4 clock skew; checkpoints without archival peers; CI wiring for convergence tests on every merge. |
| 2026-05-13 | **Progress (batch 4):** Tx-batch **tx_count** vs commit on inbound `ProtocolTxBatch` and recovery; `parse_tx_batch_commit_value` / `encode_tx_batch_commit_value` / `protocol_tx_batch_disagrees_with_commit`; `ledger_cycle_index` + tests; wall-clock doc cross-link. |
| 2026-05-13 | **Progress (batch 5):** Tracing targets `catalyst.consensus.tx_batch`, `reconcile`, `prev_root`; `tx_batch_follower_wait_budget_default` / `tx_batch_follower_wait_budget_ms` in `consensus_limits` with tests; `ledger_cycle_index_monotonic_in_time`; `producer_selection_order_depends_on_prev_cycle_seed` in `catalyst-core`. Checklist §1.4 / §5.3 / §6.1 updated. |
| 2026-05-13 | **Progress (batch 6):** **Phased execution plan** (multi-session) in this doc; pure helpers `tx_batch_follower_deadline_ms`, `tx_batch_follower_hard_stop_ms`, `resolve_empty_construction_takeaway` / `EmptyConstructionResolution` (unit-tested) with `wait_for_tx_construction_entries` delegating to them; `make consensus-checklist-tests` + `scripts/run-consensus-ci-parity.sh` for Phase 0 CI slice (`catalyst-cli`, `catalyst-consensus`, `catalyst-core` `protocol::` tests). **Next:** §7.2 async / P2P batch-drop harness; §7.1 storage-backed applied `state_root`. |
| 2026-05-13 | **Progress (batch 7):** §7.2 — `wait_for_tx_construction_entries` takes `Option<&P2pService>` (production `Some`); **`tx_batch_follower_wait_tests`** in `node.rs` for follower stall vs `tx_count=0` vs leader miss + ram short-circuit. Phased plan **Phase 1** row updated. **Next:** libp2p-level batch-drop for one follower; §7.1 storage-backed `state_root` harness. |
| 2026-05-13 | **Progress (batch 8):** §7.1 — `catalyst_consensus::convergence_test_support` (shared sim + `produce_single_cycle_converged_lsu`); integration tests refactored to use it; **`node::storage_state_root_convergence_tests`** (three TempDir-backed stores, identical `state_root` after each applied LSU). Checklist §7.1 + Phase **2** + repo map updated. **Next:** multi-process / libp2p §7.2; EVM-heavy §7.1 scenarios. |
| 2026-05-13 | **Progress (batch 9):** §2.2 / Phase **5** partial — `phase_body_matches_current_cycle` + doc in `consensus.rs`, unit test; Phase **0** gate now runs **`cargo test -p catalyst-consensus --test convergence_harness`** (`makefile`, `scripts/run-consensus-ci-parity.sh`). **Next:** `node.rs` envelope-wide cycle audit; §7.4 skew harness. |
| 2026-05-13 | **Progress (batch 10):** P2P tx-batch ingress — `p2p_tx_batch_max_cycle_slack`, `p2p_tx_batch_cycle_allows_ingress`, `ledger_cycle_skew_vs_wall` + tests (`consensus_limits.rs`); `TransactionBatch` + `TxBatchControl::Commit` gated vs wall cycle (`node.rs`); `ResyncRequest` unchanged; **`ensure_consensus_p2p_cli_metrics_registered`** + counters from `start_node` (`main.rs`); **`node::worker_registry_load_tests`**; `protocol-params.md` row for `CATALYST_P2P_TX_BATCH_MAX_CYCLE_SLACK`. Checklist §2.2 / §5.2 / §6.1 / §7.4 / Phase **6** rows updated. **Next:** remaining `MessageType` ingress audit; multi-node skew / batch-drop harnesses. |
| 2026-05-13 | **Progress (batch 11):** LSU P2P far-future guard — `p2p_lsu_cycle_max_wall_lead`, `p2p_lsu_cycle_within_wall_lead` + tests; applied to `LsuCidGossip`, `LsuGossip`, `LsuFinalityAttestationMsg`, `LsuFinalityCidMsg`, `LsuRangeRequest`, `LsuRangeResponse` (per-ref + filtered head), `FileResponse` LSU completion; metric `consensus_p2p_lsu_cycle_rejected_total`; `protocol-params.md` (`CATALYST_P2P_LSU_MAX_WALL_LEAD`). Removed duplicate batch-10 revision row. **Next:** `Transaction` / phase envelope parity; libp2p batch-drop harness; §4 fork-choice implementation once ruled. |
| 2026-05-13 | **Progress (batch 12):** **§3.2 / `LsuGossip`:** when `require_lsu_finality` is effective, apply and relay require DFS + passing `verify_lsu_finality_for_apply` (uses `consensus:lsu_finality_cid:{cycle}` when message has no CID). **§6.1:** counters `consensus_fork_reconcile_{succeeded,aborted,error}_total`, `consensus_lsu_apply_skipped_prev_root_total`, `consensus_lsu_gossip_apply_rejected_finality_total`. **§7.2:** `follower_batch_arrives_mid_wait_returns_entries` in `tx_batch_follower_wait_tests`. Checklist §7.2 / Phase **1** / §3.2 / §6.1 updated. **Next:** libp2p single-follower batch drop; §4 fork-choice. |
| 2026-05-16 | **Progress (batch 13):** **§4.1 fork choice (same-cycle):** `crates/catalyst-cli/src/fork_choice.rs` — tier `Certified` > `QuorumInferred` > `None`; tie-break smaller `lsu_hash`; wired into `persist_lsu_history`, `try_reconcile_fork_from_quorum_lsu`, LsuCidGossip / FileResponse / LsuGossip apply; metric `consensus_fork_choice_rejected_weaker_total`. Doc: `consensus-quorum-and-fork-choice.md` §Fork choice at a fixed cycle. **Next:** cross-cycle highest-certified-head policy; integration test with two competing LSUs at one cycle. |
| 2026-05-16 | **Progress (batch 14):** **Agreed fork-choice policy (all 7 items):** certified-only when `require_lsu_finality` effective; certified equivocation stall (prod) / tie-break (testnet, `CATALYST_ALLOW_CERT_EQUIVOCATION_TIEBREAK`); `fork_choice_apply_gate`, `try_reorg_stronger_lsu_at_applied_cycle`; `consensus:certified_lsu_hash:{cycle}`; default `require_lsu_finality=true`; normative `consensus-quorum-and-fork-choice.md` rewrite. **Next:** integration test matrix §7; checkpoint cross-cycle sync. |
| 2026-05-16 | **Progress (batch 15):** **§7 fork-choice integration:** `node::fork_choice_integration_tests` — 8 tokio tests (persist, apply gate, equivocation, reconcile + reorg). Run via `cargo test -p catalyst-cli fork_choice_integration`. **Next:** multi-node libp2p adversarial harness; same-cycle drop still §7.2. |
