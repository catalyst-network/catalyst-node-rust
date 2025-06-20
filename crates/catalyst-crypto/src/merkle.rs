use crate::{Hash, hash::sha256};
use serde::{Deserialize, Serialize};

/// Merkle tree implementation for efficient verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleTree {
    pub root: Hash,
    pub leaves: Vec<Hash>,
    pub layers: Vec<Vec<Hash>>,
}

impl MerkleTree {
    /// Build a Merkle tree from leaf data
    pub fn new(leaf_data: Vec<Vec<u8>>) -> Self {
        if leaf_data.is_empty() {
            // Empty tree has zero hash as root
            return Self {
                root: Hash::new([0u8; 32]),
                leaves: Vec::new(),
                layers: Vec::new(),
            };
        }

        // Hash all leaf data
        let leaves: Vec<Hash> = leaf_data.iter().map(|data| sha256(data)).collect();
        
        let mut layers = vec![leaves.clone()];
        let mut current_layer = leaves.clone();

        // Build tree bottom-up
        while current_layer.len() > 1 {
            let mut next_layer = Vec::new();
            
            for chunk in current_layer.chunks(2) {
                let hash = if chunk.len() == 2 {
                    hash_pair(&chunk[0], &chunk[1])
                } else {
                    // Odd number of nodes - hash with itself
                    hash_pair(&chunk[0], &chunk[0])
                };
                next_layer.push(hash);
            }
            
            layers.push(next_layer.clone());
            current_layer = next_layer;
        }

        let root = current_layer[0];

        Self {
            root,
            leaves,
            layers,
        }
    }

    /// Generate a proof for a leaf at the given index
    pub fn generate_proof(&self, leaf_index: usize) -> Option<MerkleProof> {
        if leaf_index >= self.leaves.len() {
            return None;
        }

        let mut proof = MerkleProof::new(self.leaves[leaf_index]);
        let mut current_index = leaf_index;

        // Build proof path from leaf to root
        for layer in &self.layers[..self.layers.len() - 1] {
            let sibling_index = if current_index % 2 == 0 {
                current_index + 1
            } else {
                current_index - 1
            };

            if sibling_index < layer.len() {
                let is_right = current_index % 2 == 0;
                proof.add_sibling(layer[sibling_index], is_right);
            }

            current_index /= 2;
        }

        Some(proof)
    }

    /// Verify that a leaf is included in the tree
    pub fn verify_inclusion(&self, _leaf: &Hash, proof: &MerkleProof) -> bool {
        proof.verify(&self.root)
    }
}

/// Merkle proof for inclusion verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf: Hash,
    pub path: Vec<Hash>,
    pub indices: Vec<bool>, // true = right, false = left
}

impl MerkleProof {
    pub fn new(leaf: Hash) -> Self {
        Self {
            leaf,
            path: Vec::new(),
            indices: Vec::new(),
        }
    }
    
    pub fn add_sibling(&mut self, sibling: Hash, is_right: bool) {
        self.path.push(sibling);
        self.indices.push(is_right);
    }
    
    pub fn verify(&self, root: &Hash) -> bool {
        let mut current = self.leaf;
        
        for (sibling, &is_right) in self.path.iter().zip(self.indices.iter()) {
            current = if is_right {
                // Current is left, sibling is right
                hash_pair(&current, sibling)
            } else {
                // Current is right, sibling is left  
                hash_pair(sibling, &current)
            };
        }
        
        current == *root
    }
}

/// Hash two values together for Merkle tree construction
fn hash_pair(left: &Hash, right: &Hash) -> Hash {
    let mut combined = Vec::new();
    combined.extend_from_slice(left.as_bytes());
    combined.extend_from_slice(right.as_bytes());
    sha256(&combined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_tree_construction() {
        let data = vec![
            b"data1".to_vec(),
            b"data2".to_vec(),
            b"data3".to_vec(),
            b"data4".to_vec(),
        ];

        let tree = MerkleTree::new(data);
        assert_eq!(tree.leaves.len(), 4);
        assert!(tree.layers.len() > 1);
    }

    #[test]
    fn test_merkle_proof() {
        let data = vec![
            b"data1".to_vec(),
            b"data2".to_vec(),
            b"data3".to_vec(),
            b"data4".to_vec(),
        ];

        let tree = MerkleTree::new(data);
        let proof = tree.generate_proof(0).unwrap();
        
        assert!(tree.verify_inclusion(&tree.leaves[0], &proof));
        assert!(proof.verify(&tree.root));
    }

    #[test]
    fn test_empty_tree() {
        let tree = MerkleTree::new(vec![]);
        assert_eq!(tree.root, Hash::new([0u8; 32]));
        assert_eq!(tree.leaves.len(), 0);
    }

    #[test]
    fn test_single_leaf() {
        let data = vec![b"single".to_vec()];
        let tree = MerkleTree::new(data);
        
        assert_eq!(tree.leaves.len(), 1);
        assert_eq!(tree.root, tree.leaves[0]);
    }
}