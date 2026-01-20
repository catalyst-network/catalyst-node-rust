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
    /// Authenticated state root after applying this LSU.
    pub state_root: Hash,
}

impl_catalyst_serialize!(LsuCidGossip, cycle, lsu_hash, cid, state_root);

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

