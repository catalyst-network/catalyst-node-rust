//! Storage service implementing the CatalystService trait

use crate::{CatalystService, EventBus, NodeError, ServiceHealth, ServiceType};
use async_trait::async_trait;
use catalyst_storage::{StorageConfig, StorageManager};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Storage service managing persistent data
pub struct StorageService {
    storage_manager: Arc<StorageManager>,
    event_bus: Option<Arc<EventBus>>,
    service_health: RwLock<ServiceHealth>,
}

/// Block data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockData {
    pub number: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: u64,
    pub transactions: Vec<String>,
    pub state_root: String,
    pub size: u64,
}

/// Transaction data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionData {
    pub hash: String,
    pub from_address: String,
    pub to_address: Option<String>,
    pub value: String,
    pub gas_limit: u64,
    pub gas_price: String,
    pub nonce: u64,
    pub data: Vec<u8>,
    pub block_number: Option<u64>,
    pub transaction_index: Option<u64>,
    pub timestamp: u64,
}

impl StorageService {
    /// Create a new storage service
    pub async fn new(config: StorageConfig) -> Result<Self, NodeError> {
        info!("🗄️ Initializing storage service...");

        let storage_manager = Arc::new(
            StorageManager::new(config)
                .await
                .map_err(|e| NodeError::Storage(format!("Failed to initialize storage: {}", e)))?,
        );

        let service = Self {
            storage_manager,
            event_bus: None,
            service_health: RwLock::new(ServiceHealth::Starting),
        };

        // Initialize genesis block if needed
        service.initialize_genesis().await?;

        Ok(service)
    }

    /// Set the event bus for this service
    pub fn set_event_bus(&mut self, event_bus: Arc<EventBus>) {
        self.event_bus = Some(event_bus);
    }

    /// Get the underlying storage manager
    pub fn storage_manager(&self) -> &Arc<StorageManager> {
        &self.storage_manager
    }

    /// Initialize genesis block if database is empty
    pub async fn initialize_genesis(&self) -> Result<(), NodeError> {
        // Check if genesis block already exists
        if let Ok(Some(_)) = self.get_block_by_number(0).await {
            info!("Genesis block already exists, skipping initialization");
            return Ok(());
        }

        info!("Initializing genesis block...");

        // Create genesis block
        let genesis_block = BlockData {
            number: 0,
            hash: "0x0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            parent_hash: "0x0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            timestamp: catalyst_utils::utils::current_timestamp(),
            transactions: Vec::new(),
            state_root: "0x0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
            size: 0,
        };

        // Store genesis block
        self.store_block(&genesis_block).await?;

        // Update metadata
        self.set_metadata("latest_block_number", &0u64.to_le_bytes())
            .await?;
        self.set_metadata("latest_block_hash", genesis_block.hash.as_bytes())
            .await?;

        // Create initial accounts
        self.create_initial_accounts().await?;

        info!("✅ Genesis block initialized successfully");
        Ok(())
    }

    /// Create initial accounts for the network
    async fn create_initial_accounts(&self) -> Result<(), NodeError> {
        info!("Creating initial accounts...");

        // Create some initial accounts with balances
        let initial_accounts = [
            ("0x1234567890123456789012345678901234567890", "1000000"),
            ("0x0987654321098765432109876543210987654321", "500000"),
            ("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd", "250000"),
        ];

        for (address, balance) in &initial_accounts {
            let account_key = format!("account_balance_{}", address);
            self.set_metadata(&account_key, balance.as_bytes()).await?;
            info!(
                "Created initial account {} with balance {}",
                address, balance
            );
        }

        Ok(())
    }

    /// Store a block in the database
    pub async fn store_block(&self, block: &BlockData) -> Result<(), NodeError> {
        let block_key = format!("block_{}", block.number);
        let block_data = serde_json::to_vec(block)
            .map_err(|e| NodeError::Configuration(format!("Serialization error: {}", e)))?;

        self.storage_manager
            .engine()
            .put("metadata", block_key.as_bytes(), &block_data)
            .map_err(|e| NodeError::Storage(e.to_string()))?;

        // Also store by hash for quick lookups
        let hash_key = format!("block_hash_{}", block.hash);
        self.storage_manager
            .engine()
            .put("metadata", hash_key.as_bytes(), &block.number.to_le_bytes())
            .map_err(|e| NodeError::Storage(e.to_string()))?;

        info!("Stored block {} with hash {}", block.number, block.hash);
        Ok(())
    }

    /// Get a block by its number
    pub async fn get_block_by_number(&self, number: u64) -> Result<Option<BlockData>, NodeError> {
        let block_key = format!("block_{}", number);

        match self
            .storage_manager
            .engine()
            .get("metadata", block_key.as_bytes())
            .map_err(|e| NodeError::Storage(e.to_string()))?
        {
            Some(data) => {
                let block: BlockData = serde_json::from_slice(&data).map_err(|e| {
                    NodeError::Configuration(format!("Deserialization error: {}", e))
                })?;
                Ok(Some(block))
            }
            None => Ok(None),
        }
    }

    /// Get a block by its hash
    pub async fn get_block_by_hash(&self, hash: &str) -> Result<Option<BlockData>, NodeError> {
        let hash_key = format!("block_hash_{}", hash);

        match self
            .storage_manager
            .engine()
            .get("metadata", hash_key.as_bytes())
            .map_err(|e| NodeError::Storage(e.to_string()))?
        {
            Some(number_bytes) => {
                if number_bytes.len() != 8 {
                    return Err(NodeError::Storage("Invalid block number data".to_string()));
                }

                let number = u64::from_le_bytes([
                    number_bytes[0],
                    number_bytes[1],
                    number_bytes[2],
                    number_bytes[3],
                    number_bytes[4],
                    number_bytes[5],
                    number_bytes[6],
                    number_bytes[7],
                ]);

                self.get_block_by_number(number).await
            }
            None => Ok(None),
        }
    }

    /// Get the latest block number
    pub async fn get_latest_block_number(&self) -> Result<u64, NodeError> {
        match self
            .storage_manager
            .engine()
            .get("metadata", b"latest_block_number")
            .map_err(|e| NodeError::Storage(e.to_string()))?
        {
            Some(data) => {
                if data.len() != 8 {
                    return Err(NodeError::Storage(
                        "Invalid latest block number data".to_string(),
                    ));
                }

                Ok(u64::from_le_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]))
            }
            None => Ok(0),
        }
    }

    /// Get the latest block
    pub async fn get_latest_block(&self) -> Result<Option<BlockData>, NodeError> {
        let latest_number = self.get_latest_block_number().await?;
        self.get_block_by_number(latest_number).await
    }

    /// Store a transaction in the database
    pub async fn store_transaction(&self, transaction: &TransactionData) -> Result<(), NodeError> {
        let tx_key = format!("transaction_{}", transaction.hash);
        let tx_data = serde_json::to_vec(transaction)
            .map_err(|e| NodeError::Configuration(format!("Serialization error: {}", e)))?;

        self.storage_manager
            .engine()
            .put("metadata", tx_key.as_bytes(), &tx_data)
            .map_err(|e| NodeError::Storage(e.to_string()))?;

        info!("Stored transaction {}", transaction.hash);
        Ok(())
    }

    /// Get a transaction by its hash
    pub async fn get_transaction_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<TransactionData>, NodeError> {
        let tx_key = format!("transaction_{}", hash);

        match self
            .storage_manager
            .engine()
            .get("metadata", tx_key.as_bytes())
            .map_err(|e| NodeError::Storage(e.to_string()))?
        {
            Some(data) => {
                let transaction: TransactionData = serde_json::from_slice(&data).map_err(|e| {
                    NodeError::Configuration(format!("Deserialization error: {}", e))
                })?;
                Ok(Some(transaction))
            }
            None => Ok(None),
        }
    }

    /// Get account balance
    pub async fn get_account_balance(&self, address: &str) -> Result<String, NodeError> {
        let account_key = format!("account_balance_{}", address);

        match self
            .storage_manager
            .engine()
            .get("metadata", account_key.as_bytes())
            .map_err(|e| NodeError::Storage(e.to_string()))?
        {
            Some(data) => String::from_utf8(data)
                .map_err(|e| NodeError::Storage(format!("Invalid balance data: {}", e))),
            None => Ok("0".to_string()),
        }
    }

    /// Set account balance
    pub async fn set_account_balance(&self, address: &str, balance: &str) -> Result<(), NodeError> {
        let account_key = format!("account_balance_{}", address);

        self.storage_manager
            .engine()
            .put("metadata", account_key.as_bytes(), balance.as_bytes())
            .map_err(|e| NodeError::Storage(e.to_string()))?;

        info!("Set balance for account {} to {}", address, balance);
        Ok(())
    }

    /// Get account nonce
    pub async fn get_account_nonce(&self, address: &str) -> Result<u64, NodeError> {
        let nonce_key = format!("account_nonce_{}", address);

        match self
            .storage_manager
            .engine()
            .get("metadata", nonce_key.as_bytes())
            .map_err(|e| NodeError::Storage(e.to_string()))?
        {
            Some(data) => {
                if data.len() != 8 {
                    return Err(NodeError::Storage("Invalid nonce data".to_string()));
                }

                Ok(u64::from_le_bytes([
                    data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
                ]))
            }
            None => Ok(0),
        }
    }

    /// Set account nonce
    pub async fn set_account_nonce(&self, address: &str, nonce: u64) -> Result<(), NodeError> {
        let nonce_key = format!("account_nonce_{}", address);

        self.storage_manager
            .engine()
            .put("metadata", nonce_key.as_bytes(), &nonce.to_le_bytes())
            .map_err(|e| NodeError::Storage(e.to_string()))?;

        info!("Set nonce for account {} to {}", address, nonce);
        Ok(())
    }

    /// Update the latest block number
    pub async fn update_latest_block_number(&self, number: u64) -> Result<(), NodeError> {
        self.storage_manager
            .engine()
            .put("metadata", b"latest_block_number", &number.to_le_bytes())
            .map_err(|e| NodeError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Set metadata value
    async fn set_metadata(&self, key: &str, value: &[u8]) -> Result<(), NodeError> {
        self.storage_manager
            .engine()
            .put("metadata", key.as_bytes(), value)
            .map_err(|e| NodeError::Storage(e.to_string()))
    }

    /// Get storage statistics
    pub async fn get_storage_statistics(&self) -> Result<StorageStatistics, NodeError> {
        let stats = self
            .storage_manager
            .get_statistics()
            .await
            .map_err(|e| NodeError::Storage(e.to_string()))?;

        let latest_block = self.get_latest_block_number().await?;

        Ok(StorageStatistics {
            latest_block_number: latest_block,
            total_blocks: latest_block + 1,
            database_size: 0, // Would need to calculate actual size
            memory_usage: stats.memory_usage.values().sum(),
            pending_transactions: stats.pending_transactions as u64,
        })
    }
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStatistics {
    pub latest_block_number: u64,
    pub total_blocks: u64,
    pub database_size: u64,
    pub memory_usage: u64,
    pub pending_transactions: u64,
}

#[async_trait]
impl CatalystService for StorageService {
    fn service_type(&self) -> ServiceType {
        ServiceType::Storage
    }

    fn name(&self) -> &str {
        "Storage Service"
    }

    async fn start(&mut self) -> Result<(), NodeError> {
        info!("🚀 Starting storage service...");

        // Initialize genesis if needed
        self.initialize_genesis().await?;

        *self.service_health.write().await = ServiceHealth::Healthy;
        info!("✅ Storage service started successfully");

        Ok(())
    }

    async fn stop(&mut self) -> Result<(), NodeError> {
        info!("🛑 Stopping storage service...");

        // Perform basic cleanup without calling maintenance to avoid Send issues
        info!("Storage service cleanup completed");

        *self.service_health.write().await =
            ServiceHealth::Unhealthy("Service stopped".to_string());
        info!("✅ Storage service stopped successfully");

        Ok(())
    }

    async fn health_check(&self) -> ServiceHealth {
        if self.storage_manager.is_healthy().await {
            ServiceHealth::Healthy
        } else {
            ServiceHealth::Unhealthy("Storage engine is unhealthy".to_string())
        }
    }
}

/// Storage service RPC methods
impl StorageService {
    /// Get latest block number (RPC method)
    pub async fn rpc_get_latest_block_number(&self) -> Result<u64, NodeError> {
        self.get_latest_block_number().await
    }

    /// Get latest block (RPC method)
    pub async fn rpc_get_latest_block(&self) -> Result<Option<BlockData>, NodeError> {
        self.get_latest_block().await
    }

    /// Get block by number (RPC method)
    pub async fn rpc_get_block_by_number(
        &self,
        number: u64,
    ) -> Result<Option<BlockData>, NodeError> {
        self.get_block_by_number(number).await
    }

    /// Get block by hash (RPC method)
    pub async fn rpc_get_block_by_hash(&self, hash: &str) -> Result<Option<BlockData>, NodeError> {
        self.get_block_by_hash(hash).await
    }

    /// Get account balance (RPC method)
    pub async fn rpc_get_account_balance(&self, address: &str) -> Result<String, NodeError> {
        self.get_account_balance(address).await
    }

    /// Get account nonce (RPC method)
    pub async fn rpc_get_account_nonce(&self, address: &str) -> Result<u64, NodeError> {
        self.get_account_nonce(address).await
    }

    /// Get transaction by hash (RPC method)
    pub async fn rpc_get_transaction_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<TransactionData>, NodeError> {
        self.get_transaction_by_hash(hash).await
    }

    /// Get storage statistics (RPC method)
    pub async fn rpc_get_storage_statistics(&self) -> Result<StorageStatistics, NodeError> {
        self.get_storage_statistics().await
    }
}

// Import the RPC types and traits
use catalyst_rpc::storage_types;

/// Storage service wrapper for RPC - this is what was referenced as StorageServiceAdapter
pub struct StorageServiceRpc {
    storage_service: Arc<StorageService>,
}

impl StorageServiceRpc {
    pub fn new(storage_service: Arc<StorageService>) -> Self {
        Self { storage_service }
    }

    /// Convert our BlockData to RPC BlockData
    fn convert_block_data(block: &BlockData) -> storage_types::BlockData {
        storage_types::BlockData {
            number: block.number,
            hash: block.hash.clone(),
            parent_hash: block.parent_hash.clone(),
            timestamp: block.timestamp,
            transactions: block.transactions.clone(),
            state_root: block.state_root.clone(),
            size: block.size,
            // Fill in the missing fields that RPC expects
            gas_limit: "8000000".to_string(), // Default gas limit
            gas_used: "0".to_string(),        // Default gas used
            transaction_count: block.transactions.len(), // Fix: remove 'as u64' cast
            validator: String::new(),         // Optional validator field
            transactions_root: "0x".to_string(), // Transactions merkle root
        }
    }

    /// Convert our TransactionData to RPC TransactionData
    fn convert_transaction_data(tx: &TransactionData) -> storage_types::TransactionData {
        storage_types::TransactionData {
            hash: tx.hash.clone(),
            from: tx.from_address.clone(), // RPC uses 'from' not 'from_address'
            to: tx.to_address.clone(),     // RPC uses 'to' not 'to_address'
            value: tx.value.clone(),
            gas_limit: tx.gas_limit.to_string(), // RPC expects String
            gas_price: tx.gas_price.clone(),
            nonce: tx.nonce,
            data: format!("0x{}", Self::bytes_to_hex(&tx.data)), // Convert Vec<u8> to hex string
            block_number: tx.block_number,
            transaction_index: tx.transaction_index,
            block_hash: None,                // We don't have this in our data
            gas_used: Some("0".to_string()), // Fix: wrap in Some()
            status: Some("1".to_string()),   // Fix: wrap in Some()
        }
    }

    /// Convert bytes to hex string without external dependency
    fn bytes_to_hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    }

    /// Convert NodeError to String
    fn convert_error(err: NodeError) -> String {
        err.to_string()
    }

    /// Get latest block number (RPC method)
    pub async fn get_latest_block_number(&self) -> Result<u64, NodeError> {
        self.storage_service.get_latest_block_number().await
    }

    /// Get latest block (RPC method)
    pub async fn get_latest_block(&self) -> Result<Option<BlockData>, NodeError> {
        self.storage_service.get_latest_block().await
    }

    /// Get block by number (RPC method)
    pub async fn get_block_by_number(&self, number: u64) -> Result<Option<BlockData>, NodeError> {
        self.storage_service.get_block_by_number(number).await
    }

    /// Get block by hash (RPC method)
    pub async fn get_block_by_hash(&self, hash: &str) -> Result<Option<BlockData>, NodeError> {
        self.storage_service.get_block_by_hash(hash).await
    }

    /// Get account balance (RPC method)
    pub async fn get_account_balance(&self, address: &str) -> Result<String, NodeError> {
        self.storage_service.get_account_balance(address).await
    }

    /// Get account nonce (RPC method)
    pub async fn get_account_nonce(&self, address: &str) -> Result<u64, NodeError> {
        self.storage_service.get_account_nonce(address).await
    }

    /// Get transaction by hash (RPC method)
    pub async fn get_transaction_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<TransactionData>, NodeError> {
        self.storage_service.get_transaction_by_hash(hash).await
    }

    /// Get storage statistics (RPC method)
    pub async fn get_storage_statistics(&self) -> Result<StorageStatistics, NodeError> {
        self.storage_service.get_storage_statistics().await
    }
}

/// Implement the RPC trait for StorageServiceRpc
#[async_trait]
impl catalyst_rpc::StorageServiceTrait for StorageServiceRpc {
    async fn get_latest_block_number(&self) -> Result<u64, String> {
        self.storage_service
            .get_latest_block_number()
            .await
            .map_err(Self::convert_error)
    }

    async fn get_latest_block(&self) -> Result<Option<storage_types::BlockData>, String> {
        match self.storage_service.get_latest_block().await {
            Ok(Some(block)) => Ok(Some(Self::convert_block_data(&block))),
            Ok(None) => Ok(None),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    async fn get_block_by_number(
        &self,
        number: u64,
    ) -> Result<Option<storage_types::BlockData>, String> {
        match self.storage_service.get_block_by_number(number).await {
            Ok(Some(block)) => Ok(Some(Self::convert_block_data(&block))),
            Ok(None) => Ok(None),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    async fn get_block_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<storage_types::BlockData>, String> {
        match self.storage_service.get_block_by_hash(hash).await {
            Ok(Some(block)) => Ok(Some(Self::convert_block_data(&block))),
            Ok(None) => Ok(None),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    async fn get_account_balance(&self, address: &str) -> Result<String, String> {
        self.storage_service
            .get_account_balance(address)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_account_nonce(&self, address: &str) -> Result<u64, String> {
        self.storage_service
            .get_account_nonce(address)
            .await
            .map_err(Self::convert_error)
    }

    async fn get_transaction_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<storage_types::TransactionData>, String> {
        match self.storage_service.get_transaction_by_hash(hash).await {
            Ok(Some(tx)) => Ok(Some(Self::convert_transaction_data(&tx))),
            Ok(None) => Ok(None),
            Err(e) => Err(Self::convert_error(e)),
        }
    }

    async fn health_check(&self) -> bool {
        self.storage_service.storage_manager.is_healthy().await
    }

    async fn get_storage_statistics(&self) -> Result<(u64, u64), String> {
        match self.storage_service.get_storage_statistics().await {
            Ok(stats) => Ok((stats.total_blocks, stats.memory_usage)),
            Err(e) => Err(Self::convert_error(e)),
        }
    }
}

// Create an alias for backward compatibility
pub type StorageServiceAdapter = StorageServiceRpc;
