//! Protocol-aligned types and helpers derived from the Catalyst research papers.
//!
//! Primary reference: Catalyst consensus paper v1.2:
//! `https://catalystnet.org/media/CatalystConsensusPaper.pdf`
//!
//! This module is intentionally "spec-shaped": it provides the structures and deterministic
//! helper functions needed by the consensus engine and networking layer, even if the full
//! cryptographic implementations (range proofs, aggregated signatures, DFS addressing, etc.)
//! are still work-in-progress in other crates.

use serde::{Deserialize, Serialize};

use crate::types::{BlockHash, NodeId, Timestamp};

/// Canonical transaction id used for mempool/network deduplication.
///
/// Current definition (stable for dev/testnet):
/// - `tx_id = blake2b_256(bincode(Transaction))`
///
/// This MUST remain consistent across:
/// - RPC `catalyst_sendRawTransaction` return values
/// - P2P tx gossip ids / mempool persistence keys
pub fn transaction_id(tx: &Transaction) -> Result<Hash32, String> {
    use blake2::{Blake2b512, Digest};
    let bytes = bincode::serialize(tx).map_err(|e| format!("serialize tx: {e}"))?;
    let mut hasher = Blake2b512::new();
    hasher.update(&bytes);
    let out = hasher.finalize();
    let mut h = [0u8; 32];
    h.copy_from_slice(&out[..32]);
    Ok(h)
}

/// Canonical signing payload for a transaction: `bincode(TransactionCore) || timestamp_le`.
///
/// This is a temporary “real signature validation” step while aggregated signatures and
/// signature-hash trees are still being completed across crates.
pub fn transaction_signing_payload(core: &TransactionCore, timestamp: u64) -> Result<Vec<u8>, String> {
    let mut out = bincode::serialize(core).map_err(|e| format!("serialize core: {e}"))?;
    out.extend_from_slice(&timestamp.to_le_bytes());
    Ok(out)
}

/// Ledger cycle number.
pub type CycleNumber = u64;

/// Blake2b-based 32-byte hash used across protocol artifacts.
pub type Hash32 = [u8; 32];

/// Producer identifier (paper uses `Idj`).
pub type ProducerId = NodeId;

/// Worker identifier (paper uses `Idi`).
pub type WorkerId = NodeId;

/// Transaction type as described in §4.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionType {
    NonConfidentialTransfer,
    ConfidentialTransfer,
    DataStorageRequest,
    DataStorageRetrieve,
    SmartContract,
    /// Register the sender as an on-chain worker/validator candidate (minimal scaffold).
    WorkerRegistration,
}

/// Amount component of an entry (§4.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryAmount {
    /// Non-confidential transfer amount (8-byte signed integer in the paper).
    NonConfidential(i64),
    /// Confidential transfer amount (Pedersen commitment + range proof).
    ///
    /// We keep it as bytes for now; exact curve/serialization belongs in `catalyst-crypto`.
    Confidential {
        commitment: [u8; 32],
        /// Bulletproof range proof (paper mentions ~672 bytes; treat as opaque).
        range_proof: Vec<u8>,
    },
}

/// Transaction entry (§4.3): public key + amount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionEntry {
    /// 32-byte public key (paper §4.3).
    pub public_key: [u8; 32],
    pub amount: EntryAmount,
}

/// Aggregated signature (§4.4). Opaque 64-byte signature for now.
///
/// NOTE: We store this as `Vec<u8>` because `serde` doesn't implement (de)serialization
/// for arrays > 32 by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregatedSignature(pub Vec<u8>);

/// Transaction core message fields (§4.2) excluding signature and timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionCore {
    pub tx_type: TransactionType,
    pub entries: Vec<TransactionEntry>,
    /// Sender nonce for replay protection (monotonic per-sender counter).
    pub nonce: u64,
    /// Locking time (paper uses 4 bytes, "point in time after which can be processed").
    pub lock_time: u32,
    /// Transaction fees paid by participants (8 bytes).
    pub fees: u64,
    /// Optional data field (up to 60 bytes).
    pub data: Vec<u8>,
}

/// Catalyst transaction (§4.2): core + aggregated signature + timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub core: TransactionCore,
    pub signature: AggregatedSignature,
    /// Timestamp (paper uses 4 bytes). We keep u64 for convenience.
    pub timestamp: u64,
}

impl Transaction {
    pub fn validate_basic(&self) -> Result<(), String> {
        match self.core.tx_type {
            TransactionType::WorkerRegistration => {
                // Minimal: single entry identifying the worker pubkey with amount 0.
                if self.core.entries.len() != 1 {
                    return Err("WorkerRegistration must contain exactly 1 entry".to_string());
                }
                match self.core.entries[0].amount {
                    EntryAmount::NonConfidential(v) if v == 0 => {}
                    _ => return Err("WorkerRegistration entry amount must be NonConfidential(0)".to_string()),
                }
            }
            TransactionType::SmartContract => {
                // Minimal: single entry identifying the caller pubkey with amount 0.
                if self.core.entries.len() != 1 {
                    return Err("SmartContract must contain exactly 1 entry".to_string());
                }
                match self.core.entries[0].amount {
                    EntryAmount::NonConfidential(v) if v == 0 => {}
                    _ => return Err("SmartContract entry amount must be NonConfidential(0)".to_string()),
                }
            }
            _ => {
                if self.core.entries.len() < 2 {
                    return Err("Transaction must contain at least 2 entries".to_string());
                }
            }
        }
        if self.core.nonce == 0 {
            return Err("Transaction nonce must be > 0".to_string());
        }
        if self.core.data.len() > 60 {
            return Err("Transaction data field must be <= 60 bytes".to_string());
        }
        Ok(())
    }

    pub fn signing_payload(&self) -> Result<Vec<u8>, String> {
        transaction_signing_payload(&self.core, self.timestamp)
    }
}

/// Determine the canonical sender public key for a transaction (current single-sender rule).
///
/// Rules (scaffold, aligned with current node/RPC expectations):
/// - `WorkerRegistration` / `SmartContract`: sender is `entries[0].public_key`
/// - transfers: sender is the (single) pubkey with a negative `NonConfidential` amount
///
/// Returns `None` if:
/// - there is no identifiable sender
/// - multiple distinct negative-amount senders are present (multi-sender unsupported for now)
pub fn transaction_sender_pubkey(tx: &Transaction) -> Option<[u8; 32]> {
    match tx.core.tx_type {
        TransactionType::WorkerRegistration | TransactionType::SmartContract => {
            tx.core.entries.get(0).map(|e| e.public_key)
        }
        _ => {
            let mut sender: Option<[u8; 32]> = None;
            for e in &tx.core.entries {
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

/// Check whether `tx.core.lock_time` (treated as unix seconds) is unlocked at `now_secs`.
pub fn transaction_is_unlocked(tx: &Transaction, now_secs: u64) -> bool {
    (tx.core.lock_time as u64) <= now_secs
}

/// Validate basic structural rules plus lock-time at `now_secs`.
pub fn validate_basic_and_unlocked(tx: &Transaction, now_secs: u64) -> Result<(), String> {
    tx.validate_basic()?;
    if tx.core.nonce == 0 {
        return Err("Transaction nonce must be > 0".to_string());
    }
    if !transaction_is_unlocked(tx, now_secs) {
        return Err(format!(
            "Transaction not yet unlocked: lock_time={} now={}",
            tx.core.lock_time, now_secs
        ));
    }
    Ok(())
}

/// Partial ledger state update `ΔLn,j` (§5.2.1): sorted entries + tx-signature hash tree root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartialLedgerStateUpdate {
    pub cycle: CycleNumber,
    /// `LfE` in the paper.
    pub sorted_entries: Vec<TransactionEntry>,
    /// Root/hash of the transaction-signature hash tree `dn`.
    pub tx_signatures_root: Hash32,
    /// Total fees extracted for the cycle (`xf`).
    pub total_fees: u64,
    pub created_at: Timestamp,
}

/// Compensation entry (§4.3) used to reward producers in the LSU.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompensationEntry {
    pub producer_id: ProducerId,
    pub public_key: [u8; 32],
    pub amount: u64,
}

/// Full ledger state update `LSU`/`ΔLn` (§5.2.3/5.2.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerStateUpdate {
    pub cycle: CycleNumber,
    pub partial: PartialLedgerStateUpdate,
    pub compensation: Vec<CompensationEntry>,
    /// `Ln(prod)`
    pub producers_ok: Vec<ProducerId>,
    /// `Ln(vote)`
    pub voters_ok: Vec<ProducerId>,
    /// Optional content address (DFS) once stored.
    pub dfs_address: Option<String>,
}

/// Producer quantity `hj` (§5.2.1): first hash + producer id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducerQuantity {
    pub cycle: CycleNumber,
    pub first_hash: Hash32,
    pub producer_id: ProducerId,
}

/// Producer candidate `cj` (§5.2.2): majority hash + hash-tree witness of producer list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducerCandidate {
    pub cycle: CycleNumber,
    pub majority_first_hash: Hash32,
    pub producers_ok_witness: Hash32,
    pub producer_id: ProducerId,
}

/// Producer vote `vj` (§5.2.3): hash(LSU) + witness of voters list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducerVote {
    pub cycle: CycleNumber,
    pub ledger_state_hash: Hash32,
    pub voters_witness: Hash32,
    pub producer_id: ProducerId,
}

/// Producer output `oj` (§6.2 / §5.2.4): DFS address + witness of `Ln(vote)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProducerOutput {
    pub cycle: CycleNumber,
    pub dfs_address: String,
    pub voters_witness: Hash32,
    pub producer_id: ProducerId,
}

/// Deterministic producer selection for cycle `Cn+1` (§2.2.1).
///
/// Paper summary:
/// - Draw pseudo-random number `r_{n+1}` using Merkle root of `ΔL_{n-1}` as PRNG seed.
/// - For each worker id `Id_i`, compute `u_i = Id_i XOR r_{n+1}`.
/// - Sort `u_i` ascending and pick the first `P` corresponding workers as producers.
pub fn select_producers_for_next_cycle(
    worker_pool: &[WorkerId],
    prev_cycle_merkle_root: &BlockHash,
    producer_count: usize,
) -> Vec<WorkerId> {
    let r = producer_selection_r_n_plus_1(prev_cycle_merkle_root);

    let mut scored: Vec<(Hash32, WorkerId)> = worker_pool
        .iter()
        .cloned()
        .map(|id| (xor32(id, r), id))
        .collect();

    scored.sort_by(|(a, _), (b, _)| a.cmp(b));

    scored
        .into_iter()
        .take(producer_count.min(worker_pool.len()))
        .map(|(_, id)| id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn producer_selection_is_deterministic_and_bounded() {
        let workers: Vec<WorkerId> = (0u8..10u8)
            .map(|i| {
                let mut id = [0u8; 32];
                id[0] = i;
                id
            })
            .collect();

        let seed: BlockHash = [42u8; 32];

        let a = select_producers_for_next_cycle(&workers, &seed, 4);
        let b = select_producers_for_next_cycle(&workers, &seed, 4);
        assert_eq!(a, b);
        assert_eq!(a.len(), 4);

        // If we ask for more than we have, we should just get all of them.
        let all = select_producers_for_next_cycle(&workers, &seed, 999);
        assert_eq!(all.len(), workers.len());
    }

    #[test]
    fn producer_selection_r_is_domain_separated_and_stable() {
        // Fixed test vector to prevent accidental hash algorithm drift.
        // Computed as: blake2b_512(seed || PRODUCER_SELECTION_DOMAIN_TAG) truncated to 32 bytes.
        let seed: BlockHash = [42u8; 32];
        let r = producer_selection_r_n_plus_1(&seed);
        assert_eq!(
            hex::encode(r),
            "aa206f69386f0622bcae8aca302c10e7b8bd6b238b512f8180d98248267123ac"
        );
    }

    #[test]
    fn producer_selection_r_differs_from_legacy_prng() {
        // Ensure domain separation is effective (and we don't accidentally use the legacy tag).
        let seed: BlockHash = [42u8; 32];
        let a = producer_selection_r_n_plus_1(&seed);
        let b = prng_32(&seed);
        assert_ne!(a, b);
    }

    #[test]
    fn tx_sender_pubkey_is_determined_for_single_sender_transfer() {
        let from = [1u8; 32];
        let to = [2u8; 32];
        let tx = Transaction {
            core: TransactionCore {
                tx_type: TransactionType::NonConfidentialTransfer,
                entries: vec![
                    TransactionEntry {
                        public_key: from,
                        amount: EntryAmount::NonConfidential(-1),
                    },
                    TransactionEntry {
                        public_key: to,
                        amount: EntryAmount::NonConfidential(1),
                    },
                ],
                nonce: 1,
                lock_time: 0,
                fees: 0,
                data: Vec::new(),
            },
            signature: AggregatedSignature(vec![0u8; 64]),
            timestamp: 0,
        };
        assert_eq!(transaction_sender_pubkey(&tx), Some(from));
    }

    #[test]
    fn tx_sender_pubkey_is_none_for_multi_sender_negative_entries() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let tx = Transaction {
            core: TransactionCore {
                tx_type: TransactionType::NonConfidentialTransfer,
                entries: vec![
                    TransactionEntry {
                        public_key: a,
                        amount: EntryAmount::NonConfidential(-1),
                    },
                    TransactionEntry {
                        public_key: b,
                        amount: EntryAmount::NonConfidential(-1),
                    },
                ],
                nonce: 1,
                lock_time: 0,
                fees: 0,
                data: Vec::new(),
            },
            signature: AggregatedSignature(vec![0u8; 64]),
            timestamp: 0,
        };
        assert_eq!(transaction_sender_pubkey(&tx), None);
    }
}

fn xor32(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = a[i] ^ b[i];
    }
    out
}

/// Domain separation tag for producer selection randomness derivation.
///
/// This is part of "Wire v1" consensus-critical hashing rules.
const PRODUCER_SELECTION_DOMAIN_TAG: &[u8] = b"catalyst:producer_selection:r_n_plus_1:v1";

/// Deterministically derive `r_{n+1}` (32 bytes) from a 32-byte previous-cycle commitment.
///
/// Spec intent (Consensus v1.2 §2.2.1):
/// - seed should be derived from the previous cycle’s commitment (paper mentions Merkle root)
///
/// Current implementation contract:
/// - caller supplies a 32-byte commitment (for now we accept the previous LSU hash/state commitment
///   as the available 32-byte value in the node; later we should switch to the paper’s exact root
///   once implemented in storage/consensus).
/// - `r_{n+1} = blake2b_256(seed || DOMAIN_TAG)` where `blake2b_256` means truncation of Blake2b-512.
fn producer_selection_r_n_plus_1(seed: &[u8; 32]) -> [u8; 32] {
    use blake2::{Blake2b512, Digest};

    let mut h = Blake2b512::new();
    h.update(seed);
    h.update(PRODUCER_SELECTION_DOMAIN_TAG);
    let out = h.finalize();
    let mut r = [0u8; 32];
    r.copy_from_slice(&out[..32]);
    r
}

/// Simple deterministic PRNG to derive 32 bytes from a 32-byte seed.
///
/// NOTE: Replace with the paper’s full hashing/PRNG approach once the crypto crate
/// defines it. This is stable and deterministic for tests.
fn prng_32(seed: &[u8; 32]) -> [u8; 32] {
    use blake2::{Blake2b512, Digest};

    // NOTE: Consensus paper v1.2 §1.2 specifies Blake2b as the hashing choice.
    // For now we follow the same convention used elsewhere in the repo:
    // `blake2b_256(x) := first_32_bytes(blake2b_512(x))`
    //
    // Domain separation tag (legacy placeholder).
    let mut h = Blake2b512::new();
    h.update(seed);
    h.update(b"catalyst-prng-32");
    let out = h.finalize();
    let mut r = [0u8; 32];
    r.copy_from_slice(&out[..32]);
    r
}

