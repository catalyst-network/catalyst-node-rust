use crate::merkle::{leaf_hash_for_kv, MerkleProof, MerkleStep};
use crate::{StorageError, StorageResult};
use catalyst_utils::Hash;
use blake2::Blake2b;
use blake2::digest::{consts::U32, Digest};
use std::collections::{BTreeMap, BTreeSet};

const TREE_DEPTH: usize = 256;

fn h_node(left: &Hash, right: &Hash) -> Hash {
    // Domain-separated internal node hash: H(0x01 || left || right)
    let mut h = Blake2b::<U32>::new();
    h.update([1u8]);
    h.update(left);
    h.update(right);
    h.finalize().into()
}

fn h_empty_leaf() -> Hash {
    // Domain-separated empty leaf hash: H(0x02)
    let mut h = Blake2b::<U32>::new();
    h.update([2u8]);
    h.finalize().into()
}

pub fn empty_leaf_hash() -> Hash {
    h_empty_leaf()
}

fn key_path(key: &[u8]) -> [u8; 32] {
    // 256-bit path = blake2b-256(0x03 || key)
    let mut h = Blake2b::<U32>::new();
    h.update([3u8]);
    h.update(key);
    h.finalize().into()
}

fn idx_lsb(idx: &[u8; 32]) -> u8 {
    idx[31] & 1
}

fn idx_xor1(idx: &[u8; 32]) -> [u8; 32] {
    let mut out = *idx;
    out[31] ^= 1;
    out
}

fn idx_shr1(idx: &[u8; 32]) -> [u8; 32] {
    // big-endian >> 1
    let mut out = [0u8; 32];
    let mut carry = 0u8;
    for (i, b) in idx.iter().enumerate() {
        let next_carry = b & 1;
        out[i] = (b >> 1) | (carry << 7);
        carry = next_carry;
    }
    out
}

fn empty_hashes() -> [Hash; TREE_DEPTH + 1] {
    // empty[h] = hash of empty subtree of height h
    // height 0 => leaf
    let mut out = [[0u8; 32]; TREE_DEPTH + 1];
    out[0] = h_empty_leaf();
    for h in 1..=TREE_DEPTH {
        out[h] = h_node(&out[h - 1], &out[h - 1]);
    }
    out
}

fn build_depth_maps(
    leaf_map: &BTreeMap<[u8; 32], Hash>,
) -> StorageResult<(Hash, Vec<BTreeMap<[u8; 32], Hash>>)> {
    let empty = empty_hashes();
    let mut depths: Vec<BTreeMap<[u8; 32], Hash>> =
        (0..=TREE_DEPTH).map(|_| BTreeMap::new()).collect();
    depths[TREE_DEPTH] = leaf_map.clone();

    for d in (1..=TREE_DEPTH).rev() {
        let mut next: BTreeMap<[u8; 32], Hash> = BTreeMap::new();
        let mut processed: BTreeSet<[u8; 32]> = BTreeSet::new();

        let cur = &depths[d];
        for (idx, hcur) in cur.iter() {
            if processed.contains(idx) {
                continue;
            }
            let sib = idx_xor1(idx);
            let hsib = cur.get(&sib).copied().unwrap_or(empty[TREE_DEPTH - d]);

            let (left, right) = if idx_lsb(idx) == 0 { (*hcur, hsib) } else { (hsib, *hcur) };
            let parent_idx = idx_shr1(idx);
            let ph = h_node(&left, &right);

            if let Some(existing) = next.get(&parent_idx) {
                if existing != &ph {
                    return Err(StorageError::internal(
                        "Sparse merkle: parent hash collision (non-deterministic leaf set?)".to_string(),
                    ));
                }
            } else {
                next.insert(parent_idx, ph);
            }

            processed.insert(*idx);
            processed.insert(sib);
        }
        depths[d - 1] = next;
    }

    let root = depths[0]
        .get(&[0u8; 32])
        .copied()
        .unwrap_or(empty[TREE_DEPTH]);
    Ok((root, depths))
}

pub fn compute_root_from_iter<I>(iter: I) -> StorageResult<Hash>
where
    I: IntoIterator<Item = (Box<[u8]>, Box<[u8]>)>,
{
    let mut leaf_map: BTreeMap<[u8; 32], Hash> = BTreeMap::new();
    for (k, v) in iter {
        let idx = key_path(&k);
        let leaf = leaf_hash_for_kv(&k, &v);
        leaf_map.insert(idx, leaf);
    }
    let (root, _depths) = build_depth_maps(&leaf_map)?;
    Ok(root)
}

pub fn compute_root_and_proof_from_iter<I>(
    iter: I,
    key: &[u8],
) -> StorageResult<Option<(Hash, Vec<u8>, MerkleProof)>>
where
    I: IntoIterator<Item = (Box<[u8]>, Box<[u8]>)>,
{
    let mut leaf_map: BTreeMap<[u8; 32], Hash> = BTreeMap::new();
    let mut target_value: Option<Vec<u8>> = None;

    for (k, v) in iter {
        if k.as_ref() == key {
            target_value = Some(v.to_vec());
        }
        let idx = key_path(&k);
        let leaf = leaf_hash_for_kv(&k, &v);
        leaf_map.insert(idx, leaf);
    }

    let value = match target_value {
        Some(v) => v,
        None => return Ok(None),
    };

    let (root, depths) = build_depth_maps(&leaf_map)?;
    let empty = empty_hashes();

    let mut idx = key_path(key);
    let leaf = leaf_hash_for_kv(key, &value);
    let mut steps: Vec<MerkleStep> = Vec::with_capacity(TREE_DEPTH);
    for d in (1..=TREE_DEPTH).rev() {
        let sib = idx_xor1(&idx);
        let hsib = depths[d]
            .get(&sib)
            .copied()
            .unwrap_or(empty[TREE_DEPTH - d]);
        let is_right = idx_lsb(&idx) == 1;
        steps.push(MerkleStep {
            sibling_is_left: is_right,
            sibling: hsib,
        });
        idx = idx_shr1(&idx);
    }

    Ok(Some((
        root,
        value,
        MerkleProof { leaf, steps },
    )))
}

pub fn compute_root_and_multi_proofs_from_iter<I>(
    iter: I,
    keys: &[Vec<u8>],
) -> StorageResult<(Hash, Vec<(Vec<u8>, Option<Vec<u8>>, MerkleProof)>)>
where
    I: IntoIterator<Item = (Box<[u8]>, Box<[u8]>)>,
{
    let mut leaf_map: BTreeMap<[u8; 32], Hash> = BTreeMap::new();
    let mut values: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();

    for (k, v) in iter {
        values.insert(k.to_vec(), v.to_vec());
        let idx = key_path(&k);
        let leaf = leaf_hash_for_kv(&k, &v);
        leaf_map.insert(idx, leaf);
    }

    let (root, depths) = build_depth_maps(&leaf_map)?;
    let empty = empty_hashes();

    let mut out: Vec<(Vec<u8>, Option<Vec<u8>>, MerkleProof)> = Vec::with_capacity(keys.len());
    for key in keys {
        let idx0 = key_path(key);
        let maybe_value = values.get(key).cloned();
        let leaf = match &maybe_value {
            Some(v) => leaf_hash_for_kv(key, v),
            None => h_empty_leaf(),
        };

        let mut idx = idx0;
        let mut steps: Vec<MerkleStep> = Vec::with_capacity(TREE_DEPTH);
        for d in (1..=TREE_DEPTH).rev() {
            let sib = idx_xor1(&idx);
            let hsib = depths[d]
                .get(&sib)
                .copied()
                .unwrap_or(empty[TREE_DEPTH - d]);
            let is_right = idx_lsb(&idx) == 1;
            steps.push(MerkleStep {
                sibling_is_left: is_right,
                sibling: hsib,
            });
            idx = idx_shr1(&idx);
        }

        out.push((key.clone(), maybe_value, MerkleProof { leaf, steps }));
    }

    Ok((root, out))
}



/// Verify a sparse merkle proof against the canonical key path.
///
/// Important: callers MUST NOT trust `proof.steps[*].sibling_is_left` as input from an untrusted party.
/// We recompute left/right ordering from the key path bits.
pub fn verify_proof_for_key_value(
    root: &Hash,
    key: &[u8],
    value: Option<&[u8]>,
    proof: &MerkleProof,
) -> bool {
    if proof.steps.len() != TREE_DEPTH {
        return false;
    }

    let mut cur: Hash = match value {
        Some(v) => leaf_hash_for_kv(key, v),
        None => h_empty_leaf(),
    };

    let mut idx = key_path(key);
    for step in &proof.steps {
        // Derive direction from key path bit.
        let is_right = idx_lsb(&idx) == 1;
        cur = if is_right {
            h_node(&step.sibling, &cur)
        } else {
            h_node(&cur, &step.sibling)
        };
        idx = idx_shr1(&idx);
    }

    &cur == root
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_verifies_for_present_key() {
        let items: Vec<(Box<[u8]>, Box<[u8]>)> = vec![
            (b"a".to_vec().into_boxed_slice(), b"1".to_vec().into_boxed_slice()),
            (b"b".to_vec().into_boxed_slice(), b"2".to_vec().into_boxed_slice()),
        ];
        let key = b"b";
        let Some((root, value, proof)) = compute_root_and_proof_from_iter(items, key).unwrap() else {
            panic!("expected present proof");
        };
        assert!(verify_proof_for_key_value(&root, key, Some(&value), &proof));
    }

    #[test]
    fn proof_verifies_for_absent_key() {
        let items: Vec<(Box<[u8]>, Box<[u8]>)> = vec![
            (b"a".to_vec().into_boxed_slice(), b"1".to_vec().into_boxed_slice()),
            (b"b".to_vec().into_boxed_slice(), b"2".to_vec().into_boxed_slice()),
        ];
        let key = b"missing".to_vec();
        let (root, proofs) = compute_root_and_multi_proofs_from_iter(items, &[key.clone()]).unwrap();
        let (_k, maybe_value, proof) = proofs.into_iter().next().unwrap();
        assert!(maybe_value.is_none());
        assert!(verify_proof_for_key_value(&root, &key, None, &proof));
    }
}
