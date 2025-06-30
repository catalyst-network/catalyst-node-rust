use crate::{
    types::*, error::ConsensusError, producer::ProducerManager
};
use catalyst_utils::{CatalystResult, Hash};
use catalyst_utils::logging::{LogCategory, log_info, log_warn, log_error};
use catalyst_utils::metrics::{increment_counter, observe_histogram, time_operation};
use catalyst_network::{NetworkMessage, MessageEnvelope};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, mpsc};
use tokio::time::{timeout, sleep};

/// Main consensus engine implementing the 4-phase collaborative consensus
pub struct CollaborativeConsensus {
    config: ConsensusConfig,
    producer_manager: Option<ProducerManager>,
    selected_producers: Vec<ProducerId>,
    current_cycle: CycleNumber,
    network_sender: Option<mpsc::UnboundedSender<MessageEnvelope>>,
    is_running: Arc<RwLock<bool>>,
}

impl CollaborativeConsensus {
    /// Create new consensus engine
    pub fn new(config: ConsensusConfig) -> Self {
        Self {
            config,
            producer_manager: None,
            selected_producers: Vec::new(),
            current_cycle: 0,
            network_sender: None,
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    /// Set producer manager for this node
    pub fn set_producer_manager(&mut self, manager: ProducerManager) {
        self.producer_manager = Some(manager);
    }

    /// Set network sender for broadcasting messages
    pub fn set_network_sender(&mut self, sender: mpsc::UnboundedSender<MessageEnvelope>) {
        self.network_sender = Some(sender);
    }

    /// Start consensus for a new cycle
    pub async fn start_cycle(
        &mut self,
        cycle_number: CycleNumber,
        selected_producers: Vec<ProducerId>,
        transactions: Vec<TransactionEntry>,
    ) -> CatalystResult<Option<LedgerStateUpdate>> {
        log_info!(LogCategory::Consensus, "Starting consensus cycle {}", cycle_number);
        increment_counter!("consensus_cycles_total", 1);
        
        self.current_cycle = cycle_number;
        self.selected_producers = selected_producers.clone();
        
        // Check if we are a selected producer
        let producer_manager = match &self.producer_manager {
            Some(manager) => {
                let producer = manager.get_producer().await;
                if selected_producers.contains(&producer.id) {
                    log_info!(LogCategory::Consensus, "Participating as producer in cycle {}", cycle_number);
                    manager.reset_for_cycle(cycle_number).await;
                    Some(manager)
                } else {
                    log_info!(LogCategory::Consensus, "Not selected as producer for cycle {}, observing only", cycle_number);
                    None
                }
            }
            None => {
                log_info!(LogCategory::Consensus, "No producer manager configured, observing only");
                None
            }
        };

        // Set running flag
        *self.is_running.write().await = true;

        let result = if let Some(manager) = producer_manager {
            self.execute_producer_consensus(manager, transactions).await
        } else {
            self.observe_consensus().await
        };

        // Clear running flag
        *self.is_running.write().await = false;

        result
    }

    /// Execute full consensus as a producer
    async fn execute_producer_consensus(
        &self,
        manager: &ProducerManager,
        transactions: Vec<TransactionEntry>,
    ) -> CatalystResult<Option<LedgerStateUpdate>> {
        log_info!(LogCategory::Consensus, "Executing producer consensus for cycle {}", self.current_cycle);
        
        let result = time_operation!("consensus_full_cycle_duration", {
            self.run_four_phase_consensus(manager, transactions).await
        });

        match &result {
            Ok(Some(update)) => {
                log_info!(LogCategory::Consensus, "Consensus cycle {} completed successfully", self.current_cycle);
                observe_histogram!("consensus_transaction_count", update.partial_update.transaction_entries.len() as f64);
                observe_histogram!("consensus_producer_count", update.producer_list.len() as f64);
            }
            Ok(None) => {
                log_warn!(LogCategory::Consensus, "Consensus cycle {} completed but no update generated", self.current_cycle);
            }
            Err(e) => {
                log_error!(LogCategory::Consensus, "Consensus cycle {} failed: {}", self.current_cycle, e);
                increment_counter!("consensus_failures_total", 1);
            }
        }

        result
    }

    /// Run the 4-phase consensus protocol
    async fn run_four_phase_consensus(
        &self,
        manager: &ProducerManager,
        transactions: Vec<TransactionEntry>,
    ) -> CatalystResult<Option<LedgerStateUpdate>> {
        // Phase 1: Construction
        let construction_result = self.run_construction_phase(manager, transactions).await?;
        
        // Phase 2: Campaigning
        let campaigning_result = self.run_campaigning_phase(manager).await?;
        
        // Phase 3: Voting
        let voting_result = self.run_voting_phase(manager).await?;
        
        // Phase 4: Synchronization
        let final_result = self.run_synchronization_phase(manager).await?;
        
        // Return the final ledger state update
        Ok(final_result)
    }

    /// Phase 1: Construction - Build partial ledger state update
    async fn run_construction_phase(
        &self,
        manager: &ProducerManager,
        transactions: Vec<TransactionEntry>,
    ) -> CatalystResult<ProducerQuantity> {
        log_info!(LogCategory::Consensus, "Starting construction phase");
        
        let phase_timeout = Duration::from_millis(self.config.construction_phase_ms);
        let construction_future = manager.execute_construction_phase(transactions);
        
        let quantity = timeout(phase_timeout, construction_future)
            .await
            .map_err(|_| ConsensusError::PhaseTimeout {
                phase: "construction".to_string(),
                duration_ms: self.config.construction_phase_ms,
            })??;

        // Broadcast our quantity
        self.broadcast_message(&quantity).await?;
        
        // Collect quantities from other producers for a limited time
        let collection_timeout = Duration::from_millis(self.config.construction_phase_ms);
        let _collected = self.collect_producer_quantities(collection_timeout).await?;
        
        Ok(quantity)
    }

    /// Phase 2: Campaigning - Find majority partial update
    async fn run_campaigning_phase(
        &self,
        manager: &ProducerManager,
    ) -> CatalystResult<ProducerCandidate> {
        log_info!(LogCategory::Consensus, "Starting campaigning phase");
        
        // Collect quantities from previous phase (in practice, we'd use the actual collected data)
        let collected_quantities = Vec::new(); // TODO: Use actual collected data
        
        let phase_timeout = Duration::from_millis(self.config.campaigning_phase_ms);
        let campaigning_future = manager.execute_campaigning_phase(collected_quantities);
        
        let candidate = timeout(phase_timeout, campaigning_future)
            .await
            .map_err(|_| ConsensusError::PhaseTimeout {
                phase: "campaigning".to_string(),
                duration_ms: self.config.campaigning_phase_ms,
            })??;

        // Broadcast our candidate
        self.broadcast_message(&candidate).await?;
        
        // Collect candidates from other producers
        let collection_timeout = Duration::from_millis(self.config.campaigning_phase_ms);
        let _collected = self.collect_producer_candidates(collection_timeout).await?;
        
        Ok(candidate)
    }

    /// Phase 3: Voting - Create final ledger state update
    async fn run_voting_phase(
        &self,
        manager: &ProducerManager,
    ) -> CatalystResult<ProducerVote> {
        log_info!(LogCategory::Consensus, "Starting voting phase");
        
        // Collect candidates from previous phase
        let collected_candidates = Vec::new(); // TODO: Use actual collected data
        
        let phase_timeout = Duration::from_millis(self.config.voting_phase_ms);
        let voting_future = manager.execute_voting_phase(collected_candidates);
        
        let vote = timeout(phase_timeout, voting_future)
            .await
            .map_err(|_| ConsensusError::PhaseTimeout {
                phase: "voting".to_string(),
                duration_ms: self.config.voting_phase_ms,
            })??;

        // Broadcast our vote
        self.broadcast_message(&vote).await?;
        
        // Collect votes from other producers
        let collection_timeout = Duration::from_millis(self.config.voting_phase_ms);
        let _collected = self.collect_producer_votes(collection_timeout).await?;
        
        Ok(vote)
    }

    /// Phase 4: Synchronization - Finalize and broadcast result
    async fn run_synchronization_phase(
        &self,
        manager: &ProducerManager,
    ) -> CatalystResult<Option<LedgerStateUpdate>> {
        log_info!(LogCategory::Consensus, "Starting synchronization phase");
        
        // Collect votes from previous phase
        let collected_votes = Vec::new(); // TODO: Use actual collected data
        
        let phase_timeout = Duration::from_millis(self.config.synchronization_phase_ms);
        let sync_future = manager.execute_synchronization_phase(collected_votes);
        
        let output = timeout(phase_timeout, sync_future)
            .await
            .map_err(|_| ConsensusError::PhaseTimeout {
                phase: "synchronization".to_string(),
                duration_ms: self.config.synchronization_phase_ms,
            })??;

        // Broadcast our output
        self.broadcast_message(&output).await?;
        
        // Get the final ledger state update
        let producer = manager.get_producer().await;
        Ok(producer.ledger_update)
    }

    /// Observe consensus without participating (for non-producer nodes)
    async fn observe_consensus(&self) -> CatalystResult<Option<LedgerStateUpdate>> {
        log_info!(LogCategory::Consensus, "Observing consensus cycle {}", self.current_cycle);
        
        // Wait for the cycle to complete and collect final outputs
        let total_cycle_time = Duration::from_millis(self.config.cycle_duration_ms);
        let _outputs = self.collect_producer_outputs(total_cycle_time).await?;
        
        // TODO: Determine final ledger state update from collected outputs
        // For now, return None indicating we observed but didn't generate an update
        Ok(None)
    }

    /// Broadcast a consensus message to the network
    async fn broadcast_message<T: NetworkMessage>(&self, message: &T) -> CatalystResult<()> {
        if let Some(sender) = &self.network_sender {
            let envelope = MessageEnvelope::from_message(
                message,
                "consensus_engine".to_string(),
                None, // Broadcast to all
            ).map_err(|e| ConsensusError::Network(e.to_string()))?;
            
            sender.send(envelope)
                .map_err(|e| ConsensusError::Network(format!("Failed to send message: {}", e)))?;
        }
        Ok(())
    }

    /// Collect producer quantities during construction phase
    async fn collect_producer_quantities(
        &self,
        timeout_duration: Duration,
    ) -> CatalystResult<Vec<ProducerQuantity>> {
        // TODO: Implement actual message collection from network
        // For now, return empty collection
        sleep(timeout_duration).await;
        Ok(Vec::new())
    }

    /// Collect producer candidates during campaigning phase
    async fn collect_producer_candidates(
        &self,
        timeout_duration: Duration,
    ) -> CatalystResult<Vec<ProducerCandidate>> {
        // TODO: Implement actual message collection from network
        sleep(timeout_duration).await;
        Ok(Vec::new())
    }

    /// Collect producer votes during voting phase
    async fn collect_producer_votes(
        &self,
        timeout_duration: Duration,
    ) -> CatalystResult<Vec<ProducerVote>> {
        // TODO: Implement actual message collection from network
        sleep(timeout_duration).await;
        Ok(Vec::new())
    }

    /// Collect producer outputs during synchronization phase
    async fn collect_producer_outputs(
        &self,
        timeout_duration: Duration,
    ) -> CatalystResult<Vec<ProducerOutput>> {
        // TODO: Implement actual message collection from network
        sleep(timeout_duration).await;
        Ok(Vec::new())
    }

    /// Check if consensus is currently running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Stop the current consensus cycle
    pub async fn stop(&self) {
        *self.is_running.write().await = false;
        log_info!(LogCategory::Consensus, "Consensus engine stopped");
    }

    /// Get current cycle number
    pub fn current_cycle(&self) -> CycleNumber {
        self.current_cycle
    }

    /// Get selected producers for current cycle
    pub fn selected_producers(&self) -> &[ProducerId] {
        &self.selected_producers
    }

    /// Get consensus configuration
    pub fn config(&self) -> &ConsensusConfig {
        &self.config
    }
}

/// Message collector for gathering consensus messages from network
pub struct ConsensusMessageCollector {
    quantities: HashMap<ProducerId, ProducerQuantity>,
    candidates: HashMap<ProducerId, ProducerCandidate>,
    votes: HashMap<ProducerId, ProducerVote>,
    outputs: HashMap<ProducerId, ProducerOutput>,
}

impl ConsensusMessageCollector {
    pub fn new() -> Self {
        Self {
            quantities: HashMap::new(),
            candidates: HashMap::new(),
            votes: HashMap::new(),
            outputs: HashMap::new(),
        }
    }

    /// Add a producer quantity
    pub fn add_quantity(&mut self, quantity: ProducerQuantity) {
        self.quantities.insert(quantity.producer_id.clone(), quantity);
    }

    /// Add a producer candidate
    pub fn add_candidate(&mut self, candidate: ProducerCandidate) {
        self.candidates.insert(candidate.producer_id.clone(), candidate);
    }

    /// Add a producer vote
    pub fn add_vote(&mut self, vote: ProducerVote) {
        self.votes.insert(vote.producer_id.clone(), vote);
    }

    /// Add a producer output
    pub fn add_output(&mut self, output: ProducerOutput) {
        self.outputs.insert(output.producer_id.clone(), output);
    }

    /// Get collected quantities
    pub fn get_quantities(&self) -> Vec<ProducerQuantity> {
        self.quantities.values().cloned().collect()
    }

    /// Get collected candidates
    pub fn get_candidates(&self) -> Vec<ProducerCandidate> {
        self.candidates.values().cloned().collect()
    }

    /// Get collected votes
    pub fn get_votes(&self) -> Vec<ProducerVote> {
        self.votes.values().cloned().collect()
    }

    /// Get collected outputs
    pub fn get_outputs(&self) -> Vec<ProducerOutput> {
        self.outputs.values().cloned().collect()
    }

    /// Clear all collected messages
    pub fn clear(&mut self) {
        self.quantities.clear();
        self.candidates.clear();
        self.votes.clear();
        self.outputs.clear();
    }

    /// Get statistics about collected messages
    pub fn get_stats(&self) -> CollectorStats {
        CollectorStats {
            quantities_count: self.quantities.len(),
            candidates_count: self.candidates.len(),
            votes_count: self.votes.len(),
            outputs_count: self.outputs.len(),
        }
    }
}

/// Statistics about message collection
#[derive(Debug, Clone)]
pub struct CollectorStats {
    pub quantities_count: usize,
    pub candidates_count: usize,
    pub votes_count: usize,
    pub outputs_count: usize,
}

/// Producer selection algorithm for determining consensus participants
pub struct ProducerSelection;

impl ProducerSelection {
    /// Select producers for a given cycle using deterministic randomization
    pub fn select_producers(
        cycle_number: CycleNumber,
        previous_state_hash: Hash,
        worker_pool: &[ProducerId],
        producer_count: usize,
    ) -> CatalystResult<Vec<ProducerId>> {
        if worker_pool.is_empty() {
            return Ok(Vec::new());
        }

        if producer_count >= worker_pool.len() {
            return Ok(worker_pool.to_vec());
        }

        // Create deterministic seed from cycle number and previous state
        let seed = Self::create_selection_seed(cycle_number, previous_state_hash);
        
        // Use XOR-based selection as described in the paper
        let mut selections: Vec<(ProducerId, u64)> = worker_pool
            .iter()
            .map(|producer_id| {
                let producer_hash = Self::hash_producer_id(producer_id);
                let selection_value = Self::xor_hash_with_seed(producer_hash, seed);
                (producer_id.clone(), selection_value)
            })
            .collect();

        // Sort by selection value and take the first `producer_count`
        selections.sort_by_key(|(_, value)| *value);
        
        let selected: Vec<ProducerId> = selections
            .into_iter()
            .take(producer_count)
            .map(|(producer_id, _)| producer_id)
            .collect();

        log_info!(LogCategory::Consensus, 
            "Selected {} producers for cycle {} from pool of {}", 
            selected.len(), cycle_number, worker_pool.len()
        );

        Ok(selected)
    }

    /// Create deterministic seed for producer selection
    fn create_selection_seed(cycle_number: CycleNumber, previous_state_hash: Hash) -> u64 {
        use blake2::{Blake2b256, Digest};
        
        let mut hasher = Blake2b256::new();
        hasher.update(&cycle_number.to_be_bytes());
        hasher.update(&previous_state_hash);
        
        let result = hasher.finalize();
        let mut seed_bytes = [0u8; 8];
        seed_bytes.copy_from_slice(&result[..8]);
        u64::from_be_bytes(seed_bytes)
    }

    /// Hash a producer ID to a deterministic value
    fn hash_producer_id(producer_id: &str) -> u64 {
        use blake2::{Blake2b256, Digest};
        
        let mut hasher = Blake2b256::new();
        hasher.update(producer_id.as_bytes());
        
        let result = hasher.finalize();
        let mut hash_bytes = [0u8; 8];
        hash_bytes.copy_from_slice(&result[..8]);
        u64::from_be_bytes(hash_bytes)
    }

    /// XOR hash with seed for selection randomization
    fn xor_hash_with_seed(hash: u64, seed: u64) -> u64 {
        hash ^ seed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::producer::Producer;
    use tokio_test;

    #[tokio::test]
    async fn test_consensus_creation() {
        let config = ConsensusConfig::default();
        let consensus = CollaborativeConsensus::new(config);
        
        assert_eq!(consensus.current_cycle(), 0);
        assert!(consensus.selected_producers().is_empty());
        assert!(!consensus.is_running().await);
    }

    #[tokio::test]
    async fn test_producer_selection() {
        let worker_pool = vec![
            "producer_1".to_string(),
            "producer_2".to_string(),
            "producer_3".to_string(),
            "producer_4".to_string(),
            "producer_5".to_string(),
        ];
        
        let selected = ProducerSelection::select_producers(
            1,
            [0u8; 32],
            &worker_pool,
            3,
        ).unwrap();
        
        assert_eq!(selected.len(), 3);
        
        // Selection should be deterministic
        let selected_again = ProducerSelection::select_producers(
            1,
            [0u8; 32],
            &worker_pool,
            3,
        ).unwrap();
        
        assert_eq!(selected, selected_again);
    }

    #[tokio::test]
    async fn test_message_collector() {
        let mut collector = ConsensusMessageCollector::new();
        
        let quantity = ProducerQuantity {
            first_hash: [1u8; 32],
            producer_id: "test_producer".to_string(),
            timestamp: current_timestamp_ms(),
        };
        
        collector.add_quantity(quantity.clone());
        
        let quantities = collector.get_quantities();
        assert_eq!(quantities.len(), 1);
        assert_eq!(quantities[0], quantity);
        
        let stats = collector.get_stats();
        assert_eq!(stats.quantities_count, 1);
        assert_eq!(stats.candidates_count, 0);
    }

    #[tokio::test]
    async fn test_consensus_with_producer() {
        let config = ConsensusConfig::default();
        let mut consensus = CollaborativeConsensus::new(config);
        
        let producer = Producer::new(
            "test_producer".to_string(),
            [0u8; 32],
            1,
        );
        
        let manager = ProducerManager::new(producer, ConsensusConfig::default());
        consensus.set_producer_manager(manager);
        
        // Test that we can start a cycle (will fail without network setup, but validates structure)
        let selected_producers = vec!["test_producer".to_string()];
        let transactions = vec![];
        
        let result = consensus.start_cycle(1, selected_producers, transactions).await;
        
        // Should fail due to missing network setup, but validates the flow
        assert!(result.is_err());
    }
}