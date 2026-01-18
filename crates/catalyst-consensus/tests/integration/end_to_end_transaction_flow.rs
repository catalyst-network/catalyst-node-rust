// tests/integration/end_to_end_transaction_flow.rs
//! INTEGRATION-001: End-to-End Transaction Flow Integration Test
//! 
//! This test validates that a transaction successfully processes through all 4 phases
//! of the collaborative consensus mechanism and results in proper state changes.
//!
//! Test Flow:
//! 1. Setup test environment with multiple nodes
//! 2. Create and submit a transaction
//! 3. Validate Construction Phase (producers collect and build partial updates)
//! 4. Validate Campaigning Phase (verify majority agreement and create candidates)
//! 5. Validate Voting Phase (supermajority vote on candidate)
//! 6. Validate Synchronization Phase (broadcast and update ledger state)
//! 7. Verify final state changes are correctly stored

use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use anyhow::Result;

// Import all the catalyst crates
use catalyst_core::{
    traits::{StateManager, NetworkMessage},
    types::{CatalystError, CatalystResult, Transaction, TransactionType, TransactionEntry, EntryType},
};
use catalyst_utils::{Hash256, blake2b_hash};
use catalyst_crypto::{
    KeyPair, Curve25519KeyPair, MuSigSignature, 
    PedersenCommitment, PrivateKey, PublicKey
};
use catalyst_config::CatalystConfig;
use catalyst_storage::{StorageManager, RocksDbStorage};
use catalyst_network::{
    NetworkManager, PeerManager, GossipProtocol, NetworkEvent, PeerId
};
use catalyst_consensus::{
    CollaborativeConsensus, ConsensusPhase, LedgerUpdate, ProducerSelection,
    ConstructionPhase, CampaigningPhase, VotingPhase, SynchronizationPhase,
    ConsensusResult, ProducerCandidate, ConsensusMetrics
};
use catalyst_runtime_evm::{CatalystEVM, ExecutionResult, GasMeter};
use catalyst_dfs::{DistributedFileSystem, IpfsManager, FileHash};
use catalyst_service_bus::{ServiceBus, EventStream, BlockchainEvent};

/// Test fixture for end-to-end transaction testing
#[derive(Debug)]
pub struct TransactionFlowTestFixture {
    nodes: Vec<TestNode>,
    config: CatalystConfig,
    test_keypairs: Vec<Curve25519KeyPair>,
}

/// Represents a test node in our network
#[derive(Debug)]
pub struct TestNode {
    pub id: PeerId,
    pub storage: Arc<dyn StateManager>,
    pub network: Arc<NetworkManager>,
    pub consensus: Arc<CollaborativeConsensus>,
    pub evm: Arc<CatalystEVM>,
    pub dfs: Arc<DistributedFileSystem>,
    pub service_bus: Arc<ServiceBus>,
    pub keypair: Curve25519KeyPair,
}

impl TransactionFlowTestFixture {
    /// Create a new test fixture with the specified number of nodes
    pub async fn new(node_count: usize) -> Result<Self> {
        let config = CatalystConfig::test_config();
        let mut nodes = Vec::new();
        let mut test_keypairs = Vec::new();

        for i in 0..node_count {
            let keypair = Curve25519KeyPair::generate();
            let node = TestNode::new(i, &config, keypair.clone()).await?;
            test_keypairs.push(keypair.clone());
            nodes.push(node);
        }

        // Connect all nodes to each other
        for i in 0..nodes.len() {
            for j in (i + 1)..nodes.len() {
                nodes[i].network.connect_peer(nodes[j].id.clone()).await?;
                nodes[j].network.connect_peer(nodes[i].id.clone()).await?;
            }
        }

        Ok(Self {
            nodes,
            config,
            test_keypairs,
        })
    }

    /// Wait for all nodes to be ready
    pub async fn wait_for_network_ready(&self) -> Result<()> {
        let timeout_duration = Duration::from_secs(30);
        
        for node in &self.nodes {
            timeout(timeout_duration, async {
                while !node.network.is_connected().await {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Ok::<(), CatalystError>(())
            }).await??;
        }

        println!("All {} nodes connected and ready", self.nodes.len());
        Ok(())
    }

    /// Create a test transaction
    pub fn create_test_transaction(&self) -> Result<Transaction> {
        let sender_keypair = &self.test_keypairs[0];
        let receiver_pubkey = self.test_keypairs[1].public_key();
        
        let transaction = Transaction {
            tx_type: TransactionType::Transfer,
            entries: vec![
                TransactionEntry {
                    pubkey: receiver_pubkey,
                    amount: 1000, // 1000 KAT tokens
                    entry_type: EntryType::NonConfidential,
                }
            ],
            signature: MuSigSignature::default(), // Will be filled by signing
            locking_time: 0,
            fees: 10, // 10 KAT fee
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs() as u32,
            data: None,
        };

        // Sign the transaction
        let signed_transaction = sender_keypair.sign_transaction(transaction)?;
        Ok(signed_transaction)
    }
}

impl TestNode {
    pub async fn new(
        index: usize, 
        config: &CatalystConfig, 
        keypair: Curve25519KeyPair
    ) -> Result<Self> {
        let node_config = config.node_config(index);
        
        // Initialize storage
        let storage = Arc::new(RocksDbStorage::new_test(
            &format!("test_node_{}", index)
        ).await?);

        // Initialize network
        let network = Arc::new(NetworkManager::new(
            keypair.public_key().into(),
            node_config.network.clone(),
            storage.clone(),
        ).await?);

        // Initialize consensus
        let consensus = Arc::new(CollaborativeConsensus::new(
            keypair.clone(),
            storage.clone(),
            network.clone(),
            node_config.consensus.clone(),
        ).await?);

        // Initialize EVM runtime
        let evm = Arc::new(CatalystEVM::new(
            storage.clone(),
            node_config.evm.clone(),
        ).await?);

        // Initialize DFS
        let dfs = Arc::new(DistributedFileSystem::new(
            network.clone(),
            storage.clone(),
            node_config.dfs.clone(),
        ).await?);

        // Initialize service bus
        let service_bus = Arc::new(ServiceBus::new(
            consensus.clone(),
            network.clone(),
            node_config.service_bus.clone(),
        ).await?);

        Ok(Self {
            id: keypair.public_key().into(),
            storage,
            network,
            consensus,
            evm,
            dfs,
            service_bus,
            keypair,
        })
    }

    /// Get current ledger state for validation
    pub async fn get_ledger_state(&self) -> Result<LedgerState> {
        self.storage.get_current_ledger_state().await
    }

    /// Check if node is acting as producer in current cycle
    pub async fn is_producer(&self) -> Result<bool> {
        self.consensus.is_current_producer().await
    }
}

/// Main integration test function
#[tokio::test]
async fn test_end_to_end_transaction_flow() -> Result<()> {
    // Test configuration
    const NODE_COUNT: usize = 5;
    const CONSENSUS_TIMEOUT: Duration = Duration::from_secs(60);
    
    println!("🚀 Starting INTEGRATION-001: End-to-End Transaction Flow Test");
    
    // Step 1: Setup test environment
    println!("📋 Step 1: Setting up test environment with {} nodes", NODE_COUNT);
    let fixture = TransactionFlowTestFixture::new(NODE_COUNT).await?;
    
    // Step 2: Wait for network to be ready
    println!("🌐 Step 2: Waiting for network connectivity");
    fixture.wait_for_network_ready().await?;
    
    // Step 3: Get initial state snapshots
    println!("📸 Step 3: Capturing initial state snapshots");
    let initial_states: Vec<_> = futures::future::try_join_all(
        fixture.nodes.iter().map(|node| node.get_ledger_state())
    ).await?;
    
    // Step 4: Create and submit transaction
    println!("💰 Step 4: Creating and submitting test transaction");
    let test_transaction = fixture.create_test_transaction()?;
    let tx_hash = blake2b_hash(&test_transaction.serialize()?);
    
    println!("   Transaction hash: {:?}", tx_hash);
    println!("   Amount: 1000 KAT tokens");
    println!("   Fee: 10 KAT");
    
    // Submit transaction to first node
    fixture.nodes[0].consensus.submit_transaction(test_transaction.clone()).await?;
    
    // Step 5: Monitor consensus phases with timeout
    println!("⚡ Step 5: Monitoring 4-phase collaborative consensus");
    
    let consensus_result = timeout(CONSENSUS_TIMEOUT, async {
        monitor_consensus_phases(&fixture, &tx_hash).await
    }).await??;
    
    // Step 6: Validate final state changes
    println!("✅ Step 6: Validating final state changes");
    validate_state_changes(&fixture, &initial_states, &test_transaction).await?;
    
    // Step 7: Validate service bus events
    println!("📡 Step 7: Validating service bus event propagation");
    validate_service_bus_events(&fixture, &tx_hash).await?;
    
    // Step 8: Validate DFS storage
    println!("💾 Step 8: Validating DFS storage of consensus data");
    validate_dfs_storage(&fixture, &consensus_result).await?;
    
    println!("🎉 INTEGRATION-001 completed successfully!");
    println!("   ✓ Transaction processed through all 4 consensus phases");
    println!("   ✓ State changes correctly applied across all nodes");
    println!("   ✓ Network messages properly propagated");
    println!("   ✓ Service bus events delivered");
    println!("   ✓ DFS storage validated");
    
    Ok(())
}

/// Monitor and validate each phase of the consensus process
async fn monitor_consensus_phases(
    fixture: &TransactionFlowTestFixture,
    tx_hash: &Hash256,
) -> Result<ConsensusResult> {
    println!("   🔨 Phase 1: Construction - Producers collecting transactions");
    
    // Wait for construction phase to complete
    let construction_result = wait_for_phase_completion(
        &fixture.nodes,
        ConsensusPhase::Construction,
        Duration::from_secs(15),
    ).await?;
    
    validate_construction_phase(&fixture.nodes, &construction_result).await?;
    println!("   ✓ Construction phase completed successfully");
    
    println!("   📢 Phase 2: Campaigning - Verifying majority agreement");
    
    // Wait for campaigning phase
    let campaigning_result = wait_for_phase_completion(
        &fixture.nodes,
        ConsensusPhase::Campaigning,
        Duration::from_secs(15),
    ).await?;
    
    validate_campaigning_phase(&fixture.nodes, &campaigning_result).await?;
    println!("   ✓ Campaigning phase completed successfully");
    
    println!("   🗳️  Phase 3: Voting - Achieving supermajority consensus");
    
    // Wait for voting phase
    let voting_result = wait_for_phase_completion(
        &fixture.nodes,
        ConsensusPhase::Voting,
        Duration::from_secs(15),
    ).await?;
    
    validate_voting_phase(&fixture.nodes, &voting_result).await?;
    println!("   ✓ Voting phase completed successfully");
    
    println!("   🔄 Phase 4: Synchronization - Broadcasting final state");
    
    // Wait for synchronization phase
    let sync_result = wait_for_phase_completion(
        &fixture.nodes,
        ConsensusPhase::Synchronization,
        Duration::from_secs(15),
    ).await?;
    
    validate_synchronization_phase(&fixture.nodes, &sync_result).await?;
    println!("   ✓ Synchronization phase completed successfully");
    
    Ok(sync_result)
}

/// Wait for a specific consensus phase to complete across all nodes
async fn wait_for_phase_completion(
    nodes: &[TestNode],
    phase: ConsensusPhase,
    timeout_duration: Duration,
) -> Result<ConsensusResult> {
    timeout(timeout_duration, async {
        loop {
            let mut all_completed = true;
            let mut results = Vec::new();
            
            for node in nodes {
                if let Some(result) = node.consensus.get_phase_result(phase).await? {
                    results.push(result);
                } else {
                    all_completed = false;
                    break;
                }
            }
            
            if all_completed && !results.is_empty() {
                // Return the first result (they should all be equivalent)
                return Ok(results[0].clone());
            }
            
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await?
}

/// Validate the construction phase results
async fn validate_construction_phase(
    nodes: &[TestNode],
    result: &ConsensusResult,
) -> Result<()> {
    // Check that producers were selected and created partial updates
    assert!(result.producers.len() > 0, "No producers selected");
    assert!(result.partial_updates.len() > 0, "No partial updates created");
    
    // Verify that all nodes have consistent view of producers
    for node in nodes {
        let node_producers = node.consensus.get_current_producers().await?;
        assert_eq!(
            node_producers.len(), 
            result.producers.len(),
            "Inconsistent producer count across nodes"
        );
    }
    
    println!("     ✓ {} producers selected", result.producers.len());
    println!("     ✓ {} partial updates created", result.partial_updates.len());
    
    Ok(())
}

/// Validate the campaigning phase results
async fn validate_campaigning_phase(
    nodes: &[TestNode],
    result: &ConsensusResult,
) -> Result<()> {
    // Check that majority agreement was reached
    assert!(result.majority_agreement, "Majority agreement not reached");
    assert!(result.candidates.len() > 0, "No candidates created");
    
    // Verify threshold was met
    let threshold = calculate_majority_threshold(nodes.len());
    assert!(
        result.agreement_count >= threshold,
        "Agreement count {} below threshold {}",
        result.agreement_count,
        threshold
    );
    
    println!("     ✓ Majority agreement reached ({}/{})", 
             result.agreement_count, nodes.len());
    println!("     ✓ {} candidates created", result.candidates.len());
    
    Ok(())
}

/// Validate the voting phase results
async fn validate_voting_phase(
    nodes: &[TestNode],
    result: &ConsensusResult,
) -> Result<()> {
    // Check that supermajority was achieved (>67%)
    let supermajority_threshold = (nodes.len() * 2) / 3 + 1;
    assert!(
        result.vote_count >= supermajority_threshold,
        "Supermajority not achieved: {} < {}",
        result.vote_count,
        supermajority_threshold
    );
    
    assert!(result.selected_candidate.is_some(), "No candidate selected");
    
    println!("     ✓ Supermajority achieved ({}/{})", 
             result.vote_count, nodes.len());
    println!("     ✓ Final candidate selected");
    
    Ok(())
}

/// Validate the synchronization phase results
async fn validate_synchronization_phase(
    nodes: &[TestNode],
    result: &ConsensusResult,
) -> Result<()> {
    // Check that all nodes have updated their state
    for node in nodes {
        let current_state = node.get_ledger_state().await?;
        assert!(
            current_state.block_height > 0,
            "Node state not updated after synchronization"
        );
    }
    
    // Verify all nodes have the same final state
    let states: Vec<_> = futures::future::try_join_all(
        nodes.iter().map(|node| node.get_ledger_state())
    ).await?;
    
    let first_state = &states[0];
    for (i, state) in states.iter().enumerate().skip(1) {
        assert_eq!(
            first_state.state_root, state.state_root,
            "Node {} has different state root", i
        );
        assert_eq!(
            first_state.block_height, state.block_height,
            "Node {} has different block height", i
        );
    }
    
    println!("     ✓ All nodes synchronized to block height {}", 
             first_state.block_height);
    println!("     ✓ State root consistency verified");
    
    Ok(())
}

/// Validate that state changes were applied correctly
async fn validate_state_changes(
    fixture: &TransactionFlowTestFixture,
    initial_states: &[LedgerState],
    transaction: &Transaction,
) -> Result<()> {
    let sender_address = fixture.test_keypairs[0].public_key().to_address();
    let receiver_address = fixture.test_keypairs[1].public_key().to_address();
    
    // Check state changes on first node (they should all be identical)
    let final_state = fixture.nodes[0].get_ledger_state().await?;
    let initial_state = &initial_states[0];
    
    // Verify sender balance decreased
    let sender_initial = initial_state.get_balance(&sender_address)?;
    let sender_final = final_state.get_balance(&sender_address)?;
    let expected_sender_final = sender_initial - 1000 - 10; // amount + fee
    
    assert_eq!(
        sender_final, expected_sender_final,
        "Sender balance incorrect: expected {}, got {}",
        expected_sender_final, sender_final
    );
    
    // Verify receiver balance increased
    let receiver_initial = initial_state.get_balance(&receiver_address)?;
    let receiver_final = final_state.get_balance(&receiver_address)?;
    let expected_receiver_final = receiver_initial + 1000;
    
    assert_eq!(
        receiver_final, expected_receiver_final,
        "Receiver balance incorrect: expected {}, got {}",
        expected_receiver_final, receiver_final
    );
    
    println!("   ✓ Sender balance: {} → {} (-1010 KAT)", 
             sender_initial, sender_final);
    println!("   ✓ Receiver balance: {} → {} (+1000 KAT)", 
             receiver_initial, receiver_final);
    
    Ok(())
}

/// Validate service bus event propagation
async fn validate_service_bus_events(
    fixture: &TransactionFlowTestFixture,
    tx_hash: &Hash256,
) -> Result<()> {
    // Check that transaction events were emitted
    for (i, node) in fixture.nodes.iter().enumerate() {
        let events = node.service_bus.get_recent_events(10).await?;
        
        // Look for transaction-related events
        let tx_events: Vec<_> = events.iter()
            .filter(|event| match event {
                BlockchainEvent::TransactionProcessed { hash, .. } => hash == tx_hash,
                BlockchainEvent::BlockFinalized { transactions, .. } => {
                    transactions.contains(tx_hash)
                },
                _ => false,
            })
            .collect();
        
        assert!(
            !tx_events.is_empty(),
            "Node {} did not emit transaction events", i
        );
    }
    
    println!("   ✓ Transaction events properly emitted by all nodes");
    
    Ok(())
}

/// Validate DFS storage of consensus data
async fn validate_dfs_storage(
    fixture: &TransactionFlowTestFixture,
    consensus_result: &ConsensusResult,
) -> Result<()> {
    // Check that consensus data was stored in DFS
    for node in &fixture.nodes {
        // Verify ledger update is stored
        if let Some(update_hash) = &consensus_result.ledger_update_hash {
            let stored_data = node.dfs.retrieve_file(update_hash).await?;
            assert!(!stored_data.is_empty(), "Ledger update not stored in DFS");
        }
        
        // Verify historical data is accessible
        let recent_blocks = node.dfs.get_recent_blocks(5).await?;
        assert!(!recent_blocks.is_empty(), "No recent blocks in DFS");
    }
    
    println!("   ✓ Consensus data properly stored in DFS");
    
    Ok(())
}

/// Calculate majority threshold for agreement
fn calculate_majority_threshold(node_count: usize) -> usize {
    (node_count / 2) + 1
}

// Helper types that should be defined in the actual crates
#[derive(Debug, Clone)]
pub struct LedgerState {
    pub block_height: u64,
    pub state_root: Hash256,
    pub account_balances: std::collections::HashMap<Address, u64>,
}

impl LedgerState {
    pub fn get_balance(&self, address: &Address) -> Result<u64> {
        Ok(self.account_balances.get(address).copied().unwrap_or(0))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address([u8; 20]);

impl From<PublicKey> for Address {
    fn from(pubkey: PublicKey) -> Self {
        let hash = blake2b_hash(&pubkey.as_bytes());
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&hash.as_bytes()[0..20]);
        Address(addr)
    }
}

#[derive(Debug, Clone)]
pub struct TransactionEntry {
    pub pubkey: PublicKey,
    pub amount: u64,
    pub entry_type: EntryType,
}

#[derive(Debug, Clone)]
pub enum EntryType {
    NonConfidential,
    Confidential(PedersenCommitment),
}

#[derive(Debug, Clone)]
pub enum TransactionType {
    Transfer,
    ContractDeploy,
    ContractCall,
}

// Extension trait for PublicKey
trait PublicKeyExt {
    fn to_address(&self) -> Address;
}

impl PublicKeyExt for PublicKey {
    fn to_address(&self) -> Address {
        Address::from(self.clone())
    }
}

// Extension trait for Curve25519KeyPair
trait KeyPairExt {
    fn sign_transaction(&self, transaction: Transaction) -> Result<Transaction>;
}

impl KeyPairExt for Curve25519KeyPair {
    fn sign_transaction(&self, mut transaction: Transaction) -> Result<Transaction> {
        let message = transaction.get_signing_message()?;
        let signature = self.sign(&message)?;
        transaction.signature = signature;
        Ok(transaction)
    }
}

impl Transaction {
    fn get_signing_message(&self) -> Result<Vec<u8>> {
        // Create message for signing (all fields except signature)
        let mut message = Vec::new();
        message.extend_from_slice(&(self.tx_type as u8).to_le_bytes());
        message.extend_from_slice(&self.fees.to_le_bytes());
        message.extend_from_slice(&self.timestamp.to_le_bytes());
        
        for entry in &self.entries {
            message.extend_from_slice(&entry.pubkey.as_bytes());
            message.extend_from_slice(&entry.amount.to_le_bytes());
        }
        
        if let Some(data) = &self.data {
            message.extend_from_slice(data);
        }
        
        Ok(message)
    }
    
    fn serialize(&self) -> Result<Vec<u8>> {
        // Serialize transaction for hashing
        let mut serialized = Vec::new();
        serialized.extend_from_slice(&bincode::serialize(self)?);
        Ok(serialized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fixture_creation() -> Result<()> {
        let fixture = TransactionFlowTestFixture::new(3).await?;
        assert_eq!(fixture.nodes.len(), 3);
        assert_eq!(fixture.test_keypairs.len(), 3);
        Ok(())
    }

    #[tokio::test] 
    async fn test_transaction_creation() -> Result<()> {
        let fixture = TransactionFlowTestFixture::new(2).await?;
        let transaction = fixture.create_test_transaction()?;
        assert_eq!(transaction.entries[0].amount, 1000);
        assert_eq!(transaction.fees, 10);
        Ok(())
    }

    #[tokio::test]
    async fn test_network_connectivity() -> Result<()> {
        let fixture = TransactionFlowTestFixture::new(3).await?;
        fixture.wait_for_network_ready().await?;
        
        // All nodes should be connected
        for node in &fixture.nodes {
            assert!(node.network.is_connected().await);
        }
        Ok(())
    }
}