//! Simple bandwidth management without governor dependency

use crate::config::BandwidthConfig;
use std::time::Instant;
use tokio::sync::RwLock;

pub struct BandwidthManager {
    config: BandwidthConfig,
    usage_stats: RwLock<BandwidthStats>,
}

#[derive(Debug, Default, Clone)]
pub struct BandwidthStats {
    pub total_uploaded: u64,
    pub total_downloaded: u64,
    pub upload_rate: f64,      // bytes/sec
    pub download_rate: f64,    // bytes/sec
    pub last_update: Instant,
    pub peak_upload_rate: f64,
    pub peak_download_rate: f64,
}

impl BandwidthManager {
    pub fn new(config: &BandwidthConfig) -> Self {
        Self {
            config: config.clone(),
            usage_stats: RwLock::new(BandwidthStats {
                last_update: Instant::now(),
                ..Default::default()
            }),
        }
    }
    
    pub async fn can_send(&self, _bytes: usize) -> bool {
        // Simple implementation - always allow for now
        true
    }
    
    pub async fn can_receive(&self, _bytes: usize) -> bool {
        // Simple implementation - always allow for now
        true
    }
    
    pub async fn record_upload(&self, bytes: usize) {
        let mut stats = self.usage_stats.write().await;
        stats.total_uploaded += bytes as u64;
        log::trace!("Recorded upload: {} bytes", bytes);
    }
    
    pub async fn record_download(&self, bytes: usize) {
        let mut stats = self.usage_stats.write().await;
        stats.total_downloaded += bytes as u64;
        log::trace!("Recorded download: {} bytes", bytes);
    }
    
    pub async fn update_rates(&self) {
        let mut stats = self.usage_stats.write().await;
        let now = Instant::now();
        let elapsed = now.duration_since(stats.last_update).as_secs_f64();
        
        if elapsed > 0.0 {
            // Simple rate calculation - would be more sophisticated in real implementation
            stats.upload_rate = 0.0;
            stats.download_rate = 0.0;
            stats.last_update = now;
        }
    }
    
    pub async fn get_usage(&self) -> BandwidthUsage {
        let stats = self.usage_stats.read().await;
        BandwidthUsage {
            upload_rate: stats.upload_rate,
            download_rate: stats.download_rate,
            total_uploaded: stats.total_uploaded,
            total_downloaded: stats.total_downloaded,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BandwidthUsage {
    pub upload_rate: f64,
    pub download_rate: f64,
    pub total_uploaded: u64,
    pub total_downloaded: u64,
}