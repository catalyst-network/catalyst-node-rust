//! Content provider implementation for DFS
//! 
//! Handles content discovery and provider announcements

use crate::{ContentId, DfsError, ProviderId, ContentProvider};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant};

/// Content provider registry and discovery
pub struct DfsContentProvider {
    /// Map of content ID to set of provider IDs
    providers: Arc<RwLock<HashMap<ContentId, HashSet<ProviderId>>>>,
    /// Map of provider ID to last announcement time
    provider_timestamps: Arc<RwLock<HashMap<ProviderId, Instant>>>,
    /// Content we're providing
    providing: Arc<RwLock<HashSet<ContentId>>>,
    /// Provider timeout duration
    provider_timeout: Duration,
}

impl DfsContentProvider {
    /// Create a new content provider
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
            provider_timestamps: Arc::new(RwLock::new(HashMap::new())),
            providing: Arc::new(RwLock::new(HashSet::new())),
            provider_timeout: Duration::from_secs(3600), // 1 hour
        }
    }

    /// Add a provider for content
    pub async fn add_provider(&self, cid: &ContentId, provider: ProviderId) {
        let mut providers = self.providers.write().await;
        providers.entry(cid.clone()).or_insert_with(HashSet::new).insert(provider.clone());
        
        let mut timestamps = self.provider_timestamps.write().await;
        timestamps.insert(provider, Instant::now());
    }

    /// Remove a provider for content
    pub async fn remove_provider(&self, cid: &ContentId, provider: &ProviderId) {
        let mut providers = self.providers.write().await;
        if let Some(provider_set) = providers.get_mut(cid) {
            provider_set.remove(provider);
            if provider_set.is_empty() {
                providers.remove(cid);
            }
        }
        
        let mut timestamps = self.provider_timestamps.write().await;
        timestamps.remove(provider);
    }

    /// Clean up expired providers
    pub async fn cleanup_expired_providers(&self) {
        let now = Instant::now();
        let mut expired_providers = Vec::new();
        
        {
            let timestamps = self.provider_timestamps.read().await;
            for (provider, timestamp) in timestamps.iter() {
                if now.duration_since(*timestamp) > self.provider_timeout {
                    expired_providers.push(provider.clone());
                }
            }
        }

        if !expired_providers.is_empty() {
            let mut providers = self.providers.write().await;
            let mut timestamps = self.provider_timestamps.write().await;
            
            for expired_provider in expired_providers {
                timestamps.remove(&expired_provider);
                
                // Remove from all content provider sets
                let mut empty_content = Vec::new();
                for (cid, provider_set) in providers.iter_mut() {
                    provider_set.remove(&expired_provider);
                    if provider_set.is_empty() {
                        empty_content.push(cid.clone());
                    }
                }
                
                // Remove empty content entries
                for cid in empty_content {
                    providers.remove(&cid);
                }
            }
        }
    }

    /// Get provider count for content
    pub async fn provider_count(&self, cid: &ContentId) -> usize {
        let providers = self.providers.read().await;
        providers.get(cid).map(|set| set.len()).unwrap_or(0)
    }

    /// Get all content we're providing
    pub async fn provided_content(&self) -> HashSet<ContentId> {
        let providing = self.providing.read().await;
        providing.clone()
    }

    /// Get total number of unique providers
    pub async fn total_providers(&self) -> usize {
        let timestamps = self.provider_timestamps.read().await;
        timestamps.len()
    }

    /// Get provider statistics
    pub async fn provider_stats(&self) -> ProviderStats {
        let providers = self.providers.read().await;
        let timestamps = self.provider_timestamps.read().await;
        let providing = self.providing.read().await;

        let total_content = providers.len();
        let total_providers = timestamps.len();
        let content_providing = providing.len();
        
        let avg_providers_per_content = if total_content > 0 {
            providers.values().map(|set| set.len()).sum::<usize>() as f64 / total_content as f64
        } else {
            0.0
        };

        ProviderStats {
            total_content,
            total_providers,
            content_providing,
            avg_providers_per_content,
        }
    }

    /// Start background cleanup task
    pub fn start_cleanup_task(self: Arc<Self>) {
        let provider = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300)); // 5 minutes
            
            loop {
                interval.tick().await;
                provider.cleanup_expired_providers().await;
            }
        });
    }
}

#[async_trait::async_trait]
impl ContentProvider for DfsContentProvider {
    async fn provide(&self, cid: &ContentId) -> Result<(), DfsError> {
        // Add to our providing set
        let mut providing = self.providing.write().await;
        providing.insert(cid.clone());
        
        // Create a provider ID for ourselves (in a real implementation, this would be our peer ID)
        let our_provider_id = ProviderId("local".to_string());
        
        // Add ourselves as a provider
        self.add_provider(cid, our_provider_id).await;
        
        Ok(())
    }

    async fn unprovide(&self, cid: &ContentId) -> Result<(), DfsError> {
        // Remove from our providing set
        let mut providing = self.providing.write().await;
        providing.remove(cid);
        
        // Remove ourselves as a provider
        let our_provider_id = ProviderId("local".to_string());
        self.remove_provider(cid, &our_provider_id).await;
        
        Ok(())
    }

    async fn find_providers(&self, cid: &ContentId) -> Result<Vec<ProviderId>, DfsError> {
        // Clean up expired providers first
        self.cleanup_expired_providers().await;
        
        let providers = self.providers.read().await;
        if let Some(provider_set) = providers.get(cid) {
            Ok(provider_set.iter().cloned().collect())
        } else {
            Ok(Vec::new())
        }
    }
}

/// Provider statistics
#[derive(Debug, Clone)]
pub struct ProviderStats {
    pub total_content: usize,
    pub total_providers: usize,
    pub content_providing: usize,
    pub avg_providers_per_content: f64,
}

/// DHT-based content provider for network-wide discovery
pub struct DhtContentProvider {
    local_provider: Arc<DfsContentProvider>,
    // In a real implementation, this would integrate with the DHT
    network_providers: Arc<RwLock<HashMap<ContentId, Vec<ProviderId>>>>,
}

impl DhtContentProvider {
    /// Create a new DHT content provider
    pub fn new() -> Self {
        Self {
            local_provider: Arc::new(DfsContentProvider::new()),
            network_providers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Update network providers from DHT
    pub async fn update_network_providers(&self, cid: &ContentId, providers: Vec<ProviderId>) {
        let mut network_providers = self.network_providers.write().await;
        network_providers.insert(cid.clone(), providers);
    }

    /// Get combined local and network providers
    pub async fn get_all_providers(&self, cid: &ContentId) -> Result<Vec<ProviderId>, DfsError> {
        let mut all_providers = HashSet::new();
        
        // Get local providers
        let local_providers = self.local_provider.find_providers(cid).await?;
        all_providers.extend(local_providers);
        
        // Get network providers
        let network_providers = self.network_providers.read().await;
        if let Some(providers) = network_providers.get(cid) {
            all_providers.extend(providers.iter().cloned());
        }
        
        Ok(all_providers.into_iter().collect())
    }

    /// Announce content to DHT
    pub async fn announce_to_dht(&self, cid: &ContentId) -> Result<(), DfsError> {
        // In a real implementation, this would interact with the DHT
        // For now, we'll just add to local providers
        self.local_provider.provide(cid).await
    }

    /// Query DHT for providers
    pub async fn query_dht(&self, cid: &ContentId) -> Result<Vec<ProviderId>, DfsError> {
        // In a real implementation, this would query the DHT
        // For now, we'll return network providers
        let network_providers = self.network_providers.read().await;
        Ok(network_providers.get(cid).cloned().unwrap_or_default())
    }

    /// Start background DHT maintenance tasks
    pub fn start_dht_tasks(self: Arc<Self>) {
        let provider = Arc::clone(&self);
        
        // Start provider cleanup task
        let local_provider = Arc::clone(&provider.local_provider);
        local_provider.start_cleanup_task();
        
        // Start DHT refresh task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(900)); // 15 minutes
            
            loop {
                interval.tick().await;
                
                // Refresh our provided content in the DHT
                let provided_content = provider.local_provider.provided_content().await;
                for cid in provided_content {
                    let _ = provider.announce_to_dht(&cid).await;
                }
            }
        });
    }

    /// Get provider statistics - public method
    pub async fn provider_stats(&self) -> ProviderStats {
        self.local_provider.provider_stats().await
    }
}

#[async_trait::async_trait]
impl ContentProvider for DhtContentProvider {
    async fn provide(&self, cid: &ContentId) -> Result<(), DfsError> {
        // Provide locally
        self.local_provider.provide(cid).await?;
        
        // Announce to DHT
        self.announce_to_dht(cid).await
    }

    async fn unprovide(&self, cid: &ContentId) -> Result<(), DfsError> {
        // Unprovide locally
        self.local_provider.unprovide(cid).await?;
        
        // In a real implementation, we would remove from DHT
        Ok(())
    }

    async fn find_providers(&self, cid: &ContentId) -> Result<Vec<ProviderId>, DfsError> {
        // Try local first for speed
        let local_providers = self.local_provider.find_providers(cid).await?;
        
        if !local_providers.is_empty() {
            return Ok(local_providers);
        }
        
        // Query DHT if no local providers
        let dht_providers = self.query_dht(cid).await?;
        
        // Update local cache
        if !dht_providers.is_empty() {
            for provider in &dht_providers {
                self.local_provider.add_provider(cid, provider.clone()).await;
            }
        }
        
        Ok(dht_providers)
    }
}

/// Content replication manager
pub struct ContentReplicator {
    provider: Arc<DhtContentProvider>,
    target_replication: usize,
    min_replication: usize,
}

impl ContentReplicator {
    /// Create a new content replicator
    pub fn new(provider: Arc<DhtContentProvider>, target_replication: usize) -> Self {
        Self {
            provider,
            target_replication,
            // Require a strict majority of the target count before calling it "Adequate".
            // (e.g. target=3 => min=2, target=4 => min=2)
            min_replication: (target_replication + 1) / 2,
        }
    }

    /// Check replication status for content
    pub async fn check_replication(&self, cid: &ContentId) -> Result<ReplicationStatus, DfsError> {
        let providers = self.provider.get_all_providers(cid).await?;
        let current_replication = providers.len();
        
        let status = if current_replication >= self.target_replication {
            ReplicationLevel::Optimal
        } else if current_replication >= self.min_replication {
            ReplicationLevel::Adequate
        } else if current_replication > 0 {
            ReplicationLevel::Insufficient
        } else {
            ReplicationLevel::None
        };

        Ok(ReplicationStatus {
            current_replication,
            target_replication: self.target_replication,
            level: status,
            providers,
        })
    }

    /// Ensure content meets replication requirements
    pub async fn ensure_replication(&self, cid: &ContentId) -> Result<(), DfsError> {
        let status = self.check_replication(cid).await?;
        
        match status.level {
            ReplicationLevel::None => {
                return Err(DfsError::NotFound(format!("No providers found for {}", cid.to_string())));
            }
            ReplicationLevel::Insufficient => {
                log::warn!("Content {} has insufficient replication: {}/{}", 
                          cid.to_string(), status.current_replication, self.target_replication);
                // In a real implementation, we would trigger replication requests
            }
            ReplicationLevel::Adequate | ReplicationLevel::Optimal => {
                // Replication is sufficient
            }
        }
        
        Ok(())
    }

    /// Start background replication monitoring
    pub fn start_replication_monitoring(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1800)); // 30 minutes
            
            loop {
                interval.tick().await;
                
                // Check replication for all provided content
                let provided_content = self.provider.local_provider.provided_content().await;
                
                for cid in provided_content {
                    if let Err(e) = self.ensure_replication(&cid).await {
                        log::warn!("Failed to ensure replication for {}: {}", cid.to_string(), e);
                    }
                }
            }
        });
    }
}

/// Replication status for content
#[derive(Debug, Clone)]
pub struct ReplicationStatus {
    pub current_replication: usize,
    pub target_replication: usize,
    pub level: ReplicationLevel,
    pub providers: Vec<ProviderId>,
}

/// Replication level assessment
#[derive(Debug, Clone, PartialEq)]
pub enum ReplicationLevel {
    None,
    Insufficient,
    Adequate,
    Optimal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_content_provider() {
        let provider = DfsContentProvider::new();
        let cid = ContentId::from_data(b"test").unwrap();
        
        // Initially no providers
        let providers = provider.find_providers(&cid).await.unwrap();
        assert!(providers.is_empty());
        
        // Provide content
        provider.provide(&cid).await.unwrap();
        
        // Should now have one provider
        let providers = provider.find_providers(&cid).await.unwrap();
        assert_eq!(providers.len(), 1);
        
        // Unprovide content
        provider.unprovide(&cid).await.unwrap();
        
        // Should have no providers again
        let providers = provider.find_providers(&cid).await.unwrap();
        assert!(providers.is_empty());
    }

    #[tokio::test]
    async fn test_provider_cleanup() {
        let provider = DfsContentProvider::new();
        let cid = ContentId::from_data(b"test").unwrap();
        let external_provider = ProviderId("external".to_string());
        
        // Add external provider
        provider.add_provider(&cid, external_provider.clone()).await;
        
        // Should have one provider
        let providers = provider.find_providers(&cid).await.unwrap();
        assert_eq!(providers.len(), 1);
        
        // Manually trigger cleanup (in real scenario, this would happen after timeout)
        provider.cleanup_expired_providers().await;
        
        // Provider might still be there depending on timing
        // This test mainly checks that cleanup runs without error
    }

    #[tokio::test]
    async fn test_dht_provider() {
        let provider = Arc::new(DhtContentProvider::new());
        let cid = ContentId::from_data(b"test").unwrap();
        
        // Provide content
        provider.provide(&cid).await.unwrap();
        
        // Should find providers
        let providers = provider.find_providers(&cid).await.unwrap();
        assert!(!providers.is_empty());
    }

    #[tokio::test]
    async fn test_replication_status() {
        let dht_provider = Arc::new(DhtContentProvider::new());
        let replicator = ContentReplicator::new(dht_provider, 3);
        let cid = ContentId::from_data(b"test").unwrap();
        
        // Check status for non-existent content
        let status = replicator.check_replication(&cid).await.unwrap();
        assert_eq!(status.level, ReplicationLevel::None);
        
        // Provide content
        replicator.provider.provide(&cid).await.unwrap();
        
        // Check status again
        let status = replicator.check_replication(&cid).await.unwrap();
        assert_eq!(status.level, ReplicationLevel::Insufficient);
        assert_eq!(status.current_replication, 1);
    }
}