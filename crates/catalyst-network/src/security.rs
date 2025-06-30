use crate::{
    config::SecurityConfig,
    error::{NetworkError, NetworkResult},
    messaging::NetworkMessage,
};

use catalyst_utils::{
    logging::*,
    metrics::*,
};

use libp2p::PeerId;
use std::{
    collections::HashSet,
    net::IpAddr,
    time::Instant,
};
use tokio::sync::RwLock;

pub struct SecurityManager {
    config: SecurityConfig,
    blocked_peers: RwLock<HashSet<PeerId>>,
    blocked_ips: RwLock<HashSet<IpAddr>>,
    security_stats: RwLock<SecurityStats>,
}

#[derive(Debug, Default)]
pub struct SecurityStats {
    pub blocked_connections: u64,
    pub rejected_messages: u64,
    pub security_violations: u64,
    pub last_violation: Option<Instant>,
}

impl SecurityManager {
    pub fn new(config: &SecurityConfig) -> Self {
        Self {
            config: config.clone(),
            blocked_peers: RwLock::new(config.blocked_peers.iter().filter_map(|s| s.parse().ok()).collect()),
            blocked_ips: RwLock::new(config.blocked_ips.clone()),
            security_stats: RwLock::new(SecurityStats::default()),
        }
    }
    
    pub async fn validate_peer_connection(&self, peer_id: &PeerId, ip: &IpAddr) -> NetworkResult<()> {
        // Check if peer is blocked
        if self.blocked_peers.read().await.contains(peer_id) {
            let mut stats = self.security_stats.write().await;
            stats.blocked_connections += 1;
            stats.last_violation = Some(Instant::now());
            
            increment_counter!("network_security_blocked_connections_total", 1);
            
            return Err(NetworkError::SecurityFailed(
                format!("Peer {} is blocked", peer_id)
            ));
        }
        
        // Check if IP is blocked
        if self.blocked_ips.read().await.contains(ip) {
            let mut stats = self.security_stats.write().await;
            stats.blocked_connections += 1;
            stats.last_violation = Some(Instant::now());
            
            increment_counter!("network_security_blocked_ips_total", 1);
            
            return Err(NetworkError::SecurityFailed(
                format!("IP {} is blocked", ip)
            ));
        }
        
        // Check allowed peers (if whitelist is configured)
        if !self.config.allowed_peers.is_empty() {
            let peer_str = peer_id.to_string();
            if !self.config.allowed_peers.contains(&peer_str) {
                let mut stats = self.security_stats.write().await;
                stats.blocked_connections += 1;
                
                return Err(NetworkError::SecurityFailed(
                    format!("Peer {} not in allowlist", peer_id)
                ));
            }
        }
        
        Ok(())
    }
    
    pub async fn validate_message(&self, message: &NetworkMessage, sender: &PeerId) -> NetworkResult<()> {
        // Check message size
        if message.transport.size_bytes > self.config.max_message_size {
            let mut stats = self.security_stats.write().await;
            stats.rejected_messages += 1;
            stats.security_violations += 1;
            
            increment_counter!("network_security_oversized_messages_total", 1);
            
            return Err(NetworkError::SecurityFailed(
                format!("Message size {} exceeds limit {}", 
                    message.transport.size_bytes, 
                    self.config.max_message_size)
            ));
        }
        
        // Check if message is too old
        let message_age = message.age();
        if message_age > std::time::Duration::from_secs(300) { // 5 minutes
            let mut stats = self.security_stats.write().await;
            stats.rejected_messages += 1;
            
            return Err(NetworkError::SecurityFailed(
                "Message too old".to_string()
            ));
        }
        
        // Check sender reputation (placeholder)
        if self.is_sender_suspicious(sender).await {
            let mut stats = self.security_stats.write().await;
            stats.rejected_messages += 1;
            stats.security_violations += 1;
            
            return Err(NetworkError::SecurityFailed(
                format!("Sender {} has low trust score", sender)
            ));
        }
        
        Ok(())
    }
    
    async fn is_sender_suspicious(&self, _peer_id: &PeerId) -> bool {
        // Placeholder implementation
        false
    }
    
    pub async fn block_peer(&self, peer_id: PeerId) {
        self.blocked_peers.write().await.insert(peer_id);
        log_warn!(LogCategory::Network, "Blocked peer {}", peer_id);
        increment_counter!("network_security_peers_blocked_total", 1);
    }
    
    pub async fn block_ip(&self, ip: IpAddr) {
        self.blocked_ips.write().await.insert(ip);
        log_warn!(LogCategory::Network, "Blocked IP {}", ip);
        increment_counter!("network_security_ips_blocked_total", 1);
    }
    
    pub async fn unblock_peer(&self, peer_id: &PeerId) {
        if self.blocked_peers.write().await.remove(peer_id) {
            log_info!(LogCategory::Network, "Unblocked peer {}", peer_id);
        }
    }
    
    pub async fn get_stats(&self) -> SecurityStats {
        self.security_stats.read().await.clone()
    }
}