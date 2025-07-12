
use catalyst_core::{
    NetworkModule, StorageModule, ConsensusModule, 
    RuntimeModule, NodeId, NodeRole,
    NodeStatus, SyncStatus, ResourceMetrics,
};
use catalyst_config::CatalystConfig;
use catalyst_network::NetworkManager;
use catalyst_storage::StorageManager;
use catalyst_rpc::RpcServer;
use catalyst_service_bus::ServiceBus;
use catalyst_dfs::LocalDfs;
use catalyst_consensus::ConsensusManager;
use std::sync::Arc;
use tokio::sync::RwLock;

// Update the CatalystNode struct to use concrete types:
pub struct CatalystNode {
    pub id: NodeId,
    pub role: NodeRole,
    pub config: CatalystConfig,
    pub network: Arc<NetworkManager>,
    pub storage: Arc<StorageManager>,
    pub consensus: Arc<ConsensusManager>,
    pub rpc_server: Arc<RpcServer>,
    pub service_bus: Arc<ServiceBus>,
    pub dfs: Arc<LocalDfs>,
    pub status: Arc<RwLock<NodeStatus>>,
}