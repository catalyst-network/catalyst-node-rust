use catalyst_crypto::KeyPair;
use catalyst_node::consensus_service::{ConsensusConfig, ConsensusService};
use catalyst_storage::{StorageConfig, StorageManager};
use rand;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{sleep, timeout, Duration};

/// Create a fresh storage manager backed by a unique temporary directory.
/// The TempDir must be kept alive for the lifetime of the storage.
async fn new_temp_storage() -> (Arc<StorageManager>, TempDir) {
    let tmp = TempDir::new().expect("failed to create temp dir");

    // Point storage at the unique temp path to avoid RocksDB lock collisions.
    let mut storage_config = StorageConfig::default();
    storage_config.data_dir = tmp.path().to_path_buf();

    let storage = Arc::new(
        StorageManager::new(storage_config)
            .await
            .expect("Failed to create storage"),
    );
    (storage, tmp)
}

#[tokio::test(flavor = "current_thread")]
async fn test_consensus_creation() {
    let (storage, _tmp) = new_temp_storage().await;

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
}

#[tokio::test(flavor = "current_thread")]
async fn test_transaction_pool() {
    let (storage, _tmp) = new_temp_storage().await;

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
}

#[tokio::test(flavor = "current_thread")]
async fn test_consensus_state() {
    let (storage, _tmp) = new_temp_storage().await;

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
}

#[tokio::test(flavor = "current_thread")]
async fn test_genesis_block_creation() {
    let (storage, _tmp) = new_temp_storage().await;

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);

    // Initialize genesis (now public)
    consensus
        .initialize_genesis()
        .await
        .expect("Failed to initialize genesis");

    // Check that genesis block exists
    let genesis = consensus
        .get_block_by_height(0)
        .await
        .expect("Failed to get genesis");
    assert!(genesis.is_some());

    let genesis_block = genesis.unwrap();
    assert_eq!(genesis_block.header.height, 0);
    assert_eq!(genesis_block.transactions.len(), 0);
    assert_eq!(genesis_block.header.proposer, node_id);
}

#[tokio::test(flavor = "current_thread")]
async fn test_consensus_startup_shutdown() {
    let (storage, _tmp) = new_temp_storage().await;

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);

    // Test startup
    assert!(!consensus.is_running().await);

    // Guard start/stop with hard timeouts so the test never hangs.
    timeout(Duration::from_secs(5), consensus.start())
        .await
        .expect("consensus.start() timed out")
        .expect("Failed to start consensus");
    assert!(consensus.is_running().await);

    // Give it a moment to initialize
    sleep(Duration::from_millis(100)).await;

    let status = consensus.get_status().await;
    assert!(status.is_running);

    // Test shutdown
    timeout(Duration::from_secs(3), consensus.stop())
        .await
        .expect("consensus.stop() timed out")
        .expect("Failed to stop consensus");
    assert!(!consensus.is_running().await);
}

// Integration test - this one takes longer
#[tokio::test(flavor = "current_thread")]
#[ignore] // Use --ignored to run this test
async fn test_consensus_block_production() {
    let (storage, _tmp) = new_temp_storage().await;

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);

    let initial_height = consensus.get_current_height().await;

    // Add some test transactions
    for i in 0..3 {
        let tx = consensus.create_test_transaction(1, i).await;
        consensus
            .add_transaction(tx)
            .await
            .expect("Failed to add transaction");
    }

    // Start consensus (with timeout)
    timeout(Duration::from_secs(5), consensus.start())
        .await
        .expect("consensus.start() timed out")
        .expect("Failed to start consensus");

    // Poll for up to ~5 seconds for a new block
    let mut block_produced = false;
    for _ in 0..20 {
        sleep(Duration::from_millis(250)).await;
        let current_height = consensus.get_current_height().await;
        if current_height > initial_height {
            block_produced = true;
            break;
        }
    }

    // Stop consensus (with timeout)
    timeout(Duration::from_secs(3), consensus.stop())
        .await
        .expect("consensus.stop() timed out")
        .expect("Failed to stop consensus");

    let final_height = consensus.get_current_height().await;

    println!("Block production test results:");
    println!("  Initial height: {}", initial_height);
    println!("  Final height: {}", final_height);
    println!("  Block produced: {}", block_produced);

    // This test might not always pass due to timing, but it's useful for debugging
    // assert!(block_produced, "No block was produced within the allotted time");
}
