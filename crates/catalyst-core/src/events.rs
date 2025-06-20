use serde::{Deserialize, Serialize};
use crate::{Address, TxHash, BlockHash, TokenAmount, Timestamp, NodeId};

/// System-wide events that can be subscribed to
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemEvent {
    /// Blockchain events
    Blockchain(BlockchainEvent),
    /// Network events
    Network(NetworkEvent),
    /// Consensus events
    Consensus(ConsensusEvent),
    /// Storage events
    Storage(StorageEvent),
    /// Module lifecycle events
    Module(ModuleEvent),
}

/// Blockchain-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockchainEvent {
    /// New block added to chain
    NewBlock {
        block_hash: BlockHash,
        block_height: u64,
        timestamp: Timestamp,
        tx_count: u32,
        producer_id: NodeId,
    },
    /// New transaction in mempool
    NewTransaction {
        tx_hash: TxHash,
        from_address: Option<Address>,
        to_address: Option<Address>,
        amount: Option<TokenAmount>,
        timestamp: Timestamp,
    },
    /// Transaction confirmed in block
    TransactionConfirmed {
        tx_hash: TxHash,
        block_hash: BlockHash,
        block_height: u64,
        gas_used: u64,
        success: bool,
    },
    /// Token transfer event
    TokenTransfer {
        from_address: Address,
        to_address: Address,
        amount: TokenAmount,
        tx_hash: TxHash,
        block_height: u64,
    },
    /// Contract deployed
    ContractDeployed {
        contract_address: Address,
        deployer_address: Address,
        tx_hash: TxHash,
        block_height: u64,
        runtime_type: String,
    },
    /// Contract function called
    ContractCall {
        contract_address: Address,
        caller_address: Address,
        function_data: Vec<u8>,
        tx_hash: TxHash,
        block_height: u64,
        gas_used: u64,
        success: bool,
    },
    /// Custom contract event
    ContractEvent {
        contract_address: Address,
        event_name: String,
        topics: Vec<[u8; 32]>,
        data: Vec<u8>,
        tx_hash: TxHash,
        block_height: u64,
    },
}

/// Network-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkEvent {
    /// Peer connected
    PeerConnected {
        peer_id: NodeId,
        address: String,
        role: crate::NodeRole,
    },
    /// Peer disconnected
    PeerDisconnected {
        peer_id: NodeId,
        reason: String,
    },
    /// Message received from peer
    MessageReceived {
        peer_id: NodeId,
        message_type: String,
        size_bytes: u32,
    },
    /// Network partition detected
    NetworkPartition {
        partition_size: u32,
        connected_peers: Vec<NodeId>,
    },
    /// Network healing (partition resolved)
    NetworkHealed {
        total_peers: u32,
        healing_duration_ms: u64,
    },
}

/// Consensus-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusEvent {
    /// New ledger cycle started
    CycleStarted {
        cycle_id: u64,
        producer_count: u32,
        partition_id: u32,
        start_time: Timestamp,
    },
    /// Ledger cycle completed
    CycleCompleted {
        cycle_id: u64,
        block_hash: BlockHash,
        participant_count: u32,
        duration_ms: u64,
    },
    /// Node selected as producer
    SelectedAsProducer {
        cycle_id: u64,
        partition_id: u32,
        selection_time: Timestamp,
    },
    /// Consensus phase transition
    PhaseTransition {
        cycle_id: u64,
        from_phase: ConsensusPhase,
        to_phase: ConsensusPhase,
        timestamp: Timestamp,
    },
    /// Consensus failure/timeout
    ConsensusFailed {
        cycle_id: u64,
        phase: ConsensusPhase,
        reason: String,
        participant_count: u32,
    },
}

/// Consensus phases from the protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusPhase {
    Construction,
    Campaigning,
    Voting,
    Synchronization,
}

/// Storage-related events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageEvent {
    /// File stored in DFS
    FileStored {
        file_hash: String,
        size_bytes: u64,
        storage_node: NodeId,
        timestamp: Timestamp,
    },
    /// File retrieved from DFS
    FileRetrieved {
        file_hash: String,
        requester_node: NodeId,
        timestamp: Timestamp,
    },
    /// File pinned
    FilePinned {
        file_hash: String,
        pin_count: u32,
        timestamp: Timestamp,
    },
    /// Storage capacity changed
    StorageCapacityChanged {
        node_id: NodeId,
        old_capacity: u64,
        new_capacity: u64,
        timestamp: Timestamp,
    },
    /// Storage node joined network
    StorageNodeJoined {
        node_id: NodeId,
        capacity_bytes: u64,
        location: Option<String>,
    },
    /// Storage node left network
    StorageNodeLeft {
        node_id: NodeId,
        reason: String,
    },
}

/// Module lifecycle events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ModuleEvent {
    /// Module initialized
    ModuleInitialized {
        module_name: String,
        version: String,
        timestamp: Timestamp,
    },
    /// Module started
    ModuleStarted {
        module_name: String,
        timestamp: Timestamp,
    },
    /// Module stopped
    ModuleStopped {
        module_name: String,
        reason: String,
        timestamp: Timestamp,
    },
    /// Module failed
    ModuleFailed {
        module_name: String,
        error: String,
        timestamp: Timestamp,
    },
    /// Module health check failed
    HealthCheckFailed {
        module_name: String,
        error: String,
        timestamp: Timestamp,
    },
}

/// Event subscription configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSubscription {
    /// Unique subscription ID
    pub id: String,
    /// Event filter
    pub filter: EventFilter,
    /// Delivery method
    pub delivery: DeliveryMethod,
    /// Created timestamp
    pub created_at: Timestamp,
    /// Statistics
    pub stats: SubscriptionStats,
}

/// Event filter for subscriptions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    /// System event types to include
    pub event_types: Option<Vec<String>>,
    /// Contract addresses to filter by
    pub contract_addresses: Option<Vec<Address>>,
    /// Transaction addresses to filter by
    pub addresses: Option<Vec<Address>>,
    /// Event topics to filter by
    pub topics: Option<Vec<[u8; 32]>>,
    /// Block height range
    pub block_range: Option<BlockRange>,
    /// Only include successful events
    pub success_only: bool,
}

/// Block range for filtering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockRange {
    pub from_block: Option<u64>,
    pub to_block: Option<u64>,
}

/// Event delivery methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeliveryMethod {
    /// WebSocket stream
    WebSocket {
        connection_id: String,
    },
    /// HTTP webhook
    Webhook {
        url: String,
        retry_policy: crate::traits::RetryPolicy,
        authentication: Option<crate::traits::WebhookAuth>,
    },
    /// Internal channel (for module-to-module communication)
    Internal {
        channel_id: String,
    },
}

/// Subscription statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionStats {
    /// Total events delivered
    pub events_delivered: u64,
    /// Failed delivery attempts
    pub delivery_failures: u64,
    /// Last successful delivery
    pub last_delivery: Option<Timestamp>,
    /// Last delivery failure
    pub last_failure: Option<Timestamp>,
    /// Average delivery latency (ms)
    pub avg_latency_ms: f64,
}

/// Event batch for efficient delivery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBatch {
    /// Batch ID
    pub batch_id: String,
    /// Events in this batch
    pub events: Vec<SystemEvent>,
    /// Batch timestamp
    pub timestamp: Timestamp,
    /// Sequence number
    pub sequence: u64,
}

/// Event delivery confirmation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryConfirmation {
    /// Event or batch ID
    pub id: String,
    /// Delivery status
    pub status: DeliveryStatus,
    /// Delivery timestamp
    pub delivered_at: Timestamp,
    /// Response time in milliseconds
    pub response_time_ms: u64,
    /// Error message if failed
    pub error: Option<String>,
}

/// Event delivery status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DeliveryStatus {
    Success,
    Failed,
    Retrying,
    Abandoned,
}