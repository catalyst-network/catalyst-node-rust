// File: examples/real_consensus_example.rs
// Example demonstrating the real consensus mechanism

use catalyst_node::{NodeBuilder, consensus_service::ConsensusConfig, block_production::BlockProductionStats};
use catalyst_config::CatalystConfig;
use catalyst_storage::StorageConfig;
use catalyst_rpc::RpcConfig;
use tokio::time::{sleep, Duration};
use tracing::{info, Level};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    info!("🚀 Starting Catalyst Node with Real Consensus Example");

    // Create configurations
    let node_config = CatalystConfig::for_development();
    let storage_config = StorageConfig::for_development();
    let rpc_config = RpcConfig::default();

    // Build the node with real consensus
    let mut node = NodeBuilder::new(node_config)
        .with_storage(storage_config)
        .with_rpc(rpc_config)
        .build()
        .await?;

    // Start the node
    info!("🔄 Starting node...");
    node.start().await?;

    // Start automatic test transaction generation (every 10 seconds)
    info!("🔄 Starting automatic test transaction generation...");
    node.start_auto_test_transactions(10).await?;

    // Monitor consensus for 5 minutes
    info!("📊 Monitoring consensus for 5 minutes...");
    let monitor_duration = Duration::from_secs(300); // 5 minutes
    let start_time = std::time::Instant::now();

    let mut last_height = 0u64;
    let mut blocks_produced = 0u64;

    while start_time.elapsed() < monitor_duration {
        // Check consensus status every 10 seconds
        sleep(Duration::from_secs(10)).await;

        // Get current status
        let current_height = node.get_block_height().await;
        let consensus_status = node.get_consensus_status().await;
        let debug_info = node.get_consensus_debug_info().await?;

        // Log current state
        info!("📈 Consensus Status:");
        info!("   Current Height: {}", current_height);
        
        if let Some(status) = consensus_status {
            info!("   Current Round: {}", status.current_round);
            info!("   Current Phase: {:?}", status.current_phase);
            info!("   Transaction Pool Size: {}", status.transaction_pool_size);
            
            if let Some(proposer) = status.proposer {
                info!("   Current Proposer: {:?}", hex::encode(&proposer[..8]));
            }
        }

        // Check if new blocks were produced
        if current_height > last_height {
            let new_blocks = current_height - last_height;
            blocks_produced += new_blocks;
            last_height = current_height;
            
            info!("🎉 NEW BLOCK(S) PRODUCED! {} new blocks, total: {}", new_blocks, blocks_produced);
            
            // Get block production statistics
            if let Some(block_production) = node.block_production() {
                match block_production.get_statistics().await {
                    Ok(stats) => {
                        info!("📊 Block Production Stats:");
                        info!("   Total Blocks: {}", stats.total_blocks_produced);
                        info!("   Average Block Time: {:.1}s", stats.average_block_time);
                        info!("   Transactions Processed: {}", stats.transactions_processed);
                    }
                    Err(e) => {
                        info!("⚠️ Could not get block production stats: {}", e);
                    }
                }
            }
        }

        // Log debug information
        info!("🔍 Debug Info:");
        info!("   Consensus Phase: {}", debug_info.current_phase);
        info!("   Validator Count: {}", debug_info.validator_count);
        
        if let Some(locked_block) = &debug_info.locked_block {
            info!("   Locked Block: {}", locked_block);
        }
        
        if let Some(valid_block) = &debug_info.valid_block {
            info!("   Valid Block: {}", valid_block);
        }

        info!("⏱️ Time elapsed: {:.1}s / {:.1}s", 
              start_time.elapsed().as_secs_f64(),
              monitor_duration.as_secs_f64());
        info!("─────────────────────────────────────────");
    }

    // Final statistics
    info!("📊 FINAL RESULTS:");
    info!("   Total Blocks Produced: {}", blocks_produced);
    info!("   Final Height: {}", node.get_block_height().await);
    info!("   Average Production Rate: {:.2} blocks/minute", 
          blocks_produced as f64 / 5.0);

    // Test manual block triggering
    info!("🔧 Testing manual block trigger...");
    node.trigger_test_block().await?;
    
    // Wait a bit to see if it triggers a block
    sleep(Duration::from_secs(45)).await;
    
    let final_height = node.get_block_height().await;
    info!("📈 Height after manual trigger: {}", final_height);

    // Stop the node
    info!("🛑 Stopping node...");
    node.stop().await?;

    info!("✅ Real consensus example completed successfully!");
    info!("🎯 Consensus produced {} blocks in 5 minutes", blocks_produced);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyst_node::consensus_service::{ConsensusService, ConsensusConfig};
    use catalyst_crypto::KeyPair;
    use catalyst_storage::StorageManager;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_real_consensus_basic_functionality() {
        // Initialize logging for test
        let _ = tracing_subscriber::fmt().try_init();

        // Create storage
        let storage = Arc::new(
            StorageManager::new(StorageConfig::for_development())
                .await
                .expect("Failed to create storage")
        );

        // Create consensus service
        let keypair = KeyPair::generate(&mut rand::thread_rng());
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
    }

    #[tokio::test]
    async fn test_transaction_pool() {
        let _ = tracing_subscriber::fmt().try_init();

        let storage = Arc::new(
            StorageManager::new(StorageConfig::for_development())
                .await
                .expect("Failed to create storage")
        );

        let keypair = KeyPair::generate(&mut rand::thread_rng());
        let node_id = [1u8; 32];
        let config = ConsensusConfig::default();

        let consensus = ConsensusService::new(node_id, keypair, storage, config);
        
        // Create and add test transaction
        let tx = consensus.create_test_transaction(1, 0).await;
        consensus.add_transaction(tx).await.expect("Failed to add transaction");
        
        // Check transaction pool
        let status = consensus.get_status().await;
        assert_eq!(status.transaction_pool_size, 1);
    }

    #[tokio::test]
    async fn test_block_creation() {
        let _ = tracing_subscriber::fmt().try_init();

        let storage = Arc::new(
            StorageManager::new(StorageConfig::for_development())
                .await
                .expect("Failed to create storage")
        );

        let keypair = KeyPair::generate(&mut rand::thread_rng());
        let node_id = [1u8; 32];
        let config = ConsensusConfig::default();

        let consensus = ConsensusService::new(node_id, keypair, storage, config);
        
        // Initialize genesis
        consensus.initialize_genesis().await.expect("Failed to initialize genesis");
        
        // Check that genesis block exists
        let genesis = consensus.get_block_by_height(0).await.expect("Failed to get genesis");
        assert!(genesis.is_some());
        
        let genesis_block = genesis.unwrap();
        assert_eq!(genesis_block.header.height, 0);
        assert_eq!(genesis_block.transactions.len(), 0);
    }
}