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

use catalyst_utils::{
    CatalystDeserialize, CatalystSerialize, impl_catalyst_serialize,
    error::{CatalystError, CatalystResult},
};

pub const TX_WIRE_MAGIC_V1: [u8; 4] = *b"CTX1";
pub const TX_SIG_DOMAIN_V1: &[u8] = b"CATALYST_SIG_V1";
pub const TX_WIRE_MAGIC_V2: [u8; 4] = *b"CTX2";
pub const TX_SIG_DOMAIN_V2: &[u8] = b"CATALYST_SIG_V2";

// Safety limits for tx parsing/validation (anti-DoS).
// These are conservative defaults for public testnets.
pub const MAX_TX_WIRE_BYTES: usize = 64 * 1024; // 64 KiB (hex decoded, including magic)
pub const MAX_TX_SIGNATURE_BYTES: usize = 8 * 1024; // room for PQ sigs later
pub const MAX_TX_SENDER_PUBKEY_BYTES: usize = 4 * 1024; // room for PQ pubkeys later

/// Signature scheme id (forward-compatible).
///
/// For now, only Schnorr is accepted by consensus/RPC.
pub mod sig_scheme {
    /// Schnorr signature over Catalyst v1/v2 signing payload (current).
    pub const SCHNORR_V1: u8 = 0;
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

/// V1 signing payload for external wallets: deterministic and domain-separated.
///
/// Format:
/// - domain = "CATALYST_SIG_V1"
/// - chain_id (u64 le)
/// - genesis_hash (32 bytes)
/// - core (CatalystSerialize)
/// - timestamp (u64 le)
pub fn transaction_signing_payload_v1(
    core: &TransactionCore,
    timestamp: u64,
    chain_id: u64,
    genesis_hash: [u8; 32],
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(TX_SIG_DOMAIN_V1);
    out.extend_from_slice(&chain_id.to_le_bytes());
    out.extend_from_slice(&genesis_hash);
    let core_bytes = CatalystSerialize::serialize(core)
        .map_err(|e| format!("serialize core v1: {e}"))?;
    out.extend_from_slice(&core_bytes);
    out.extend_from_slice(&timestamp.to_le_bytes());
    Ok(out)
}

/// V2 signing payload for external wallets: deterministic and forward-compatible.
///
/// Format:
/// - domain = "CATALYST_SIG_V2"
/// - chain_id (u64 le)
/// - genesis_hash (32 bytes)
/// - signature_scheme (u8)
/// - sender_pubkey (Option<Vec<u8>>)   (reserved for PQ / non-32-byte pubkeys)
/// - core (CatalystSerialize)
/// - timestamp (u64 le)
pub fn transaction_signing_payload_v2(
    core: &TransactionCore,
    timestamp: u64,
    chain_id: u64,
    genesis_hash: [u8; 32],
    signature_scheme: u8,
    sender_pubkey: &Option<Vec<u8>>,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(TX_SIG_DOMAIN_V2);
    out.extend_from_slice(&chain_id.to_le_bytes());
    out.extend_from_slice(&genesis_hash);
    out.push(signature_scheme);
    let pk_bytes = catalyst_utils::CatalystSerialize::serialize(sender_pubkey)
        .map_err(|e| format!("serialize sender_pubkey: {e}"))?;
    out.extend_from_slice(&pk_bytes);
    let core_bytes = CatalystSerialize::serialize(core)
        .map_err(|e| format!("serialize core v2: {e}"))?;
    out.extend_from_slice(&core_bytes);
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

impl CatalystSerialize for TransactionType {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        let mut out = Vec::with_capacity(1);
        self.serialize_to(&mut out)?;
        Ok(out)
    }

    fn serialize_to<W: std::io::Write>(&self, writer: &mut W) -> CatalystResult<()> {
        let tag: u8 = match self {
            TransactionType::NonConfidentialTransfer => 0,
            TransactionType::ConfidentialTransfer => 1,
            TransactionType::DataStorageRequest => 2,
            TransactionType::DataStorageRetrieve => 3,
            TransactionType::SmartContract => 4,
            TransactionType::WorkerRegistration => 5,
        };
        tag.serialize_to(writer)
    }

    fn serialized_size(&self) -> usize {
        1
    }
}

impl CatalystDeserialize for TransactionType {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        let mut cursor = std::io::Cursor::new(data);
        Self::deserialize_from(&mut cursor)
    }

    fn deserialize_from<R: std::io::Read>(reader: &mut R) -> CatalystResult<Self> {
        let tag = u8::deserialize_from(reader)?;
        Ok(match tag {
            0 => TransactionType::NonConfidentialTransfer,
            1 => TransactionType::ConfidentialTransfer,
            2 => TransactionType::DataStorageRequest,
            3 => TransactionType::DataStorageRetrieve,
            4 => TransactionType::SmartContract,
            5 => TransactionType::WorkerRegistration,
            _ => {
                return Err(CatalystError::Serialization(format!(
                    "invalid TransactionType tag: {tag}"
                )))
            }
        })
    }
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

impl CatalystSerialize for EntryAmount {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        let mut out = Vec::with_capacity(self.serialized_size());
        self.serialize_to(&mut out)?;
        Ok(out)
    }

    fn serialize_to<W: std::io::Write>(&self, writer: &mut W) -> CatalystResult<()> {
        match self {
            EntryAmount::NonConfidential(v) => {
                0u8.serialize_to(writer)?;
                v.serialize_to(writer)?;
            }
            EntryAmount::Confidential {
                commitment,
                range_proof,
            } => {
                1u8.serialize_to(writer)?;
                commitment.serialize_to(writer)?;
                range_proof.serialize_to(writer)?;
            }
        }
        Ok(())
    }

    fn serialized_size(&self) -> usize {
        match self {
            EntryAmount::NonConfidential(_) => 1 + 8,
            EntryAmount::Confidential {
                commitment: _,
                range_proof,
            } => 1 + 32 + range_proof.serialized_size(),
        }
    }
}

impl CatalystDeserialize for EntryAmount {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        let mut cursor = std::io::Cursor::new(data);
        Self::deserialize_from(&mut cursor)
    }

    fn deserialize_from<R: std::io::Read>(reader: &mut R) -> CatalystResult<Self> {
        let tag = u8::deserialize_from(reader)?;
        Ok(match tag {
            0 => EntryAmount::NonConfidential(i64::deserialize_from(reader)?),
            1 => EntryAmount::Confidential {
                commitment: <[u8; 32]>::deserialize_from(reader)?,
                range_proof: Vec::<u8>::deserialize_from(reader)?,
            },
            _ => {
                return Err(CatalystError::Serialization(format!(
                    "invalid EntryAmount tag: {tag}"
                )))
            }
        })
    }
}

/// Transaction entry (§4.3): public key + amount.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionEntry {
    /// 32-byte public key (paper §4.3).
    pub public_key: [u8; 32],
    pub amount: EntryAmount,
}

impl_catalyst_serialize!(TransactionEntry, public_key, amount);

/// Aggregated signature (§4.4). Opaque 64-byte signature for now.
///
/// NOTE: We store this as `Vec<u8>` because `serde` doesn't implement (de)serialization
/// for arrays > 32 by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregatedSignature(pub Vec<u8>);

impl CatalystSerialize for AggregatedSignature {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(&self.0)
    }

    fn serialize_to<W: std::io::Write>(&self, writer: &mut W) -> CatalystResult<()> {
        self.0.serialize_to(writer)
    }

    fn serialized_size(&self) -> usize {
        self.0.serialized_size()
    }
}

impl CatalystDeserialize for AggregatedSignature {
    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        Ok(Self(<Vec<u8> as CatalystDeserialize>::deserialize(data)?))
    }

    fn deserialize_from<R: std::io::Read>(reader: &mut R) -> CatalystResult<Self> {
        Ok(Self(Vec::<u8>::deserialize_from(reader)?))
    }
}

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

impl_catalyst_serialize!(TransactionCore, tx_type, entries, nonce, lock_time, fees, data);

/// Catalyst transaction (§4.2): core + aggregated signature + timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionV1 {
    pub core: TransactionCore,
    pub signature: AggregatedSignature,
    /// Timestamp (paper uses 4 bytes). We keep u64 for convenience.
    pub timestamp: u64,
}

impl_catalyst_serialize!(TransactionV1, core, signature, timestamp);

/// Catalyst transaction v2: adds signature scheme id and optional sender pubkey blob (for PQ).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transaction {
    pub core: TransactionCore,
    /// Signature scheme id (see `sig_scheme`).
    pub signature_scheme: u8,
    pub signature: AggregatedSignature,
    /// Optional sender public key blob (used when sender address is a hash of a larger key).
    pub sender_pubkey: Option<Vec<u8>>,
    /// Timestamp (paper uses 4 bytes). We keep u64 for convenience.
    pub timestamp: u64,
}

impl_catalyst_serialize!(Transaction, core, signature_scheme, signature, sender_pubkey, timestamp);

pub fn encode_wire_tx_v1(tx: &Transaction) -> Result<Vec<u8>, String> {
    // v1 wire format is defined over TransactionV1.
    let v1 = TransactionV1 {
        core: tx.core.clone(),
        signature: tx.signature.clone(),
        timestamp: tx.timestamp,
    };
    let mut out = Vec::new();
    out.extend_from_slice(&TX_WIRE_MAGIC_V1);
    let body = CatalystSerialize::serialize(&v1).map_err(|e| format!("serialize tx v1: {e}"))?;
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn encode_wire_tx_v2(tx: &Transaction) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    out.extend_from_slice(&TX_WIRE_MAGIC_V2);
    let body = CatalystSerialize::serialize(tx).map_err(|e| format!("serialize tx v2: {e}"))?;
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode_wire_tx_any(bytes: &[u8]) -> Result<Transaction, String> {
    if bytes.len() > MAX_TX_WIRE_BYTES {
        return Err(format!(
            "tx too large: bytes={} max={}",
            bytes.len(),
            MAX_TX_WIRE_BYTES
        ));
    }
    if bytes.len() >= 4 && bytes[0..4] == TX_WIRE_MAGIC_V2 {
        return <Transaction as CatalystDeserialize>::deserialize(&bytes[4..])
            .map_err(|e| format!("decode v2 tx: {e}"));
    }
    if bytes.len() >= 4 && bytes[0..4] == TX_WIRE_MAGIC_V1 {
        let v1 = <TransactionV1 as CatalystDeserialize>::deserialize(&bytes[4..])
            .map_err(|e| format!("decode v1 tx: {e}"))?;
        return Ok(Transaction {
            core: v1.core,
            signature_scheme: sig_scheme::SCHNORR_V1,
            signature: v1.signature,
            sender_pubkey: None,
            timestamp: v1.timestamp,
        });
    }
    // Legacy: bincode(Transaction)
    let v1 = bincode::deserialize::<TransactionV1>(bytes).map_err(|e| format!("decode legacy tx: {e}"))?;
    Ok(Transaction {
        core: v1.core,
        signature_scheme: sig_scheme::SCHNORR_V1,
        signature: v1.signature,
        sender_pubkey: None,
        timestamp: v1.timestamp,
    })
}

pub fn tx_id_v1(tx: &Transaction) -> Result<[u8; 32], String> {
    use blake2::{Blake2b512, Digest};
    let bytes = encode_wire_tx_v1(tx)?;
    let mut h = Blake2b512::new();
    h.update(&bytes);
    let out = h.finalize();
    let mut id = [0u8; 32];
    id.copy_from_slice(&out[..32]);
    Ok(id)
}

pub fn tx_id_v2(tx: &Transaction) -> Result<[u8; 32], String> {
    use blake2::{Blake2b512, Digest};
    let bytes = encode_wire_tx_v2(tx)?;
    let mut h = Blake2b512::new();
    h.update(&bytes);
    let out = h.finalize();
    let mut id = [0u8; 32];
    id.copy_from_slice(&out[..32]);
    Ok(id)
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

/// Transaction status as observed by a node.
///
/// This is a pragmatic “receipt” state machine for testnet ergonomics; it is not yet a
/// full mainnet finality model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxStatus {
    /// Accepted for gossip / mempool, but not yet included in an applied cycle.
    Pending,
    /// Selected into a cycle’s batch (best-effort; may still fail to apply).
    Selected,
    /// Included in a cycle that was applied locally (and therefore part of the node’s head).
    Applied,
    /// Explicitly dropped/rejected (optional; not all paths set this).
    Dropped,
}

/// Persisted transaction metadata used to serve receipt-like RPCs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TxMeta {
    /// 32-byte tx id (blake2b512(bincode(tx))[..32] in the current node implementation).
    pub tx_id: [u8; 32],
    pub status: TxStatus,
    pub received_at_ms: u64,
    pub sender: Option<[u8; 32]>,
    pub nonce: u64,
    pub fees: u64,
    pub selected_cycle: Option<u64>,
    pub applied_cycle: Option<u64>,
    pub applied_lsu_hash: Option<[u8; 32]>,
    pub applied_state_root: Option<[u8; 32]>,
    /// Whether the tx executed successfully (best-effort).
    pub applied_success: Option<bool>,
    /// Optional error/revert string (best-effort).
    pub applied_error: Option<String>,
    /// EVM-only: gas used (if available).
    pub evm_gas_used: Option<u64>,
    /// EVM-only: return data (raw bytes).
    pub evm_return: Option<Vec<u8>>,
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
        // Generic safety caps (anti-DoS). Scheme-specific validation belongs in `catalyst-crypto::txauth`.
        if self.signature.0.is_empty() {
            return Err("Transaction signature must be non-empty".to_string());
        }
        if self.signature.0.len() > MAX_TX_SIGNATURE_BYTES {
            return Err("Transaction signature is too large".to_string());
        }
        if let Some(pk) = &self.sender_pubkey {
            if pk.len() > MAX_TX_SENDER_PUBKEY_BYTES {
                return Err("Transaction sender_pubkey is too large".to_string());
            }
        }
        Ok(())
    }

    pub fn signing_payload(&self) -> Result<Vec<u8>, String> {
        transaction_signing_payload(&self.core, self.timestamp)
    }

    pub fn signing_payload_v1(&self, chain_id: u64, genesis_hash: [u8; 32]) -> Result<Vec<u8>, String> {
        transaction_signing_payload_v1(&self.core, self.timestamp, chain_id, genesis_hash)
    }

    pub fn signing_payload_v2(&self, chain_id: u64, genesis_hash: [u8; 32]) -> Result<Vec<u8>, String> {
        transaction_signing_payload_v2(
            &self.core,
            self.timestamp,
            chain_id,
            genesis_hash,
            self.signature_scheme,
            &self.sender_pubkey,
        )
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

    #[test]
    fn wallet_tx_v2_wire_roundtrip_and_txid_is_deterministic() {
        let from = [1u8; 32];
        let to = [2u8; 32];
        let tx = Transaction {
            core: TransactionCore {
                tx_type: TransactionType::NonConfidentialTransfer,
                entries: vec![
                    TransactionEntry {
                        public_key: from,
                        amount: EntryAmount::NonConfidential(-7),
                    },
                    TransactionEntry {
                        public_key: to,
                        amount: EntryAmount::NonConfidential(7),
                    },
                ],
                nonce: 1,
                lock_time: 0,
                fees: 3,
                data: Vec::new(),
            },
            signature_scheme: sig_scheme::SCHNORR_V1,
            signature: AggregatedSignature(vec![0u8; 64]),
            sender_pubkey: None,
            timestamp: 1_700_000_000_000u64,
        };

        let wire = encode_wire_tx_v2(&tx).unwrap();
        assert!(wire.starts_with(&TX_WIRE_MAGIC_V2));
        let decoded = decode_wire_tx_any(&wire).unwrap();
        assert_eq!(decoded, tx);

        let txid1 = tx_id_v2(&tx).unwrap();
        let txid2 = tx_id_v2(&tx).unwrap();
        assert_eq!(txid1, txid2);
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

