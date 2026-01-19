use std::collections::HashMap;
use std::time::{Duration, Instant};

use catalyst_consensus::types::{hash_data, TransactionEntry};
use catalyst_core::protocol as corep;
use catalyst_core::protocol::EntryAmount;
use blake2::Digest;
use catalyst_utils::{
    impl_catalyst_serialize, CatalystDeserialize, CatalystResult, CatalystSerialize, Hash, MessageType,
    NetworkMessage,
};
use catalyst_utils::network::MessagePriority;
use serde::{Deserialize, Serialize};

/// Lightweight transaction gossip message.
///
/// For now, we gossip `catalyst-consensus` `TransactionEntry` items directly, because the
/// consensus engine consumes them during Construction. Later, this should be replaced by
/// protocol-faithful transactions (aggregated signature, locking time, etc.) from `catalyst-core`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TxGossip {
    pub tx_id: Hash,
    pub entries: Vec<TransactionEntry>,
    pub created_at_ms: u64,
}

impl_catalyst_serialize!(TxGossip, tx_id, entries, created_at_ms);

impl NetworkMessage for TxGossip {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(self)
    }

    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        CatalystDeserialize::deserialize(data)
    }

    fn message_type(&self) -> MessageType {
        MessageType::Transaction
    }

    fn priority(&self) -> u8 {
        MessagePriority::High as u8
    }

    fn ttl(&self) -> u32 {
        60 // 1 minute
    }
}

impl TxGossip {
    pub fn new(entries: Vec<TransactionEntry>, created_at_ms: u64) -> CatalystResult<Self> {
        // Stable tx id for deduplication.
        let tx_id = hash_data(&entries)?;
        Ok(Self {
            tx_id,
            entries,
            created_at_ms,
        })
    }
}

/// Protocol-shaped transaction gossip message.
///
/// This is closer to the paper than `TxGossip`: it includes locking time + aggregated signature.
/// The node converts accepted protocol transactions into `catalyst-consensus` `TransactionEntry`
/// items at cycle time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolTxGossip {
    pub tx_id: Hash,
    pub tx: corep::Transaction,
    pub received_at_ms: u64,
}

impl NetworkMessage for ProtocolTxGossip {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        bincode::serialize(self)
            .map_err(|e| catalyst_utils::error::CatalystError::Serialization(e.to_string()))
    }

    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        bincode::deserialize(data)
            .map_err(|e| catalyst_utils::error::CatalystError::Serialization(e.to_string()))
    }

    fn message_type(&self) -> MessageType {
        MessageType::Transaction
    }

    fn priority(&self) -> u8 {
        MessagePriority::High as u8
    }

    fn ttl(&self) -> u32 {
        300
    }
}

impl ProtocolTxGossip {
    pub fn new(tx: corep::Transaction, received_at_ms: u64) -> CatalystResult<Self> {
        // Stable tx id for deduplication: hash bincode(tx)
        let bytes = bincode::serialize(&tx)
            .map_err(|e| catalyst_utils::error::CatalystError::Serialization(e.to_string()))?;
        let mut hasher = blake2::Blake2b512::new();
        hasher.update(&bytes);
        let result = hasher.finalize();
        let mut tx_id = [0u8; 32];
        tx_id.copy_from_slice(&result[..32]);

        Ok(Self { tx_id, tx, received_at_ms })
    }

    pub fn validate_basic(&self, now_secs: u64) -> Result<(), String> {
        self.tx.validate_basic()?;
        // Interpret lock_time as unix seconds "not before".
        if (self.tx.core.lock_time as u64) > now_secs {
            return Err(format!(
                "Transaction not yet unlocked: lock_time={} now={}",
                self.tx.core.lock_time, now_secs
            ));
        }
        Ok(())
    }

    pub fn to_consensus_entries(&self) -> Vec<TransactionEntry> {
        // Map protocol entries into consensus entries (temporary bridge).
        // - Confidential amounts are skipped for now.
        // - Each entry signature gets the aggregated signature bytes (deterministic placeholder).
        let sig = self.tx.signature.0.clone();
        let mut out = Vec::new();
        for e in &self.tx.core.entries {
            let amount = match &e.amount {
                EntryAmount::NonConfidential(v) => *v,
                EntryAmount::Confidential { .. } => continue,
            };
            out.push(TransactionEntry {
                public_key: e.public_key,
                amount,
                signature: sig.clone(),
            });
        }
        out
    }
}

/// Leader-selected transaction batch for a specific cycle.
///
/// This is a temporary mechanism to keep Construction deterministic across producers:
/// all producers use the same `entries` set for a given cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TxBatch {
    pub cycle: u64,
    pub batch_hash: Hash,
    pub entries: Vec<TransactionEntry>,
}

impl_catalyst_serialize!(TxBatch, cycle, batch_hash, entries);

impl NetworkMessage for TxBatch {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(self)
    }

    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        CatalystDeserialize::deserialize(data)
    }

    fn message_type(&self) -> MessageType {
        MessageType::TransactionBatch
    }

    fn priority(&self) -> u8 {
        MessagePriority::High as u8
    }

    fn ttl(&self) -> u32 {
        30
    }
}

impl TxBatch {
    pub fn new(cycle: u64, entries: Vec<TransactionEntry>) -> CatalystResult<Self> {
        let batch_hash = hash_data(&entries)?;
        Ok(Self {
            cycle,
            batch_hash,
            entries,
        })
    }
}

#[derive(Debug, Clone)]
struct MempoolItem {
    entries: Vec<TransactionEntry>,
    inserted_at: Instant,
}

/// Minimal in-memory mempool.
#[derive(Debug, Default)]
pub struct Mempool {
    by_id: HashMap<Hash, MempoolItem>,
    ttl: Duration,
    max_txs: usize,
}

impl Mempool {
    pub fn new(ttl: Duration, max_txs: usize) -> Self {
        Self {
            by_id: HashMap::new(),
            ttl,
            max_txs: max_txs.max(1),
        }
    }

    pub fn insert(&mut self, tx: TxGossip) -> bool {
        self.evict_expired();
        if self.by_id.len() >= self.max_txs {
            // Drop if at capacity (simple policy).
            return false;
        }
        if self.by_id.contains_key(&tx.tx_id) {
            return false;
        }
        self.by_id.insert(
            tx.tx_id,
            MempoolItem {
                entries: tx.entries,
                inserted_at: Instant::now(),
            },
        );
        true
    }

    pub fn insert_protocol(&mut self, tx: ProtocolTxGossip, now_secs: u64) -> bool {
        // Drop invalid / not-yet-unlocked txs early.
        if tx.validate_basic(now_secs).is_err() {
            return false;
        }

        self.evict_expired();
        if self.by_id.len() >= self.max_txs {
            return false;
        }
        if self.by_id.contains_key(&tx.tx_id) {
            return false;
        }

        self.by_id.insert(
            tx.tx_id,
            MempoolItem {
                entries: tx.to_consensus_entries(),
                inserted_at: Instant::now(),
            },
        );
        true
    }

    pub fn evict_expired(&mut self) {
        let ttl = self.ttl;
        self.by_id.retain(|_, v| v.inserted_at.elapsed() <= ttl);
    }

    /// Freeze a snapshot of mempool entries for the next cycle.
    /// Returns a flat list of transaction entries and removes all txs from the pool.
    pub fn freeze_and_drain_entries(&mut self, max_entries: usize) -> Vec<TransactionEntry> {
        self.evict_expired();

        let mut out: Vec<TransactionEntry> = Vec::new();
        for item in self.by_id.values() {
            for e in &item.entries {
                if out.len() >= max_entries {
                    break;
                }
                out.push(e.clone());
            }
            if out.len() >= max_entries {
                break;
            }
        }

        self.by_id.clear();
        out
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }
}

