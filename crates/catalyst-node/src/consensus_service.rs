// File: crates/catalyst-node/src/consensus_service.rs
// Real consensus service implementation with proper block production

use crate::{CatalystService, Event, EventBus, NodeError, ServiceHealth, ServiceType};
use anyhow::Result;
use async_trait::async_trait;
use blake2::{Blake2b512, Digest};
use catalyst_crypto::KeyPair;
use catalyst_storage::StorageManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, Duration, Instant};
use tracing::{debug, error, info};

/// Consensus phases in the Catalyst protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusPhase {
    Propose,  // Phase 1: Block proposal
    Vote,     // Phase 2: Voting on proposals
    Commit,   // Phase 3: Commitment to chosen block
    Finalize, // Phase 4: Block finalization
}

/// Consensus message types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusMessage {
    Proposal {
        block: Block,
        proposer: [u8; 32],
        round: u64,
        signature: Vec<u8>,
    },
    Vote {
        block_hash: [u8; 32],
        voter: [u8; 32],
        round: u64,
        vote_type: VoteType,
        signature: Vec<u8>,
    },
    Commit {
        block_hash: [u8; 32],
        committer: [u8; 32],
        round: u64,
        signature: Vec<u8>,
    },
}

/// Vote types in consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VoteType {
    Prevote,
    Precommit,
}

/// Block structure for consensus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub hash: [u8; 32],
}

/// Block header structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub height: u64,
    pub previous_hash: [u8; 32],
    pub merkle_root: [u8; 32],
    pub timestamp: u64,
    pub proposer: [u8; 32],
    pub round: u64,
    pub consensus_data: ConsensusData,
}

/// Transaction structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: [u8; 32],
    pub from: Option<[u8; 20]>,
    pub to: Option<[u8; 20]>,
    pub amount: u64,
    pub fee: u64,
    pub nonce: u64,
    pub data: Vec<u8>,
    pub signature: Vec<u8>,
    pub timestamp: u64,
}

/// Consensus-specific data in block header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusData {
    pub validator_set_hash: [u8; 32],
    pub evidence_hash: [u8; 32],
    pub next_validator_set_hash: [u8; 32],
}

/// Validator information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Validator {
    pub address: [u8; 32],
    pub public_key: Vec<u8>,
    pub voting_power: u64,
    pub proposer_priority: i64,
}

/// Consensus state information
#[derive(Debug, Clone)]
pub struct ConsensusState {
    pub height: u64,
    pub round: u64,
    pub phase: ConsensusPhase,
    pub locked_block: Option<[u8; 32]>,
    pub valid_block: Option<[u8; 32]>,
    pub proposer: Option<[u8; 32]>,
    pub validators: Vec<Validator>,
}

/// Vote tracker for consensus rounds
#[derive(Debug, Default)]
struct VoteTracker {
    prevotes: HashMap<[u8; 32], Vec<[u8; 32]>>, // block_hash -> list of voters
    precommits: HashMap<[u8; 32], Vec<[u8; 32]>>, // block_hash -> list of voters
    voting_power: HashMap<[u8; 32], u64>,       // voter -> voting power
}

impl VoteTracker {
    fn add_vote(
        &mut self,
        block_hash: [u8; 32],
        voter: [u8; 32],
        vote_type: VoteType,
        voting_power: u64,
    ) {
        self.voting_power.insert(voter, voting_power);

        let vote_map = match vote_type {
            VoteType::Prevote => &mut self.prevotes,
            VoteType::Precommit => &mut self.precommits,
        };

        vote_map
            .entry(block_hash)
            .or_insert_with(Vec::new)
            .push(voter);
    }

    fn get_voting_power(&self, block_hash: &[u8; 32], vote_type: VoteType) -> u64 {
        let vote_map = match vote_type {
            VoteType::Prevote => &self.prevotes,
            VoteType::Precommit => &self.precommits,
        };

        vote_map
            .get(block_hash)
            .map(|voters| {
                voters
                    .iter()
                    .map(|voter| self.voting_power.get(voter).unwrap_or(&0))
                    .sum()
            })
            .unwrap_or(0)
    }

    fn has_majority(&self, block_hash: &[u8; 32], vote_type: VoteType, total_power: u64) -> bool {
        let power = self.get_voting_power(block_hash, vote_type);
        power * 3 > total_power * 2 // More than 2/3
    }
}

/// Real consensus service implementation
pub struct ConsensusService {
    /// Node identity
    node_id: [u8; 32],
    /// Cryptographic keypair
    keypair: KeyPair,
    /// Storage manager
    storage: Arc<StorageManager>,
    /// Event bus for communication
    event_bus: Option<Arc<EventBus>>,
    /// Current consensus state
    state: Arc<RwLock<ConsensusState>>,
    /// Vote tracker for current round
    vote_tracker: Arc<Mutex<VoteTracker>>,
    /// Whether service is running
    is_running: Arc<RwLock<bool>>,
    /// Pending transactions pool
    transaction_pool: Arc<Mutex<Vec<Transaction>>>,
    /// Configuration
    config: ConsensusConfig,
    /// Last block time for timing calculations
    last_block_time: Arc<Mutex<Option<Instant>>>,
}

/// Consensus configuration
#[derive(Debug, Clone)]
pub struct ConsensusConfig {
    pub phase_duration: Duration,
    pub block_time: Duration,
    pub max_transactions_per_block: usize,
    pub max_block_size: usize,
    pub validator_set_size: usize,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            phase_duration: Duration::from_secs(10), // 10 seconds per phase
            block_time: Duration::from_secs(40),     // 40 seconds per block (4 phases)
            max_transactions_per_block: 1000,
            max_block_size: 1024 * 1024, // 1MB
            validator_set_size: 100,
        }
    }
}

impl ConsensusService {
    /// Create a new consensus service
    pub fn new(
        node_id: [u8; 32],
        keypair: KeyPair,
        storage: Arc<StorageManager>,
        config: ConsensusConfig,
    ) -> Self {
        // Initialize with a single validator (this node) for testing
        let initial_validator = Validator {
            address: node_id,
            public_key: keypair.public_key().to_bytes().to_vec(),
            voting_power: 100,
            proposer_priority: 0,
        };

        let initial_state = ConsensusState {
            height: 1,
            round: 0,
            phase: ConsensusPhase::Propose,
            locked_block: None,
            valid_block: None,
            proposer: Some(node_id),
            validators: vec![initial_validator],
        };

        Self {
            node_id,
            keypair,
            storage,
            event_bus: None,
            state: Arc::new(RwLock::new(initial_state)),
            vote_tracker: Arc::new(Mutex::new(VoteTracker::default())),
            is_running: Arc::new(RwLock::new(false)),
            transaction_pool: Arc::new(Mutex::new(Vec::new())),
            config,
            last_block_time: Arc::new(Mutex::new(None)),
        }
    }

    /// Set event bus for communication
    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    /// Start the consensus service
    pub async fn start(&self) -> Result<(), NodeError> {
        info!("🚀 Starting real Catalyst consensus service");

        *self.is_running.write().await = true;
        *self.last_block_time.lock().await = Some(Instant::now());

        // Initialize genesis block if needed
        self.initialize_genesis().await?;

        // Start consensus rounds
        self.start_consensus_loop().await;

        info!("✅ Real consensus service started successfully");
        Ok(())
    }

    /// Stop the consensus service
    pub async fn stop(&self) -> Result<(), NodeError> {
        info!("🛑 Stopping consensus service");
        *self.is_running.write().await = false;
        Ok(())
    }

    /// Initialize genesis block (public for testing)
    pub async fn initialize_genesis(&self) -> Result<(), NodeError> {
        // Check if genesis already exists
        if self.get_block_by_height(0).await?.is_some() {
            info!("Genesis block already exists");
            return Ok(());
        }

        info!("Creating genesis block...");

        let genesis_header = BlockHeader {
            height: 0,
            previous_hash: [0u8; 32],
            merkle_root: [0u8; 32],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            proposer: self.node_id,
            round: 0,
            consensus_data: ConsensusData {
                validator_set_hash: self
                    .calculate_validator_set_hash(&self.state.read().await.validators),
                evidence_hash: [0u8; 32],
                next_validator_set_hash: [0u8; 32],
            },
        };

        let genesis_block = Block {
            header: genesis_header,
            transactions: Vec::new(),
            hash: [0u8; 32],
        };

        self.store_block(&genesis_block).await?;
        info!("✅ Genesis block created and stored");

        Ok(())
    }

    /// Start the main consensus loop
    async fn start_consensus_loop(&self) {
        let consensus = Arc::new(self.clone());

        tokio::spawn(async move {
            let mut phase_timer = interval(consensus.config.phase_duration);

            info!(
                "🔄 Starting consensus loop with {}s phases",
                consensus.config.phase_duration.as_secs()
            );

            loop {
                phase_timer.tick().await;

                if !*consensus.is_running.read().await {
                    break;
                }

                if let Err(e) = consensus.advance_consensus().await {
                    error!("❌ Consensus error: {}", e);
                }
            }

            info!("🔄 Consensus loop stopped");
        });
    }

    /// Advance consensus to the next phase
    async fn advance_consensus(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut state = self.state.write().await;

        debug!(
            "📍 Current phase: {:?}, Height: {}, Round: {}",
            state.phase, state.height, state.round
        );

        match state.phase {
            ConsensusPhase::Propose => {
                if state.proposer == Some(self.node_id) {
                    self.handle_propose_phase(&mut state).await?;
                }
                state.phase = ConsensusPhase::Vote;
            }
            ConsensusPhase::Vote => {
                self.handle_vote_phase(&mut state).await?;
                state.phase = ConsensusPhase::Commit;
            }
            ConsensusPhase::Commit => {
                let should_finalize = self.handle_commit_phase(&mut state).await?;
                if should_finalize {
                    state.phase = ConsensusPhase::Finalize;
                } else {
                    // Move to next round
                    state.round += 1;
                    state.phase = ConsensusPhase::Propose;
                    self.update_proposer(&mut state);
                }
            }
            ConsensusPhase::Finalize => {
                self.handle_finalize_phase(&mut state).await?;
                // Move to next height
                state.height += 1;
                state.round = 0;
                state.phase = ConsensusPhase::Propose;
                state.locked_block = None;
                state.valid_block = None;
                self.update_proposer(&mut state);
                // Reset vote tracker for new height
                *self.vote_tracker.lock().await = VoteTracker::default();
            }
        }

        Ok(())
    }

    /// Handle propose phase
    async fn handle_propose_phase(
        &self,
        state: &mut ConsensusState,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "🔨 Proposing block at height {}, round {}",
            state.height, state.round
        );

        // Get transactions from pool
        let transactions = self.select_transactions().await?;

        // Create block
        let block = self
            .create_block(state.height, state.round, transactions)
            .await?;

        // Store our proposal
        state.valid_block = Some(block.hash);

        // In a real implementation, we would broadcast the proposal
        // For now, we'll just log it
        info!(
            "📋 Proposed block {} with {} transactions",
            state.height,
            block.transactions.len()
        );
        info!("   Block hash: {:?}", hex::encode(&block.hash[..8]));
        info!(
            "   Proposer: {:?}",
            hex::encode(&state.proposer.unwrap_or([0u8; 32])[..8])
        );

        // Store the block proposal
        self.store_block_proposal(&block).await?;

        Ok(())
    }

    /// Handle vote phase
    async fn handle_vote_phase(
        &self,
        state: &mut ConsensusState,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "🗳️ Voting phase at height {}, round {}",
            state.height, state.round
        );

        // Vote for our valid block if we have one
        if let Some(block_hash) = state.valid_block {
            self.cast_vote(block_hash, VoteType::Prevote, state.round)
                .await?;
            info!(
                "✅ Cast prevote for block {:?}",
                hex::encode(&block_hash[..8])
            );
        }

        Ok(())
    }

    /// Handle commit phase
    async fn handle_commit_phase(
        &self,
        state: &mut ConsensusState,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "📝 Commit phase at height {}, round {}",
            state.height, state.round
        );

        let vote_tracker = self.vote_tracker.lock().await;
        let total_voting_power = self.calculate_total_voting_power(&state.validators);

        // Check if any block has majority prevotes
        if let Some(block_hash) = state.valid_block {
            if vote_tracker.has_majority(&block_hash, VoteType::Prevote, total_voting_power) {
                drop(vote_tracker);

                // Cast precommit vote
                self.cast_vote(block_hash, VoteType::Precommit, state.round)
                    .await?;
                info!(
                    "✅ Cast precommit for block {:?}",
                    hex::encode(&block_hash[..8])
                );

                // Lock on this block
                state.locked_block = Some(block_hash);

                // Check if we can finalize
                let vote_tracker = self.vote_tracker.lock().await;
                if vote_tracker.has_majority(&block_hash, VoteType::Precommit, total_voting_power) {
                    return Ok(true); // Move to finalize
                }
            }
        }

        Ok(false) // Continue to next round
    }

    /// Handle finalize phase
    async fn handle_finalize_phase(
        &self,
        state: &mut ConsensusState,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("🎯 Finalizing block at height {}", state.height);

        if let Some(block_hash) = state.locked_block {
            // Retrieve and finalize the block
            if let Some(block) = self.get_block_proposal_by_hash(&block_hash).await? {
                self.finalize_block(&block).await?;

                info!("🎉 Block {} finalized successfully!", state.height);
                info!("   Hash: {:?}", hex::encode(&block.hash[..8]));
                info!("   Transactions: {}", block.transactions.len());

                // Publish block event
                if let Some(event_bus) = &self.event_bus {
                    event_bus
                        .publish(Event::BlockReceived {
                            block_hash: hex::encode(&block.hash),
                            block_number: state.height,
                        })
                        .await;
                }

                // Update last block time
                *self.last_block_time.lock().await = Some(Instant::now());
            }
        }

        Ok(())
    }

    /// Create a new block
    async fn create_block(
        &self,
        height: u64,
        round: u64,
        transactions: Vec<Transaction>,
    ) -> Result<Block, Box<dyn std::error::Error + Send + Sync>> {
        let previous_hash = if height > 0 {
            self.get_latest_block_hash().await?
        } else {
            [0u8; 32]
        };

        let merkle_root = self.calculate_merkle_root(&transactions);
        let state = self.state.read().await;

        let header = BlockHeader {
            height,
            previous_hash,
            merkle_root,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            proposer: self.node_id,
            round,
            consensus_data: ConsensusData {
                validator_set_hash: self.calculate_validator_set_hash(&state.validators),
                evidence_hash: [0u8; 32],
                next_validator_set_hash: [0u8; 32],
            },
        };

        let hash = self.calculate_block_hash(&header, &transactions);

        Ok(Block {
            header,
            transactions,
            hash,
        })
    }

    /// Select transactions from the pool
    async fn select_transactions(
        &self,
    ) -> Result<Vec<Transaction>, Box<dyn std::error::Error + Send + Sync>> {
        let mut pool = self.transaction_pool.lock().await;

        // For now, take all transactions up to the limit
        let limit = self.config.max_transactions_per_block.min(pool.len());
        let selected = pool.drain(..limit).collect();

        // In a real implementation, we would:
        // 1. Validate transactions
        // 2. Sort by fee (highest first)
        // 3. Check for conflicts
        // 4. Ensure total size doesn't exceed block limit

        Ok(selected)
    }

    /// Add transaction to pool
    pub async fn add_transaction(&self, transaction: Transaction) -> Result<(), NodeError> {
        let mut pool = self.transaction_pool.lock().await;
        pool.push(transaction);
        Ok(())
    }

    /// Create a test transaction
    pub async fn create_test_transaction(&self, height: u64, index: u32) -> Transaction {
        let mut hasher = Blake2b512::new();
        hasher.update(b"test_transaction");
        hasher.update(&height.to_be_bytes());
        hasher.update(&index.to_be_bytes());
        hasher.update(
            &std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
                .to_be_bytes(),
        );

        let hash_result = hasher.finalize();
        let mut tx_id = [0u8; 32];
        tx_id.copy_from_slice(&hash_result[..32]); // Take first 32 bytes

        Transaction {
            id: tx_id,
            from: Some([1u8; 20]),
            to: Some([2u8; 20]),
            amount: 100,
            fee: 1,
            nonce: index as u64,
            data: format!("Test transaction {} in block {}", index, height).into_bytes(),
            signature: vec![0u8; 64], // Mock signature
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }

    /// Cast a vote
    async fn cast_vote(
        &self,
        block_hash: [u8; 32],
        vote_type: VoteType,
        _round: u64,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut vote_tracker = self.vote_tracker.lock().await;
        vote_tracker.add_vote(block_hash, self.node_id, vote_type, 100); // 100 voting power
        Ok(())
    }

    /// Update proposer for next round/height
    fn update_proposer(&self, state: &mut ConsensusState) {
        // Simple round-robin for now
        if !state.validators.is_empty() {
            let index = (state.round as usize) % state.validators.len();
            state.proposer = Some(state.validators[index].address);
        }
    }

    /// Calculate total voting power
    fn calculate_total_voting_power(&self, validators: &[Validator]) -> u64 {
        validators.iter().map(|v| v.voting_power).sum()
    }

    /// Calculate validator set hash
    fn calculate_validator_set_hash(&self, validators: &[Validator]) -> [u8; 32] {
        let mut hasher = Blake2b512::new();
        for validator in validators {
            hasher.update(&validator.address);
            hasher.update(&validator.voting_power.to_be_bytes());
        }
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result[..32]); // Take first 32 bytes
        hash
    }

    /// Calculate merkle root of transactions
    fn calculate_merkle_root(&self, transactions: &[Transaction]) -> [u8; 32] {
        if transactions.is_empty() {
            return [0u8; 32];
        }

        let mut hasher = Blake2b512::new();
        for tx in transactions {
            hasher.update(&tx.id);
        }

        let result = hasher.finalize();
        let mut root = [0u8; 32];
        root.copy_from_slice(&result[..32]); // Take first 32 bytes
        root
    }

    /// Calculate block hash
    fn calculate_block_hash(&self, header: &BlockHeader, transactions: &[Transaction]) -> [u8; 32] {
        let mut hasher = Blake2b512::new();
        hasher.update(&header.height.to_be_bytes());
        hasher.update(&header.previous_hash);
        hasher.update(&header.merkle_root);
        hasher.update(&header.timestamp.to_be_bytes());
        hasher.update(&header.proposer);
        hasher.update(&header.round.to_be_bytes());

        // Include transaction data
        for tx in transactions {
            hasher.update(&tx.id);
        }

        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result[..32]); // Take first 32 bytes
        hash
    }

    /// Store block proposal (temporary storage during consensus)
    async fn store_block_proposal(
        &self,
        block: &Block,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("proposal_{}", hex::encode(&block.hash));
        let data = serde_json::to_vec(block)?;

        self.storage
            .engine()
            .put("proposals", key.as_bytes(), &data)?;

        Ok(())
    }

    /// Get block proposal by hash
    async fn get_block_proposal_by_hash(
        &self,
        hash: &[u8; 32],
    ) -> Result<Option<Block>, Box<dyn std::error::Error + Send + Sync>> {
        let key = format!("proposal_{}", hex::encode(hash));

        match self.storage.engine().get("proposals", key.as_bytes())? {
            Some(data) => {
                let block: Block = serde_json::from_slice(&data)?;
                Ok(Some(block))
            }
            None => Ok(None),
        }
    }

    /// Store finalized block
    async fn store_block(&self, block: &Block) -> Result<(), NodeError> {
        let block_key = format!("block_{}", block.header.height);
        let block_data = serde_json::to_vec(block)
            .map_err(|e| NodeError::Configuration(format!("Serialization error: {}", e)))?;

        // Use 'metadata' column family instead of 'blocks' since it exists
        self.storage
            .engine()
            .put("metadata", block_key.as_bytes(), &block_data)
            .map_err(|e| NodeError::Storage(e.to_string()))?;

        // Store by hash for quick lookups
        let hash_key = format!("block_hash_{}", hex::encode(&block.hash));
        self.storage
            .engine()
            .put(
                "metadata",
                hash_key.as_bytes(),
                &block.header.height.to_le_bytes(),
            )
            .map_err(|e| NodeError::Storage(e.to_string()))?;

        // Update latest block metadata
        self.storage
            .engine()
            .put(
                "metadata",
                b"latest_block_height",
                &block.header.height.to_le_bytes(),
            )
            .map_err(|e| NodeError::Storage(e.to_string()))?;

        self.storage
            .engine()
            .put("metadata", b"latest_block_hash", &block.hash)
            .map_err(|e| NodeError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Finalize block (store permanently and clean up proposals)
    async fn finalize_block(
        &self,
        block: &Block,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Store the finalized block
        self.store_block(block).await?;

        // Remove from proposals
        let key = format!("proposal_{}", hex::encode(&block.hash));
        let _ = self.storage.engine().delete("proposals", key.as_bytes());

        // Process transactions (update state, accounts, etc.)
        // This would be implemented based on your transaction processing logic

        Ok(())
    }

    /// Get block by height (public method)
    pub async fn get_block_by_height(&self, height: u64) -> Result<Option<Block>, NodeError> {
        let block_key = format!("block_{}", height);

        // Use 'metadata' column family instead of 'blocks'
        match self
            .storage
            .engine()
            .get("metadata", block_key.as_bytes())
            .map_err(|e| NodeError::Storage(e.to_string()))?
        {
            Some(data) => {
                let block: Block = serde_json::from_slice(&data).map_err(|e| {
                    NodeError::Configuration(format!("Deserialization error: {}", e))
                })?;
                Ok(Some(block))
            }
            None => Ok(None),
        }
    }

    /// Get latest block hash
    async fn get_latest_block_hash(
        &self,
    ) -> Result<[u8; 32], Box<dyn std::error::Error + Send + Sync>> {
        match self
            .storage
            .engine()
            .get("metadata", b"latest_block_hash")?
        {
            Some(hash_bytes) => {
                if hash_bytes.len() != 32 {
                    return Err("Invalid block hash length".into());
                }
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&hash_bytes);
                Ok(hash)
            }
            None => Ok([0u8; 32]), // Genesis
        }
    }

    /// Get current consensus state
    pub async fn get_consensus_state(&self) -> ConsensusState {
        self.state.read().await.clone()
    }

    /// Get current height
    pub async fn get_current_height(&self) -> u64 {
        self.state.read().await.height
    }

    /// Check if running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Start auto transaction generation for testing
    pub async fn start_auto_transaction_generation(&self) {
        let consensus = Arc::new(self.clone());

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5)); // Generate transaction every 5 seconds

            info!("🔄 Starting automatic transaction generation");

            loop {
                interval.tick().await;

                if !*consensus.is_running.read().await {
                    break;
                }

                let height = consensus.get_current_height().await;
                let tx = consensus
                    .create_test_transaction(height, rand::random())
                    .await;

                if let Err(e) = consensus.add_transaction(tx).await {
                    error!("Failed to add test transaction: {}", e);
                } else {
                    debug!("Added test transaction to pool");
                }
            }
        });
    }
}

impl Clone for ConsensusService {
    fn clone(&self) -> Self {
        Self {
            node_id: self.node_id,
            keypair: self.keypair.clone(),
            storage: Arc::clone(&self.storage),
            event_bus: self.event_bus.clone(),
            state: Arc::clone(&self.state),
            vote_tracker: Arc::clone(&self.vote_tracker),
            is_running: Arc::clone(&self.is_running),
            transaction_pool: Arc::clone(&self.transaction_pool),
            config: self.config.clone(),
            last_block_time: Arc::clone(&self.last_block_time),
        }
    }
}

#[async_trait]
impl CatalystService for ConsensusService {
    async fn start(&mut self) -> Result<(), NodeError> {
        ConsensusService::start(self).await
    }

    async fn stop(&mut self) -> Result<(), NodeError> {
        ConsensusService::stop(self).await
    }

    async fn health_check(&self) -> ServiceHealth {
        if self.is_running().await {
            ServiceHealth::Healthy
        } else {
            ServiceHealth::Unhealthy("Consensus service not running".to_string())
        }
    }

    fn service_type(&self) -> ServiceType {
        ServiceType::Consensus
    }

    fn name(&self) -> &str {
        "Real Catalyst Consensus Service"
    }
}

/// Consensus status for external queries
#[derive(Debug, Clone)]
pub struct ConsensusStatus {
    pub is_running: bool,
    pub current_height: u64,
    pub current_round: u64,
    pub current_phase: ConsensusPhase,
    pub proposer: Option<[u8; 32]>,
    pub validator_count: usize,
    pub transaction_pool_size: usize,
}

impl ConsensusService {
    /// Get detailed consensus status
    pub async fn get_status(&self) -> ConsensusStatus {
        let state = self.state.read().await;
        let pool_size = self.transaction_pool.lock().await.len();

        ConsensusStatus {
            is_running: *self.is_running.read().await,
            current_height: state.height,
            current_round: state.round,
            current_phase: state.phase,
            proposer: state.proposer,
            validator_count: state.validators.len(),
            transaction_pool_size: pool_size,
        }
    }
}
