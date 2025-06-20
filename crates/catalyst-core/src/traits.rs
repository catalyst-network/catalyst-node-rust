use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;
use crate::{
    Block, Transaction, NetworkMessage, Event, CatalystResult, 
    NodeId, BlockHash, Address, TokenAmount, LedgerCycle,
    ResourceProof, ConsensusMessage,
};

/// Core trait for all Catalyst modules
#[async_trait]
pub trait CatalystModule: Send + Sync {
    /// Module name for identification
    fn name(&self) -> &'static str;
    
    /// Module version
    fn version(&self) -> &'static str;
    
    /// Initialize the module
    async fn initialize(&mut self) -> CatalystResult<()>;
    
    /// Start the module (begin processing)
    async fn start(&mut self) -> CatalystResult<()>;
    
    /// Stop the module gracefully
    async fn stop(&mut self) -> CatalystResult<()>;
    
    /// Check if module is healthy
    async fn health_check(&self) -> CatalystResult<bool>;
}

/// Network module for P2P communication
#[async_trait]
pub trait NetworkModule: CatalystModule {
    /// Broadcast a message to the network
    async fn broadcast(&self, message: NetworkMessage) -> CatalystResult<()>;
    
    /// Send a message to a specific peer
    async fn send_to_peer(&self, peer_id: NodeId, message: NetworkMessage) -> CatalystResult<()>;
    
    /// Subscribe to network events
    async fn subscribe(&self) -> CatalystResult<Pin<Box<dyn Stream<Item = NetworkMessage> + Send>>>;
    
    /// Get connected peers
    async fn get_peers(&self) -> CatalystResult<Vec<NodeId>>;
    
    /// Connect to a peer
    async fn connect_peer(&self, address: &str) -> CatalystResult<NodeId>;
    
    /// Disconnect from a peer
    async fn disconnect_peer(&self, peer_id: NodeId) -> CatalystResult<()>;
}

/// Storage module for data persistence
#[async_trait]
pub trait StorageModule: CatalystModule {
    /// Store a key-value pair
    async fn put(&self, key: &[u8], value: &[u8]) -> CatalystResult<()>;
    
    /// Retrieve a value by key
    async fn get(&self, key: &[u8]) -> CatalystResult<Option<Vec<u8>>>;
    
    /// Delete a key-value pair
    async fn delete(&self, key: &[u8]) -> CatalystResult<()>;
    
    /// Check if a key exists
    async fn exists(&self, key: &[u8]) -> CatalystResult<bool>;
    
    /// Iterate over keys with a prefix
    async fn iter_prefix(&self, prefix: &[u8]) -> CatalystResult<Pin<Box<dyn Stream<Item = (Vec<u8>, Vec<u8>)> + Send>>>;
    
    /// Get storage statistics
    async fn stats(&self) -> CatalystResult<StorageStats>;
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_keys: u64,
    pub total_size_bytes: u64,
    pub free_space_bytes: u64,
}

/// Consensus module implementing the collaborative consensus protocol
#[async_trait]
pub trait ConsensusModule: CatalystModule {
    /// Start a new ledger cycle as a producer
    async fn start_cycle(&self, cycle: LedgerCycle) -> CatalystResult<()>;
    
    /// Process consensus messages
    async fn process_consensus_message(&self, message: ConsensusMessage) -> CatalystResult<()>;
    
    /// Get current cycle information
    async fn current_cycle(&self) -> CatalystResult<Option<LedgerCycle>>;
    
    /// Check if this node is a producer for the current cycle
    async fn is_producer(&self) -> CatalystResult<bool>;
    
    /// Validate a proposed block
    async fn validate_block(&self, block: &Block) -> CatalystResult<bool>;
    
    /// Finalize a block (add to ledger)
    async fn finalize_block(&self, block: Block) -> CatalystResult<()>;
    
    /// Get the latest finalized block
    async fn latest_block(&self) -> CatalystResult<Option<Block>>;
}

/// Runtime module for executing smart contracts
#[async_trait]
pub trait RuntimeModule: CatalystModule {
    /// Execute a transaction against this runtime
    async fn execute_transaction(&self, tx: &Transaction) -> CatalystResult<ExecutionResult>;
    
    /// Deploy a smart contract
    async fn deploy_contract(&self, code: &[u8], constructor_args: &[u8]) -> CatalystResult<Address>;
    
    /// Call a smart contract function
    async fn call_contract(
        &self, 
        address: &Address, 
        function_data: &[u8],
        caller: &Address,
        value: TokenAmount,
        gas_limit: u64,
    ) -> CatalystResult<ExecutionResult>;
    
    /// Get contract code
    async fn get_contract_code(&self, address: &Address) -> CatalystResult<Option<Vec<u8>>>;
    
    /// Get contract storage
    async fn get_storage(&self, address: &Address, key: &[u8]) -> CatalystResult<Option<Vec<u8>>>;
    
    /// Estimate gas for a transaction
    async fn estimate_gas(&self, tx: &Transaction) -> CatalystResult<u64>;
    
    /// Get supported runtime type
    fn runtime_type(&self) -> RuntimeType;
}

/// Execution result from runtime
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub success: bool,
    pub gas_used: u64,
    pub return_data: Vec<u8>,
    pub logs: Vec<ExecutionLog>,
    pub state_changes: Vec<StateChange>,
}

/// Execution log entry
#[derive(Debug, Clone)]
pub struct ExecutionLog {
    pub address: Address,
    pub topics: Vec<[u8; 32]>,
    pub data: Vec<u8>,
}

/// State change record
#[derive(Debug, Clone)]
pub struct StateChange {
    pub address: Address,
    pub storage_key: Vec<u8>,
    pub old_value: Option<Vec<u8>>,
    pub new_value: Option<Vec<u8>>,
}

/// Runtime type enumeration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeType {
    EVM,
    SVM,
    WASM,
    Native,
}

/// Service bus module for Web2 integration
#[async_trait]
pub trait ServiceBusModule: CatalystModule {
    /// Publish an event to subscribers
    async fn publish_event(&self, event: Event) -> CatalystResult<()>;
    
    /// Subscribe to events matching a filter
    async fn subscribe_events(&self, filter: EventFilter) -> CatalystResult<Pin<Box<dyn Stream<Item = Event> + Send>>>;
    
    /// Register a webhook endpoint
    async fn register_webhook(&self, endpoint: WebhookEndpoint) -> CatalystResult<String>;
    
    /// Unregister a webhook
    async fn unregister_webhook(&self, webhook_id: &str) -> CatalystResult<()>;
    
    /// Get active subscriptions
    async fn get_subscriptions(&self) -> CatalystResult<Vec<EventSubscription>>;
}

/// Event filter for service bus subscriptions
#[derive(Debug, Clone)]
pub struct EventFilter {
    pub event_types: Option<Vec<String>>,
    pub addresses: Option<Vec<Address>>,
    pub topics: Option<Vec<[u8; 32]>>,
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
}

/// Webhook endpoint configuration
#[derive(Debug, Clone)]
pub struct WebhookEndpoint {
    pub url: String,
    pub filter: EventFilter,
    pub retry_policy: RetryPolicy,
    pub authentication: Option<WebhookAuth>,
}

/// Retry policy for webhook delivery
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_delay_ms: u64,
    pub max_delay_ms: u64,
    pub backoff_multiplier: f64,
}

/// Webhook authentication
#[derive(Debug, Clone)]
pub enum WebhookAuth {
    None,
    Bearer { token: String },
    Basic { username: String, password: String },
    Custom { headers: Vec<(String, String)> },
}

/// Event subscription information
#[derive(Debug, Clone)]
pub struct EventSubscription {
    pub id: String,
    pub filter: EventFilter,
    pub created_at: u64,
    pub events_delivered: u64,
}

/// Distributed File System module
#[async_trait]
pub trait DfsModule: CatalystModule {
    /// Store a file and return its content hash
    async fn store_file(&self, data: &[u8]) -> CatalystResult<String>;
    
    /// Retrieve a file by its content hash
    async fn get_file(&self, hash: &str) -> CatalystResult<Option<Vec<u8>>>;
    
    /// Check if a file exists
    async fn file_exists(&self, hash: &str) -> CatalystResult<bool>;
    
    /// Pin a file (ensure it stays available)
    async fn pin_file(&self, hash: &str) -> CatalystResult<()>;
    
    /// Unpin a file
    async fn unpin_file(&self, hash: &str) -> CatalystResult<()>;
    
    /// List pinned files
    async fn list_pinned(&self) -> CatalystResult<Vec<String>>;
    
    /// Get file metadata
    async fn file_metadata(&self, hash: &str) -> CatalystResult<Option<FileMetadata>>;
    
    /// Provide storage space to the network
    async fn provide_storage(&self, capacity_bytes: u64) -> CatalystResult<()>;
}

/// File metadata
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub hash: String,
    pub size_bytes: u64,
    pub created_at: u64,
    pub pin_count: u32,
    pub availability_score: f64,
}

/// Configuration module for managing node settings
pub trait ConfigModule: Send + Sync {
    /// Get a configuration value
    fn get<T>(&self, key: &str) -> CatalystResult<Option<T>> 
    where T: serde::de::DeserializeOwned;
    
    /// Set a configuration value
    fn set<T>(&mut self, key: &str, value: T) -> CatalystResult<()>
    where T: serde::Serialize;
    
    /// Save configuration to disk
    fn save(&self) -> CatalystResult<()>;
    
    /// Reload configuration from disk
    fn reload(&mut self) -> CatalystResult<()>;
}