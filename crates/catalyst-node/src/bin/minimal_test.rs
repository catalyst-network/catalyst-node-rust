use catalyst_node::consensus_service::{ConsensusService, ConsensusConfig};
use catalyst_crypto::KeyPair;
use catalyst_storage::{StorageManager, StorageConfig};
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{info, Level};
use tracing_subscriber;
use rand;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("🧪 Minimal Consensus Test Starting...");

    // Create basic components
    let storage_config = StorageConfig::default(); // Use default instead of for_development
    let storage = Arc::new(StorageManager::new(storage_config).await?);
    
    let mut rng = rand::thread_rng();
    let keypair = KeyPair::generate(&mut rng);
    let node_id = [1u8; 32]; // Simple node ID
    
    let consensus_config = ConsensusConfig::default();
    
    info!("✅ Created storage and consensus components");

    // Create consensus service
    let consensus = ConsensusService::new(node_id, keypair, storage, consensus_config);
    
    info!("✅ Consensus service created");

    // Test basic functionality
    let initial_height = consensus.get_current_height().await;
    info!("📏 Initial height: {}", initial_height);

    let status = consensus.get_status().await;
    info!("📊 Initial status:");
    info!("   Running: {}", status.is_running);
    info!("   Height: {}", status.current_height);
    info!("   Validators: {}", status.validator_count);
    info!("   Pool size: {}", status.transaction_pool_size);

    // Test transaction creation
    let test_tx = consensus.create_test_transaction(1, 0).await;
    info!("✅ Created test transaction: {:?}", hex::encode(&test_tx.id[..8]));

    // Add transaction to pool
    consensus.add_transaction(test_tx).await?;
    
    let updated_status = consensus.get_status().await;
    info!("📈 After adding transaction - Pool size: {}", updated_status.transaction_pool_size);

    // Test consensus state
    let consensus_state = consensus.get_consensus_state().await;
    info!("🔍 Consensus state:");
    info!("   Height: {}", consensus_state.height);
    info!("   Round: {}", consensus_state.round);
    info!("   Phase: {:?}", consensus_state.phase);
    info!("   Validator count: {}", consensus_state.validators.len());

    // Start consensus briefly
    info!("🚀 Starting consensus for 30 seconds...");
    consensus.start().await?;
    
    // Monitor for 30 seconds
    for i in 1..=6 {
        sleep(Duration::from_secs(5)).await;
        
        let height = consensus.get_current_height().await;
        let status = consensus.get_status().await;
        let state = consensus.get_consensus_state().await;
        
        info!("📊 Update {}/6 ({}s):", i, i * 5);
        info!("   Height: {}", height);
        info!("   Round: {}", status.current_round);
        info!("   Phase: {:?}", state.phase);
        info!("   Pool: {}", status.transaction_pool_size);
        
        if let Some(proposer) = state.proposer {
            info!("   Proposer: {:?}", hex::encode(&proposer[..4]));
        }
        
        if height > initial_height {
            info!("🎉 NEW BLOCK PRODUCED! Height increased from {} to {}", initial_height, height);
        }
    }

    // Stop consensus
    info!("🛑 Stopping consensus...");
    consensus.stop().await?;

    let final_height = consensus.get_current_height().await;
    info!("📈 Final height: {}", final_height);

    if final_height > initial_height {
        info!("✅ SUCCESS: Consensus produced {} blocks!", final_height - initial_height);
    } else {
        info!("⚠️ No blocks were produced in 30 seconds (this might be normal for the timing)");
    }

    info!("🎯 Minimal consensus test completed!");

    Ok(())
}