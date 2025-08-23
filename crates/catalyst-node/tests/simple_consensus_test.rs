// File: crates/catalyst-node/tests/simple_consensus_test.rs
// Simple consensus tests without private method calls

use catalyst_crypto::KeyPair;
use catalyst_node::consensus_service::{ConsensusConfig, ConsensusService};
use catalyst_storage::{StorageConfig, StorageManager};
use rand;
use std::sync::Arc;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_basic_consensus_creation() {
    // Test creating consensus service
    let storage_config = StorageConfig::default();
    let storage = Arc::new(
        StorageManager::new(storage_config)
            .await
            .expect("Failed to create storage"),
    );

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);

    // Test initial state
    assert!(!consensus.is_running().await);
    assert_eq!(consensus.get_current_height().await, 1); // Starts at height 1

    // Test consensus status
    let status = consensus.get_status().await;
    assert!(!status.is_running);
    assert_eq!(status.current_height, 1);
    assert_eq!(status.validator_count, 1);
    assert_eq!(status.transaction_pool_size, 0);

    println!("✅ Basic consensus creation test passed!");
}

#[tokio::test]
async fn test_transaction_management() {
    let storage_config = StorageConfig::default();
    let storage = Arc::new(
        StorageManager::new(storage_config)
            .await
            .expect("Failed to create storage"),
    );

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);

    // Create and add test transaction
    let tx = consensus.create_test_transaction(1, 0).await;
    consensus
        .add_transaction(tx)
        .await
        .expect("Failed to add transaction");

    // Check transaction pool
    let status = consensus.get_status().await;
    assert_eq!(status.transaction_pool_size, 1);

    // Add another transaction
    let tx2 = consensus.create_test_transaction(1, 1).await;
    consensus
        .add_transaction(tx2)
        .await
        .expect("Failed to add transaction");

    let status2 = consensus.get_status().await;
    assert_eq!(status2.transaction_pool_size, 2);

    println!("✅ Transaction management test passed!");
}

#[tokio::test]
async fn test_consensus_state_management() {
    let storage_config = StorageConfig::default();
    let storage = Arc::new(
        StorageManager::new(storage_config)
            .await
            .expect("Failed to create storage"),
    );

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);

    let state = consensus.get_consensus_state().await;

    // Check initial consensus state
    assert_eq!(state.height, 1);
    assert_eq!(state.round, 0);
    assert_eq!(state.validators.len(), 1);
    assert_eq!(state.validators[0].address, node_id);
    assert_eq!(state.validators[0].voting_power, 100);
    assert!(state.proposer.is_some());
    assert_eq!(state.proposer.unwrap(), node_id);

    println!("✅ Consensus state management test passed!");
}

#[tokio::test]
async fn test_consensus_lifecycle() {
    let storage_config = StorageConfig::default();
    let storage = Arc::new(
        StorageManager::new(storage_config)
            .await
            .expect("Failed to create storage"),
    );

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);

    // Test startup
    assert!(!consensus.is_running().await);

    consensus.start().await.expect("Failed to start consensus");
    assert!(consensus.is_running().await);

    // Give it a moment to initialize
    sleep(Duration::from_millis(100)).await;

    let status = consensus.get_status().await;
    assert!(status.is_running);

    // Test shutdown
    consensus.stop().await.expect("Failed to stop consensus");
    assert!(!consensus.is_running().await);

    println!("✅ Consensus lifecycle test passed!");
}

#[tokio::test]
async fn test_transaction_uniqueness() {
    let storage_config = StorageConfig::default();
    let storage = Arc::new(
        StorageManager::new(storage_config)
            .await
            .expect("Failed to create storage"),
    );

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);

    // Create multiple transactions and ensure they're unique
    let tx1 = consensus.create_test_transaction(1, 0).await;
    let tx2 = consensus.create_test_transaction(1, 1).await;
    let tx3 = consensus.create_test_transaction(2, 0).await;

    // Transaction IDs should be different
    assert_ne!(tx1.id, tx2.id);
    assert_ne!(tx1.id, tx3.id);
    assert_ne!(tx2.id, tx3.id);

    // Other fields should be as expected
    assert_eq!(tx1.nonce, 0);
    assert_eq!(tx2.nonce, 1);
    assert_eq!(tx3.nonce, 0);

    assert_eq!(tx1.amount, 100);
    assert_eq!(tx2.amount, 100);
    assert_eq!(tx3.amount, 100);

    println!("✅ Transaction uniqueness test passed!");
}
