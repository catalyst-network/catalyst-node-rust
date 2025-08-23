use catalyst_crypto::KeyPair;
use catalyst_node::consensus_service::{ConsensusConfig, ConsensusService};
use catalyst_storage::{StorageConfig, StorageManager};
use rand;
use std::sync::Arc;
use tokio::time::{sleep, timeout, Duration};

/// Create an isolated RocksDB directory per test to avoid CF-creation races.
async fn new_temp_storage() -> (Arc<StorageManager>, tempfile::TempDir) {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let mut cfg = StorageConfig::default();
    cfg.data_dir = tmp.path().to_path_buf();
    let storage = Arc::new(
        StorageManager::new(cfg)
            .await
            .expect("Failed to create storage"),
    );
    (storage, tmp)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_basic_consensus_creation() {
    let (storage, _tmp) = new_temp_storage().await;

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);

    // Explicitly create genesis to avoid first-run RocksDB CF races.
    consensus
        .initialize_genesis()
        .await
        .expect("Failed to initialize genesis");

    assert!(!consensus.is_running().await);
    assert_eq!(consensus.get_current_height().await, 1);

    let status = consensus.get_status().await;
    assert!(!status.is_running);
    assert_eq!(status.current_height, 1);
    assert_eq!(status.validator_count, 1);
    assert_eq!(status.transaction_pool_size, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_transaction_management() {
    let (storage, _tmp) = new_temp_storage().await;

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);
    consensus
        .initialize_genesis()
        .await
        .expect("Failed to initialize genesis");

    // Add two transactions and verify pool size increments.
    let tx1 = consensus.create_test_transaction(1, 0).await;
    consensus.add_transaction(tx1).await.expect("add tx1");
    let status1 = consensus.get_status().await;
    assert_eq!(status1.transaction_pool_size, 1);

    let tx2 = consensus.create_test_transaction(1, 1).await;
    consensus.add_transaction(tx2).await.expect("add tx2");
    let status2 = consensus.get_status().await;
    assert_eq!(status2.transaction_pool_size, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_transaction_uniqueness() {
    let (storage, _tmp) = new_temp_storage().await;

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);
    consensus
        .initialize_genesis()
        .await
        .expect("Failed to initialize genesis");

    let tx1 = consensus.create_test_transaction(1, 0).await;
    let tx2 = consensus.create_test_transaction(1, 1).await;
    let tx3 = consensus.create_test_transaction(2, 0).await;

    assert_ne!(tx1.id, tx2.id);
    assert_ne!(tx1.id, tx3.id);
    assert_ne!(tx2.id, tx3.id);

    assert_eq!(tx1.nonce, 0);
    assert_eq!(tx2.nonce, 1);
    assert_eq!(tx3.nonce, 0);

    assert_eq!(tx1.amount, 100);
    assert_eq!(tx2.amount, 100);
    assert_eq!(tx3.amount, 100);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_consensus_state_management() {
    let (storage, _tmp) = new_temp_storage().await;

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);
    consensus
        .initialize_genesis()
        .await
        .expect("Failed to initialize genesis");

    let state = consensus.get_consensus_state().await;

    assert_eq!(state.height, 1);
    assert_eq!(state.round, 0);
    assert_eq!(state.validators.len(), 1);
    assert_eq!(state.validators[0].address, node_id);
    assert_eq!(state.validators[0].voting_power, 100);
    assert_eq!(state.proposer, Some(node_id));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_consensus_lifecycle() {
    let (storage, _tmp) = new_temp_storage().await;

    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32];
    let config = ConsensusConfig::default();

    let consensus = ConsensusService::new(node_id, keypair, storage, config);

    // Pre-init genesis to avoid racing column-family creation.
    consensus
        .initialize_genesis()
        .await
        .expect("Failed to initialize genesis");

    // Start with a timeout so this test never hangs the suite.
    timeout(Duration::from_secs(10), consensus.start())
        .await
        .expect("consensus.start() timed out")
        .expect("Failed to start consensus");

    // Poll get_status() with retries; first run can still be warming up.
    let start = std::time::Instant::now();
    let running = loop {
        match timeout(Duration::from_secs(3), async { consensus.get_status().await }).await {
            Ok(status) => break status.is_running,
            Err(_) => {
                if start.elapsed() > Duration::from_secs(10) {
                    panic!("get_status() not available after 10s");
                }
                sleep(Duration::from_millis(150)).await;
            }
        }
    };
    assert!(running, "consensus should report running after start");

    // Stop with a timeout, too.
    timeout(Duration::from_secs(10), consensus.stop())
        .await
        .expect("consensus.stop() timed out")
        .expect("Failed to stop consensus");
    assert!(!consensus.is_running().await);
}
