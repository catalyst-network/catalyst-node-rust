//! Authentication and authorization for the Service Bus

use crate::{config::AuthConfig, error::ServiceBusError};
use catalyst_utils::{CatalystResult, logging::LogCategory};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::{SystemTime, UNIX_EPOCH}};

/// JWT claims structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: String,
    
    /// Issued at timestamp
    pub iat: u64,
    
    /// Expiration timestamp
    pub exp: u64,
    
    /// Permissions granted to this token
    pub permissions: Vec<String>,
    
    /// Connection limits
    pub max_connections: Option<usize>,
    
    /// Rate limit overrides
    pub rate_limit_override: Option<u32>,
}

/// Authentication token
#[derive(Debug, Clone)]
pub struct AuthToken {
    /// Token string
    pub token: String,
    
    /// Decoded claims
    pub claims: Claims,
}

/// Authentication manager
pub struct AuthManager {
    /// Authentication configuration
    config: AuthConfig,
    
    /// Encoding key for JWT
    encoding_key: EncodingKey,
    
    /// Decoding key for JWT
    decoding_key: DecodingKey,
    
    /// JWT validation rules
    validation: Validation,
    
    /// Valid API keys
    api_keys: HashSet<String>,
}

impl AuthManager {
    /// Create a new authentication manager
    pub fn new(config: AuthConfig) -> CatalystResult<Self> {
        let encoding_key = EncodingKey::from_secret(config.jwt_secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(config.jwt_secret.as_bytes());
        
        let mut validation = Validation::default();
        validation.validate_exp = true;
        
        let api_keys: HashSet<String> = config.api_keys.iter().cloned().collect();
        
        Ok(Self {
            config,
            encoding_key,
            decoding_key,
            validation,
            api_keys,
        })
    }
    
    /// Generate a new JWT token
    pub fn generate_token(
        &self,
        user_id: String,
        permissions: Vec<String>,
        max_connections: Option<usize>,
        rate_limit_override: Option<u32>,
    ) -> CatalystResult<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let claims = Claims {
            sub: user_id,
            iat: now,
            exp: now + self.config.token_expiry.as_secs(),
            permissions,
            max_connections,
            rate_limit_override,
        };
        
        encode(&Header::default(), &claims, &self.encoding_key)
            .map_err(|e| ServiceBusError::AuthenticationFailed(format!("Failed to generate token: {}", e)).into())
    }
    
    /// Validate and decode a JWT token
    pub fn validate_token(&self, token: &str) -> CatalystResult<AuthToken> {
        let token_data = decode::<Claims>(token, &self.decoding_key, &self.validation)
            .map_err(|e| ServiceBusError::AuthenticationFailed(format!("Invalid token: {}", e)))?;
        
        Ok(AuthToken {
            token: token.to_string(),
            claims: token_data.claims,
        })
    }
    
    /// Validate API key
    pub fn validate_api_key(&self, api_key: &str) -> bool {
        self.api_keys.contains(api_key)
    }
    
    /// Check if authentication is required
    pub fn is_auth_required(&self) -> bool {
        self.config.enabled
    }
    
    /// Check if anonymous access is allowed
    pub fn is_anonymous_allowed(&self) -> bool {
        self.config.allow_anonymous
    }
    
    /// Authenticate a request with token or API key
    pub fn authenticate(&self, auth_header: Option<&str>, api_key: Option<&str>) -> CatalystResult<Option<AuthToken>> {
        // If authentication is disabled, allow all requests
        if !self.config.enabled {
            return Ok(None);
        }
        
        // Check API key first
        if let Some(key) = api_key {
            if self.validate_api_key(key) {
                // Create a synthetic token for API key authentication
                let claims = Claims {
                    sub: "api_key_user".to_string(),
                    iat: catalyst_utils::utils::current_timestamp(),
                    exp: catalyst_utils::utils::current_timestamp() + 3600, // 1 hour
                    permissions: vec!["read".to_string(), "write".to_string()], // Full permissions for API keys
                    max_connections: None,
                    rate_limit_override: None,
                };
                
                return Ok(Some(AuthToken {
                    token: key.to_string(),
                    claims,
                }));
            }
        }
        
        // Check JWT token
        if let Some(auth_header) = auth_header {
            if let Some(token) = auth_header.strip_prefix("Bearer ") {
                let auth_token = self.validate_token(token)?;
                return Ok(Some(auth_token));
            }
        }
        
        // Check if anonymous access is allowed
        if self.config.allow_anonymous {
            let claims = Claims {
                sub: "anonymous".to_string(),
                iat: catalyst_utils::utils::current_timestamp(),
                exp: catalyst_utils::utils::current_timestamp() + 3600,
                permissions: vec!["read".to_string()], // Read-only for anonymous
                max_connections: Some(1), // Limited connections
                rate_limit_override: None,
            };
            
            return Ok(Some(AuthToken {
                token: "anonymous".to_string(),
                claims,
            }));
        }
        
        Err(ServiceBusError::AuthenticationFailed("No valid authentication provided".to_string()).into())
    }
    
    /// Check if user has specific permission
    pub fn has_permission(auth_token: &AuthToken, permission: &str) -> bool {
        auth_token.claims.permissions.contains(&permission.to_string())
    }
    
    /// Get user's connection limit
    pub fn get_connection_limit(auth_token: &AuthToken, default_limit: usize) -> usize {
        auth_token.claims.max_connections.unwrap_or(default_limit)
    }
    
    /// Get user's rate limit override
    pub fn get_rate_limit_override(auth_token: &AuthToken) -> Option<u32> {
        auth_token.claims.rate_limit_override
    }
}

/// Permission constants
pub mod permissions {
    pub const READ: &str = "read";
    pub const WRITE: &str = "write";
    pub const ADMIN: &str = "admin";
    pub const SUBSCRIBE: &str = "subscribe";
    pub const HISTORY: &str = "history";
}

/// Authentication middleware for Axum
pub mod middleware {
    use super::*;
    use axum::{
        extract::{Request, State},
        http::{HeaderMap, StatusCode},
        middleware::Next,
        response::Response,
    };
    use std::sync::Arc;
    
    /// Authentication state for middleware
    #[derive(Clone)]
    pub struct AuthState {
        pub auth_manager: Arc<AuthManager>,
    }
    
    /// Authentication middleware
    pub async fn auth_middleware(
        State(auth_state): State<AuthState>,
        headers: HeaderMap,
        mut request: Request,
        next: Next,
    ) -> Result<Response, StatusCode> {
        // Extract authentication information
        let auth_header = headers.get("Authorization")
            .and_then(|h| h.to_str().ok());
        
        let api_key = headers.get("X-API-Key")
            .and_then(|h| h.to_str().ok());
        
        // Authenticate the request
        match auth_state.auth_manager.authenticate(auth_header, api_key) {
            Ok(Some(auth_token)) => {
                // Add auth token to request extensions
                request.extensions_mut().insert(auth_token);
                Ok(next.run(request).await)
            }
            Ok(None) => {
                // Authentication disabled, proceed without token
                Ok(next.run(request).await)
            }
            Err(_) => {
                catalyst_utils::logging::log_warn!(
                    LogCategory::ServiceBus,
                    "Authentication failed for request"
                );
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }
    
    /// Extract auth token from request
    pub fn extract_auth_token(request: &Request) -> Option<&AuthToken> {
        request.extensions().get::<AuthToken>()
    }
}

/// Rate limiting based on authentication
pub struct AuthRateLimiter {
    /// Default rate limiter
    default_limiter: governor::RateLimiter<
        governor::state::NotKeyed,
        governor::state::InMemoryState,
        governor::clock::DefaultClock,
    >,
}

impl AuthRateLimiter {
    /// Create a new authenticated rate limiter
    pub fn new(default_rps: u32, burst_size: u32) -> Self {
        use governor::{Quota, RateLimiter};
        use std::num::NonZeroU32;
        
        let quota = Quota::per_second(NonZeroU32::new(default_rps).unwrap())
            .allow_burst(NonZeroU32::new(burst_size).unwrap());
        
        let default_limiter = RateLimiter::direct(quota);
        
        Self {
            default_limiter,
        }
    }
    
    /// Check if request is allowed based on auth token
    pub fn check_rate_limit(&self, auth_token: Option<&AuthToken>) -> bool {
        // For now, use default limiter for all users
        // In production, you'd maintain per-user rate limiters
        self.default_limiter.check().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn create_test_config() -> AuthConfig {
        AuthConfig {
            enabled: true,
            jwt_secret: "test_secret".to_string(),
            token_expiry: Duration::from_secs(3600),
            api_keys: vec!["test_api_key".to_string()],
            allow_anonymous: false,
        }
    }

    #[test]
    fn test_auth_manager_creation() {
        let config = create_test_config();
        let auth_manager = AuthManager::new(config).unwrap();
        assert!(auth_manager.is_auth_required());
    }

    #[test]
    fn test_token_generation_and_validation() {
        let config = create_test_config();
        let auth_manager = AuthManager::new(config).unwrap();
        
        let token = auth_manager.generate_token(
            "test_user".to_string(),
            vec!["read".to_string(), "write".to_string()],
            Some(5),
            None,
        ).unwrap();
        
        let auth_token = auth_manager.validate_token(&token).unwrap();
        assert_eq!(auth_token.claims.sub, "test_user");
        assert!(auth_token.claims.permissions.contains(&"read".to_string()));
    }

    #[test]
    fn test_api_key_validation() {
        let config = create_test_config();
        let auth_manager = AuthManager::new(config).unwrap();
        
        assert!(auth_manager.validate_api_key("test_api_key"));
        assert!(!auth_manager.validate_api_key("invalid_key"));
    }

    #[test]
    fn test_authentication() {
        let config = create_test_config();
        let auth_manager = AuthManager::new(config).unwrap();
        
        // Test API key authentication
        let result = auth_manager.authenticate(None, Some("test_api_key")).unwrap();
        assert!(result.is_some());
        
        // Test JWT authentication
        let token = auth_manager.generate_token(
            "test_user".to_string(),
            vec!["read".to_string()],
            None,
            None,
        ).unwrap();
        
        let auth_header = format!("Bearer {}", token);
        let result = auth_manager.authenticate(Some(&auth_header), None).unwrap();
        assert!(result.is_some());
        
        // Test failed authentication
        let result = auth_manager.authenticate(None, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_permissions() {
        let config = create_test_config();
        let auth_manager = AuthManager::new(config).unwrap();
        
        let token = auth_manager.generate_token(
            "test_user".to_string(),
            vec!["read".to_string()],
            None,
            None,
        ).unwrap();
        
        let auth_token = auth_manager.validate_token(&token).unwrap();
        
        assert!(AuthManager::has_permission(&auth_token, "read"));
        assert!(!AuthManager::has_permission(&auth_token, "write"));
    }

    #[test]
    fn test_anonymous_access() {
        let mut config = create_test_config();
        config.allow_anonymous = true;
        
        let auth_manager = AuthManager::new(config).unwrap();
        
        let result = auth_manager.authenticate(None, None).unwrap();
        assert!(result.is_some());
        
        let auth_token = result.unwrap();
        assert_eq!(auth_token.claims.sub, "anonymous");
        assert!(AuthManager::has_permission(&auth_token, "read"));
        assert!(!AuthManager::has_permission(&auth_token, "write"));
    }

    #[test]
    fn test_rate_limiter() {
        let limiter = AuthRateLimiter::new(10, 5);
        
        // Should allow initial requests
        assert!(limiter.check_rate_limit(None));
        
        // Test with auth token
        let config = create_test_config();
        let auth_manager = AuthManager::new(config).unwrap();
        let token = auth_manager.generate_token(
            "test_user".to_string(),
            vec!["read".to_string()],
            None,
            Some(100), // High rate limit
        ).unwrap();
        let auth_token = auth_manager.validate_token(&token).unwrap();
        
        assert!(limiter.check_rate_limit(Some(&auth_token)));
    }
}