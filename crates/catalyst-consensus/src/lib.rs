pub mod collaborative;
pub mod producer;
pub mod validator;
pub mod cycle;
pub mod messages;

use async_trait::async_trait;
use catalyst_core::{
    ConsensusModule, CatalystModule, CatalystResult, Block, Transaction,
    ConsensusMessage, LedgerCycle, NodeId, ConsensusError, BlockHash
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

pub use collaborative::CollaborativeConsensus;
pub use producer::ProducerManager;
pub use validator::TransactionValidator;
pub use cycle::CycleManager;
pub use messages::*;

/// Main consensus implementation for Catalyst Network
pub struct CatalystConsensus {
    /// Collaborative consensus engine
    consensus_engine: Arc<CollaborativeConsensus>,
    /// Producer management
    producer_manager: Arc<ProducerManager>,
    /// Transaction validation
    validator: Arc<TransactionValidator>,
    /// Cycle management
    cycle_manager: Arc<CycleManager>,
    /// Current state
    state: Arc<RwLock<ConsensusState>>,
}

/// Internal consensus state
#[derive(Debug, Clone)]
pub struct ConsensusState {
    /// Current ledger cycle
    pub current_cycle: Option<LedgerCycle>,
    /// Whether this node is currently a producer
    pub is_producer: bool,
    /// Current consensus phase
    pub current_phase: ConsensusPhase,
    /// Latest finalized block
    pub latest_block: Option<Block>,
    /// Pending transactions
    pub pending_transactions: Vec<Transaction>,
}

/// Consensus phases from the 4-phase protocol
#[derive(Debug, Clone, PartialEq)]
pub enum ConsensusPhase {
    Idle,
    Construction,
    Campaigning, 
    Voting,
    Synchronization,
}

impl CatalystConsensus {
    /// Create a new consensus instance
    pub fn new(
        node_id: NodeId,
        storage: Arc<dyn catalyst_core::StorageModule>,
        network: Arc<dyn catalyst_core::NetworkModule>,
    ) -> Self {
        let validator = Arc::new(TransactionValidator::new());
        let cycle_manager = Arc::new(CycleManager::new(node_id));
        let producer_manager = Arc::new(ProducerManager::new(node_id, storage.clone()));
        let consensus_engine = Arc::new(CollaborativeConsensus::new(
            node_id,
            storage.clone(),
            network.clone(),
            producer_manager.clone(),
        ));

        let state = Arc::new(RwLock::new(ConsensusState {
            current_cycle: None,
            is_producer: false,
            current_phase: ConsensusPhase::Idle,
            latest_block: None,
            pending_transactions: Vec::new(),
        }));

        Self {
            consensus_engine,
            producer_manager,
            validator,
            cycle_manager,
            state,
        }
    }

    /// Add a transaction to the pending pool
    pub async fn add_transaction(&self, transaction: Transaction) -> CatalystResult<()> {
        // Validate transaction first
        if !self.validator.validate_transaction(&transaction).await? {
            return Err(ConsensusError::BlockValidationFailed {
                reason: "Transaction validation failed".to_string(),
            }.into());
        }

        let mut state = self.state.write().await;
        state.pending_transactions.push(transaction);
        
        info!("Added transaction to pending pool, total: {}", state.pending_transactions.len());
        Ok(())
    }

    /// Get pending transactions for block creation
    pub async fn get_pending_transactions(&self) -> CatalystResult<Vec<Transaction>> {
        let state = self.state.read().await;
        Ok(state.pending_transactions.clone())
    }

    /// Clear pending transactions (after block creation)
    pub async fn clear_pending_transactions(&self) -> CatalystResult<()> {
        let mut state = self.state.write().await;
        state.pending_transactions.clear();
        Ok(())
    }

    /// Handle phase transition
    async fn transition_to_phase(&self, new_phase: ConsensusPhase) -> CatalystResult<()> {
        let mut state = self.state.write().await;
        let old_phase = state.current_phase.clone();
        state.current_phase = new_phase.clone();
        
        info!("Consensus phase transition: {:?} -> {:?}", old_phase, new_phase);
        
        match new_phase {
            ConsensusPhase::Construction => {
                self.consensus_engine.start_construction_phase().await?;
            },
            ConsensusPhase::Campaigning => {
                self.consensus_engine.start_campaigning_phase().await?;
            },
            ConsensusPhase::Voting => {
                self.consensus_engine.start_voting_phase().await?;
            },
            ConsensusPhase::Synchronization => {
                self.consensus_engine.start_synchronization_phase().await?;
            },
            ConsensusPhase::Idle => {
                // Cycle completed, prepare for next
            },
        }
        
        Ok(())
    }
}

#[async_trait]
impl CatalystModule for CatalystConsensus {
    fn name(&self) -> &'static str {
        "catalyst-consensus"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    async fn initialize(&mut self) -> CatalystResult<()> {
        info!("Initializing Catalyst consensus module");
        
        // Initialize sub-components
        // Note: In a real implementation, these would be mutable
        // This is a simplified version for the structure
        
        Ok(())
    }

    async fn start(&mut self) -> CatalystResult<()> {
        info!("Starting Catalyst consensus module");
        
        // Start cycle management
        self.cycle_manager.start().await?;
        
        // Start producer management
        // self.producer_manager.start().await?;
        
        // Start consensus engine
        // self.consensus_engine.start().await?;
        
        Ok(())
    }

    async fn stop(&mut self) -> CatalystResult<()> {
        info!("Stopping Catalyst consensus module");
        
        // Stop all components gracefully
        self.cycle_manager.stop().await?;
        
        let mut state = self.state.write().await;
        state.current_phase = ConsensusPhase::Idle;
        state.is_producer = false;
        
        Ok(())
    }

    async fn health_check(&self) -> CatalystResult<bool> {
        // Check if consensus is making progress
        let state = self.state.read().await;
        
        // Basic health checks
        let is_healthy = match state.current_phase {
            ConsensusPhase::Idle => true,
            _ => {
                // Check if we're not stuck in a phase too long
                // This would need actual timing logic
                true
            }
        };
        
        Ok(is_healthy)
    }
}

#[async_trait]
impl ConsensusModule for CatalystConsensus {
    async fn start_cycle(&self, cycle: LedgerCycle) -> CatalystResult<()> {
        info!("Starting new ledger cycle: {}", cycle.cycle_id);
        
        let mut state = self.state.write().await;
        state.current_cycle = Some(cycle.clone());
        
        // Check if we're selected as a producer for this cycle
        let is_producer = self.producer_manager.is_selected_producer(&cycle).await?;
        state.is_producer = is_producer;
        
        if is_producer {
            info!("Selected as producer for cycle {}", cycle.cycle_id);
            // Transition to construction phase
            drop(state); // Release lock before async call
            self.transition_to_phase(ConsensusPhase::Construction).await?;
        } else {
            info!("Participating as validator for cycle {}", cycle.cycle_id);
        }
        
        Ok(())
    }

    async fn process_consensus_message(&self, message: ConsensusMessage) -> CatalystResult<()> {
        match message {
            ConsensusMessage::ProducerQuantity { producer_id, hash_value, cycle_id } => {
                self.consensus_engine.handle_producer_quantity(producer_id, hash_value, cycle_id).await?;
            },
            ConsensusMessage::ProducerCandidate { producer_id, candidate_hash, producer_list_hash, cycle_id } => {
                self.consensus_engine.handle_producer_candidate(producer_id, candidate_hash, producer_list_hash, cycle_id).await?;
            },
            ConsensusMessage::ProducerVote { producer_id, ledger_update_hash, voter_list_hash, cycle_id } => {
                self.consensus_engine.handle_producer_vote(producer_id, ledger_update_hash, voter_list_hash, cycle_id).await?;
            },
            ConsensusMessage::ProducerOutput { producer_id, dfs_address, voter_list_hash, cycle_id } => {
                self.consensus_engine.handle_producer_output(producer_id, dfs_address, voter_list_hash, cycle_id).await?;
            },
        }
        Ok(())
    }

    async fn current_cycle(&self) -> CatalystResult<Option<LedgerCycle>> {
        let state = self.state.read().await;
        Ok(state.current_cycle.clone())
    }

    async fn is_producer(&self) -> CatalystResult<bool> {
        let state = self.state.read().await;
        Ok(state.is_producer)
    }

    async fn validate_block(&self, block: &Block) -> CatalystResult<bool> {
        self.validator.validate_block(block).await
    }

    async fn finalize_block(&self, block: Block) -> CatalystResult<()> {
        info!("Finalizing block at height {}", block.header.height);
        
        // Update state
        let mut state = self.state.write().await;
        state.latest_block = Some(block.clone());
        
        // Remove finalized transactions from pending pool
        let block_tx_hashes: std::collections::HashSet<_> = block.transactions
            .iter()
            .map(|tx| tx.hash.clone())
            .collect();
            
        state.pending_transactions.retain(|tx| !block_tx_hashes.contains(&tx.hash));
        
        info!("Block finalized, remaining pending transactions: {}", state.pending_transactions.len());
        
        Ok(())
    }

    async fn latest_block(&self) -> CatalystResult<Option<Block>> {
        let state = self.state.read().await;
        Ok(state.latest_block.clone())
    }
}