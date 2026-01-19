//! Networked DFS implementation combining local storage with P2P networking
//! 
//! Provides a full distributed file system with IPFS compatibility

//! Networked DFS implementation combining local storage with P2P networking
//! 
//! Provides a full distributed file system with IPFS compatibility

use crate::{
    ContentId, ContentMetadata, DfsConfig, DfsError, DfsStats, 
    DistributedFileSystem, GcResult, CategorizedStorage, ContentCategory,
    LocalDfsStorage, DfsSwarm, SwarmConfig, NetworkEvent, ProviderId,
    Hash, LedgerStateUpdate  // Use local types instead of catalyst_core
};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{Duration, timeout};
use std::collections::HashMap;

/// Networked DFS that combines local storage with P2P networking
pub struct NetworkedDfs {
    local_storage: Arc<LocalDfsStorage>,
    swarm: Arc<RwLock<DfsSwarm>>,
    network_events: mpsc::UnboundedReceiver<NetworkEvent>,
    config: DfsConfig,
    // Cache of known providers for content
    provider_cache: Arc<RwLock<HashMap<ContentId, Vec<ProviderId>>>>,
}

impl NetworkedDfs {
    /// Create a new networked DFS instance
    pub async fn new(config: DfsConfig) -> Result<Self, DfsError> {
        // Create local storage
        let local_storage = Arc::new(LocalDfsStorage::new(config.clone()).await?);

        // Setup network events channel
        let (event_sender, network_events) = mpsc::unbounded_channel();

        // Create swarm configuration
        let swarm_config = SwarmConfig::default();

        // Create P2P swarm
        let swarm = Arc::new(RwLock::new(
            DfsSwarm::new(swarm_config, event_sender).await?
        ));

        let provider_cache = Arc::new(RwLock::new(HashMap::new()));

        let mut networked_dfs = Self {
            local_storage,
            swarm,
            network_events,
            config,
            provider_cache,
        };

        // Start background tasks
        networked_dfs.start_background_tasks().await?;

        Ok(networked_dfs)
    }

    /// Start background tasks for network event processing
    async fn start_background_tasks(&mut self) -> Result<(), DfsError> {
        let swarm = Arc::clone(&self.swarm);
        let provider_cache = Arc::clone(&self.provider_cache);
        let local_storage = Arc::clone(&self.local_storage);

        // Spawn network event processor
        tokio::spawn(async move {
            loop {
                let mut swarm_guard = swarm.write().await;
                
                if let Some(event) = swarm_guard.next_event().await {
                    drop(swarm_guard); // Release lock before processing
                    
                    match event {
                        NetworkEvent::ContentRequest { from, cid } => {
                            // Check if we have the content locally
                            if let Ok(true) = local_storage.has(&cid).await {
                                if let Ok(data) = local_storage.get(&cid).await {
                                    let mut swarm_guard = swarm.write().await;
                                    swarm_guard.send_content(from, cid, data);
                                }
                            }
                        }
                        
                        NetworkEvent::ProviderAnnouncement { from, cid } => {
                            // Update provider cache
                            let mut cache = provider_cache.write().await;
                            let providers = cache.entry(cid).or_insert_with(Vec::new);
                            let provider_id = ProviderId(from.to_string());
                            if !providers.contains(&provider_id) {
                                providers.push(provider_id);
                            }
                        }
                        
                        NetworkEvent::PeerConnected(peer_id) => {
                            log::info!("Connected to peer: {}", peer_id);
                        }
                        
                        NetworkEvent::PeerDisconnected(peer_id) => {
                            log::info!("Disconnected from peer: {}", peer_id);
                        }
                        
                        _ => {}
                    }
                }
                
                // Small delay to prevent busy loop
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        // Spawn provider cache cleanup task
        let provider_cache_cleanup = Arc::clone(&self.provider_cache);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            
            loop {
                interval.tick().await;
                
                // Clean up old provider entries
                let mut cache = provider_cache_cleanup.write().await;
                if cache.len() > 10000 {
                    // Remove oldest entries if cache gets too large
                    let keys_to_remove: Vec<ContentId> = cache.keys().take(1000).cloned().collect();
                    for key in keys_to_remove {
                        cache.remove(&key);
                    }
                }
            }
        });

        Ok(())
    }

    /// Get content from network if not available locally
    async fn get_from_network(&self, cid: &ContentId) -> Result<Vec<u8>, DfsError> {
        // Check provider cache first
        let providers = {
            let cache = self.provider_cache.read().await;
            cache.get(cid).cloned().unwrap_or_default()
        };

        if !providers.is_empty() {
            let mut swarm = self.swarm.write().await;
            if let Ok(data) = swarm.request_content(cid.clone()).await {
                return Ok(data);
            }
        }

        // Fallback to DHT discovery
        let mut swarm = self.swarm.write().await;
        let network_providers = swarm.find_providers(cid).await?;
        
        if !network_providers.is_empty() {
            // Update cache
            {
                let mut cache = self.provider_cache.write().await;
                cache.insert(cid.clone(), network_providers);
            }
            
            // Try to get content
            if let Ok(data) = swarm.request_content(cid.clone()).await {
                return Ok(data);
            }
        }

        Err(DfsError::NotFound(format!("Content not found on network: {}", cid.to_string())))
    }

    /// Announce content to network
    async fn announce_content(&self, cid: &ContentId) {
        let mut swarm = self.swarm.write().await;
        swarm.provide_content(cid.clone());
    }

    /// Get local peer ID
    pub async fn local_peer_id(&self) -> String {
        let swarm = self.swarm.read().await;
        swarm.local_peer_id().to_string()
    }

    /// Get network statistics
    pub async fn network_stats(&self) -> Result<NetworkStats, DfsError> {
        let swarm = self.swarm.read().await;
        Ok(swarm.network_stats())
    }

    /// Connect to a specific peer
    pub async fn connect_peer(&self, peer_id: String, address: String) -> Result<(), DfsError> {
        use std::str::FromStr;
        
        let peer_id = libp2p::PeerId::from_str(&peer_id)
            .map_err(|e| DfsError::Network(format!("Invalid peer ID: {}", e)))?;
        
        let address = address.parse()
            .map_err(|e| DfsError::Network(format!("Invalid address: {}", e)))?;

        let mut swarm = self.swarm.write().await;
        swarm.add_address(peer_id, address);
        
        Ok(())
    }

    /// Bootstrap the DHT network
    pub async fn bootstrap(&self) -> Result<(), DfsError> {
        let mut swarm = self.swarm.write().await;
        swarm.bootstrap()
    }
}

#[async_trait::async_trait]
impl DistributedFileSystem for NetworkedDfs {
    async fn put(&self, data: Vec<u8>) -> Result<ContentId, DfsError> {
        // Store locally first
        let cid = self.local_storage.put(data).await?;
        
        // Announce to network
        self.announce_content(&cid).await;
        
        Ok(cid)
    }

    async fn get(&self, cid: &ContentId) -> Result<Vec<u8>, DfsError> {
        // Try local storage first
        match self.local_storage.get(cid).await {
            Ok(data) => Ok(data),
            Err(DfsError::NotFound(_)) => {
                // Try to get from network
                let data = self.get_from_network(cid).await?;
                
                // Store locally for future requests
                let _ = self.local_storage.put(data.clone()).await;
                
                Ok(data)
            }
            Err(e) => Err(e),
        }
    }

    async fn has(&self, cid: &ContentId) -> Result<bool, DfsError> {
        // Check local storage
        if self.local_storage.has(cid).await? {
            return Ok(true);
        }

        // Check if we know providers for this content
        let cache = self.provider_cache.read().await;
        Ok(cache.contains_key(cid))
    }

    async fn pin(&self, cid: &ContentId) -> Result<(), DfsError> {
        // Ensure we have the content locally
        if !self.local_storage.has(cid).await? {
            let _ = self.get(cid).await?;
        }
        
        // Pin locally
        self.local_storage.pin(cid).await?;
        
        // Announce that we provide this content
        self.announce_content(cid).await;
        
        Ok(())
    }

    async fn unpin(&self, cid: &ContentId) -> Result<(), DfsError> {
        self.local_storage.unpin(cid).await
    }

    async fn metadata(&self, cid: &ContentId) -> Result<ContentMetadata, DfsError> {
        self.local_storage.metadata(cid).await
    }

    async fn list(&self) -> Result<Vec<ContentMetadata>, DfsError> {
        self.local_storage.list().await
    }

    async fn gc(&self) -> Result<GcResult, DfsError> {
        self.local_storage.gc().await
    }

    async fn stats(&self) -> Result<DfsStats, DfsError> {
        let mut stats = self.local_storage.stats().await?;
        
        // Add network-specific stats
        let provider_cache = self.provider_cache.read().await;
        stats.network_requests = provider_cache.len() as u64;
        
        Ok(stats)
    }
}

#[async_trait::async_trait]
impl CategorizedStorage for NetworkedDfs {
    async fn put_categorized(
        &self, 
        data: Vec<u8>, 
        category: ContentCategory
    ) -> Result<ContentId, DfsError> {
        let cid = self.local_storage.put_categorized(data, category).await?;
        
        // Announce to network
        self.announce_content(&cid).await;
        
        Ok(cid)
    }

    async fn list_by_category(
        &self, 
        category: ContentCategory
    ) -> Result<Vec<ContentMetadata>, DfsError> {
        self.local_storage.list_by_category(category).await
    }

    async fn store_ledger_update(
        &self, 
        update: &LedgerStateUpdate
    ) -> Result<ContentId, DfsError> {
        let cid = self.local_storage.store_ledger_update(update).await?;
        
        // Announce ledger updates to network with high priority
        self.announce_content(&cid).await;
        
        Ok(cid)
    }

    async fn get_ledger_update(
        &self, 
        cid: &ContentId
    ) -> Result<LedgerStateUpdate, DfsError> {
        // Try local first, then network
        match self.local_storage.get_ledger_update(cid).await {
            Ok(update) => Ok(update),
            Err(DfsError::NotFound(_)) => {
                // Try to get from network
                let data = self.get_from_network(cid).await?;
                
                // Deserialize as ledger update using serde_json instead of custom trait
                serde_json::from_slice(&data)
                    .map_err(|e| DfsError::Serialization(format!("Failed to deserialize ledger update: {}", e)))
            }
            Err(e) => Err(e),
        }
    }
}

/// Network statistics for monitoring
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub connected_peers: usize,
    pub provided_content_count: usize,
    pub pending_requests: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_networked_dfs_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = DfsConfig {
            storage_dir: temp_dir.path().to_path_buf(),
            enable_networking: false, // Disable networking for tests
            ..Default::default()
        };
        
        let dfs = NetworkedDfs::new(config).await.unwrap();
        assert!(dfs.local_peer_id().await.len() > 0);
    }

    #[tokio::test]
    async fn test_content_storage_and_retrieval() {
        let temp_dir = TempDir::new().unwrap();
        let config = DfsConfig {
            storage_dir: temp_dir.path().to_path_buf(),
            enable_networking: false, // Disable networking for tests
            ..Default::default()
        };
        
        let dfs = NetworkedDfs::new(config).await.unwrap();
        
        let data = b"test content".to_vec();
        let cid = dfs.put(data.clone()).await.unwrap();
        
        let retrieved = dfs.get(&cid).await.unwrap();
        assert_eq!(data, retrieved);
    }

    #[tokio::test]
    async fn test_categorized_storage() {
        let temp_dir = TempDir::new().unwrap();
        let config = DfsConfig {
            storage_dir: temp_dir.path().to_path_buf(),
            enable_networking: false, // Disable networking for tests
            ..Default::default()
        };
        
        let dfs = NetworkedDfs::new(config).await.unwrap();
        
        let data = b"contract data".to_vec();
        let cid = dfs.put_categorized(data, ContentCategory::Contract).await.unwrap();
        
        let contracts = dfs.list_by_category(ContentCategory::Contract).await.unwrap();
        assert_eq!(contracts.len(), 1);
        assert_eq!(contracts[0].cid, cid);
    }
}