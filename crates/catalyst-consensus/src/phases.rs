use crate::{
    types::*, error::ConsensusError, Producer
};
use catalyst_utils::{CatalystResult, Hash};
use catalyst_utils::logging::{LogCategory, log_info, log_warn, log_error};
use catalyst_utils::metrics::{increment_counter, observe_histogram, time_operation};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use indexmap::IndexMap;

/// Construction Phase Implementation
pub struct ConstructionPhase {
    producer: Producer,
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

        // Create hash tree of transaction signatures
        let signatures: Vec<_> = sorted_entries.iter().map(|e| e.signature).collect();
        let signatures_hash = self.compute_hash_tree(&signatures)?;

        // Calculate total fees
        let total_fees = sorted_entries.iter()
            .filter(|entry| entry.amount < 0)  // Spending entries
            .map(|entry| (entry.amount.abs() as u64).saturating_sub(entry.amount.abs() as u64))
            .sum();

        // Create partial ledger state update
        let partial_update = PartialLedgerStateUpdate {
            transaction_entries: sorted_entries,
            transaction_signatures_hash: signatures_hash,
            total_fees,
            timestamp: current_timestamp_ms(),
        };

        // Hash the partial update
        let first_hash = hash_data(&partial_update)?;
        
        // Store in producer cache
        self.producer.partial_update = Some(partial_update);
        self.producer.first_hash = Some(first_hash);

        // Create producer quantity
        let quantity = ProducerQuantity {
            first_hash,
            producer_id: self.producer.id.clone(),
            timestamp: current_timestamp_ms(),
        };

        observe_histogram!("consensus_transaction_count", sorted_entries.len() as f64);
        
        Ok(quantity)
    }

    fn compute_hash_tree(&self, signatures: &[catalyst_crypto::Signature]) -> CatalystResult<Hash> {
        use blake2::{Blake2b256, Digest};
        
        if signatures.is_empty() {
            return Ok([0u8; 32]);
        }

        let mut hasher = Blake2b256::new();
        for sig in signatures {
            hasher.update(&sig.to_bytes());
        }
        
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result[..32]);
        Ok(hash)
    }
}

/// Campaigning Phase Implementation
pub struct CampaigningPhase {
    producer: Producer,
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
    pub async fn execute(&mut self, min_data: usize, threshold: f64) -> CatalystResult<ProducerCandidate> {
        log_info!(LogCategory::Consensus, "Starting campaigning phase for cycle {}", self.producer.cycle_number);
        increment_counter!("consensus_campaigning_phase_total", 1);
        
        let result = time_operation!("consensus_campaigning_phase_duration", {
            self.find_majority_candidate(min_data, threshold).await
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

    async fn find_majority_candidate(&mut self, min_data: usize, threshold: f64) -> CatalystResult<ProducerCandidate> {
        // Check minimum data requirement
        if self.collected_quantities.len() < min_data {
            return Err(ConsensusError::InsufficientData {
                got: self.collected_quantities.len(),
                required: min_data,
            }.into());
        }

        // Count occurrences of each first hash
        let mut hash_counts: HashMap<Hash, Vec<String>> = HashMap::new();
        for (producer_id, quantity) in &self.collected_quantities {
            hash_counts.entry(quantity.first_hash)
                .or_insert_with(Vec::new)
                .push(producer_id.clone());
        }

        // Find majority hash
        let total_count = self.collected_quantities.len();
        let required_majority = (total_count as f64 * threshold).ceil() as usize;
        
        let (majority_hash, producer_list) = hash_counts
            .into_iter()
            .max_by_key(|(_, producers)| producers.len())
            .ok_or_else(|| ConsensusError::NoMajority {
                highest: 0,
                threshold: required_majority,
            })?;

        if producer_list.len() < required_majority {
            return Err(ConsensusError::NoMajority {
                highest: producer_list.len(),
                threshold: required_majority,
            }.into());
        }

        // Include self if we have the majority hash
        let mut final_producer_list = producer_list;
        if let Some(our_hash) = self.producer.first_hash {
            if our_hash == majority_hash && !final_producer_list.contains(&self.producer.id) {
                final_producer_list.push(self.producer.id.clone());
            }
        }

        // Create hash of producer list for verification
        let producer_list_hash = self.hash_producer_list(&final_producer_list)?;
        
        // Store producer list
        self.producer.producer_list = Some(final_producer_list);

        let candidate = ProducerCandidate {
            majority_hash,
            producer_list_hash,
            producer_id: self.producer.id.clone(),
            timestamp: current_timestamp_ms(),
        };

        observe_histogram!("consensus_majority_size", producer_list.len() as f64);
        
        Ok(candidate)
    }

    fn hash_producer_list(&self, producer_list: &[String]) -> CatalystResult<Hash> {
        use blake2::{Blake2b256, Digest};
        
        let mut sorted_list = producer_list.to_vec();
        sorted_list.sort();
        
        let mut hasher = Blake2b256::new();
        for producer_id in &sorted_list {
            hasher.update(producer_id.as_bytes());
        }
        
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result[..32]);
        Ok(hash)
    }
}

/// Voting Phase Implementation
pub struct VotingPhase {
    producer: Producer,
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
    pub async fn execute(&mut self, min_data: usize, threshold: f64, reward_config: &RewardConfig) -> CatalystResult<ProducerVote> {
        log_info!(LogCategory::Consensus, "Starting voting phase for cycle {}", self.producer.cycle_number);
        increment_counter!("consensus_voting_phase_total", 1);
        
        let result = time_operation!("consensus_voting_phase_duration", {
            self.create_final_ledger_update(min_data, threshold, reward_config).await
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

    async fn create_final_ledger_update(&mut self, min_data: usize, threshold: f64, reward_config: &RewardConfig) -> CatalystResult<ProducerVote> {
        // Check if we can participate (must have correct hash)
        let our_hash = self.producer.first_hash.ok_or_else(|| {
            ConsensusError::ConsensusFailed {
                reason: "No first hash available for voting".to_string(),
            }
        })?;

        // Verify minimum data and find majority
        if self.collected_candidates.len() < min_data {
            return Err(ConsensusError::InsufficientData {
                got: self.collected_candidates.len(),
                required: min_data,
            }.into());
        }

        // Find majority hash among candidates
        let mut hash_counts: HashMap<Hash, Vec<String>> = HashMap::new();
        for (producer_id, candidate) in &self.collected_candidates {
            hash_counts.entry(candidate.majority_hash)
                .or_insert_with(Vec::new)
                .push(producer_id.clone());
        }

        let total_count = self.collected_candidates.len();
        let required_majority = (total_count as f64 * threshold).ceil() as usize;
        
        let (majority_hash, voting_producers) = hash_counts
            .into_iter()
            .max_by_key(|(_, producers)| producers.len())
            .ok_or_else(|| ConsensusError::NoMajority {
                highest: 0,
                threshold: required_majority,
            })?;

        if voting_producers.len() < required_majority {
            return Err(ConsensusError::NoMajority {
                highest: voting_producers.len(),
                threshold: required_majority,
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
        
        // Create compensation entries
        let compensation_entries = self.create_compensation_entries(&final_producer_list, reward_config)?;

        // Build complete ledger state update
        let partial_update = self.producer.partial_update.as_ref()
            .ok_or_else(|| ConsensusError::ConsensusFailed {
                reason: "No partial update available".to_string(),
            })?
            .clone();

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

        // Create vote list hash
        let vote_list_hash = self.hash_producer_list(&voting_producers)?;
        
        // Include self in vote list
        let mut final_vote_list = voting_producers;
        if !final_vote_list.contains(&self.producer.id) {
            final_vote_list.push(self.producer.id.clone());
        }
        self.producer.vote_list = Some(final_vote_list);

        let vote = ProducerVote {
            ledger_state_hash,
            vote_list_hash,
            producer_id: self.producer.id.clone(),
            timestamp: current_timestamp_ms(),
        };

        Ok(vote)
    }

    fn create_final_producer_list(&self, majority_hash: &Hash) -> CatalystResult<Vec<String>> {
        // Collect all producer lists from candidates with majority hash
        let mut producer_counts: HashMap<String, usize> = HashMap::new();
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
        let final_list: Vec<String> = producer_counts
            .into_iter()
            .filter(|(_, count)| *count >= threshold)
            .map(|(producer_id, _)| producer_id)
            .collect();

        Ok(final_list)
    }

    fn create_compensation_entries(&self, producer_list: &[String], reward_config: &RewardConfig) -> CatalystResult<Vec<CompensationEntry>> {
        let mut entries = Vec::new();
        
        if producer_list.is_empty() {
            return Ok(entries);
        }

        let reward_per_producer = reward_config.producer_reward / producer_list.len() as u64;
        
        for producer_id in producer_list {
            // In a real implementation, we'd look up the producer's public key
            // For now, we'll use a placeholder
            let public_key = [0u8; 32]; // Placeholder
            
            entries.push(CompensationEntry {
                producer_id: producer_id.clone(),
                public_key,
                amount: reward_per_producer,
            });
        }

        Ok(entries)
    }

    fn hash_producer_list(&self, producer_list: &[String]) -> CatalystResult<Hash> {
        use blake2::{Blake2b256, Digest};
        
        let mut sorted_list = producer_list.to_vec();
        sorted_list.sort();
        
        let mut hasher = Blake2b256::new();
        for producer_id in &sorted_list {
            hasher.update(producer_id.as_bytes());
        }
        
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result[..32]);
        Ok(hash)
    }
}

/// Synchronization Phase Implementation
pub struct SynchronizationPhase {
    producer: Producer,
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
    pub async fn execute(&mut self, min_data: usize, threshold: f64) -> CatalystResult<ProducerOutput> {
        log_info!(LogCategory::Consensus, "Starting synchronization phase for cycle {}", self.producer.cycle_number);
        increment_counter!("consensus_synchronization_phase_total", 1);
        
        let result = time_operation!("consensus_synchronization_phase_duration", {
            self.finalize_ledger_update(min_data, threshold).await
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

    async fn finalize_ledger_update(&mut self, min_data: usize, threshold: f64) -> CatalystResult<ProducerOutput> {
        // Check minimum data requirement
        if self.collected_votes.len() < min_data {
            return Err(ConsensusError::InsufficientData {
                got: self.collected_votes.len(),
                required: min_data,
            }.into());
        }

        // Find majority ledger state hash
        let mut hash_counts: HashMap<Hash, Vec<String>> = HashMap::new();
        for (producer_id, vote) in &self.collected_votes {
            hash_counts.entry(vote.ledger_state_hash)
                .or_insert_with(Vec::new)
                .push(producer_id.clone());
        }

        let total_count = self.collected_votes.len();
        let required_majority = (total_count as f64 * threshold).ceil() as usize;
        
        let (final_hash, vote_producers) = hash_counts
            .into_iter()
            .max_by_key(|(_, producers)| producers.len())
            .ok_or_else(|| ConsensusError::NoMajority {
                highest: 0,
                threshold: required_majority,
            })?;

        if vote_producers.len() < required_majority {
            return Err(ConsensusError::NoMajority {
                highest: vote_producers.len(),
                threshold: required_majority,
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
        let cn = self.producer.producer_list.as_ref()
            .map(|list| list.len())
            .unwrap_or(0);
        let threshold = (cn + 1) / 2;  // Cn/2 threshold
        
        let final_list: Vec<String> = vote_counts
            .into_iter()
            .filter(|(_, count)| *count >= threshold)
            .map(|(producer_id, _)| producer_id)
            .collect();

        Ok(final_list)
    }

    async fn store_to_dfs(&self, ledger_update: &LedgerStateUpdate) -> CatalystResult<catalyst_utils::Address> {
        // Serialize the ledger update
        let serialized = catalyst_utils::CatalystSerialize::serialize(ledger_update)
            .map_err(|e| ConsensusError::Serialization(e.to_string()))?;
        
        // In a real implementation, this would store to the distributed file system
        // For now, we'll create a content-based address using hash
        use blake2::{Blake2b256, Digest};
        let mut hasher = Blake2b256::new();
        hasher.update(&serialized);
        let result = hasher.finalize();
        
        let mut address = [0u8; 21];
        address.copy_from_slice(&result[..21]);
        
        // TODO: Actually store to DFS
        log_info!(LogCategory::Consensus, "Storing ledger update to DFS with address: {:?}", address);
        
        Ok(address)
    }

    fn hash_producer_list(&self, producer_list: &[String]) -> CatalystResult<Hash> {
        use blake2::{Blake2b256, Digest};
        
        let mut sorted_list = producer_list.to_vec();
        sorted_list.sort();
        
        let mut hasher = Blake2b256::new();
        for producer_id in &sorted_list {
            hasher.update(producer_id.as_bytes());
        }
        
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result[..32]);
        Ok(hash)
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