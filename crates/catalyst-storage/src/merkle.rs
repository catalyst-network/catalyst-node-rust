use crate::{StorageError, StorageResult};
use catalyst_utils::Hash;
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleStep {
    /// true if sibling is on the left (i.e. sibling || current), false if on the right (current || sibling)
    pub sibling_is_left: bool,
    pub sibling: Hash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf: Hash,
    pub steps: Vec<MerkleStep>,
}

fn h_leaf(key: &[u8], value: &[u8]) -> Hash {
    // Domain-separated leaf hash: H(0x00 || key_len || key || value_len || value)
    let mut h = Sha256::new();
    h.update([0u8]);
    h.update((key.len() as u64).to_le_bytes());
    h.update(key);
    h.update((value.len() as u64).to_le_bytes());
    h.update(value);
    h.finalize().into()
}

fn h_node(left: &Hash, right: &Hash) -> Hash {
    // Domain-separated internal node hash: H(0x01 || left || right)
    let mut h = Sha256::new();
    h.update([1u8]);
    h.update(left);
    h.update(right);
    h.finalize().into()
}

pub fn merkle_root(leaves: &[Hash]) -> Hash {
    if leaves.is_empty() {
        return [0u8; 32];
    }
    let mut level: Vec<Hash> = leaves.to_vec();
    while level.len() > 1 {
        let mut next: Vec<Hash> = Vec::with_capacity((level.len() + 1) / 2);
        let mut i = 0usize;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() { level[i + 1] } else { level[i] };
            next.push(h_node(&left, &right));
            i += 2;
        }
        level = next;
    }
    level[0]
}

pub fn merkle_proof(leaves: &[Hash], index: usize) -> StorageResult<MerkleProof> {
    if leaves.is_empty() {
        return Err(StorageError::internal("Cannot build proof for empty tree".to_string()));
    }
    if index >= leaves.len() {
        return Err(StorageError::internal("Merkle proof index out of bounds".to_string()));
    }

    let mut idx = index;
    let mut level: Vec<Hash> = leaves.to_vec();
    let mut steps: Vec<MerkleStep> = Vec::new();

    while level.len() > 1 {
        let is_right = idx % 2 == 1;
        let sib_idx = if is_right { idx - 1 } else { idx + 1 };
        let sibling = if sib_idx < level.len() { level[sib_idx] } else { level[idx] };
        steps.push(MerkleStep {
            sibling_is_left: is_right,
            sibling,
        });

        // build next level
        let mut next: Vec<Hash> = Vec::with_capacity((level.len() + 1) / 2);
        let mut i = 0usize;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() { level[i + 1] } else { level[i] };
            next.push(h_node(&left, &right));
            i += 2;
        }
        level = next;
        idx /= 2;
    }

    Ok(MerkleProof {
        leaf: leaves[index],
        steps,
    })
}

pub fn verify_proof(root: &Hash, proof: &MerkleProof) -> bool {
    let mut cur = proof.leaf;
    for step in &proof.steps {
        cur = if step.sibling_is_left {
            h_node(&step.sibling, &cur)
        } else {
            h_node(&cur, &step.sibling)
        };
    }
    &cur == root
}

pub fn leaf_hash_for_kv(key: &[u8], value: &[u8]) -> Hash {
    h_leaf(key, value)
}

