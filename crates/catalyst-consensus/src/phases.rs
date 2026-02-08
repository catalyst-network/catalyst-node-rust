use crate::{
    types::*, error::ConsensusError, producer::Producer
};
use catalyst_utils::{CatalystResult, Hash};
use catalyst_utils::logging::{LogCategory, log_info, log_warn, log_error};
use catalyst_utils::{increment_counter, observe_histogram, time_operation};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};
use indexmap::IndexMap;

/// Construction Phase Implementation
pub struct ConstructionPhase {
    pub(crate) producer: Producer,
}

impl ConstructionPhase {
    pub fn new(producer: Producer) -> Self {
        Self { producer }
    }

    /// Execute construction phase
    pub async fn execute(&mut self, transactions: Vec<TransactionEntry>) -> CatalystResult<ProducerQuantity> {
        log_info!(LogCategory::Consensus, "Starting construction phase for cycle {}", self.producer.cycle_number);
        increment_counter!("consensus_construction_phase_total", 1);
        
        let result = time_operation!("consensus_construction_phase_duration", {
            self.build_partial_update(transactions).await
        });
        
        match result {
            Ok(quantity) => {
                log_info!(LogCategory::Consensus, "Construction phase completed successfully");
                Ok(quantity)
            }
            Err(e) => {
                log_error!(LogCategory::Consensus, "Construction phase failed: {}", e);
                Err(e)
            }
        }
    }

    async fn build_partial_update(&mut self, transactions: Vec<TransactionEntry>) -> CatalystResult<ProducerQuantity> {
        // Deterministic ordering (spec-shaped):
        // - group/sort by transaction signature hash (paper uses a signature-hash tree `dn`)
        // - then by pubkey bytes
        // - then by amount
        //
        // Rationale: entry ordering must be stable across producers for the same frozen input set,
        // otherwise `first_hash` will never converge to a majority.
        let mut sorted_entries = transactions;
        sorted_entries.sort_by(|a, b| {
            let ha = signature_leaf_hash(&a.signature);
            let hb = signature_leaf_hash(&b.signature);
            ha.cmp(&hb)
                .then_with(|| a.public_key.cmp(&b.public_key))
                .then_with(|| a.amount.cmp(&b.amount))
                .then_with(|| a.signature.len().cmp(&b.signature.len()))
        });
        let entry_count = sorted_entries.len();

        // Create hash tree of transaction signatures (`dn` root).
        // Leaves are hash(signature_bytes), and the root is computed as a standard binary Merkle root.
        let signature_leaves: Vec<Hash> = sorted_entries
            .iter()
            .map(|e| signature_leaf_hash(&e.signature))
            .collect();
        let signatures_hash = compute_merkle_root(&signature_leaves);

        // Fees are scaffolding in the current implementation; keep this deterministic.
        // TODO(#184/#186): compute fees from tx model and enforce at mempool + formation time.
        let total_fees = 0u64;

        // Create partial ledger state update
        let partial_update = PartialLedgerStateUpdate {
            transaction_entries: sorted_entries,
            transaction_signatures_hash: signatures_hash,
            total_fees,
            // Must be deterministic across producers for the same cycle; otherwise the
            // partial update hash (first_hash) will never converge to a majority.
            timestamp: self.producer.cycle_number,
        };

        // Hash the partial update
        let first_hash = hash_data(&partial_update)?;
        
        // Store in producer cache
        self.producer.partial_update = Some(partial_update);
        self.producer.first_hash = Some(first_hash);

        // Create producer quantity
        let quantity = ProducerQuantity {
            first_hash,
            cycle_number: self.producer.cycle_number,
            producer_id: self.producer.id.clone(),
            timestamp: current_timestamp_ms(),
        };

        observe_histogram!("consensus_transaction_count", entry_count as f64);
        
        Ok(quantity)
    }
}

fn signature_leaf_hash(sig: &[u8]) -> Hash {
    use blake2::{Blake2b512, Digest};
    let mut h = Blake2b512::new();
    h.update(sig);
    let out = h.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&out[..32]);
    hash
}

fn compute_merkle_root(leaves: &[Hash]) -> Hash {
    use blake2::{Blake2b512, Digest};

    if leaves.is_empty() {
        return [0u8; 32];
    }
    if leaves.len() == 1 {
        return leaves[0];
    }

    let mut level: Vec<Hash> = leaves.to_vec();
    while level.len() > 1 {
        let mut next: Vec<Hash> = Vec::with_capacity((level.len() + 1) / 2);
        let mut i = 0usize;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() { level[i + 1] } else { level[i] };
            let mut h = Blake2b512::new();
            h.update(left);
            h.update(right);
            let out = h.finalize();
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&out[..32]);
            next.push(hash);
            i += 2;
        }
        level = next;
    }
    level[0]
}

#[cfg(test)]
mod construction_tests {
    use super::*;
    use crate::producer::Producer;

    fn entry(pk_byte: u8, amt: i64, sig_tag: u8) -> TransactionEntry {
        TransactionEntry {
            public_key: [pk_byte; 32],
            amount: amt,
            signature: vec![sig_tag; 64],
        }
    }

    #[tokio::test]
    async fn construction_is_deterministic_across_permutations() {
        let producer = Producer::new("p1".to_string(), [1u8; 32], 42);
        let mut phase = ConstructionPhase::new(producer);

        // Two "tx boundaries" (different signature tags), each with two entries.
        let a1 = entry(10, -3, 0xA1);
        let a2 = entry(11, 3, 0xA1);
        let b1 = entry(20, -7, 0xB2);
        let b2 = entry(21, 7, 0xB2);

        let base = vec![a1.clone(), a2.clone(), b1.clone(), b2.clone()];
        let perm1 = vec![b2.clone(), a2.clone(), b1.clone(), a1.clone()];
        let perm2 = vec![a2.clone(), b1.clone(), a1.clone(), b2.clone()];

        let q_base = phase.execute(base).await.unwrap();
        let first_hash_base = q_base.first_hash;
        let partial_base = phase.producer.partial_update.clone().unwrap();

        // Re-run with different orderings; same cycle/prod id should yield same first_hash + same sorted entries.
        let mut phase1 = ConstructionPhase::new(Producer::new("p1".to_string(), [1u8; 32], 42));
        let q1 = phase1.execute(perm1).await.unwrap();
        let partial1 = phase1.producer.partial_update.clone().unwrap();

        let mut phase2 = ConstructionPhase::new(Producer::new("p1".to_string(), [1u8; 32], 42));
        let q2 = phase2.execute(perm2).await.unwrap();
        let partial2 = phase2.producer.partial_update.clone().unwrap();

        assert_eq!(first_hash_base, q1.first_hash);
        assert_eq!(first_hash_base, q2.first_hash);
        assert_eq!(partial_base.transaction_entries, partial1.transaction_entries);
        assert_eq!(partial_base.transaction_entries, partial2.transaction_entries);
        assert_eq!(partial_base.transaction_signatures_hash, partial1.transaction_signatures_hash);
        assert_eq!(partial_base.transaction_signatures_hash, partial2.transaction_signatures_hash);
        assert_eq!(partial_base.total_fees, 0);
    }
}

/// Campaigning Phase Implementation
pub struct CampaigningPhase {
    pub(crate) producer: Producer,
    collected_quantities: HashMap<String, ProducerQuantity>,
}

impl CampaigningPhase {
    pub fn new(producer: Producer) -> Self {
        Self {
            producer,
            collected_quantities: HashMap::new(),
        }
    }

    /// Add a collected producer quantity
    pub fn add_quantity(&mut self, quantity: ProducerQuantity) {
        self.collected_quantities.insert(quantity.producer_id.clone(), quantity);
    }

    /// Execute campaigning phase
    pub async fn execute(&mut self, required_majority: usize) -> CatalystResult<ProducerCandidate> {
        log_info!(LogCategory::Consensus, "Starting campaigning phase for cycle {}", self.producer.cycle_number);
        increment_counter!("consensus_campaigning_phase_total", 1);
        
        let result = time_operation!("consensus_campaigning_phase_duration", {
            self.find_majority_candidate(required_majority).await
        });
        
        match result {
            Ok(candidate) => {
                log_info!(LogCategory::Consensus, "Campaigning phase completed successfully");
                Ok(candidate)
            }
            Err(e) => {
                log_error!(LogCategory::Consensus, "Campaigning phase failed: {}", e);
                Err(e)
            }
        }
    }

    pub fn expected_producers(&self) -> usize {
        self.producer.expected_producers
    }

    async fn find_majority_candidate(&mut self, required_majority: usize) -> CatalystResult<ProducerCandidate> {
        // Check minimum data requirement
        if self.collected_quantities.len() < required_majority {
            return Err(ConsensusError::InsufficientData {
                got: self.collected_quantities.len(),
                required: required_majority,
            }.into());
        }

        // Count occurrences of each first hash.
        //
        // Use BTreeMap for deterministic iteration and sort producer id lists so:
        // - candidate producer list is deterministic
        // - tie-breaking is deterministic
        let mut hash_counts: BTreeMap<Hash, Vec<String>> = BTreeMap::new();
        for (producer_id, quantity) in &self.collected_quantities {
            hash_counts
                .entry(quantity.first_hash)
                .or_insert_with(Vec::new)
                .push(producer_id.clone());
        }
        for v in hash_counts.values_mut() {
            v.sort();
            v.dedup();
        }

        // Find majority hash (required_majority is derived from membership, not from received count).
        // If there is a tie in counts, choose the smallest hash bytes (deterministic).
        let mut best_hash: Option<Hash> = None;
        let mut best_list: Vec<String> = Vec::new();
        for (h, producers) in hash_counts.iter() {
            if producers.len() > best_list.len() {
                best_hash = Some(*h);
                best_list = producers.clone();
                continue;
            }
            if producers.len() == best_list.len() {
                if best_hash.map(|bh| *h < bh).unwrap_or(true) {
                    best_hash = Some(*h);
                    best_list = producers.clone();
                }
            }
        }
        let Some(majority_hash) = best_hash else {
            return Err(ConsensusError::NoMajority {
                highest: 0,
                threshold: required_majority,
                details: format!(" (phase=campaigning cycle={})", self.producer.cycle_number),
            }.into());
        };
        let producer_list = best_list;

        if producer_list.len() < required_majority {
            log_warn!(
                LogCategory::Consensus,
                "No majority first_hash for cycle {}: need {} got {} counts={:?}",
                self.producer.cycle_number,
                required_majority,
                producer_list.len(),
                hash_counts.iter().map(|(h, ps)| (hex::encode(h), ps.len())).collect::<Vec<_>>()
            );
            return Err(ConsensusError::NoMajority {
                highest: producer_list.len(),
                threshold: required_majority,
                details: format!(
                    " (phase=campaigning cycle={} first_hash_counts={:?})",
                    self.producer.cycle_number,
                    hash_counts.iter().map(|(h, ps)| (hex::encode(h), ps.len())).collect::<Vec<_>>()
                ),
            }.into());
        }

        // Include self if we have the majority hash
        let producer_list_len = producer_list.len();
        let mut final_producer_list = producer_list;
        if let Some(our_hash) = self.producer.first_hash {
            if our_hash == majority_hash && !final_producer_list.contains(&self.producer.id) {
                final_producer_list.push(self.producer.id.clone());
            }
        }
        final_producer_list.sort();
        final_producer_list.dedup();

        // Create witness/hash of producer list for verification (spec-shaped).
        let producer_list_hash = hash_producer_list_merkle(&final_producer_list);
        
        // Store producer list
        self.producer.producer_list = Some(final_producer_list);

        let candidate = ProducerCandidate {
            majority_hash,
            producer_list_hash,
            cycle_number: self.producer.cycle_number,
            producer_id: self.producer.id.clone(),
            timestamp: current_timestamp_ms(),
        };

        observe_histogram!("consensus_majority_size", producer_list_len as f64);
        
        Ok(candidate)
    }

    fn hash_producer_list(&self, producer_list: &[String]) -> CatalystResult<Hash> {
        use blake2::{Blake2b512, Digest};
        
        let mut sorted_list = producer_list.to_vec();
        sorted_list.sort();
        
        let mut hasher = Blake2b512::new();
        for producer_id in &sorted_list {
            hasher.update(producer_id.as_bytes());
        }
        
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result[..32]);
        Ok(hash)
    }
}

fn producer_leaf_hash(producer_id: &str) -> Hash {
    use blake2::{Blake2b512, Digest};
    let mut h = Blake2b512::new();
    h.update(producer_id.as_bytes());
    let out = h.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&out[..32]);
    hash
}

fn hash_producer_list_merkle(producer_list: &[String]) -> Hash {
    let mut ids = producer_list.to_vec();
    ids.sort();
    ids.dedup();
    let leaves: Vec<Hash> = ids.iter().map(|s| producer_leaf_hash(s)).collect();
    compute_merkle_root(&leaves)
}

#[cfg(test)]
mod campaigning_tests {
    use super::*;
    use crate::producer::Producer;

    fn pq(pid: &str, cycle: u64, h: u8) -> ProducerQuantity {
        ProducerQuantity {
            first_hash: [h; 32],
            cycle_number: cycle,
            producer_id: pid.to_string(),
            timestamp: 0,
        }
    }

    #[tokio::test]
    async fn campaigning_selects_majority_hash_and_builds_deterministic_witness() {
        let mut producer = Producer::new("p1".to_string(), [1u8; 32], 7);
        producer.expected_producers = 3;
        producer.first_hash = Some([1u8; 32]);
        let mut phase = CampaigningPhase::new(producer);

        // Two producers choose hash=1, one chooses hash=2.
        phase.add_quantity(pq("p1", 7, 1));
        phase.add_quantity(pq("p2", 7, 1));
        phase.add_quantity(pq("p3", 7, 2));

        let cand = phase.execute(2).await.unwrap();
        assert_eq!(cand.majority_hash, [1u8; 32]);

        let list = phase.producer.producer_list.clone().unwrap();
        assert_eq!(list, vec!["p1".to_string(), "p2".to_string()]);
        assert_eq!(cand.producer_list_hash, hash_producer_list_merkle(&list));
    }

    #[tokio::test]
    async fn campaigning_tie_breaks_on_smallest_hash() {
        let mut producer = Producer::new("pX".to_string(), [1u8; 32], 9);
        producer.expected_producers = 4;
        producer.first_hash = Some([2u8; 32]);
        let mut phase = CampaigningPhase::new(producer);

        // Two hashes have equal counts (2 and 2), required_majority=2 satisfied.
        phase.add_quantity(pq("a", 9, 2));
        phase.add_quantity(pq("b", 9, 2));
        phase.add_quantity(pq("c", 9, 1));
        phase.add_quantity(pq("d", 9, 1));

        let cand = phase.execute(2).await.unwrap();
        // Choose the smaller hash bytes: [1;32] < [2;32]
        assert_eq!(cand.majority_hash, [1u8; 32]);
    }

    #[tokio::test]
    async fn campaigning_is_deterministic_across_insertion_orders() {
        let mut producer = Producer::new("p1".to_string(), [1u8; 32], 11);
        producer.expected_producers = 3;
        producer.first_hash = Some([3u8; 32]);

        let mut a = CampaigningPhase::new(producer.clone());
        a.add_quantity(pq("p1", 11, 3));
        a.add_quantity(pq("p2", 11, 3));
        a.add_quantity(pq("p3", 11, 4));
        let c1 = a.execute(2).await.unwrap();

        let mut b = CampaigningPhase::new(producer);
        b.add_quantity(pq("p3", 11, 4));
        b.add_quantity(pq("p2", 11, 3));
        b.add_quantity(pq("p1", 11, 3));
        let c2 = b.execute(2).await.unwrap();

        assert_eq!(c1.majority_hash, c2.majority_hash);
        assert_eq!(c1.producer_list_hash, c2.producer_list_hash);
        assert_eq!(a.producer.producer_list, b.producer.producer_list);
    }
}

/// Voting Phase Implementation
pub struct VotingPhase {
    pub(crate) producer: Producer,
    collected_candidates: HashMap<String, ProducerCandidate>,
}

impl VotingPhase {
    pub fn new(producer: Producer) -> Self {
        Self {
            producer,
            collected_candidates: HashMap::new(),
        }
    }

    /// Add a collected producer candidate
    pub fn add_candidate(&mut self, candidate: ProducerCandidate) {
        self.collected_candidates.insert(candidate.producer_id.clone(), candidate);
    }

    /// Execute voting phase
    pub async fn execute(&mut self, required_majority: usize, reward_config: &RewardConfig) -> CatalystResult<ProducerVote> {
        log_info!(LogCategory::Consensus, "Starting voting phase for cycle {}", self.producer.cycle_number);
        increment_counter!("consensus_voting_phase_total", 1);
        
        let result = time_operation!("consensus_voting_phase_duration", {
            self.create_final_ledger_update(required_majority, reward_config).await
        });
        
        match result {
            Ok(vote) => {
                log_info!(LogCategory::Consensus, "Voting phase completed successfully");
                Ok(vote)
            }
            Err(e) => {
                log_error!(LogCategory::Consensus, "Voting phase failed: {}", e);
                Err(e)
            }
        }
    }

    pub fn expected_producers(&self) -> usize {
        self.producer.expected_producers
    }

    async fn create_final_ledger_update(&mut self, required_majority: usize, reward_config: &RewardConfig) -> CatalystResult<ProducerVote> {
        // Check if we can participate (must have correct hash)
        let our_hash = self.producer.first_hash.ok_or_else(|| {
            ConsensusError::ConsensusFailed {
                reason: "No first hash available for voting".to_string(),
            }
        })?;

        // Verify minimum data and find majority
        if self.collected_candidates.len() < required_majority {
            return Err(ConsensusError::InsufficientData {
                got: self.collected_candidates.len(),
                required: required_majority,
            }.into());
        }

        // Find majority hash among candidates (deterministic).
        // - use BTreeMap for deterministic iteration
        // - sort/dedup producer lists per hash
        // - tie-break on smallest hash bytes
        let mut hash_counts: BTreeMap<Hash, Vec<String>> = BTreeMap::new();
        for (producer_id, candidate) in &self.collected_candidates {
            hash_counts
                .entry(candidate.majority_hash)
                .or_insert_with(Vec::new)
                .push(producer_id.clone());
        }
        for v in hash_counts.values_mut() {
            v.sort();
            v.dedup();
        }

        let mut best_hash: Option<Hash> = None;
        let mut best_list: Vec<String> = Vec::new();
        for (h, producers) in hash_counts.iter() {
            if producers.len() > best_list.len() {
                best_hash = Some(*h);
                best_list = producers.clone();
                continue;
            }
            if producers.len() == best_list.len() {
                if best_hash.map(|bh| *h < bh).unwrap_or(true) {
                    best_hash = Some(*h);
                    best_list = producers.clone();
                }
            }
        }
        let Some(majority_hash) = best_hash else {
            return Err(ConsensusError::NoMajority {
                highest: 0,
                threshold: required_majority,
                details: format!(" (phase=voting cycle={})", self.producer.cycle_number),
            }
            .into());
        };
        let mut voting_producers = best_list;

        if voting_producers.len() < required_majority {
            return Err(ConsensusError::NoMajority {
                highest: voting_producers.len(),
                threshold: required_majority,
                details: format!(" (phase=voting cycle={} candidate_majority_hash_counts=ambiguous)", self.producer.cycle_number),
            }.into());
        }

        // Verify we can participate
        if our_hash != majority_hash {
            return Err(ConsensusError::ConsensusFailed {
                reason: "Our hash doesn't match majority, cannot vote".to_string(),
            }.into());
        }

        // Spec-shaped list selection:
        // The network message currently does not carry the full witness list, only a witness hash.
        // Until we introduce witness list transport, treat the set of candidate submitters for the
        // chosen `majority_hash` as `Ln(prod)` for this scaffold.
        //
        // Deterministically include self if missing.
        if !voting_producers.contains(&self.producer.id) {
            voting_producers.push(self.producer.id.clone());
        }
        voting_producers.sort();
        voting_producers.dedup();
        let final_producer_list = voting_producers.clone();
        
        // Create compensation entries
        let compensation_entries = self.create_compensation_entries(&final_producer_list, reward_config)?;

        // Build complete ledger state update
        let partial_update = self.producer.partial_update.as_ref()
            .ok_or_else(|| ConsensusError::ConsensusFailed {
                reason: "No partial update available".to_string(),
            })?
            .clone();

        // Deterministic ordering is critical: this structure is hashed and becomes the vote value.
        let final_producer_list = final_producer_list;
        let voting_producers = voting_producers;

        let ledger_update = LedgerStateUpdate {
            partial_update,
            compensation_entries,
            cycle_number: self.producer.cycle_number,
            producer_list: final_producer_list,
            vote_list: voting_producers.clone(),
        };

        // Hash the complete ledger state update
        let ledger_state_hash = hash_data(&ledger_update)?;
        
        // Store the ledger update
        self.producer.ledger_update = Some(ledger_update);

        // Create vote list witness/hash (spec-shaped) and store vote list.
        let vote_list_hash = hash_producer_list_merkle(&voting_producers);
        self.producer.vote_list = Some(voting_producers.clone());

        let vote = ProducerVote {
            ledger_state_hash,
            vote_list_hash,
            cycle_number: self.producer.cycle_number,
            producer_id: self.producer.id.clone(),
            timestamp: current_timestamp_ms(),
        };

        Ok(vote)
    }

    fn create_final_producer_list(&self, majority_hash: &Hash) -> CatalystResult<Vec<String>> {
        // Collect all producer lists from candidates with majority hash
        let mut producer_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut total_candidates = 0;

        for candidate in self.collected_candidates.values() {
            if candidate.majority_hash == *majority_hash {
                total_candidates += 1;
                // In a real implementation, we'd need to decode the producer list from the hash
                // For now, we'll use the producer who submitted the candidate
                *producer_counts.entry(candidate.producer_id.clone()).or_insert(0) += 1;
            }
        }

        // Include only producers that appear in at least P/2 lists
        let threshold = (total_candidates + 1) / 2;  // P/2 threshold
        let mut final_list: Vec<String> = producer_counts
            .into_iter()
            .filter(|(_, count)| *count >= threshold)
            .map(|(producer_id, _)| producer_id)
            .collect();
        final_list.sort();

        Ok(final_list)
    }

    fn create_compensation_entries(&self, producer_list: &[String], reward_config: &RewardConfig) -> CatalystResult<Vec<CompensationEntry>> {
        let mut entries = Vec::new();
        
        if producer_list.is_empty() {
            return Ok(entries);
        }

        let reward_per_producer = reward_config.producer_reward / producer_list.len() as u64;
        
        // Deterministic ordering (this list is hashed as part of LSU).
        let mut sorted = producer_list.to_vec();
        sorted.sort();
        for producer_id in &sorted {
            // In a real implementation, we'd look up the producer's public key
            // For now, we'll use a placeholder
            let public_key = [0u8; 32]; // Placeholder
            
            entries.push(CompensationEntry {
                producer_id: producer_id.clone(),
                public_key,
                amount: reward_per_producer,
            });
        }

        // Ensure stable entry ordering (by producer id).
        entries.sort_by(|a, b| a.producer_id.cmp(&b.producer_id));
        Ok(entries)
    }

    fn hash_producer_list(&self, producer_list: &[String]) -> CatalystResult<Hash> {
        use blake2::{Blake2b512, Digest};
        
        let mut sorted_list = producer_list.to_vec();
        sorted_list.sort();
        
        let mut hasher = Blake2b512::new();
        for producer_id in &sorted_list {
            hasher.update(producer_id.as_bytes());
        }
        
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result[..32]);
        Ok(hash)
    }
}

#[cfg(test)]
mod voting_tests {
    use super::*;
    use crate::producer::Producer;

    fn candidate(pid: &str, cycle: u64, majority_h: u8, list_hash: Hash) -> ProducerCandidate {
        ProducerCandidate {
            majority_hash: [majority_h; 32],
            producer_list_hash: list_hash,
            cycle_number: cycle,
            producer_id: pid.to_string(),
            timestamp: 0,
        }
    }

    fn mk_partial() -> PartialLedgerStateUpdate {
        PartialLedgerStateUpdate {
            transaction_entries: Vec::new(),
            transaction_signatures_hash: [0u8; 32],
            total_fees: 0,
            timestamp: 0,
        }
    }

    #[tokio::test]
    async fn voting_selects_majority_hash_and_is_deterministic() {
        let mut producer = Producer::new("p1".to_string(), [1u8; 32], 12);
        producer.expected_producers = 3;
        producer.first_hash = Some([7u8; 32]);
        producer.partial_update = Some(mk_partial());

        let mut phase_a = VotingPhase::new(producer.clone());
        let list_hash = hash_producer_list_merkle(&vec!["p1".to_string(), "p2".to_string()]);
        phase_a.add_candidate(candidate("p1", 12, 7, list_hash));
        phase_a.add_candidate(candidate("p2", 12, 7, list_hash));
        phase_a.add_candidate(candidate("p3", 12, 9, list_hash));
        let vote_a = phase_a.execute(2, &RewardConfig::default()).await.unwrap();

        // Insert in different order → same output hashes.
        let mut phase_b = VotingPhase::new(producer);
        phase_b.add_candidate(candidate("p3", 12, 9, list_hash));
        phase_b.add_candidate(candidate("p2", 12, 7, list_hash));
        phase_b.add_candidate(candidate("p1", 12, 7, list_hash));
        let vote_b = phase_b.execute(2, &RewardConfig::default()).await.unwrap();

        assert_eq!(vote_a.ledger_state_hash, vote_b.ledger_state_hash);
        assert_eq!(vote_a.vote_list_hash, vote_b.vote_list_hash);
        assert_eq!(phase_a.producer.vote_list, phase_b.producer.vote_list);
        assert_eq!(phase_a.producer.ledger_update.as_ref().unwrap().producer_list, vec!["p1".to_string(), "p2".to_string()]);
    }

    #[tokio::test]
    async fn voting_tie_breaks_on_smallest_hash() {
        let mut producer = Producer::new("p1".to_string(), [1u8; 32], 13);
        producer.expected_producers = 4;
        producer.first_hash = Some([2u8; 32]);
        producer.partial_update = Some(mk_partial());

        let mut phase = VotingPhase::new(producer);
        let lh = [0u8; 32];
        // 2 votes for hash=2, 2 votes for hash=1 (tie). Choose smallest hash => 1.
        phase.add_candidate(candidate("a", 13, 2, lh));
        phase.add_candidate(candidate("b", 13, 2, lh));
        phase.add_candidate(candidate("c", 13, 1, lh));
        phase.add_candidate(candidate("d", 13, 1, lh));

        // Our hash is 2, but tie-break picks 1, so we cannot vote.
        let err = phase.execute(2, &RewardConfig::default()).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("cannot vote")
                || msg.contains("doesn't match majority")
                || msg.contains("doesn")
        ); // tolerate error string changes
    }
}

/// Synchronization Phase Implementation
pub struct SynchronizationPhase {
    pub(crate) producer: Producer,
    collected_votes: HashMap<String, ProducerVote>,
}

impl SynchronizationPhase {
    pub fn new(producer: Producer) -> Self {
        Self {
            producer,
            collected_votes: HashMap::new(),
        }
    }

    /// Add a collected producer vote
    pub fn add_vote(&mut self, vote: ProducerVote) {
        self.collected_votes.insert(vote.producer_id.clone(), vote);
    }

    /// Execute synchronization phase
    pub async fn execute(&mut self, required_majority: usize) -> CatalystResult<ProducerOutput> {
        log_info!(LogCategory::Consensus, "Starting synchronization phase for cycle {}", self.producer.cycle_number);
        increment_counter!("consensus_synchronization_phase_total", 1);
        
        let result = time_operation!("consensus_synchronization_phase_duration", {
            self.finalize_ledger_update(required_majority).await
        });
        
        match result {
            Ok(output) => {
                log_info!(LogCategory::Consensus, "Synchronization phase completed successfully");
                Ok(output)
            }
            Err(e) => {
                log_error!(LogCategory::Consensus, "Synchronization phase failed: {}", e);
                Err(e)
            }
        }
    }

    pub fn expected_producers(&self) -> usize {
        self.producer.expected_producers
    }

    async fn finalize_ledger_update(&mut self, required_majority: usize) -> CatalystResult<ProducerOutput> {
        // Check minimum data requirement
        if self.collected_votes.len() < required_majority {
            return Err(ConsensusError::InsufficientData {
                got: self.collected_votes.len(),
                required: required_majority,
            }.into());
        }

        // Find majority ledger state hash (deterministic).
        // - BTreeMap for deterministic iteration
        // - sort/dedup vote producer lists
        // - tie-break on smallest hash bytes
        let mut hash_counts: BTreeMap<Hash, Vec<String>> = BTreeMap::new();
        for (producer_id, vote) in &self.collected_votes {
            hash_counts
                .entry(vote.ledger_state_hash)
                .or_insert_with(Vec::new)
                .push(producer_id.clone());
        }
        for v in hash_counts.values_mut() {
            v.sort();
            v.dedup();
        }

        let mut best_hash: Option<Hash> = None;
        let mut best_list: Vec<String> = Vec::new();
        for (h, producers) in hash_counts.iter() {
            if producers.len() > best_list.len() {
                best_hash = Some(*h);
                best_list = producers.clone();
                continue;
            }
            if producers.len() == best_list.len() {
                if best_hash.map(|bh| *h < bh).unwrap_or(true) {
                    best_hash = Some(*h);
                    best_list = producers.clone();
                }
            }
        }
        let Some(final_hash) = best_hash else {
            return Err(ConsensusError::NoMajority {
                highest: 0,
                threshold: required_majority,
                details: format!(" (phase=synchronization cycle={})", self.producer.cycle_number),
            }
            .into());
        };
        let mut vote_producers = best_list;

        if vote_producers.len() < required_majority {
            return Err(ConsensusError::NoMajority {
                highest: vote_producers.len(),
                threshold: required_majority,
                details: format!(
                    " (phase=synchronization cycle={} ledger_state_hash_counts=ambiguous)",
                    self.producer.cycle_number
                ),
            }.into());
        }

        // Spec-shaped vote list selection:
        // Like VotingPhase, we do not yet transport/validate the full witness list; only the witness hash.
        // Until vote-list witness transport is implemented, treat the set of vote submitters for the
        // final ledger_state_hash as `Ln(vote)` for this scaffold.
        if !vote_producers.contains(&self.producer.id) {
            vote_producers.push(self.producer.id.clone());
        }
        vote_producers.sort();
        vote_producers.dedup();
        let final_vote_list = vote_producers;
        
        // Get our ledger update (must match the majority)
        let ledger_update = self.producer.ledger_update.as_ref()
            .ok_or_else(|| ConsensusError::ConsensusFailed {
                reason: "No ledger update available".to_string(),
            })?;

        let our_hash = hash_data(ledger_update)?;
        if our_hash != final_hash {
            return Err(ConsensusError::ConsensusFailed {
                reason: "Our ledger update doesn't match majority".to_string(),
            }.into());
        }

        // Store to DFS and get a content-based address.
        //
        // TODO(#182): integrate a real DFS backend into the consensus engine. For now we compute a
        // deterministic address from the final LSU hash (stable across nodes and independent of
        // serialization quirks).
        let dfs_address = self.store_to_dfs(ledger_update, &final_hash).await?;
        
        // Create vote list witness/hash (spec-shaped).
        let vote_list_hash = hash_producer_list_merkle(&final_vote_list);

        let output = ProducerOutput {
            dfs_address,
            vote_list_hash,
            cycle_number: self.producer.cycle_number,
            producer_id: self.producer.id.clone(),
            timestamp: current_timestamp_ms(),
        };

        observe_histogram!("consensus_final_vote_count", final_vote_list.len() as f64);
        
        Ok(output)
    }

    async fn store_to_dfs(
        &self,
        ledger_update: &LedgerStateUpdate,
        ledger_state_hash: &Hash,
    ) -> CatalystResult<catalyst_utils::Address> {
        // Serialize the ledger update
        let serialized = catalyst_utils::CatalystSerialize::serialize(ledger_update)
            .map_err(|e| ConsensusError::Serialization(e.to_string()))?;
        
        // Deterministic content-addressing:
        // - 1 byte "scheme" prefix (0x01 = blake2b256 over LSU hash, placeholder)
        // - 20 bytes of the LSU hash
        let mut address = [0u8; 21];
        address[0] = 0x01;
        address[1..21].copy_from_slice(&ledger_state_hash[..20]);

        // TODO(#182): store `serialized` in a real DFS and return the content id / address.
        // For now, log the address deterministically so operators can correlate.
        let _ = serialized; // retained for future DFS integration
        log_info!(
            LogCategory::Consensus,
            "Storing ledger update to DFS (placeholder) addr={:?}",
            address
        );
        
        Ok(address)
    }

}

#[cfg(test)]
mod synchronization_tests {
    use super::*;
    use crate::producer::Producer;

    fn mk_partial() -> PartialLedgerStateUpdate {
        PartialLedgerStateUpdate {
            transaction_entries: Vec::new(),
            transaction_signatures_hash: [0u8; 32],
            total_fees: 0,
            timestamp: 0,
        }
    }

    fn mk_lsu(cycle: u64) -> LedgerStateUpdate {
        LedgerStateUpdate {
            partial_update: mk_partial(),
            compensation_entries: Vec::new(),
            cycle_number: cycle,
            producer_list: vec!["p1".to_string(), "p2".to_string()],
            vote_list: vec!["p1".to_string(), "p2".to_string()],
        }
    }

    fn vote(pid: &str, cycle: u64, h: u8) -> ProducerVote {
        ProducerVote {
            ledger_state_hash: [h; 32],
            vote_list_hash: [0u8; 32],
            cycle_number: cycle,
            producer_id: pid.to_string(),
            timestamp: 0,
        }
    }

    #[tokio::test]
    async fn synchronization_selects_majority_hash_and_is_deterministic() {
        let mut producer = Producer::new("p1".to_string(), [1u8; 32], 21);
        producer.expected_producers = 3;
        let lsu = mk_lsu(21);
        let lsu_hash = hash_data(&lsu).unwrap();
        producer.ledger_update = Some(lsu);

        let mut a = SynchronizationPhase::new(producer.clone());
        a.add_vote(ProducerVote { ledger_state_hash: lsu_hash, vote_list_hash: [0u8; 32], cycle_number: 21, producer_id: "p1".to_string(), timestamp: 0 });
        a.add_vote(ProducerVote { ledger_state_hash: lsu_hash, vote_list_hash: [0u8; 32], cycle_number: 21, producer_id: "p2".to_string(), timestamp: 0 });
        a.add_vote(vote("p3", 21, 7));
        let o1 = a.execute(2).await.unwrap();

        let mut b = SynchronizationPhase::new(producer);
        b.add_vote(vote("p3", 21, 7));
        b.add_vote(ProducerVote { ledger_state_hash: lsu_hash, vote_list_hash: [0u8; 32], cycle_number: 21, producer_id: "p2".to_string(), timestamp: 0 });
        b.add_vote(ProducerVote { ledger_state_hash: lsu_hash, vote_list_hash: [0u8; 32], cycle_number: 21, producer_id: "p1".to_string(), timestamp: 0 });
        let o2 = b.execute(2).await.unwrap();

        assert_eq!(o1.vote_list_hash, o2.vote_list_hash);
        assert_eq!(o1.dfs_address, o2.dfs_address);
        // scheme prefix is 0x01
        assert_eq!(o1.dfs_address[0], 0x01);
    }
}

/// Reward configuration for compensation entries
#[derive(Debug, Clone)]
pub struct RewardConfig {
    pub producer_reward: u64,
    pub voter_reward: u64,
    pub total_new_tokens: u64,
}

impl Default for RewardConfig {
    fn default() -> Self {
        Self {
            producer_reward: 1000,
            voter_reward: 100,
            total_new_tokens: 10000,
        }
    }
}