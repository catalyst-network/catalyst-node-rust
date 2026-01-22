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
        // Sort transactions by hash for deterministic ordering
        let mut sorted_entries = transactions;
        sorted_entries.sort_by_key(|entry| hash_data(entry).unwrap_or_default());
        let entry_count = sorted_entries.len();

        // Create a deterministic Merkle root of transaction signatures.
        //
        // Note: in the current testnet scaffolding, `TransactionEntry.signature` carries either:
        // - a raw Schnorr signature (transfer entries all share the tx signature bytes), or
        // - a marker-prefixed payload (worker registration / EVM marker).
        //
        // We treat these as opaque bytes at the consensus boundary and commit to them here.
        let signatures: Vec<Vec<u8>> = sorted_entries.iter().map(|e| e.signature.clone()).collect();
        let signatures_hash = signature_merkle_root(&signatures);

        // Fees are not implemented yet in the runtime/state machine; keep deterministic.
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

pub(crate) fn blake2b_256_tagged(tag: &'static [u8], parts: &[&[u8]]) -> Hash {
    use blake2::{Blake2b512, Digest};
    let mut hasher = Blake2b512::new();
    hasher.update(tag);
    for p in parts {
        hasher.update(p);
    }
    let result = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&result[..32]);
    out
}

pub(crate) fn hash_id_list_tagged(tag: &'static [u8], ids: &[String]) -> Hash {
    // Caller must ensure ids are already deterministic (sorted/deduped).
    let mut parts: Vec<&[u8]> = Vec::with_capacity(ids.len());
    for id in ids {
        parts.push(id.as_bytes());
    }
    blake2b_256_tagged(tag, &parts)
}

fn producer_id_to_pubkey_placeholder(producer_id: &str) -> Hash {
    // Deterministic placeholder mapping until producer public keys are part of membership/state.
    blake2b_256_tagged(b"catalyst:producer_id:pubkey:v1", &[producer_id.as_bytes()])
}

fn distribute_evenly(total: u64, recipients: &[String]) -> Vec<(String, u64)> {
    // Deterministic split: base + 1 for first `remainder` recipients (lexicographically sorted).
    if recipients.is_empty() {
        return Vec::new();
    }
    let base = total / recipients.len() as u64;
    let rem = (total % recipients.len() as u64) as usize;
    let mut out: Vec<(String, u64)> = Vec::with_capacity(recipients.len());
    for (i, r) in recipients.iter().enumerate() {
        let extra = if i < rem { 1 } else { 0 };
        out.push((r.clone(), base.saturating_add(extra)));
    }
    out
}

/// Deterministic Merkle root over a list of signature byte strings.
///
/// - leaf hash: `blake2b_256("catalyst:txsig:leaf:v1" || sig_bytes)`
/// - node hash: `blake2b_256("catalyst:txsig:node:v1" || left || right)`
///
/// For odd-width levels, the last node is duplicated (Bitcoin-style).
pub(crate) fn signature_merkle_root(signatures: &[Vec<u8>]) -> Hash {
    const LEAF_TAG: &[u8] = b"catalyst:txsig:leaf:v1";
    const NODE_TAG: &[u8] = b"catalyst:txsig:node:v1";

    if signatures.is_empty() {
        return [0u8; 32];
    }

    let mut level: Vec<Hash> = signatures
        .iter()
        .map(|s| blake2b_256_tagged(LEAF_TAG, &[s.as_slice()]))
        .collect();

    while level.len() > 1 {
        let mut next: Vec<Hash> = Vec::with_capacity((level.len() + 1) / 2);
        let mut i = 0usize;
        while i < level.len() {
            let left = level[i];
            let right = if i + 1 < level.len() { level[i + 1] } else { level[i] };
            next.push(blake2b_256_tagged(NODE_TAG, &[&left, &right]));
            i += 2;
        }
        level = next;
    }
    level[0]
}

#[cfg(test)]
mod construction_tests {
    use super::signature_merkle_root;

    #[test]
    fn signature_merkle_root_is_deterministic_and_nonzero() {
        let sigs = vec![vec![1u8; 64], vec![2u8; 64], vec![3u8; 64]];
        let a = signature_merkle_root(&sigs);
        let b = signature_merkle_root(&sigs);
        assert_eq!(a, b);
        assert_ne!(a, [0u8; 32]);
    }

    #[test]
    fn signature_merkle_root_empty_is_zero() {
        let sigs: Vec<Vec<u8>> = Vec::new();
        assert_eq!(signature_merkle_root(&sigs), [0u8; 32]);
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
        // Basic validation: ignore wrong-cycle or empty-producer messages.
        if quantity.cycle_number != self.producer.cycle_number {
            return;
        }
        if quantity.producer_id.is_empty() {
            return;
        }
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

        // Count occurrences of each first hash (BTreeMap for deterministic tie-breaking).
        let mut hash_counts: BTreeMap<Hash, BTreeMap<String, ()>> = BTreeMap::new();
        for q in self.collected_quantities.values() {
            hash_counts
                .entry(q.first_hash)
                .or_insert_with(BTreeMap::new)
                .insert(q.producer_id.clone(), ());
        }

        // Find the hash with the highest count; ties resolve by hash ordering (BTreeMap).
        let mut best_hash: Option<Hash> = None;
        let mut best_count: usize = 0;
        for (h, producers) in hash_counts.iter() {
            let n = producers.len();
            if n > best_count {
                best_hash = Some(*h);
                best_count = n;
            }
        }
        let Some(majority_hash) = best_hash else {
            return Err(ConsensusError::NoMajority {
                highest: 0,
                threshold: required_majority,
                details: format!(" (phase=campaigning cycle={})", self.producer.cycle_number),
            }.into());
        };
        let producer_list = hash_counts
            .get(&majority_hash)
            .map(|m| m.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

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

        // Deterministic witness list: sorted + dedup; include self if we have the majority hash.
        let producer_list_len = producer_list.len();
        let mut final_producer_list = producer_list;
        if let Some(our_hash) = self.producer.first_hash {
            if our_hash == majority_hash && !final_producer_list.contains(&self.producer.id) {
                final_producer_list.push(self.producer.id.clone());
            }
        }
        final_producer_list.sort();
        final_producer_list.dedup();

        // Create hash of producer list for verification
        let producer_list_hash = self.hash_producer_list(&final_producer_list)?;
        
        // Store producer list
        self.producer.producer_list = Some(final_producer_list.clone());

        let candidate = ProducerCandidate {
            majority_hash,
            producer_list_hash,
            producer_list: final_producer_list,
            cycle_number: self.producer.cycle_number,
            producer_id: self.producer.id.clone(),
            timestamp: current_timestamp_ms(),
        };

        observe_histogram!("consensus_majority_size", producer_list_len as f64);
        
        Ok(candidate)
    }

    fn hash_producer_list(&self, producer_list: &[String]) -> CatalystResult<Hash> {
        // Domain-separated list hashing; list must already be deterministic (sorted/deduped).
        const TAG: &[u8] = b"catalyst:campaigning:producer_list:v1";
        Ok(hash_id_list_tagged(TAG, producer_list))
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
        // Basic validation: ignore wrong-cycle candidates.
        if candidate.cycle_number != self.producer.cycle_number {
            return;
        }
        // Verify producer_list_hash matches the provided producer_list.
        let mut list = candidate.producer_list.clone();
        list.sort();
        list.dedup();
        const TAG: &[u8] = b"catalyst:campaigning:producer_list:v1";
        let expected = hash_id_list_tagged(TAG, &list);
        if expected != candidate.producer_list_hash {
            return;
        }
        // Store normalized list (sorted/deduped) to ensure deterministic downstream behavior.
        let mut candidate = candidate;
        candidate.producer_list = list;
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

        // Find majority hash among candidates
        let mut hash_counts: HashMap<Hash, Vec<String>> = HashMap::new();
        for (producer_id, candidate) in &self.collected_candidates {
            hash_counts.entry(candidate.majority_hash)
                .or_insert_with(Vec::new)
                .push(producer_id.clone());
        }

        // required_majority is derived from membership, not from received count
        let (majority_hash, voting_producers) = hash_counts
            .into_iter()
            .max_by_key(|(_, producers)| producers.len())
            .ok_or_else(|| ConsensusError::NoMajority {
                highest: 0,
                threshold: required_majority,
                details: format!(" (phase=voting cycle={})", self.producer.cycle_number),
            })?;

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

        // Create final producer list from candidates with P/2 threshold
        let final_producer_list = self.create_final_producer_list(&majority_hash)?;
        
        // Build vote list deterministically (must match what we hash and store).
        let mut final_vote_list = voting_producers.clone();
        if !final_vote_list.contains(&self.producer.id) {
            final_vote_list.push(self.producer.id.clone());
        }
        final_vote_list.sort();
        final_vote_list.dedup();

        // Create compensation entries (deterministic ordering + deterministic split)
        let compensation_entries =
            self.create_compensation_entries(&final_producer_list, &final_vote_list, reward_config)?;

        // Build complete ledger state update
        let partial_update = self.producer.partial_update.as_ref()
            .ok_or_else(|| ConsensusError::ConsensusFailed {
                reason: "No partial update available".to_string(),
            })?
            .clone();

        // Deterministic ordering is critical: this structure is hashed and becomes the vote value.
        let mut final_producer_list = final_producer_list;
        final_producer_list.sort();

        let mut voting_producers = voting_producers.clone();
        voting_producers.sort();

        let ledger_update = LedgerStateUpdate {
            partial_update,
            compensation_entries,
            cycle_number: self.producer.cycle_number,
            producer_list: final_producer_list,
            vote_list: final_vote_list.clone(),
        };

        // Hash the complete ledger state update
        let ledger_state_hash = hash_data(&ledger_update)?;
        
        // Store the ledger update
        self.producer.ledger_update = Some(ledger_update);

        // Create vote list hash (must match ledger_update.vote_list)
        let vote_list_hash = self.hash_producer_list(&final_vote_list)?;
        self.producer.vote_list = Some(final_vote_list);

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
        // Collect all witness lists from candidates with majority hash and count producer occurrences.
        let mut producer_counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut total_candidates = 0usize;

        for candidate in self.collected_candidates.values() {
            if candidate.majority_hash != *majority_hash {
                continue;
            }
            total_candidates += 1;
            for pid in &candidate.producer_list {
                *producer_counts.entry(pid.clone()).or_insert(0) += 1;
            }
        }

        // Include only producers that appear in at least P/2 witness lists.
        let threshold = (total_candidates + 1) / 2;
        let mut final_list: Vec<String> = producer_counts
            .into_iter()
            .filter(|(_, count)| *count >= threshold)
            .map(|(producer_id, _)| producer_id)
            .collect();
        final_list.sort();
        final_list.dedup();
        Ok(final_list)
    }

    fn create_compensation_entries(
        &self,
        producer_list: &[String],
        vote_list: &[String],
        reward_config: &RewardConfig,
    ) -> CatalystResult<Vec<CompensationEntry>> {
        // Deterministic recipients
        let mut producers = producer_list.to_vec();
        producers.sort();
        producers.dedup();
        let mut voters = vote_list.to_vec();
        voters.sort();
        voters.dedup();

        // Deterministic split rules (placeholder until fees + exact spec semantics are implemented).
        let producer_payouts = distribute_evenly(reward_config.producer_reward, &producers);
        let voter_payouts = distribute_evenly(reward_config.voter_reward, &voters);

        let mut entries: Vec<CompensationEntry> = Vec::new();
        for (pid, amount) in producer_payouts {
            entries.push(CompensationEntry {
                producer_id: pid.clone(),
                public_key: producer_id_to_pubkey_placeholder(&pid),
                amount,
            });
        }
        for (vid, amount) in voter_payouts {
            entries.push(CompensationEntry {
                producer_id: vid.clone(),
                public_key: producer_id_to_pubkey_placeholder(&vid),
                amount,
            });
        }

        // Stable ordering (this list is hashed as part of LSU)
        entries.sort_by(|a, b| a.producer_id.cmp(&b.producer_id).then(a.amount.cmp(&b.amount)));
        Ok(entries)
    }

    fn hash_producer_list(&self, producer_list: &[String]) -> CatalystResult<Hash> {
        // Domain-separated vote list hashing; list must already be deterministic (sorted/deduped).
        const TAG: &[u8] = b"catalyst:voting:vote_list:v1";
        Ok(hash_id_list_tagged(TAG, producer_list))
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

        // Find majority ledger state hash
        let mut hash_counts: HashMap<Hash, Vec<String>> = HashMap::new();
        for (producer_id, vote) in &self.collected_votes {
            hash_counts.entry(vote.ledger_state_hash)
                .or_insert_with(Vec::new)
                .push(producer_id.clone());
        }

        // required_majority is derived from membership, not from received count
        let (final_hash, vote_producers) = hash_counts
            .into_iter()
            .max_by_key(|(_, producers)| producers.len())
            .ok_or_else(|| ConsensusError::NoMajority {
                highest: 0,
                threshold: required_majority,
                details: format!(" (phase=synchronization cycle={})", self.producer.cycle_number),
            })?;

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

        // Create final vote list with Cn/2 threshold
        let final_vote_list = self.create_final_vote_list(&final_hash)?;
        
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

        // Store to DFS and get content-based address
        let dfs_address = self.store_to_dfs(ledger_update).await?;
        
        // Create vote list hash
        let vote_list_hash = self.hash_producer_list(&final_vote_list)?;

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

    fn create_final_vote_list(&self, majority_hash: &Hash) -> CatalystResult<Vec<String>> {
        // Collect vote counts for producers who voted for the majority hash
        let mut vote_counts: HashMap<String, usize> = HashMap::new();
        let mut total_votes = 0;

        for vote in self.collected_votes.values() {
            if vote.ledger_state_hash == *majority_hash {
                total_votes += 1;
                // In a real implementation, we'd decode the vote list from the hash
                // For now, we'll use the producer who submitted the vote
                *vote_counts.entry(vote.producer_id.clone()).or_insert(0) += 1;
            }
        }

        // Include only producers that appear in at least Cn/2 vote lists
        let cn = self
            .producer
            .producer_list
            .as_ref()
            .map(|list: &Vec<String>| list.len())
            .unwrap_or(0);
        let threshold = (cn + 1) / 2;  // Cn/2 threshold
        
        let mut final_list: Vec<String> = vote_counts
            .into_iter()
            .filter(|(_, count)| *count >= threshold)
            .map(|(producer_id, _)| producer_id)
            .collect();

        final_list.sort();
        Ok(final_list)
    }

    async fn store_to_dfs(&self, ledger_update: &LedgerStateUpdate) -> CatalystResult<catalyst_utils::Address> {
        // Serialize the ledger update
        let serialized = catalyst_utils::CatalystSerialize::serialize(ledger_update)
            .map_err(|e| ConsensusError::Serialization(e.to_string()))?;
        
        // In a real implementation, this would store to the distributed file system
        // For now, we'll create a content-based address using hash
        use blake2::{Blake2b512, Digest};
        let mut hasher = Blake2b512::new();
        hasher.update(&serialized);
        let result = hasher.finalize();
        
        let mut address = [0u8; 21];
        address.copy_from_slice(&result[..21]);
        
        // TODO: Actually store to DFS
        log_info!(LogCategory::Consensus, "Storing ledger update to DFS with address: {:?}", address);
        
        Ok(address)
    }

    fn hash_producer_list(&self, producer_list: &[String]) -> CatalystResult<Hash> {
        // Must match VotingPhase vote_list hashing.
        const TAG: &[u8] = b"catalyst:voting:vote_list:v1";
        Ok(hash_id_list_tagged(TAG, producer_list))
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