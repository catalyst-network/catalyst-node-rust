//! Checklist §7.4: multi-node clock skew simulated via `CATALYST_TEST_WALL_OFFSET_MS` + P2P ingress.

#[cfg(test)]
mod tests {
    use catalyst_utils::network::MessageEnvelope;

    use crate::consensus_limits;
    use crate::tx::{ProtocolTxBatch, TxBatchControl};
    use crate::tx_batch_p2p::{ingest_p2p_tx_batch_envelope, TxBatchP2pState};

    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, val: &str) -> Self {
            let previous = std::env::var(key).ok();
            // SAFETY: tests run single-threaded per process in `cargo test`.
            unsafe { std::env::set_var(key, val) };
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(ref v) = self.previous {
                unsafe { std::env::set_var(self.key, v) };
            } else {
                unsafe { std::env::remove_var(self.key) };
            }
        }
    }

    fn sample_protocol_batch(cycle: u64) -> (ProtocolTxBatch, MessageEnvelope, MessageEnvelope) {
        use catalyst_core::protocol as corep;
        use catalyst_core::protocol::EntryAmount;

        let sender = [5u8; 32];
        let recv = [6u8; 32];
        let mut tx = corep::Transaction {
            core: corep::TransactionCore {
                tx_type: corep::TransactionType::NonConfidentialTransfer,
                entries: vec![
                    corep::TransactionEntry {
                        public_key: sender,
                        amount: EntryAmount::NonConfidential(-10),
                    },
                    corep::TransactionEntry {
                        public_key: recv,
                        amount: EntryAmount::NonConfidential(10),
                    },
                ],
                nonce: 1,
                lock_time: 0,
                fees: 0,
                data: Vec::new(),
            },
            signature_scheme: corep::sig_scheme::SCHNORR_V1,
            signature: corep::AggregatedSignature(vec![7u8; 64]),
            sender_pubkey: None,
            timestamp: cycle.saturating_mul(60_000),
        };
        tx.core.fees = corep::min_fee(&tx);
        let batch = ProtocolTxBatch::new(cycle, vec![tx]).expect("batch");
        let commit = TxBatchControl::Commit {
            cycle,
            batch_hash: batch.batch_hash,
            tx_count: batch.txs.len() as u32,
        };
        let commit_env =
            MessageEnvelope::from_message(&commit, "skew_commit".to_string(), None).expect("commit");
        let batch_env =
            MessageEnvelope::from_message(&batch, "skew_batch".to_string(), None).expect("batch");
        (batch, commit_env, batch_env)
    }

    /// Two logical validators: in-sync node accepts batch; skewed node rejects same gossip cycle.
    #[tokio::test]
    async fn wall_offset_rejects_batch_on_skewed_node_accepts_on_sync_node() {
        let cycle_ms = 60_000u64;
        let slack = 1u64;
        let _slack_env = EnvGuard::set("CATALYST_P2P_TX_BATCH_MAX_CYCLE_SLACK", &slack.to_string());

        let cycle = 10u64;
        let (_batch, commit_env, batch_env) = sample_protocol_batch(cycle);

        // Node A: no offset — align wall index with message cycle.
        let offset_a = cycle.saturating_mul(cycle_ms).saturating_add(5_000);
        let offset_a_ms = (offset_a as i128)
            .saturating_sub(catalyst_utils::utils::current_timestamp_ms() as i128);
        let _guard_a = EnvGuard::set(
            "CATALYST_TEST_WALL_OFFSET_MS",
            &offset_a_ms.to_string(),
        );

        let state_a = TxBatchP2pState::new();
        let now_a = consensus_limits::wall_now_ms();
        assert!(consensus_limits::p2p_tx_batch_cycle_allows_ingress(
            cycle, now_a, cycle_ms, slack
        ));
        ingest_p2p_tx_batch_envelope(&state_a, &commit_env, now_a, cycle_ms, None, 4096).await;
        assert!(
            state_a.commit_ram.read().await.contains_key(&cycle),
            "in-sync node must pin commit at ingress"
        );
        assert!(
            consensus_limits::p2p_tx_batch_cycle_allows_ingress(
                cycle,
                now_a,
                cycle_ms,
                slack
            ),
            "in-sync wall must admit message cycle"
        );

        // Node B: +3 cycles ahead on wall clock.
        drop(_guard_a);
        let offset_b_ms = offset_a_ms.saturating_add(3_i128 * cycle_ms as i128);
        let _guard_b = EnvGuard::set(
            "CATALYST_TEST_WALL_OFFSET_MS",
            &offset_b_ms.to_string(),
        );
        let state_b = TxBatchP2pState::new();
        let now_b = consensus_limits::wall_now_ms();
        assert!(!consensus_limits::p2p_tx_batch_cycle_allows_ingress(
            cycle, now_b, cycle_ms, slack
        ));
        let accepted_b = ingest_p2p_tx_batch_envelope(
            &state_b,
            &batch_env,
            now_b,
            cycle_ms,
            None,
            4096,
        )
        .await;
        assert!(!accepted_b, "skewed node must reject stale batch at ingress");
    }

    /// Skewed follower with pinned commit but no batch must stall (not diverge via empty apply).
    #[tokio::test]
    async fn skewed_follower_stalls_construction_when_batch_rejected_at_ingress() {
        let _budget = EnvGuard::set("CATALYST_TX_BATCH_WAIT_BUDGET_MS", "300");
        let cycle_ms = 60_000u64;
        let cycle = 12u64;
        let (_batch, commit_env, batch_env) = sample_protocol_batch(cycle);

        let offset_b_ms = (cycle.saturating_add(4).saturating_mul(cycle_ms) as i128)
            .saturating_sub(catalyst_utils::utils::current_timestamp_ms() as i128);
        let _guard_b = EnvGuard::set(
            "CATALYST_TEST_WALL_OFFSET_MS",
            &offset_b_ms.to_string(),
        );

        let state = TxBatchP2pState::new();
        let now_b = consensus_limits::wall_now_ms();
        ingest_p2p_tx_batch_envelope(&state, &commit_env, now_b, cycle_ms, None, 4096).await;
        assert!(
            !ingest_p2p_tx_batch_envelope(&state, &batch_env, now_b, cycle_ms, None, 4096).await,
            "batch must not land on skewed ingress"
        );

        let out = crate::node::wait_for_tx_construction_entries(
            cycle,
            cycle_ms,
            "skewed_follower",
            "leader",
            false,
            4096,
            state.tx_batches.as_ref(),
            state.commit_ram.as_ref(),
            None,
            None,
        )
        .await;
        assert_eq!(out, None, "skewed follower must stall, not apply empty construction");
    }
}
