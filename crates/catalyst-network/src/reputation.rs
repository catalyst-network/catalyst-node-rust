use crate::{
    config::ReputationConfig,
    error::NetworkResult,
};

use catalyst_utils::{
    logging::*,
    metrics::*,
};

use libp2p::PeerId;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

pub struct ReputationManager {
    config: ReputationConfig,
    peer_scores: RwLock<HashMap<PeerId, PeerReputation>>,
    stats: RwLock<ReputationStats>,
}

#[derive(Debug, Clone)]
pub struct PeerReputation {
    pub peer_id: PeerId,
    pub score: f64,
    pub last_updated: Instant,
    pub successful_interactions: u64,
    pub failed_interactions: u64,
    pub connection_quality: f64,
    pub message_quality: f64,
}

#[derive(Debug, Default)]
pub struct ReputationStats {
    pub total_peers_tracked: usize,
    pub high_reputation_peers: usize,
    pub low_reputation_peers: usize,
    pub average_score: f64,
    pub score_updates: u64,
}

impl ReputationManager {
    pub fn new(config: &ReputationConfig) -> Self {
        Self {
            config: config.clone(),
            peer_scores: RwLock::new(HashMap::new()),
            stats: RwLock::new(ReputationStats::default()),
        }
    }
    
    pub async fn get_peer_score(&self, peer_id: &PeerId) -> f64 {
        self.peer_scores
            .read()
            .await
            .get(peer_id)
            .map(|rep| rep.score)
            .unwrap_or(self.config.initial_reputation)
    }
    
    pub async fn update_peer_score(&self, peer_id: PeerId, adjustment: f64, reason: &str) {
        let mut scores = self.peer_scores.write().await;
        let reputation = scores.entry(peer_id).or_insert_with(|| PeerReputation {
            peer_id,
            score: self.config.initial_reputation,
            last_updated: Instant::now(),
            successful_interactions: 0,
            failed_interactions: 0,
            connection_quality: 50.0,
            message_quality: 50.0,
        });
        
        reputation.score = (reputation.score + adjustment)
            .clamp(0.0, self.config.max_reputation);
        reputation.last_updated = Instant::now();
        
        if adjustment > 0.0 {
            reputation.successful_interactions += 1;
        } else {
            reputation.failed_interactions += 1;
        }
        
        log_debug!(
            LogCategory::Network,
            "Updated reputation for {}: {:.2} ({})",
            peer_id,
            reputation.score,
            reason
        );
        
        // Update stats
        let mut stats = self.stats.write().await;
        stats.score_updates += 1;
    }
    
    pub async fn should_connect_to_peer(&self, peer_id: &PeerId) -> bool {
        let score = self.get_peer_score(peer_id).await;
        score >= self.config.min_reputation
    }
    
    pub async fn update_scores(&self) -> NetworkResult<()> {
        if !self.config.enable_reputation {
            return Ok(());
        }
        
        let mut scores = self.peer_scores.write().await;
        let now = Instant::now();
        
        for reputation in scores.values_mut() {
            // Apply decay
            let time_since_update = now.duration_since(reputation.last_updated);
            let decay_factor = (time_since_update.as_secs_f64() / 3600.0) * self.config.decay_rate;
            
            reputation.score = (reputation.score - decay_factor)
                .max(0.0);
            
            if time_since_update > Duration::from_secs(3600) {
                reputation.last_updated = now;
            }
        }
        
        self.update_reputation_stats().await;
        Ok(())
    }
    
    async fn update_reputation_stats(&self) {
        let scores = self.peer_scores.read().await;
        let mut stats = self.stats.write().await;
        
        stats.total_peers_tracked = scores.len();
        stats.high_reputation_peers = scores.values()
            .filter(|rep| rep.score >= 75.0)
            .count();
        stats.low_reputation_peers = scores.values()
            .filter(|rep| rep.score <= 25.0)
            .count();
        
        stats.average_score = if !scores.is_empty() {
            scores.values().map(|rep| rep.score).sum::<f64>() / scores.len() as f64
        } else {
            0.0
        };
        
        set_gauge!("network_reputation_average_score", stats.average_score);
        set_gauge!("network_reputation_tracked_peers", stats.total_peers_tracked as f64);
    }
}