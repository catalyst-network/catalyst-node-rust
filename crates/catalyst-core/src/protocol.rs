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

/// Deterministic minimum fee schedule for the current implementation.
///
/// This is intentionally simple and “testnet-ready”:
/// - it enables anti-spam by requiring a non-zero fee floor
/// - it is fully deterministic from the `TransactionCore` fields
/// - it does not yet model gas used / basefee dynamics (tracked under economics epic)
pub fn min_fee_for_core(core: &TransactionCore) -> u64 {
    // Base fee per tx type (units: base token “atoms”; currently the same unit as balances).
    let base: u64 = match core.tx_type {
        TransactionType::NonConfidentialTransfer => 1,
        TransactionType::SmartContract => 5,
        TransactionType::WorkerRegistration => 1,
        // Other tx types are not wired end-to-end yet; keep a small floor.
        _ => 1,
    };

    // Small per-entry overhead to discourage huge fanout.
    let per_entry: u64 = 1;

    base.saturating_add(per_entry.saturating_mul(core.entries.len() as u64))
}

pub fn min_fee(tx: &Transaction) -> u64 {
    min_fee_for_core(&tx.core)
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
    let r = prng_32(prev_cycle_merkle_root);

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
}

fn xor32(a: [u8; 32], b: [u8; 32]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for i in 0..32 {
        out[i] = a[i] ^ b[i];
    }
    out
}

/// Simple deterministic PRNG to derive 32 bytes from a 32-byte seed.
///
/// NOTE: Replace with the paper’s full hashing/PRNG approach once the crypto crate
/// defines it. This is stable and deterministic for tests.
fn prng_32(seed: &[u8; 32]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(seed);
    h.update(b"catalyst-prng-32");
    let out = h.finalize();
    let mut r = [0u8; 32];
    r.copy_from_slice(&out[..32]);
    r
}

