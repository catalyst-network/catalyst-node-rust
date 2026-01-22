use catalyst_consensus::types::{hash_data, LedgerStateUpdate};
use catalyst_utils::{impl_catalyst_serialize, CatalystDeserialize, CatalystResult, CatalystSerialize, MessageType, NetworkMessage, Hash};
use catalyst_utils::network::MessagePriority;
use serde::{Deserialize, Serialize};

/// Gossip message carrying a DFS content address (CID) for an LSU.
///
/// In testnet, we use a shared local DFS directory (via `catalyst-dfs` local storage)
/// so nodes can fetch by CID without needing the full P2P DFS networking layer yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LsuCidGossip {
    pub cycle: u64,
    pub lsu_hash: Hash,
    pub cid: String,
    /// Previous LSU CID (cycle-1) if known, allowing late-joiners to backfill history.
    /// Encoded as "" when unknown.
    pub prev_cid: String,
    /// State root BEFORE applying this LSU (i.e., the expected prev-applied root).
    pub prev_state_root: Hash,
    /// Authenticated state root after applying this LSU.
    pub state_root: Hash,
    /// CID of a proof bundle that allows verifying the LSU transition without apply-then-check.
    ///
    /// NOTE: `impl_catalyst_serialize!` doesn't support `Option<T>`, so we encode "none" as "".
    pub proof_cid: String,
}

impl_catalyst_serialize!(LsuCidGossip, cycle, lsu_hash, cid, prev_cid, prev_state_root, state_root, proof_cid);

impl NetworkMessage for LsuCidGossip {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(self)
    }

    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        CatalystDeserialize::deserialize(data)
    }

    fn message_type(&self) -> MessageType {
        MessageType::ConsensusSync
    }

    fn priority(&self) -> u8 {
        MessagePriority::High as u8
    }

    fn ttl(&self) -> u32 {
        300
    }
}

/// Proof bundle for a single LSU state transition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateProofBundle {
    pub cycle: u64,
    pub lsu_hash: Hash,
    pub prev_state_root: Hash,
    pub new_state_root: Hash,
    pub changes: Vec<KeyProofChange>,
}

/// Proof for one touched key: old and new value proofs against prev/new roots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyProofChange {
    pub key: Vec<u8>,
    pub old_value: Vec<u8>,
    pub old_proof: catalyst_storage::merkle::MerkleProof,
    pub new_value: Vec<u8>,
    pub new_proof: catalyst_storage::merkle::MerkleProof,
}

/// Generic content request (used to fetch LSU bytes by CID over P2P).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileRequestMsg {
    pub requester: String,
    pub cid: String,
}

impl_catalyst_serialize!(FileRequestMsg, requester, cid);

impl NetworkMessage for FileRequestMsg {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(self)
    }

    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        CatalystDeserialize::deserialize(data)
    }

    fn message_type(&self) -> MessageType {
        MessageType::FileRequest
    }

    fn priority(&self) -> u8 {
        MessagePriority::High as u8
    }

    fn ttl(&self) -> u32 {
        30
    }
}

/// Generic content response (CID -> raw bytes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileResponseMsg {
    pub requester: String,
    pub cid: String,
    pub bytes: Vec<u8>,
}

impl_catalyst_serialize!(FileResponseMsg, requester, cid, bytes);

impl NetworkMessage for FileResponseMsg {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(self)
    }

    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        CatalystDeserialize::deserialize(data)
    }

    fn message_type(&self) -> MessageType {
        MessageType::FileResponse
    }

    fn priority(&self) -> u8 {
        MessagePriority::High as u8
    }

    fn ttl(&self) -> u32 {
        30
    }
}

/// Request LSU metadata (roots + prev_cid) for a given CID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LsuMetaRequest {
    pub requester: String,
    pub cid: String,
}

impl_catalyst_serialize!(LsuMetaRequest, requester, cid);

impl NetworkMessage for LsuMetaRequest {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(self)
    }

    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        CatalystDeserialize::deserialize(data)
    }

    fn message_type(&self) -> MessageType {
        MessageType::ConsensusSync
    }

    fn priority(&self) -> u8 {
        MessagePriority::High as u8
    }

    fn ttl(&self) -> u32 {
        30
    }
}

/// Gossip message carrying a full LSU for observers / late joiners.
///
/// This is a stepping-stone toward DFS-based sync: later we will gossip a DFS address (CID)
/// and fetch/verify the LSU via `catalyst-dfs`/IPFS.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LsuGossip {
    pub cycle: u64,
    pub lsu_hash: Hash,
    pub lsu: LedgerStateUpdate,
}

impl_catalyst_serialize!(LsuGossip, cycle, lsu_hash, lsu);

impl NetworkMessage for LsuGossip {
    fn serialize(&self) -> CatalystResult<Vec<u8>> {
        CatalystSerialize::serialize(self)
    }

    fn deserialize(data: &[u8]) -> CatalystResult<Self> {
        CatalystDeserialize::deserialize(data)
    }

    fn message_type(&self) -> MessageType {
        MessageType::ConsensusSync
    }

    fn priority(&self) -> u8 {
        MessagePriority::High as u8
    }

    fn ttl(&self) -> u32 {
        120
    }
}

impl LsuGossip {
    pub fn new(lsu: LedgerStateUpdate) -> CatalystResult<Self> {
        let lsu_hash = hash_data(&lsu)?;
        Ok(Self {
            cycle: lsu.cycle_number,
            lsu_hash,
            lsu,
        })
    }
}

