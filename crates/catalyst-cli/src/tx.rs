use std::collections::HashMap;
use std::time::{Duration, Instant};

use catalyst_consensus::types::{hash_data, TransactionEntry};
use catalyst_core::protocol as corep;
use catalyst_core::protocol::EntryAmount;
use blake2::Digest;
use crate::evm::{encode_evm_marker, EvmTxKind, EvmTxPayload};
use catalyst_utils::{
    impl_catalyst_serialize, CatalystDeserialize, CatalystResult, CatalystSerialize, Hash, MessageType,
    NetworkMessage,
};
use catalyst_utils::network::MessagePriority;
use serde::{Deserialize, Serialize};

// bring the crate module into scope for the binary crate build

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
        if self.tx.core.nonce == 0 {
            return Err("Transaction nonce must be > 0".to_string());
        }
        let min_fee = corep::min_fee(&self.tx);
        if self.tx.core.fees < min_fee {
            return Err(format!(
                "Transaction fee too low: fees={} min_required={}",
                self.tx.core.fees, min_fee
            ));
        }
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
        //
        // Special case:
        // - WorkerRegistration is encoded as a marker entry that is carried in the LSU and applied
        //   into on-chain state (workers:<pubkey>).
        //
        // Notes:
        // - Confidential amounts are skipped for now.
        // - Each entry signature gets the aggregated signature bytes (deterministic placeholder).
        match self.tx.core.tx_type {
            corep::TransactionType::WorkerRegistration => {
                let pk = self.tx.core.entries.get(0).map(|e| e.public_key).unwrap_or([0u8; 32]);
                let mut sig = b"WRKREG1".to_vec();
                sig.extend_from_slice(&self.tx.signature.0);
                let mut out = vec![TransactionEntry {
                    public_key: pk,
                    amount: 0,
                    signature: sig,
                }];
                // Charge fees by adding a dedicated fee debit entry under the same tx boundary (same signature bytes).
                out.extend(self.fee_debit_entries(pk, out[0].signature.clone()));
                out
            }
            corep::TransactionType::SmartContract => {
                let pk = self.tx.core.entries.get(0).map(|e| e.public_key).unwrap_or([0u8; 32]);
                let kind = bincode::deserialize::<EvmTxKind>(&self.tx.core.data)
                    .unwrap_or(EvmTxKind::Call { to: [0u8; 20], input: Vec::new() });
                let payload = EvmTxPayload { nonce: self.tx.core.nonce, kind };
                let marker = encode_evm_marker(&payload, &self.tx.signature.0).unwrap_or_else(|_| self.tx.signature.0.clone());
                let mut out = vec![TransactionEntry {
                    public_key: pk,
                    amount: 0,
                    signature: marker,
                }];
                out.extend(self.fee_debit_entries(pk, out[0].signature.clone()));
                out
            }
            _ => {
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
                if let Some(sender) = self.sender_pubkey() {
                    out.extend(self.fee_debit_entries(sender, sig.clone()));
                }
                out
            }
        }
    }

    fn fee_debit_entries(&self, sender: [u8; 32], sig: Vec<u8>) -> Vec<TransactionEntry> {
        // Fees are charged as a dedicated negative entry so the LSU application can apply it deterministically.
        // This “burns” fees (no explicit sink credit yet; routing is handled in the economics epic).
        let fees_u64 = self.tx.core.fees;
        if fees_u64 == 0 {
            return Vec::new();
        }
        let fees_i64: i64 = match i64::try_from(fees_u64) {
            Ok(v) => v,
            Err(_) => return Vec::new(), // refuse to encode impossible fee into i64 ledger delta
        };
        vec![TransactionEntry {
            public_key: sender,
            amount: -fees_i64,
            signature: sig,
        }]
    }

    pub fn sender_pubkey(&self) -> Option<[u8; 32]> {
        match self.tx.core.tx_type {
            corep::TransactionType::WorkerRegistration => self.tx.core.entries.get(0).map(|e| e.public_key),
            corep::TransactionType::SmartContract => self.tx.core.entries.get(0).map(|e| e.public_key),
            _ => {
                let mut sender: Option<[u8; 32]> = None;
                for e in &self.tx.core.entries {
                    if let EntryAmount::NonConfidential(v) = e.amount {
                        if v < 0 {
                            match sender {
                                None => sender = Some(e.public_key),
                                Some(pk) if pk == e.public_key => {}
                                Some(_) => return None,
                            }
                        }
                    }
                }
                sender
            }
        }
    }
}

#[cfg(test)]
mod fee_tests {
    use super::*;

    #[test]
    fn protocol_tx_to_consensus_entries_includes_fee_debit() {
        let now_ms = 1_700_000_000_000u64;
        let now_secs = (now_ms / 1000) as u64;
        let sender = [1u8; 32];
        let recv = [2u8; 32];
        let mut tx = corep::Transaction {
            core: corep::TransactionCore {
                tx_type: corep::TransactionType::NonConfidentialTransfer,
                entries: vec![
                    corep::TransactionEntry { public_key: sender, amount: EntryAmount::NonConfidential(-10) },
                    corep::TransactionEntry { public_key: recv, amount: EntryAmount::NonConfidential(10) },
                ],
                nonce: 1,
                lock_time: 0,
                fees: 0,
                data: Vec::new(),
            },
            signature: corep::AggregatedSignature(vec![9u8; 64]),
            timestamp: now_ms,
        };
        tx.core.fees = corep::min_fee(&tx);

        let msg = ProtocolTxGossip::new(tx, now_ms).unwrap();
        msg.validate_basic(now_secs).unwrap();
        let entries = msg.to_consensus_entries();

        // Contains the transfer entries plus 1 fee debit entry.
        assert_eq!(entries.len(), 3);
        let fee = entries.iter().find(|e| e.public_key == sender && e.amount < 0 && e.amount != -10).unwrap();
        assert_eq!(fee.amount, -(msg.tx.core.fees as i64));
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

/// Protocol transaction batch (full signed transactions) for a specific cycle.
///
/// This is used to ensure Construction only includes transactions that can be validated
/// (signature + nonce + lock_time + balance checks).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolTxBatch {
    pub cycle: u64,
    pub batch_hash: Hash,
    pub txs: Vec<corep::Transaction>,
}

impl NetworkMessage for ProtocolTxBatch {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        bincode::serialize(self)
            .map_err(|e| catalyst_utils::error::CatalystError::Serialization(e.to_string()))
    }

    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        bincode::deserialize(data)
            .map_err(|e| catalyst_utils::error::CatalystError::Serialization(e.to_string()))
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

impl ProtocolTxBatch {
    pub fn new(cycle: u64, txs: Vec<corep::Transaction>) -> CatalystResult<Self> {
        // Stable hash: hash bincode(txs)
        let bytes = bincode::serialize(&txs)
            .map_err(|e| catalyst_utils::error::CatalystError::Serialization(e.to_string()))?;
        let mut hasher = blake2::Blake2b512::new();
        hasher.update(&bytes);
        let result = hasher.finalize();
        let mut h = [0u8; 32];
        h.copy_from_slice(&result[..32]);
        Ok(Self {
            cycle,
            batch_hash: h,
            txs,
        })
    }

    pub fn verify_hash(&self) -> CatalystResult<bool> {
        let other = ProtocolTxBatch::new(self.cycle, self.txs.clone())?;
        Ok(other.batch_hash == self.batch_hash)
    }
}

#[derive(Debug, Clone)]
struct MempoolItem {
    entries: Vec<TransactionEntry>,
    sender_pubkey: Option<[u8; 32]>,
    nonce: Option<u64>,
    protocol_tx: Option<corep::Transaction>,
    inserted_at: Instant,
}

/// Minimal in-memory mempool.
#[derive(Debug, Default)]
pub struct Mempool {
    by_id: HashMap<Hash, MempoolItem>,
    ttl: Duration,
    max_txs: usize,
    max_per_sender: usize,
    drops_full: u64,
    drops_per_sender: u64,
}

impl Mempool {
    pub fn new(ttl: Duration, max_txs: usize) -> Self {
        let max_per_sender = std::env::var("CATALYST_MEMPOOL_MAX_PER_SENDER")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(64);
        Self {
            by_id: HashMap::new(),
            ttl,
            max_txs: max_txs.max(1),
            max_per_sender: max_per_sender.max(1),
            drops_full: 0,
            drops_per_sender: 0,
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
                sender_pubkey: None,
                nonce: None,
                protocol_tx: None,
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
            self.drops_full = self.drops_full.saturating_add(1);
            return false;
        }
        if self.by_id.contains_key(&tx.tx_id) {
            return false;
        }

        if let Some(sender) = tx.sender_pubkey() {
            let mut count = 0usize;
            for item in self.by_id.values() {
                if item.sender_pubkey == Some(sender) {
                    count = count.saturating_add(1);
                }
            }
            if count >= self.max_per_sender {
                self.drops_per_sender = self.drops_per_sender.saturating_add(1);
                return false;
            }
        }

        self.by_id.insert(
            tx.tx_id,
            MempoolItem {
                entries: tx.to_consensus_entries(),
                sender_pubkey: tx.sender_pubkey(),
                nonce: Some(tx.tx.core.nonce),
                protocol_tx: Some(tx.tx),
                inserted_at: Instant::now(),
            },
        );
        true
    }

    pub fn max_nonce_for_sender(&self, sender: &[u8; 32]) -> Option<u64> {
        let mut max: Option<u64> = None;
        for item in self.by_id.values() {
            if let (Some(pk), Some(nonce)) = (&item.sender_pubkey, item.nonce) {
                if pk == sender {
                    max = Some(max.map(|m| m.max(nonce)).unwrap_or(nonce));
                }
            }
        }
        max
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

    /// Snapshot protocol transactions for the next cycle (deterministic order by tx_id bytes).
    ///
    /// Note: does NOT remove them. Transactions remain pending until applied (nonce advances)
    /// or TTL eviction. This prevents accidental loss if a cycle fails.
    pub fn snapshot_protocol_txs(&mut self, max_txs: usize) -> Vec<corep::Transaction> {
        self.evict_expired();
        let mut keys: Vec<Hash> = self.by_id.keys().cloned().collect();
        keys.sort();

        let mut out: Vec<corep::Transaction> = Vec::new();
        for k in keys {
            if out.len() >= max_txs {
                break;
            }
            if let Some(item) = self.by_id.get(&k) {
                if let Some(tx) = &item.protocol_tx {
                    out.push(tx.clone());
                }
            }
        }
        out
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn drop_stats(&self) -> (u64, u64) {
        (self.drops_full, self.drops_per_sender)
    }
}

