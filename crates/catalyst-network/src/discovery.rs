//! Peer discovery service

use crate::{
    config::DiscoveryConfig,
    error::{NetworkError, NetworkResult},
};

use catalyst_utils::{
    logging::*,
    metrics::*,
};

use libp2p::{PeerId, Multiaddr};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// Peer discovery service
pub struct PeerDiscovery {
    /// Configuration
    config: DiscoveryConfig,
    
    /// Discovered peers
    discovered_peers: RwLock<HashMap<PeerId, PeerInfo>>,
    
    /// Discovery statistics
    stats: RwLock<DiscoveryStats>,
    
    /// Bootstrap state
    bootstrap_state: RwLock<BootstrapState>,
}

/// Information about a discovered peer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Peer ID
    pub peer_id: PeerId,
    
    /// Known addresses
    pub addresses: HashSet<Multiaddr>,
    
    /// Discovery method
    pub discovery_method: DiscoveryMethod,
    
    /// First discovered time
    pub discovered_at: Instant,
    
    /// Last seen time
    pub last_seen: Instant,
    
    /// Connection attempts
    pub connection_attempts: u32,
    
    /// Successful connections
    pub successful_connections: u32,
    
    /// Peer score (reliability)
    pub score: f64,
}

/// Discovery methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscoveryMethod {
    /// mDNS local discovery
    Mdns,
    
    /// Kademlia DHT
    Kademlia,
    
    /// Bootstrap peer
    Bootstrap,
    
    /// Manual addition
    Manual,
    
    /// Gossip protocol
    Gossip,
    
    /// Peer exchange
    PeerExchange,
}

/// Discovery events
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    /// New peer discovered
    PeerDiscovered {
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
        method: DiscoveryMethod,
    },
    
    /// Peer address updated
    PeerAddressUpdated {
        peer_id: PeerId,
        old_addresses: Vec<Multiaddr>,
        new_addresses: Vec<Multiaddr>,
    },
    
    /// Peer expired (not seen for too long)
    PeerExpired {
        peer_id: PeerId,
        last_seen: Instant,
    },
    
    /// Bootstrap completed
    BootstrapCompleted {
        discovered_peers: usize,
        duration: Duration,
    },
    
    /// Discovery error
    DiscoveryError {
        method: DiscoveryMethod,
        error: String,
    },
}

/// Discovery statistics
#[derive(Debug, Default)]
pub struct DiscoveryStats {
    pub total_discovered: u64,
    pub mdns_discoveries: u64,
    pub kademlia_discoveries: u64,
    pub bootstrap_discoveries: u64,
    pub manual_discoveries: u64,
    pub expired_peers: u64,
    pub discovery_errors: u64,
    pub last_discovery: Option<Instant>,
    pub average_discovery_time: Duration,
}

/// Bootstrap state
#[derive(Debug, Clone)]
pub struct BootstrapState {
    pub is_bootstrapping: bool,
    pub bootstrap_start: Option<Instant>,
    pub bootstrap_attempts: u32,
    pub last_bootstrap: Option<Instant>,
    pub bootstrap_peers_found: u32,
}

impl Default for BootstrapState {
    fn default() -> Self {
        Self {
            is_bootstrapping: false,
            bootstrap_start: None,
            bootstrap_attempts: 0,
            last_bootstrap: None,
            bootstrap_peers_found: 0,
        }
    }
}

impl PeerDiscovery {
    /// Create new peer discovery service
    pub fn new(config: &DiscoveryConfig) -> Self {
        Self {
            config: config.clone(),
            discovered_peers: RwLock::new(HashMap::new()),
            stats: RwLock::new(DiscoveryStats::default()),
            bootstrap_state: RwLock::new(BootstrapState::default()),
        }
    }
    
    /// Discover peers using configured methods
    pub async fn discover_peers(&self) -> NetworkResult<Vec<DiscoveryEvent>> {
        log_debug!(LogCategory::Network, "Starting peer discovery");
        
        let mut events = Vec::new();
        
        // Check if we need to bootstrap
        if self.should_bootstrap().await {
            if let Ok(bootstrap_events) = self.bootstrap().await {
                events.extend(bootstrap_events);
            }
        }
        
        // Clean up expired peers
        events.extend(self.cleanup_expired_peers().await);
        
        // Update metrics
        self.update_discovery_metrics().await;
        
        Ok(events)
    }
    
    /// Add discovered peer
    pub async fn add_peer(
        &self,
        peer_id: PeerId,
        addresses: Vec<Multiaddr>,
        method: DiscoveryMethod,
    ) -> DiscoveryEvent {
        log_debug!(
            LogCategory::Network,
            "Adding discovered peer {} via {:?}",
            peer_id,
            method
        );
        
        let now = Instant::now();
        let mut peers = self.discovered_peers.write().await;
        let mut stats = self.stats.write().await;
        
        if let Some(existing_peer) = peers.get_mut(&peer_id) {
            // Update existing peer
            let old_addresses: Vec<_> = existing_peer.addresses.iter().cloned().collect();
            let new_addresses: HashSet<_> = addresses.into_iter().collect();
            
            if existing_peer.addresses != new_addresses {
                existing_peer.addresses = new_addresses.clone();
                existing_peer.last_seen = now;
                
                log_debug!(
                    LogCategory::Network,
                    "Updated addresses for peer {}",
                    peer_id
                );
                
                return DiscoveryEvent::PeerAddressUpdated {
                    peer_id,
                    old_addresses,
                    new_addresses: new_addresses.into_iter().collect(),
                };
            } else {
                existing_peer.last_seen = now;
                return DiscoveryEvent::PeerDiscovered {
                    peer_id,
                    addresses: Vec::new(), // No new addresses
                    method,
                };
            }
        } else {
            // New peer
            let peer_info = PeerInfo {
                peer_id,
                addresses: addresses.iter().cloned().collect(),
                discovery_method: method.clone(),
                discovered_at: now,
                last_seen: now,
                connection_attempts: 0,
                successful_connections: 0,
                score: 50.0, // Initial neutral score
            };
            
            peers.insert(peer_id, peer_info);
            stats.total_discovered += 1;
            stats.last_discovery = Some(now);
            
            // Update method-specific stats
            match method {
                DiscoveryMethod::Mdns => stats.mdns_discoveries += 1,
                DiscoveryMethod::Kademlia => stats.kademlia_discoveries += 1,
                DiscoveryMethod::Bootstrap => stats.bootstrap_discoveries += 1,
                DiscoveryMethod::Manual => stats.manual_discoveries += 1,
                _ => {}
            }
            
            log_info!(
                LogCategory::Network,
                "Discovered new peer {} via {:?}",
                peer_id,
                method
            );
            
            increment_counter!("network_peers_discovered_total", 1);
            
            DiscoveryEvent::PeerDiscovered {
                peer_id,
                addresses,
                method,
            }
        }
    }
    
    /// Get all discovered peers
    pub async fn get_peers(&self) -> HashMap<PeerId, PeerInfo> {
        self.discovered_peers.read().await.clone()
    }
    
    /// Get peer info by ID
    pub async fn get_peer(&self, peer_id: &PeerId) -> Option<PeerInfo> {
        self.discovered_peers.read().await.get(peer_id).cloned()
    }
    
    /// Remove peer
    pub async fn remove_peer(&self, peer_id: &PeerId) -> bool {
        let removed = self.discovered_peers.write().await.remove(peer_id).is_some();
        
        if removed {
            log_debug!(LogCategory::Network, "Removed peer {}", peer_id);
        }
        
        removed
    }
    
    /// Update peer score
    pub async fn update_peer_score(&self, peer_id: &PeerId, score_delta: f64) {
        if let Some(peer_info) = self.discovered_peers.write().await.get_mut(peer_id) {
            peer_info.score = (peer_info.score + score_delta).clamp(0.0, 100.0);
            peer_info.last_seen = Instant::now();
            
            log_debug!(
                LogCategory::Network,
                "Updated peer {} score by {:.2} to {:.2}",
                peer_id,
                score_delta,
                peer_info.score
            );
        }
    }
    
    /// Record connection attempt
    pub async fn record_connection_attempt(&self, peer_id: &PeerId, successful: bool) {
        if let Some(peer_info) = self.discovered_peers.write().await.get_mut(peer_id) {
            peer_info.connection_attempts += 1;
            peer_info.last_seen = Instant::now();
            
            if successful {
                peer_info.successful_connections += 1;
                peer_info.score = (peer_info.score + 5.0).min(100.0); // Boost score for successful connections
            } else {
                peer_info.score = (peer_info.score - 2.0).max(0.0); // Reduce score for failed connections
            }
            
            log_debug!(
                LogCategory::Network,
                "Recorded connection attempt for peer {}: success={}, score={:.2}",
                peer_id,
                successful,
                peer_info.score
            );
        }
    }
    
    /// Get best peers for connection
    pub async fn get_best_peers(&self, limit: usize) -> Vec<PeerInfo> {
        let peers = self.discovered_peers.read().await;
        let mut peer_list: Vec<_> = peers.values().cloned().collect();
        
        // Sort by score (descending) and last seen (most recent first)
        peer_list.sort_by(|a, b| {
            b.score.partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.last_seen.cmp(&a.last_seen))
        });
        
        peer_list.into_iter().take(limit).collect()
    }
    
    /// Check if bootstrap is needed
    async fn should_bootstrap(&self) -> bool {
        let bootstrap_state = self.bootstrap_state.read().await;
        let peers = self.discovered_peers.read().await;
        
        // Bootstrap if:
        // 1. Never bootstrapped before
        // 2. Too few peers discovered
        // 3. Bootstrap interval exceeded
        let never_bootstrapped = bootstrap_state.last_bootstrap.is_none();
        let too_few_peers = peers.len() < 5; // Minimum threshold
        let interval_exceeded = bootstrap_state.last_bootstrap
            .map(|last| last.elapsed() > self.config.bootstrap_interval)
            .unwrap_or(true);
        
        !bootstrap_state.is_bootstrapping && 
        (never_bootstrapped || too_few_peers || interval_exceeded)
    }
    
    /// Bootstrap discovery
    async fn bootstrap(&self) -> NetworkResult<Vec<DiscoveryEvent>> {
        log_info!(LogCategory::Network, "Starting peer discovery bootstrap");
        
        let mut bootstrap_state = self.bootstrap_state.write().await;
        bootstrap_state.is_bootstrapping = true;
        bootstrap_state.bootstrap_start = Some(Instant::now());
        bootstrap_state.bootstrap_attempts += 1;
        
        drop(bootstrap_state); // Release lock
        
        let start_time = Instant::now();
        let mut events = Vec::new();
        let initial_peer_count = self.discovered_peers.read().await.len();
        
        // Simulate bootstrap discovery (in real implementation, this would trigger actual discovery)
        // For now, we'll just log the bootstrap attempt
        log_debug!(LogCategory::Network, "Bootstrap discovery initiated");
        
        // Wait a short time to simulate bootstrap process
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let final_peer_count = self.discovered_peers.read().await.len();
        let discovered_count = final_peer_count.saturating_sub(initial_peer_count);
        let duration = start_time.elapsed();
        
        // Update bootstrap state
        let mut bootstrap_state = self.bootstrap_state.write().await;
        bootstrap_state.is_bootstrapping = false;
        bootstrap_state.last_bootstrap = Some(Instant::now());
        bootstrap_state.bootstrap_peers_found += discovered_count as u32;
        
        events.push(DiscoveryEvent::BootstrapCompleted {
            discovered_peers: discovered_count,
            duration,
        });
        
        log_info!(
            LogCategory::Network,
            "Bootstrap completed: discovered {} peers in {:?}",
            discovered_count,
            duration
        );
        
        increment_counter!("network_bootstrap_attempts_total", 1);
        observe_histogram!("network_bootstrap_duration_seconds", duration.as_secs_f64());
        
        Ok(events)
    }
    
    /// Clean up expired peers
    async fn cleanup_expired_peers(&self) -> Vec<DiscoveryEvent> {
        let expiry_threshold = Duration::from_secs(600); // 10 minutes
        let now = Instant::now();
        let mut events = Vec::new();
        
        let mut peers = self.discovered_peers.write().await;
        let mut stats = self.stats.write().await;
        
        let expired_peers: Vec<_> = peers
            .iter()
            .filter(|(_, info)| now.duration_since(info.last_seen) > expiry_threshold)
            .map(|(peer_id, info)| (*peer_id, info.last_seen))
            .collect();
        
        for (peer_id, last_seen) in expired_peers {
            peers.remove(&peer_id);
            stats.expired_peers += 1;
            
            events.push(DiscoveryEvent::PeerExpired { peer_id, last_seen });
            
            log_debug!(LogCategory::Network, "Expired peer {}", peer_id);
        }
        
        if !events.is_empty() {
            increment_counter!("network_peers_expired_total", events.len() as f64);
        }
        
        events
    }
    
    /// Update discovery metrics
    async fn update_discovery_metrics(&self) {
        let peers = self.discovered_peers.read().await;
        let stats = self.stats.read().await;
        
        set_gauge!("network_discovered_peers_total", peers.len() as f64);
        set_gauge!("network_discovery_mdns_total", stats.mdns_discoveries as f64);
        set_gauge!("network_discovery_kademlia_total", stats.kademlia_discoveries as f64);
        set_gauge!("network_discovery_bootstrap_total", stats.bootstrap_discoveries as f64);
        set_gauge!("network_discovery_errors_total", stats.discovery_errors as f64);
    }
    
    /// Get discovery statistics
    pub async fn get_stats(&self) -> DiscoveryStats {
        self.stats.read().await.clone()
    }
    
    /// Get bootstrap state
    pub async fn get_bootstrap_state(&self) -> BootstrapState {
        self.bootstrap_state.read().await.clone()
    }
    
    /// Force bootstrap
    pub async fn force_bootstrap(&self) -> NetworkResult<Vec<DiscoveryEvent>> {
        log_info!(LogCategory::Network, "Forcing peer discovery bootstrap");
        
        // Reset bootstrap state
        {
            let mut bootstrap_state = self.bootstrap_state.write().await;
            bootstrap_state.last_bootstrap = None;
        }
        
        self.bootstrap().await
    }
    
    /// Clear all discovered peers
    pub async fn clear_peers(&self) {
        let mut peers = self.discovered_peers.write().await;
        let count = peers.len();
        peers.clear();
        
        log_info!(LogCategory::Network, "Cleared {} discovered peers", count);
    }
    
    /// Export peer list for persistence
    pub async fn export_peers(&self) -> Vec<(PeerId, Vec<Multiaddr>)> {
        let peers = self.discovered_peers.read().await;
        
        peers
            .values()
            .map(|info| (info.peer_id, info.addresses.iter().cloned().collect()))
            .collect()
    }
    
    /// Import peer list from persistence
    pub async fn import_peers(&self, peer_list: Vec<(PeerId, Vec<Multiaddr>)>) -> usize {
        let mut imported = 0;
        
        for (peer_id, addresses) in peer_list {
            self.add_peer(peer_id, addresses, DiscoveryMethod::Manual).await;
            imported += 1;
        }
        
        log_info!(LogCategory::Network, "Imported {} peers", imported);
        imported
    }
    
    /// Get peers by discovery method
    pub async fn get_peers_by_method(&self, method: &DiscoveryMethod) -> Vec<PeerInfo> {
        let peers = self.discovered_peers.read().await;
        
        peers
            .values()
            .filter(|info| std::mem::discriminant(&info.discovery_method) == std::mem::discriminant(method))
            .cloned()
            .collect()
    }
    
    /// Get peer addresses for connection
    pub async fn get_peer_addresses(&self, peer_id: &PeerId) -> Vec<Multiaddr> {
        self.discovered_peers
            .read()
            .await
            .get(peer_id)
            .map(|info| info.addresses.iter().cloned().collect())
            .unwrap_or_default()
    }
}

impl std::fmt::Display for DiscoveryMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscoveryMethod::Mdns => write!(f, "mDNS"),
            DiscoveryMethod::Kademlia => write!(f, "Kademlia"),
            DiscoveryMethod::Bootstrap => write!(f, "Bootstrap"),
            DiscoveryMethod::Manual => write!(f, "Manual"),
            DiscoveryMethod::Gossip => write!(f, "Gossip"),
            DiscoveryMethod::PeerExchange => write!(f, "PeerExchange"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libp2p::PeerId;
    
    #[tokio::test]
    async fn test_peer_discovery_creation() {
        let config = DiscoveryConfig::default();
        let discovery = PeerDiscovery::new(&config);
        
        let peers = discovery.get_peers().await;
        assert!(peers.is_empty());
    }
    
    #[tokio::test]
    async fn test_add_peer() {
        let config = DiscoveryConfig::default();
        let discovery = PeerDiscovery::new(&config);
        
        let peer_id = PeerId::random();
        let addresses = vec!["/ip4/127.0.0.1/tcp/8080".parse().unwrap()];
        
        let event = discovery.add_peer(
            peer_id,
            addresses.clone(),
            DiscoveryMethod::Manual,
        ).await;
        
        match event {
            DiscoveryEvent::PeerDiscovered { peer_id: discovered_id, .. } => {
                assert_eq!(discovered_id, peer_id);
            }
            _ => panic!("Expected PeerDiscovered event"),
        }
        
        let peers = discovery.get_peers().await;
        assert_eq!(peers.len(), 1);
        assert!(peers.contains_key(&peer_id));
    }
    
    #[tokio::test]
    async fn test_peer_scoring() {
        let config = DiscoveryConfig::default();
        let discovery = PeerDiscovery::new(&config);
        
        let peer_id = PeerId::random();
        let addresses = vec!["/ip4/127.0.0.1/tcp/8080".parse().unwrap()];
        
        discovery.add_peer(peer_id, addresses, DiscoveryMethod::Manual).await;
        
        // Initial score should be 50.0
        let peer_info = discovery.get_peer(&peer_id).await.unwrap();
        assert_eq!(peer_info.score, 50.0);
        
        // Update score
        discovery.update_peer_score(&peer_id, 10.0).await;
        
        let peer_info = discovery.get_peer(&peer_id).await.unwrap();
        assert_eq!(peer_info.score, 60.0);
        
        // Score should be clamped
        discovery.update_peer_score(&peer_id, 50.0).await;
        let peer_info = discovery.get_peer(&peer_id).await.unwrap();
        assert_eq!(peer_info.score, 100.0);
    }
    
    #[tokio::test]
    async fn test_connection_tracking() {
        let config = DiscoveryConfig::default();
        let discovery = PeerDiscovery::new(&config);
        
        let peer_id = PeerId::random();
        let addresses = vec!["/ip4/127.0.0.1/tcp/8080".parse().unwrap()];
        
        discovery.add_peer(peer_id, addresses, DiscoveryMethod::Manual).await;
        
        // Record successful connection
        discovery.record_connection_attempt(&peer_id, true).await;
        
        let peer_info = discovery.get_peer(&peer_id).await.unwrap();
        assert_eq!(peer_info.connection_attempts, 1);
        assert_eq!(peer_info.successful_connections, 1);
        assert!(peer_info.score > 50.0); // Score should increase
        
        // Record failed connection
        discovery.record_connection_attempt(&peer_id, false).await;
        
        let peer_info = discovery.get_peer(&peer_id).await.unwrap();
        assert_eq!(peer_info.connection_attempts, 2);
        assert_eq!(peer_info.successful_connections, 1);
    }
    
    #[tokio::test]
    async fn test_best_peers_selection() {
        let config = DiscoveryConfig::default();
        let discovery = PeerDiscovery::new(&config);
        
        // Add multiple peers with different scores
        for i in 0..5 {
            let peer_id = PeerId::random();
            let addresses = vec![format!("/ip4/127.0.0.1/tcp/{}", 8080 + i).parse().unwrap()];
            
            discovery.add_peer(peer_id, addresses, DiscoveryMethod::Manual).await;
            discovery.update_peer_score(&peer_id, i as f64 * 10.0).await;
        }
        
        let best_peers = discovery.get_best_peers(3).await;
        assert_eq!(best_peers.len(), 3);
        
        // Should be sorted by score (highest first)
        for i in 1..best_peers.len() {
            assert!(best_peers[i-1].score >= best_peers[i].score);
        }
    }
    
    #[tokio::test]
    async fn test_peer_expiry() {
        let config = DiscoveryConfig::default();
        let discovery = PeerDiscovery::new(&config);
        
        let peer_id = PeerId::random();
        let addresses = vec!["/ip4/127.0.0.1/tcp/8080".parse().unwrap()];
        
        discovery.add_peer(peer_id, addresses, DiscoveryMethod::Manual).await;
        
        // Manually set last_seen to old time
        {
            let mut peers = discovery.discovered_peers.write().await;
            if let Some(peer_info) = peers.get_mut(&peer_id) {
                peer_info.last_seen = Instant::now() - Duration::from_secs(700); // 11+ minutes ago
            }
        }
        
        let events = discovery.cleanup_expired_peers().await;
        assert_eq!(events.len(), 1);
        
        match &events[0] {
            DiscoveryEvent::PeerExpired { peer_id: expired_id, .. } => {
                assert_eq!(*expired_id, peer_id);
            }
            _ => panic!("Expected PeerExpired event"),
        }
        
        let peers = discovery.get_peers().await;
        assert!(peers.is_empty());
    }
}