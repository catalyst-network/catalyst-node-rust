// File: crates/catalyst-node/src/block_production.rs
// Block production integration with real consensus

use crate::{NodeError, consensus_service::{ConsensusService, Block}};
use catalyst_storage::StorageManager;
use std::sync::Arc;
use tracing::{info, error};
use serde::{Serialize, Deserialize};
use tokio::sync::RwLock;

/// Block production service that integrates with consensus
pub struct BlockProductionService {
    /// Storage for persisting blocks
    storage: Arc<StorageManager>,
    /// Consensus service reference
    consensus: Arc<ConsensusService>,
    /// Current block height tracking
    current_height: Arc<RwLock<u64>>,
    /// Service running state
    is_running: Arc<RwLock<bool>>,
}

/// Statistics for block production
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockProductionStats {
    pub total_blocks_produced: u64,
    pub current_height: u64,
    pub average_block_time: f64,
    pub transactions_processed: u64,
    pub consensus_rounds: u64,
}

impl BlockProductionService {
    /// Create new block production service
    pub fn new(storage: Arc<StorageManager>, consensus: Arc<ConsensusService>) -> Self {
        Self {
            storage,
            consensus,
            current_height: Arc::new(RwLock::new(0)),
            is_running: Arc::new(RwLock::new(false)),
        }
    }

    /// Initialize the block production service
    pub async fn initialize(&self) -> Result<(), NodeError> {
        info!("🏗️ Initializing block production service with real consensus");
        
        // Get current height from consensus
        let consensus_height = self.consensus.get_current_height().await;
        *self.current_height.write().await = consensus_height;
        
        info!("📏 Current block height: {}", consensus_height);
        Ok(())
    }

    /// Start block production (delegates to consensus)
    pub async fn start(&self) -> Result<(), NodeError> {
        info!("🚀 Starting block production service");
        
        *self.is_running.write().await = true;
        
        // The consensus service handles all block production
        // We just need to monitor and provide statistics
        self.start_monitoring().await;
        
        info!("✅ Block production service started");
        Ok(())
    }

    /// Stop block production
    pub async fn stop(&self) -> Result<(), NodeError> {
        info!("🛑 Stopping block production service");
        *self.is_running.write().await = false;
        Ok(())
    }

    /// Start monitoring consensus for block production events
    async fn start_monitoring(&self) {
        let service = Arc::new(self.clone());
        
        tokio::spawn(async move {
            let mut monitoring_interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
            
            info!("📊 Starting block production monitoring");
            
            loop {
                monitoring_interval.tick().await;
                
                if !*service.is_running.read().await {
                    break;
                }

                // Update our height tracking based on consensus
                let consensus_height = service.consensus.get_current_height().await;
                let mut current_height = service.current_height.write().await;
                
                if consensus_height > *current_height {
                    let blocks_produced = consensus_height - *current_height;
                    *current_height = consensus_height;
                    
                    info!("📈 Block production update: {} new blocks, current height: {}", 
                          blocks_produced, consensus_height);
                }
            }
            
            info!("📊 Block production monitoring stopped");
        });
    }

    /// Create and add a test transaction to consensus pool
    pub async fn create_test_transaction(&self) -> Result<(), NodeError> {
        let current_height = *self.current_height.read().await;
        
        // Create transaction using consensus service
        let transaction = self.consensus.create_test_transaction(current_height, rand::random()).await;
        
        // Add to consensus transaction pool
        self.consensus.add_transaction(transaction).await?;
        
        info!("✅ Test transaction added to consensus pool");
        Ok(())
    }

    /// Get a block by its height (from storage)
    pub async fn get_block_by_height(&self, height: u64) -> Result<Option<Block>, NodeError> {
        self.consensus.get_block_by_height(height).await
    }

    /// Get current block height
    pub async fn get_current_height(&self) -> u64 {
        *self.current_height.read().await
    }

    /// Get consensus status
    pub async fn get_consensus_status(&self) -> Result<crate::consensus_service::ConsensusStatus, NodeError> {
        Ok(self.consensus.get_status().await)
    }

    /// Get block production statistics
    pub async fn get_statistics(&self) -> Result<BlockProductionStats, NodeError> {
        let current_height = *self.current_height.read().await;
        let consensus_status = self.consensus.get_status().await;
        
        // Calculate basic statistics
        // In a real implementation, these would be tracked over time
        Ok(BlockProductionStats {
            total_blocks_produced: current_height,
            current_height,
            average_block_time: 40.0, // Based on our 40-second block time
            transactions_processed: current_height * 1, // Estimate: 1 tx per block average
            consensus_rounds: consensus_status.current_round,
        })
    }

    /// Check if service is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }

    /// Force consensus to create a test transaction and advance
    pub async fn trigger_test_block(&self) -> Result<(), NodeError> {
        info!("🔧 Triggering test block production");
        
        // Add some test transactions
        for i in 0..3 {
            let current_height = *self.current_height.read().await;
            let transaction = self.consensus.create_test_transaction(current_height, i).await;
            self.consensus.add_transaction(transaction).await?;
        }
        
        info!("✅ Added 3 test transactions to trigger block production");
        Ok(())
    }

    /// Start automatic test transaction generation
    pub async fn start_auto_test_transactions(&self, interval_secs: u64) {
        let service = Arc::new(self.clone());
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(interval_secs));
            
            info!("🔄 Starting automatic test transaction generation every {} seconds", interval_secs);
            
            loop {
                interval.tick().await;
                
                if !*service.is_running.read().await {
                    break;
                }

                if let Err(e) = service.create_test_transaction().await {
                    error!("❌ Failed to create test transaction: {}", e);
                } else {
                    info!("✅ Auto-generated test transaction");
                }
            }
            
            info!("🔄 Automatic test transaction generation stopped");
        });
    }

    /// Get detailed consensus state for debugging
    pub async fn get_consensus_debug_info(&self) -> Result<ConsensusDebugInfo, NodeError> {
        let consensus_state = self.consensus.get_consensus_state().await;
        let consensus_status = self.consensus.get_status().await;
        
        Ok(ConsensusDebugInfo {
            current_height: consensus_state.height,
            current_round: consensus_state.round,
            current_phase: format!("{:?}", consensus_state.phase),
            proposer: consensus_state.proposer.map(|p| hex::encode(&p[..8])),
            validator_count: consensus_state.validators.len(),
            transaction_pool_size: consensus_status.transaction_pool_size,
            locked_block: consensus_state.locked_block.map(|b| hex::encode(&b[..8])),
            valid_block: consensus_state.valid_block.map(|b| hex::encode(&b[..8])),
        })
    }
}

/// Debug information about consensus state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusDebugInfo {
    pub current_height: u64,
    pub current_round: u64,
    pub current_phase: String,
    pub proposer: Option<String>,
    pub validator_count: usize,
    pub transaction_pool_size: usize,
    pub locked_block: Option<String>,
    pub valid_block: Option<String>,
}

impl Clone for BlockProductionService {
    fn clone(&self) -> Self {
        Self {
            storage: Arc::clone(&self.storage),
            consensus: Arc::clone(&self.consensus),
            current_height: Arc::clone(&self.current_height),
            is_running: Arc::clone(&self.is_running),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use catalyst_storage::StorageConfig;
    use catalyst_crypto::KeyPair;

    #[tokio::test]
    async fn test_block_production_service_creation() {
        // Create storage
        let storage = Arc::new(
            StorageManager::new(StorageConfig::for_development())
                .await
                .expect("Failed to create storage")
        );

        // Create keypair and node ID
        let keypair = KeyPair::generate(&mut rand::thread_rng());
        let node_id = [1u8; 32];

        // Create consensus
        let consensus = Arc::new(ConsensusService::new(
            node_id,
            keypair,
            storage.clone(),
            crate::consensus_service::ConsensusConfig::default(),
        ));

        // Create block production service
        let block_production = BlockProductionService::new(storage, consensus);
        
        assert_eq!(block_production.get_current_height().await, 0);
        assert!(!block_production.is_running().await);
    }

    #[tokio::test]
    async fn test_statistics_generation() {
        // Create test service
        let storage = Arc::new(
            StorageManager::new(StorageConfig::for_development())
                .await
                .expect("Failed to create storage")
        );

        let keypair = KeyPair::generate(&mut rand::thread_rng());
        let node_id = [1u8; 32];

        let consensus = Arc::new(ConsensusService::new(
            node_id,
            keypair,
            storage.clone(),
            crate::consensus_service::ConsensusConfig::default(),
        ));

        let block_production = BlockProductionService::new(storage, consensus);
        
        // Get statistics
        let stats = block_production.get_statistics().await.unwrap();
        
        assert_eq!(stats.current_height, 0);
        assert_eq!(stats.total_blocks_produced, 0);
        assert_eq!(stats.average_block_time, 40.0);
    }
}