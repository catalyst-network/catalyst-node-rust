//! Integration tests for multi-node collaborative convergence (re-exports harness from the library).

use catalyst_consensus::convergence_test_support::{
    make_transactions, run_convergence, run_convergence_collect_hashes, run_convergence_with_txs,
    SimNetConfig,
};
use catalyst_utils::MessageType;

#[tokio::test]
async fn multi_node_convergence_no_delay() {
    run_convergence(
        3,
        5,
        SimNetConfig {
            max_delay_ms: 0,
            ..Default::default()
        },
    )
    .await;
}

#[tokio::test]
async fn multi_node_convergence_with_delay_and_reordering() {
    run_convergence(
        5,
        5,
        SimNetConfig {
            max_delay_ms: 35,
            ..Default::default()
        },
    )
    .await;
}

/// Checklist §7.1 style gate: three producers, more cycles, identical LSU hashes per cycle.
#[tokio::test]
async fn three_validators_eight_cycles_identical_lsu_hashes() {
    run_convergence(
        3,
        8,
        SimNetConfig {
            max_delay_ms: 0,
            ..Default::default()
        },
    )
    .await;
}

/// Asymmetric latency: one node is consistently slower to receive all fanout copies.
#[tokio::test]
async fn multi_node_convergence_slow_straggler_link() {
    run_convergence(
        3,
        5,
        SimNetConfig {
            max_delay_ms: 20,
            slow_dest: Some(0),
            slow_extra_ms: 150,
            ..Default::default()
        },
    )
    .await;
}

/// SimNet `TransactionBatch` skip is a hook for node-level batch-loss tests; the in-process
/// harness does not emit that type — convergence must remain unaffected here.
#[tokio::test]
async fn multi_node_convergence_transaction_batch_skip_is_noop_in_harness() {
    run_convergence(
        3,
        3,
        SimNetConfig {
            skip_dest_message: Some((2, MessageType::TransactionBatch)),
            ..Default::default()
        },
    )
    .await;
}

/// Checklist §1 / §7.2: different construction inputs change the LSU hash (single-node).
/// A multi-node run with one empty producer aborts at voting (“hash doesn't match majority”)
/// rather than committing divergent state — the node tx-batch layer must stall earlier.
#[tokio::test]
async fn different_construction_inputs_yield_different_lsu_hashes() {
    let full = run_convergence_collect_hashes(1, 1, SimNetConfig::default(), |_, cycle| {
        make_transactions(cycle)
    })
    .await;
    let empty = run_convergence_collect_hashes(1, 1, SimNetConfig::default(), |_, _| {
        Vec::new()
    })
    .await;
    assert_ne!(
        full[0][0], empty[0][0],
        "canonical vs empty construction must not produce the same LSU hash"
    );
}

/// All producers share the same canonical tx list — matches §7.1 harness expectation.
#[tokio::test]
async fn homogeneous_producer_tx_inputs_converge() {
    run_convergence_with_txs(3, 3, SimNetConfig::default(), |_, cycle| {
        make_transactions(cycle)
    })
    .await;
}
