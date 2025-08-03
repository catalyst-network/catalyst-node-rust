//! RPC server implementation details
//! Create this as crates/catalyst-rpc/src/server.rs

use crate::{CatalystRpcError, RpcConfig};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// RPC server statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcServerStats {
    pub active_connections: u32,
    pub total_requests: u64,
    pub total_responses: u64,
    pub errors: u64,
    pub uptime_seconds: u64,
    pub started_at: u64,
}

impl Default for RpcServerStats {
    fn default() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        Self {
            active_connections: 0,
            total_requests: 0,
            total_responses: 0,
            errors: 0,
            uptime_seconds: 0,
            started_at: now,
        }
    }
}

/// Server metrics collector
pub struct ServerMetrics {
    stats: Arc<RwLock<RpcServerStats>>,
}

impl ServerMetrics {
    pub fn new() -> Self {
        Self {
            stats: Arc::new(RwLock::new(RpcServerStats::default())),
        }
    }
    
    pub async fn increment_requests(&self) {
        let mut stats = self.stats.write().await;
        stats.total_requests += 1;
    }
    
    pub async fn increment_responses(&self) {
        let mut stats = self.stats.write().await;
        stats.total_responses += 1;
    }
    
    pub async fn increment_errors(&self) {
        let mut stats = self.stats.write().await;
        stats.errors += 1;
    }
    
    pub async fn update_connections(&self, count: u32) {
        let mut stats = self.stats.write().await;
        stats.active_connections = count;
    }
    
    pub async fn get_stats(&self) -> RpcServerStats {
        let mut stats = self.stats.read().await.clone();
        
        // Update uptime
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        stats.uptime_seconds = now.saturating_sub(stats.started_at);
        
        stats
    }
}

impl Default for ServerMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// CORS middleware configuration
pub struct CorsConfig {
    pub enabled: bool,
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
}

impl From<&RpcConfig> for CorsConfig {
    fn from(config: &RpcConfig) -> Self {
        Self {
            enabled: config.cors_enabled,
            allowed_origins: config.cors_origins.clone(),
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "OPTIONS".to_string(),
            ],
            allowed_headers: vec![
                "Content-Type".to_string(),
                "Authorization".to_string(),
                "X-Requested-With".to_string(),
            ],
        }
    }
}

/// Rate limiter for RPC requests
pub struct RateLimiter {
    requests_per_minute: u32,
    window_start: Arc<RwLock<std::time::Instant>>,
    request_count: Arc<RwLock<u32>>,
}

impl RateLimiter {
    pub fn new(requests_per_minute: u32) -> Self {
        Self {
            requests_per_minute,
            window_start: Arc::new(RwLock::new(std::time::Instant::now())),
            request_count: Arc::new(RwLock::new(0)),
        }
    }
    
    pub async fn check_rate_limit(&self) -> Result<(), CatalystRpcError> {
        let now = std::time::Instant::now();
        let mut window_start = self.window_start.write().await;
        let mut request_count = self.request_count.write().await;
        
        // Reset window if a minute has passed
        if now.duration_since(*window_start).as_secs() >= 60 {
            *window_start = now;
            *request_count = 0;
        }
        
        // Check if we've exceeded the limit
        if *request_count >= self.requests_per_minute {
            return Err(CatalystRpcError::Server(
                "Rate limit exceeded. Please try again later.".to_string(),
            ));
        }
        
        *request_count += 1;
        Ok(())
    }
}

/// Authentication middleware
pub struct AuthMiddleware {
    api_key: Option<String>,
    enabled: bool,
}

impl AuthMiddleware {
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            enabled: api_key.is_some(),
            api_key,
        }
    }
    
    pub fn validate_request(&self, auth_header: Option<&str>) -> Result<(), CatalystRpcError> {
        if !self.enabled {
            return Ok(());
        }
        
        let provided_key = auth_header
            .and_then(|header| header.strip_prefix("Bearer "))
            .ok_or_else(|| CatalystRpcError::InvalidParams("Missing or invalid authorization header".to_string()))?;
        
        match &self.api_key {
            Some(expected_key) if provided_key == expected_key => Ok(()),
            _ => Err(CatalystRpcError::InvalidParams("Invalid API key".to_string())),
        }
    }
}

/// Health check endpoint data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub status: String,
    pub timestamp: u64,
    pub uptime_seconds: u64,
    pub version: String,
    pub rpc_methods_available: usize,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            status: "healthy".to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            uptime_seconds: 0,
            version: "1.0.0".to_string(),
            rpc_methods_available: 13, // Number of methods in CatalystRpc trait
        }
    }
}