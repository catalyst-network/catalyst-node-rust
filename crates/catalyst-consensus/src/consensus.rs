use crate::{
    types::*, error::ConsensusError, producer::ProducerManager
};
use catalyst_utils::{CatalystResult, Hash};
use catalyst_utils::logging::{LogCategory, log_info, log_warn, log_error};
use catalyst_utils::{increment_counter, observe_histogram, time_operation};
use catalyst_utils::{NetworkMessage, MessageEnvelope};
use catalyst_utils::MessageType;
use std::collections::{BTreeMap, HashMap, VecDeque};
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
    network_receiver: Option<Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<MessageEnvelope>>>>,
    pending_envelopes: Arc<tokio::sync::Mutex<VecDeque<MessageEnvelope>>>,
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
            network_receiver: None,
            pending_envelopes: Arc::new(tokio::sync::Mutex::new(VecDeque::new())),
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    async fn drain_pending_by_type(&self, want: MessageType) -> Vec<MessageEnvelope> {
        let mut pending = self.pending_envelopes.lock().await;
        if pending.is_empty() {
            return Vec::new();
        }

        let mut keep: VecDeque<MessageEnvelope> = VecDeque::with_capacity(pending.len());
        let mut out: Vec<MessageEnvelope> = Vec::new();

        while let Some(env) = pending.pop_front() {
            if env.message_type == want {
                out.push(env);
            } else {
                keep.push_back(env);
            }
        }

        *pending = keep;
        out
    }

    /// Set producer manager for this node
    pub fn set_producer_manager(&mut self, manager: ProducerManager) {
        self.producer_manager = Some(manager);
    }

    /// Set network sender for broadcasting messages
    pub fn set_network_sender(&mut self, sender: mpsc::UnboundedSender<MessageEnvelope>) {
        self.network_sender = Some(sender);
    }

    /// Set network receiver for collecting inbound messages.
    pub fn set_network_receiver(&mut self, receiver: mpsc::UnboundedReceiver<MessageEnvelope>) {
        self.network_receiver = Some(Arc::new(tokio::sync::Mutex::new(receiver)));
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
                    manager.reset_for_cycle(cycle_number, selected_producers.len()).await;
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
        let quantity = self.run_construction_phase(manager, transactions).await?;
        let mut quantities = self
            .collect_producer_quantities(Duration::from_millis(self.config.construction_phase_ms))
            .await?;
        quantities.push(quantity.clone());
        dedup_by_producer_id_quantity(&mut quantities);
        log_info!(
            LogCategory::Consensus,
            "Construction collected quantities: {} (cycle={})",
            quantities.len(),
            self.current_cycle
        );

        // Phase 2: Campaigning
        //
        // NOTE: Networking collection is still WIP, but to make the node runnable (single
        // producer) we always feed our own message forward. When networking is wired in,
        // this should be replaced with the collected set from the network, plus our own.
        let candidate = self.run_campaigning_phase(manager, quantities).await?;
        let mut candidates = self
            .collect_producer_candidates(Duration::from_millis(self.config.campaigning_phase_ms))
            .await?;
        candidates.push(candidate.clone());
        dedup_by_producer_id_candidate(&mut candidates);
        log_info!(
            LogCategory::Consensus,
            "Campaigning collected candidates: {} (cycle={})",
            candidates.len(),
            self.current_cycle
        );

        // Phase 3: Voting
        let vote = self.run_voting_phase(manager, candidates).await?;
        let mut votes = self
            .collect_producer_votes(Duration::from_millis(self.config.voting_phase_ms))
            .await?;
        votes.push(vote.clone());
        dedup_by_producer_id_vote(&mut votes);
        log_info!(
            LogCategory::Consensus,
            "Voting collected votes: {} (cycle={})",
            votes.len(),
            self.current_cycle
        );

        // Phase 4: Synchronization
        let final_result = self.run_synchronization_phase(manager, votes).await?;
        
        // Return the final ledger state update
        Ok(final_result)
    }

    /// Phase 1: Construction - Build partial ledger state update
    async fn run_construction_phase(
        &self,
        manager: &ProducerManager,
        transactions: Vec<TransactionEntry>,
    ) -> CatalystResult<ProducerQuantity> {
        log_info!(LogCategory::Consensus, "Starting construction phase (cycle={})", self.current_cycle);
        
        let phase_timeout = Duration::from_millis(self.config.construction_phase_ms);
        let construction_future = manager.execute_construction_phase(transactions);
        let t0 = Instant::now();
        
        let quantity = timeout(phase_timeout, construction_future)
            .await
            .map_err(|_| ConsensusError::PhaseTimeout {
                phase: "construction".to_string(),
                duration_ms: self.config.construction_phase_ms,
            })??;
        observe_histogram!("consensus_phase_duration", t0.elapsed().as_secs_f64());

        // Broadcast our quantity
        self.broadcast_message(&quantity).await?;

        Ok(quantity)
    }

    /// Phase 2: Campaigning - Find majority partial update
    async fn run_campaigning_phase(
        &self,
        manager: &ProducerManager,
        collected_quantities: Vec<ProducerQuantity>,
    ) -> CatalystResult<ProducerCandidate> {
        log_info!(LogCategory::Consensus, "Starting campaigning phase (cycle={})", self.current_cycle);

        let phase_timeout = Duration::from_millis(self.config.campaigning_phase_ms);
        let campaigning_future = manager.execute_campaigning_phase(collected_quantities);
        let t0 = Instant::now();
        
        let candidate = timeout(phase_timeout, campaigning_future)
            .await
            .map_err(|_| ConsensusError::PhaseTimeout {
                phase: "campaigning".to_string(),
                duration_ms: self.config.campaigning_phase_ms,
            })??;
        observe_histogram!("consensus_phase_duration", t0.elapsed().as_secs_f64());

        // Broadcast our candidate
        self.broadcast_message(&candidate).await?;
        
        // TODO: Collect candidates from other producers via network and merge.
        
        Ok(candidate)
    }

    /// Phase 3: Voting - Create final ledger state update
    async fn run_voting_phase(
        &self,
        manager: &ProducerManager,
        collected_candidates: Vec<ProducerCandidate>,
    ) -> CatalystResult<ProducerVote> {
        log_info!(LogCategory::Consensus, "Starting voting phase (cycle={})", self.current_cycle);

        let phase_timeout = Duration::from_millis(self.config.voting_phase_ms);
        let voting_future = manager.execute_voting_phase(collected_candidates);
        let t0 = Instant::now();
        
        let vote = timeout(phase_timeout, voting_future)
            .await
            .map_err(|_| ConsensusError::PhaseTimeout {
                phase: "voting".to_string(),
                duration_ms: self.config.voting_phase_ms,
            })??;
        observe_histogram!("consensus_phase_duration", t0.elapsed().as_secs_f64());

        // Broadcast our vote
        self.broadcast_message(&vote).await?;
        
        // TODO: Collect votes from other producers via network and merge.
        
        Ok(vote)
    }

    /// Phase 4: Synchronization - Finalize and broadcast result
    async fn run_synchronization_phase(
        &self,
        manager: &ProducerManager,
        collected_votes: Vec<ProducerVote>,
    ) -> CatalystResult<Option<LedgerStateUpdate>> {
        log_info!(LogCategory::Consensus, "Starting synchronization phase (cycle={})", self.current_cycle);

        let phase_timeout = Duration::from_millis(self.config.synchronization_phase_ms);
        let sync_future = manager.execute_synchronization_phase(collected_votes);
        let t0 = Instant::now();
        
        let output = timeout(phase_timeout, sync_future)
            .await
            .map_err(|_| ConsensusError::PhaseTimeout {
                phase: "synchronization".to_string(),
                duration_ms: self.config.synchronization_phase_ms,
            })??;
        observe_histogram!("consensus_phase_duration", t0.elapsed().as_secs_f64());

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
        let Some(rx) = &self.network_receiver else {
            sleep(timeout_duration).await;
            return Ok(Vec::new());
        };

        let deadline = Instant::now() + timeout_duration;
        let mut out: HashMap<ProducerId, ProducerQuantity> = HashMap::new();

        // First, consume any previously buffered messages of this type.
        for env in self.drain_pending_by_type(MessageType::ProducerQuantity).await {
            if let Ok(q) = env.extract_message::<ProducerQuantity>() {
                if q.cycle_number == self.current_cycle && self.selected_producers.contains(&q.producer_id) {
                    out.insert(q.producer_id.clone(), q);
                }
            }
        }

        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;

            let mut guard = rx.lock().await;
            match timeout(remaining, guard.recv()).await {
                Ok(Some(env)) => {
                    if env.message_type == MessageType::ProducerQuantity {
                        if let Ok(q) = env.extract_message::<ProducerQuantity>() {
                            if q.cycle_number == self.current_cycle && self.selected_producers.contains(&q.producer_id) {
                                out.insert(q.producer_id.clone(), q);
                            }
                        }
                    } else {
                        // Buffer for later phases instead of dropping it.
                        self.pending_envelopes.lock().await.push_back(env);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        Ok(out.into_values().collect())
    }

    /// Collect producer candidates during campaigning phase
    async fn collect_producer_candidates(
        &self,
        timeout_duration: Duration,
    ) -> CatalystResult<Vec<ProducerCandidate>> {
        let Some(rx) = &self.network_receiver else {
            sleep(timeout_duration).await;
            return Ok(Vec::new());
        };

        let deadline = Instant::now() + timeout_duration;
        let mut out: HashMap<ProducerId, ProducerCandidate> = HashMap::new();

        for env in self.drain_pending_by_type(MessageType::ProducerCandidate).await {
            if let Ok(c) = env.extract_message::<ProducerCandidate>() {
                if c.cycle_number == self.current_cycle && self.selected_producers.contains(&c.producer_id) {
                    out.insert(c.producer_id.clone(), c);
                }
            }
        }

        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;

            let mut guard = rx.lock().await;
            match timeout(remaining, guard.recv()).await {
                Ok(Some(env)) => {
                    if env.message_type == MessageType::ProducerCandidate {
                        if let Ok(c) = env.extract_message::<ProducerCandidate>() {
                            if c.cycle_number == self.current_cycle && self.selected_producers.contains(&c.producer_id) {
                                out.insert(c.producer_id.clone(), c);
                            }
                        }
                    } else {
                        self.pending_envelopes.lock().await.push_back(env);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        Ok(out.into_values().collect())
    }

    /// Collect producer votes during voting phase
    async fn collect_producer_votes(
        &self,
        timeout_duration: Duration,
    ) -> CatalystResult<Vec<ProducerVote>> {
        let Some(rx) = &self.network_receiver else {
            sleep(timeout_duration).await;
            return Ok(Vec::new());
        };

        let deadline = Instant::now() + timeout_duration;
        let mut out: HashMap<ProducerId, ProducerVote> = HashMap::new();

        for env in self.drain_pending_by_type(MessageType::ProducerVote).await {
            if let Ok(v) = env.extract_message::<ProducerVote>() {
                if v.cycle_number == self.current_cycle && self.selected_producers.contains(&v.producer_id) {
                    out.insert(v.producer_id.clone(), v);
                }
            }
        }

        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;

            let mut guard = rx.lock().await;
            match timeout(remaining, guard.recv()).await {
                Ok(Some(env)) => {
                    if env.message_type == MessageType::ProducerVote {
                        if let Ok(v) = env.extract_message::<ProducerVote>() {
                            if v.cycle_number == self.current_cycle && self.selected_producers.contains(&v.producer_id) {
                                out.insert(v.producer_id.clone(), v);
                            }
                        }
                    } else {
                        self.pending_envelopes.lock().await.push_back(env);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        Ok(out.into_values().collect())
    }

    /// Collect producer outputs during synchronization phase
    async fn collect_producer_outputs(
        &self,
        timeout_duration: Duration,
    ) -> CatalystResult<Vec<ProducerOutput>> {
        let Some(rx) = &self.network_receiver else {
            sleep(timeout_duration).await;
            return Ok(Vec::new());
        };

        let deadline = Instant::now() + timeout_duration;
        let mut out: HashMap<ProducerId, ProducerOutput> = HashMap::new();

        for env in self.drain_pending_by_type(MessageType::ProducerOutput).await {
            if let Ok(o) = env.extract_message::<ProducerOutput>() {
                if o.cycle_number == self.current_cycle {
                    out.insert(o.producer_id.clone(), o);
                }
            }
        }

        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let remaining = deadline - now;

            let mut guard = rx.lock().await;
            match timeout(remaining, guard.recv()).await {
                Ok(Some(env)) => {
                    if env.message_type == MessageType::ProducerOutput {
                        if let Ok(o) = env.extract_message::<ProducerOutput>() {
                            if o.cycle_number == self.current_cycle {
                                out.insert(o.producer_id.clone(), o);
                            }
                        }
                    } else {
                        self.pending_envelopes.lock().await.push_back(env);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        Ok(out.into_values().collect())
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

fn dedup_by_producer_id_quantity(v: &mut Vec<ProducerQuantity>) {
    let mut seen: BTreeMap<String, ProducerQuantity> = BTreeMap::new();
    for q in v.drain(..) {
        seen.insert(q.producer_id.clone(), q);
    }
    *v = seen.into_values().collect();
}

fn dedup_by_producer_id_candidate(v: &mut Vec<ProducerCandidate>) {
    let mut seen: BTreeMap<String, ProducerCandidate> = BTreeMap::new();
    for c in v.drain(..) {
        seen.insert(c.producer_id.clone(), c);
    }
    *v = seen.into_values().collect();
}

fn dedup_by_producer_id_vote(v: &mut Vec<ProducerVote>) {
    let mut seen: BTreeMap<String, ProducerVote> = BTreeMap::new();
    for x in v.drain(..) {
        seen.insert(x.producer_id.clone(), x);
    }
    *v = seen.into_values().collect();
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
        use blake2::{Blake2b512, Digest};
        
        let mut hasher = Blake2b512::new();
        hasher.update(&cycle_number.to_be_bytes());
        hasher.update(&previous_state_hash);
        
        let result = hasher.finalize();
        let mut seed_bytes = [0u8; 8];
        seed_bytes.copy_from_slice(&result[..8]);
        u64::from_be_bytes(seed_bytes)
    }

    /// Hash a producer ID to a deterministic value
    fn hash_producer_id(producer_id: &str) -> u64 {
        use blake2::{Blake2b512, Digest};
        
        let mut hasher = Blake2b512::new();
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
    use crate::producer::ProducerManager;
    use tokio::sync::mpsc;

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
            cycle_number: 1,
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
        
        // Single-producer cycle should succeed without network wiring.
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_two_node_consensus_converges_with_message_passing() {
        let config = ConsensusConfig {
            cycle_duration_ms: 3000,
            construction_phase_ms: 500,
            campaigning_phase_ms: 500,
            voting_phase_ms: 500,
            synchronization_phase_ms: 500,
            freeze_window_ms: 100,
            min_producers: 2,
            confidence_threshold: 0.6,
        };

        let cycle: CycleNumber = 1;
        let selected: Vec<ProducerId> = vec!["producer_1".to_string(), "producer_2".to_string()];

        // Shared tx set (deterministic inputs).
        let transactions = vec![
            TransactionEntry {
                public_key: [10u8; 32],
                amount: -100,
                signature: vec![20u8; 64],
            },
            TransactionEntry {
                public_key: [11u8; 32],
                amount: 100,
                signature: vec![21u8; 64],
            },
        ];

        // Build node1 consensus engine with in/out channels.
        let producer1 = Producer::new("producer_1".to_string(), [1u8; 32], cycle);
        let manager1 = ProducerManager::new(producer1, config.clone());
        let (out1_tx, mut out1_rx) = mpsc::unbounded_channel::<catalyst_utils::MessageEnvelope>();
        let (in1_tx, in1_rx) = mpsc::unbounded_channel::<catalyst_utils::MessageEnvelope>();
        let mut c1 = CollaborativeConsensus::new(config.clone());
        c1.set_producer_manager(manager1);
        c1.set_network_sender(out1_tx);
        c1.set_network_receiver(in1_rx);

        // Build node2 consensus engine with in/out channels.
        let producer2 = Producer::new("producer_2".to_string(), [2u8; 32], cycle);
        let manager2 = ProducerManager::new(producer2, config.clone());
        let (out2_tx, mut out2_rx) = mpsc::unbounded_channel::<catalyst_utils::MessageEnvelope>();
        let (in2_tx, in2_rx) = mpsc::unbounded_channel::<catalyst_utils::MessageEnvelope>();
        let mut c2 = CollaborativeConsensus::new(config.clone());
        c2.set_producer_manager(manager2);
        c2.set_network_sender(out2_tx);
        c2.set_network_receiver(in2_rx);

        // Message bus: forward every outbound envelope to both nodes (best-effort).
        let bus_targets = vec![in1_tx.clone(), in2_tx.clone()];
        let bus1 = tokio::spawn(async move {
            while let Some(env) = out1_rx.recv().await {
                for tx in &bus_targets {
                    let _ = tx.send(env.clone());
                }
            }
        });
        let bus_targets2 = vec![in1_tx.clone(), in2_tx.clone()];
        let bus2 = tokio::spawn(async move {
            while let Some(env) = out2_rx.recv().await {
                for tx in &bus_targets2 {
                    let _ = tx.send(env.clone());
                }
            }
        });

        // Run cycle concurrently.
        let selected1 = selected.clone();
        let selected2 = selected.clone();
        let txs1 = transactions.clone();
        let txs2 = transactions.clone();
        let (r1, r2) = tokio::join!(
            async move { c1.start_cycle(cycle, selected1, txs1).await },
            async move { c2.start_cycle(cycle, selected2, txs2).await },
        );

        bus1.abort();
        bus2.abort();

        let u1 = r1.unwrap().expect("node1 should produce LSU");
        let u2 = r2.unwrap().expect("node2 should produce LSU");

        // Convergence: LSU hash must match.
        let h1 = hash_data(&u1).unwrap();
        let h2 = hash_data(&u2).unwrap();
        assert_eq!(h1, h2, "nodes should converge on identical LSU hash");
    }

    #[tokio::test]
    async fn test_out_of_order_messages_are_buffered_and_consumed_in_later_phases() {
        let config = ConsensusConfig {
            cycle_duration_ms: 1000,
            construction_phase_ms: 50,
            campaigning_phase_ms: 50,
            voting_phase_ms: 50,
            synchronization_phase_ms: 50,
            freeze_window_ms: 10,
            min_producers: 2,
            confidence_threshold: 0.6,
        };

        let cycle: CycleNumber = 7;
        let selected: Vec<ProducerId> = vec!["p1".to_string(), "p2".to_string()];

        let producer = Producer::new("p1".to_string(), [1u8; 32], cycle);
        let manager = ProducerManager::new(producer, config.clone());
        let mut consensus = CollaborativeConsensus::new(config.clone());
        consensus.set_producer_manager(manager);

        // Wire a receiver so collection loops read real messages.
        let (tx, rx) = mpsc::unbounded_channel::<MessageEnvelope>();
        consensus.set_network_receiver(rx);

        // Set cycle + membership (normally done in start_cycle()).
        consensus.current_cycle = cycle;
        consensus.selected_producers = selected.clone();

        // Send a campaigning message *before* quantities are collected. It should be buffered.
        let candidate = ProducerCandidate {
            majority_hash: [9u8; 32],
            producer_list_hash: [8u8; 32],
            producer_list: selected.clone(),
            cycle_number: cycle,
            producer_id: "p2".to_string(),
            timestamp: 0,
        };
        let env_c = MessageEnvelope::from_message(&candidate, "test".to_string(), None).unwrap();
        tx.send(env_c).unwrap();

        // Construction collection should not surface candidates.
        let quantities = consensus
            .collect_producer_quantities(Duration::from_millis(30))
            .await
            .unwrap();
        assert!(quantities.is_empty());

        // Now send the quantity and ensure construction collector sees it.
        let q = ProducerQuantity {
            first_hash: [7u8; 32],
            cycle_number: cycle,
            producer_id: "p2".to_string(),
            timestamp: 0,
        };
        let env_q = MessageEnvelope::from_message(&q, "test".to_string(), None).unwrap();
        tx.send(env_q).unwrap();

        let quantities = consensus
            .collect_producer_quantities(Duration::from_millis(30))
            .await
            .unwrap();
        assert_eq!(quantities.len(), 1);
        assert_eq!(quantities[0], q);

        // Campaigning collection should drain the buffered candidate.
        let candidates = consensus
            .collect_producer_candidates(Duration::from_millis(30))
            .await
            .unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0], candidate);
    }
}