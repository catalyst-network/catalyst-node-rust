use crate::{
    types::*, error::ConsensusError, phases::*
};
use catalyst_utils::{CatalystResult, Hash, PublicKey};
use catalyst_utils::logging::{LogCategory, log_info, log_warn, log_error};
use catalyst_utils::{increment_counter, set_gauge};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Producer node responsible for consensus participation
#[derive(Debug, Clone)]
pub struct Producer {
    pub id: ProducerId,
    pub public_key: PublicKey,
    pub cycle_number: CycleNumber,
    pub partial_update: Option<PartialLedgerStateUpdate>,
    pub ledger_update: Option<LedgerStateUpdate>,
    pub first_hash: Option<Hash>,
    pub producer_list: Option<Vec<ProducerId>>,
    pub vote_list: Option<Vec<ProducerId>>,
}

impl Producer {
    pub fn new(id: ProducerId, public_key: PublicKey, cycle_number: CycleNumber) -> Self {
        Self {
            id,
            public_key,
            cycle_number,
            partial_update: None,
            ledger_update: None,
            first_hash: None,
            producer_list: None,
            vote_list: None,
        }
    }

    /// Reset producer state for new cycle
    pub fn reset_for_cycle(&mut self, cycle_number: CycleNumber) {
        self.cycle_number = cycle_number;
        self.partial_update = None;
        self.ledger_update = None;
        self.first_hash = None;
        self.producer_list = None;
        self.vote_list = None;
    }
}

/// Producer manager for handling consensus phases
pub struct ProducerManager {
    producer: Arc<RwLock<Producer>>,
    config: ConsensusConfig,
    reward_config: RewardConfig,
}

impl ProducerManager {
    pub fn new(producer: Producer, config: ConsensusConfig) -> Self {
        Self {
            producer: Arc::new(RwLock::new(producer)),
            config,
            reward_config: RewardConfig::default(),
        }
    }

    pub fn with_reward_config(mut self, reward_config: RewardConfig) -> Self {
        self.reward_config = reward_config;
        self
    }

    /// Execute construction phase
    pub async fn execute_construction_phase(
        &self,
        transactions: Vec<TransactionEntry>,
    ) -> CatalystResult<ProducerQuantity> {
        let producer = self.producer.read().await.clone();
        let mut construction = ConstructionPhase::new(producer.clone());
        
        let quantity = construction.execute(transactions).await?;
        
        // Update producer state
        let mut producer_guard = self.producer.write().await;
        producer_guard.partial_update = construction.producer.partial_update;
        producer_guard.first_hash = construction.producer.first_hash;
        
        increment_counter!("consensus_phases_completed", 1);
        set_gauge!("consensus_current_cycle", producer_guard.cycle_number as f64);
        
        Ok(quantity)
    }

    /// Execute campaigning phase
    pub async fn execute_campaigning_phase(
        &self,
        collected_quantities: Vec<ProducerQuantity>,
    ) -> CatalystResult<ProducerCandidate> {
        let producer = self.producer.read().await.clone();
        let mut campaigning = CampaigningPhase::new(producer.clone());
        
        // Add collected quantities
        let collected_len = collected_quantities.len();
        for quantity in collected_quantities {
            campaigning.add_quantity(quantity);
        }
        
        let min_data = self.calculate_min_data(collected_len);
        let candidate = campaigning.execute(min_data, self.config.confidence_threshold).await?;
        
        // Update producer state
        let mut producer_guard = self.producer.write().await;
        producer_guard.producer_list = campaigning.producer.producer_list;
        
        Ok(candidate)
    }

    /// Execute voting phase
    pub async fn execute_voting_phase(
        &self,
        collected_candidates: Vec<ProducerCandidate>,
    ) -> CatalystResult<ProducerVote> {
        let producer = self.producer.read().await.clone();
        let mut voting = VotingPhase::new(producer.clone());
        
        // Add collected candidates
        let collected_len = collected_candidates.len();
        for candidate in collected_candidates {
            voting.add_candidate(candidate);
        }
        
        let min_data = self.calculate_min_data(collected_len);
        let vote = voting.execute(min_data, self.config.confidence_threshold, &self.reward_config).await?;
        
        // Update producer state
        let mut producer_guard = self.producer.write().await;
        producer_guard.ledger_update = voting.producer.ledger_update;
        producer_guard.vote_list = voting.producer.vote_list;
        
        Ok(vote)
    }

    /// Execute synchronization phase
    pub async fn execute_synchronization_phase(
        &self,
        collected_votes: Vec<ProducerVote>,
    ) -> CatalystResult<ProducerOutput> {
        let producer = self.producer.read().await.clone();
        let mut synchronization = SynchronizationPhase::new(producer);
        
        // Add collected votes
        let collected_len = collected_votes.len();
        for vote in collected_votes {
            synchronization.add_vote(vote);
        }
        
        let min_data = self.calculate_min_data(collected_len);
        let output = synchronization.execute(min_data, self.config.confidence_threshold).await?;
        
        Ok(output)
    }

    /// Get current producer state
    pub async fn get_producer(&self) -> Producer {
        self.producer.read().await.clone()
    }

    /// Reset for new cycle
    pub async fn reset_for_cycle(&self, cycle_number: CycleNumber) {
        let mut producer = self.producer.write().await;
        producer.reset_for_cycle(cycle_number);
        
        log_info!(LogCategory::Consensus, "Producer {} reset for cycle {}", producer.id, cycle_number);
    }

    /// Calculate minimum data requirement based on total expected producers
    fn calculate_min_data(&self, collected: usize) -> usize {
        // Use 75% of collected data as minimum, but at least the configured minimum
        std::cmp::max(
            (collected as f64 * 0.75) as usize,
            self.config.min_producers,
        )
    }

    /// Validate producer is eligible for current cycle
    pub async fn validate_producer_eligibility(&self, producer_id: &str, selected_producers: &[String]) -> CatalystResult<()> {
        if !selected_producers.contains(&producer_id.to_string()) {
            return Err(ConsensusError::InvalidProducer {
                producer_id: producer_id.to_string(),
            }.into());
        }
        Ok(())
    }

    /// Get consensus statistics
    pub async fn get_consensus_stats(&self) -> ConsensusStats {
        let producer = self.producer.read().await;
        
        ConsensusStats {
            cycle_number: producer.cycle_number,
            producer_id: producer.id.clone(),
            has_partial_update: producer.partial_update.is_some(),
            has_ledger_update: producer.ledger_update.is_some(),
            producer_list_size: producer.producer_list.as_ref().map(|l| l.len()).unwrap_or(0),
            vote_list_size: producer.vote_list.as_ref().map(|l| l.len()).unwrap_or(0),
        }
    }
}

/// Statistics about consensus state
#[derive(Debug, Clone)]
pub struct ConsensusStats {
    pub cycle_number: CycleNumber,
    pub producer_id: ProducerId,
    pub has_partial_update: bool,
    pub has_ledger_update: bool,
    pub producer_list_size: usize,
    pub vote_list_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_producer_creation() {
        let producer = Producer::new(
            "test_producer".to_string(),
            [0u8; 32],
            1,
        );
        
        assert_eq!(producer.id, "test_producer");
        assert_eq!(producer.cycle_number, 1);
        assert!(producer.partial_update.is_none());
    }

    #[tokio::test]
    async fn test_producer_reset() {
        let mut producer = Producer::new(
            "test_producer".to_string(),
            [0u8; 32],
            1,
        );
        
        // Simulate some state
        producer.first_hash = Some([1u8; 32]);
        producer.producer_list = Some(vec!["p1".to_string()]);
        
        // Reset for new cycle
        producer.reset_for_cycle(2);
        
        assert_eq!(producer.cycle_number, 2);
        assert!(producer.first_hash.is_none());
        assert!(producer.producer_list.is_none());
    }

    #[tokio::test]
    async fn test_producer_manager() {
        let producer = Producer::new(
            "test_producer".to_string(),
            [0u8; 32],
            1,
        );
        
        let config = ConsensusConfig::default();
        let manager = ProducerManager::new(producer, config);
        
        let stats = manager.get_consensus_stats().await;
        assert_eq!(stats.producer_id, "test_producer");
        assert_eq!(stats.cycle_number, 1);
    }
}