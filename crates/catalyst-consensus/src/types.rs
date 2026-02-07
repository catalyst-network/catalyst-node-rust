use catalyst_utils::{
    Hash, Address, PublicKey, CatalystSerialize, CatalystDeserialize, impl_catalyst_serialize,
    NetworkMessage, MessageType,
};
use catalyst_utils::network::MessagePriority;
// Keep signatures as raw bytes at the network/consensus boundary; the crypto crate
// owns the typed signature format.
use serde::{Serialize, Deserialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Producer unique identifier
pub type ProducerId = String;

/// Ledger cycle number
pub type CycleNumber = u64;

/// Configuration for consensus protocol
#[derive(Debug, Clone)]
pub struct ConsensusConfig {
    pub cycle_duration_ms: u64,
    pub construction_phase_ms: u64,
    pub campaigning_phase_ms: u64,
    pub voting_phase_ms: u64,
    pub synchronization_phase_ms: u64,
    pub freeze_window_ms: u64,
    pub min_producers: usize,
    pub confidence_threshold: f64,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            cycle_duration_ms: 60000,  // 1 minute
            construction_phase_ms: 15000,  // 15 seconds
            campaigning_phase_ms: 15000,   // 15 seconds
            voting_phase_ms: 15000,        // 15 seconds
            synchronization_phase_ms: 15000, // 15 seconds
            freeze_window_ms: 5000,        // 5 seconds
            min_producers: 3,
            confidence_threshold: 0.75,    // 75% supermajority
        }
    }
}

/// Transaction entry in ledger state update
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionEntry {
    pub public_key: PublicKey,
    pub amount: i64,  // Can be positive (receive) or negative (spend)
    /// Partial / per-entry signature bytes (paper uses 64 bytes for signatures).
    pub signature: Vec<u8>,
}

impl_catalyst_serialize!(TransactionEntry, public_key, amount, signature);

/// Partial ledger state update (without compensation entries)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartialLedgerStateUpdate {
    pub transaction_entries: Vec<TransactionEntry>,
    pub transaction_signatures_hash: Hash,
    pub total_fees: u64,
    pub timestamp: u64,
}

impl_catalyst_serialize!(PartialLedgerStateUpdate, transaction_entries, transaction_signatures_hash, total_fees, timestamp);

/// Compensation entry for rewarding producers
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompensationEntry {
    pub producer_id: ProducerId,
    pub public_key: PublicKey,
    pub amount: u64,
}

impl_catalyst_serialize!(CompensationEntry, producer_id, public_key, amount);

/// Complete ledger state update (includes compensation entries)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerStateUpdate {
    pub partial_update: PartialLedgerStateUpdate,
    pub compensation_entries: Vec<CompensationEntry>,
    pub cycle_number: CycleNumber,
    pub producer_list: Vec<ProducerId>,
    pub vote_list: Vec<ProducerId>,
}

impl_catalyst_serialize!(LedgerStateUpdate, partial_update, compensation_entries, cycle_number, producer_list, vote_list);

/// Producer quantity (Construction phase)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProducerQuantity {
    pub first_hash: Hash,
    pub cycle_number: CycleNumber,
    pub producer_id: ProducerId,
    pub timestamp: u64,
}

impl NetworkMessage for ProducerQuantity {
    fn serialize(&self) -> catalyst_utils::CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(self)
    }
    
    fn deserialize(data: &[u8]) -> catalyst_utils::CatalystResult<Self> {
        CatalystDeserialize::deserialize(data)
    }
    
    fn message_type(&self) -> MessageType {
        MessageType::ProducerQuantity
    }
    
    fn priority(&self) -> u8 {
        MessagePriority::High as u8
    }
}

impl_catalyst_serialize!(ProducerQuantity, first_hash, cycle_number, producer_id, timestamp);

/// Producer candidate (Campaigning phase)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProducerCandidate {
    pub majority_hash: Hash,
    pub producer_list_hash: Hash,
    pub cycle_number: CycleNumber,
    pub producer_id: ProducerId,
    pub timestamp: u64,
}

impl NetworkMessage for ProducerCandidate {
    fn serialize(&self) -> catalyst_utils::CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(self)
    }
    
    fn deserialize(data: &[u8]) -> catalyst_utils::CatalystResult<Self> {
        CatalystDeserialize::deserialize(data)
    }
    
    fn message_type(&self) -> MessageType {
        MessageType::ProducerCandidate
    }
    
    fn priority(&self) -> u8 {
        MessagePriority::High as u8
    }
}

impl_catalyst_serialize!(ProducerCandidate, majority_hash, producer_list_hash, cycle_number, producer_id, timestamp);

/// Producer vote (Voting phase)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProducerVote {
    pub ledger_state_hash: Hash,
    pub vote_list_hash: Hash,
    pub cycle_number: CycleNumber,
    pub producer_id: ProducerId,
    pub timestamp: u64,
}

impl NetworkMessage for ProducerVote {
    fn serialize(&self) -> catalyst_utils::CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(self)
    }
    
    fn deserialize(data: &[u8]) -> catalyst_utils::CatalystResult<Self> {
        CatalystDeserialize::deserialize(data)
    }
    
    fn message_type(&self) -> MessageType {
        MessageType::ProducerVote
    }
    
    fn priority(&self) -> u8 {
        MessagePriority::High as u8
    }
}

impl_catalyst_serialize!(ProducerVote, ledger_state_hash, vote_list_hash, cycle_number, producer_id, timestamp);

/// Producer output (Synchronization phase)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProducerOutput {
    pub dfs_address: Address,
    pub vote_list_hash: Hash,
    pub cycle_number: CycleNumber,
    pub producer_id: ProducerId,
    pub timestamp: u64,
}

impl NetworkMessage for ProducerOutput {
    fn serialize(&self) -> catalyst_utils::CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(self)
    }
    
    fn deserialize(data: &[u8]) -> catalyst_utils::CatalystResult<Self> {
        CatalystDeserialize::deserialize(data)
    }
    
    fn message_type(&self) -> MessageType {
        MessageType::ProducerOutput
    }
    
    fn priority(&self) -> u8 {
        MessagePriority::High as u8
    }
}

impl_catalyst_serialize!(ProducerOutput, dfs_address, vote_list_hash, cycle_number, producer_id, timestamp);

/// Current timestamp in milliseconds
pub fn current_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Hash a data structure
pub fn hash_data<T: CatalystSerialize>(data: &T) -> catalyst_utils::CatalystResult<Hash> {
    use blake2::{Blake2b512, Digest};
    
    let serialized = data.serialize()?;
    let mut hasher = Blake2b512::new();
    hasher.update(&serialized);
    let result = hasher.finalize();
    
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result[..32]);
    Ok(hash)
}