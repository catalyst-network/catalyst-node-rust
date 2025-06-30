//! Configuration for the Catalyst Service Bus

use catalyst_utils::{CatalystResult, CatalystError};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Service Bus configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceBusConfig {
    /// Host to bind the server to
    pub host: String,
    
    /// Port to bind the server to
    pub port: u16,
    
    /// Maximum number of concurrent WebSocket connections
    pub max_connections: usize,
    
    /// WebSocket message buffer size
    pub message_buffer_size: usize,
    
    /// Rate limiting configuration
    pub rate_limit: RateLimitConfig,
    
    /// Authentication configuration
    pub auth: AuthConfig,
    
    /// Event filtering configuration
    pub event_filter: EventFilterConfig,
    
    /// REST API configuration
    pub rest_api: RestApiConfig,
    
    /// WebSocket configuration
    pub websocket: WebSocketConfig,
    
    /// CORS configuration
    pub cors: CorsConfig,
    
    /// TLS configuration (optional)
    pub tls: Option<TlsConfig>,
}

impl Default for ServiceBusConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            max_connections: 1000,
            message_buffer_size: 1024,
            rate_limit: RateLimitConfig::default(),
            auth: AuthConfig::default(),
            event_filter: EventFilterConfig::default(),
            rest_api: RestApiConfig::default(),
            websocket: WebSocketConfig::default(),
            cors: CorsConfig::default(),
            tls: None,
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Requests per second per client
    pub requests_per_second: u32,
    
    /// Burst size (max requests in burst)
    pub burst_size: u32,
    
    /// Enable rate limiting
    pub enabled: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 100,
            burst_size: 10,
            enabled: true,
        }
    }
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Enable authentication
    pub enabled: bool,
    
    /// JWT secret key
    pub jwt_secret: String,
    
    /// Token expiration time
    pub token_expiry: Duration,
    
    /// API key authentication
    pub api_keys: Vec<String>,
    
    /// Allow anonymous connections (read-only)
    pub allow_anonymous: bool,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            jwt_secret: "catalyst-service-bus-secret".to_string(),
            token_expiry: Duration::from_secs(3600), // 1 hour
            api_keys: vec![],
            allow_anonymous: true,
        }
    }
}

/// Event filtering configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilterConfig {
    /// Maximum number of filters per connection
    pub max_filters_per_connection: usize,
    
    /// Enable event replay from history
    pub enable_replay: bool,
    
    /// Maximum replay history duration
    pub max_replay_duration: Duration,
    
    /// Event buffer size for replay
    pub replay_buffer_size: usize,
    
    /// Enable event persistence
    pub persist_events: bool,
}

impl Default for EventFilterConfig {
    fn default() -> Self {
        Self {
            max_filters_per_connection: 50,
            enable_replay: true,
            max_replay_duration: Duration::from_secs(3600), // 1 hour
            replay_buffer_size: 10000,
            persist_events: false,
        }
    }
}

/// REST API configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestApiConfig {
    /// Enable REST API
    pub enabled: bool,
    
    /// API path prefix
    pub path_prefix: String,
    
    /// Request timeout
    pub request_timeout: Duration,
    
    /// Maximum request body size (bytes)
    pub max_body_size: usize,
    
    /// Enable API documentation endpoint
    pub enable_docs: bool,
}

impl Default for RestApiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path_prefix: "/api/v1".to_string(),
            request_timeout: Duration::from_secs(30),
            max_body_size: 1024 * 1024, // 1MB
            enable_docs: true,
        }
    }
}

/// WebSocket configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketConfig {
    /// Enable WebSocket endpoint
    pub enabled: bool,
    
    /// WebSocket path
    pub path: String,
    
    /// Ping interval for keeping connections alive
    pub ping_interval: Duration,
    
    /// Connection timeout
    pub connection_timeout: Duration,
    
    /// Maximum message size
    pub max_message_size: usize,
    
    /// Enable compression
    pub enable_compression: bool,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: "/ws".to_string(),
            ping_interval: Duration::from_secs(30),
            connection_timeout: Duration::from_secs(60),
            max_message_size: 1024 * 1024, // 1MB
            enable_compression: true,
        }
    }
}

/// CORS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorsConfig {
    /// Enable CORS
    pub enabled: bool,
    
    /// Allowed origins
    pub allowed_origins: Vec<String>,
    
    /// Allowed methods
    pub allowed_methods: Vec<String>,
    
    /// Allowed headers
    pub allowed_headers: Vec<String>,
    
    /// Allow credentials
    pub allow_credentials: bool,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "OPTIONS".to_string(),
            ],
            allowed_headers: vec![
                "Content-Type".to_string(),
                "Authorization".to_string(),
                "X-API-Key".to_string(),
            ],
            allow_credentials: false,
        }
    }
}

/// TLS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Certificate file path
    pub cert_file: String,
    
    /// Private key file path
    pub key_file: String,
    
    /// CA certificate file path (optional)
    pub ca_file: Option<String>,
    
    /// Require client certificates
    pub require_client_cert: bool,
}

impl ServiceBusConfig {
    /// Load configuration from a TOML file
    pub fn from_file(path: &str) -> CatalystResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| CatalystError::Config(format!("Failed to read config file {}: {}", path, e)))?;
        
        let config: Self = toml::from_str(&content)
            .map_err(|e| CatalystError::Config(format!("Failed to parse config file {}: {}", path, e)))?;
        
        config.validate()?;
        Ok(config)
    }
    
    /// Validate the configuration
    pub fn validate(&self) -> CatalystResult<()> {
        if self.port == 0 {
            return Err(CatalystError::Config("Port cannot be 0".to_string()));
        }
        
        if self.max_connections == 0 {
            return Err(CatalystError::Config("max_connections cannot be 0".to_string()));
        }
        
        if self.message_buffer_size == 0 {
            return Err(CatalystError::Config("message_buffer_size cannot be 0".to_string()));
        }
        
        if self.auth.enabled && self.auth.jwt_secret.is_empty() {
            return Err(CatalystError::Config("JWT secret cannot be empty when auth is enabled".to_string()));
        }
        
        Ok(())
    }
    
    /// Get the bind address as a string
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_default_config() {
        let config = ServiceBusConfig::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 8080);
        assert!(config.websocket.enabled);
        assert!(config.rest_api.enabled);
    }

    #[test]
    fn test_config_validation() {
        let mut config = ServiceBusConfig::default();
        
        // Valid config should pass
        assert!(config.validate().is_ok());
        
        // Invalid port
        config.port = 0;
        assert!(config.validate().is_err());
        
        // Reset port
        config.port = 8080;
        
        // Invalid max_connections
        config.max_connections = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_bind_address() {
        let config = ServiceBusConfig {
            host: "127.0.0.1".to_string(),
            port: 9090,
            ..Default::default()
        };
        
        assert_eq!(config.bind_address(), "127.0.0.1:9090");
    }
}