//! Error types for the Service Bus

use catalyst_utils::CatalystError;
use thiserror::Error;

/// Service Bus specific errors
#[derive(Error, Debug)]
pub enum ServiceBusError {
    #[error("Invalid message format: {0}")]
    InvalidMessage(String),
    
    #[error("Connection closed")]
    ConnectionClosed,
    
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    #[error("Invalid event type: {0}")]
    InvalidEventType(String),
    
    #[error("Invalid address format: {0}")]
    InvalidAddress(String),
    
    #[error("Subscription not found: {0}")]
    SubscriptionNotFound(String),
    
    #[error("Maximum connections reached")]
    MaxConnectionsReached,
    
    #[error("Invalid filter: {0}")]
    InvalidFilter(String),
    
    #[error("WebSocket error: {0}")]
    WebSocketError(String),
    
    #[error("JSON serialization error: {0}")]
    SerializationError(String),
    
    #[error("Configuration error: {0}")]
    ConfigError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
}

impl From<ServiceBusError> for CatalystError {
    fn from(err: ServiceBusError) -> Self {
        match err {
            ServiceBusError::InvalidMessage(msg) => CatalystError::Invalid(format!("ServiceBus: {}", msg)),
            ServiceBusError::ConnectionClosed => CatalystError::Network("ServiceBus: Connection closed".to_string()),
            ServiceBusError::AuthenticationFailed(msg) => CatalystError::Invalid(format!("ServiceBus auth: {}", msg)),
            ServiceBusError::RateLimitExceeded => CatalystError::Invalid("ServiceBus: Rate limit exceeded".to_string()),
            ServiceBusError::InvalidEventType(msg) => CatalystError::Invalid(format!("ServiceBus: Invalid event type: {}", msg)),
            ServiceBusError::InvalidAddress(msg) => CatalystError::Invalid(format!("ServiceBus: Invalid address: {}", msg)),
            ServiceBusError::SubscriptionNotFound(msg) => CatalystError::NotFound(format!("ServiceBus: Subscription not found: {}", msg)),
            ServiceBusError::MaxConnectionsReached => CatalystError::Invalid("ServiceBus: Maximum connections reached".to_string()),
            ServiceBusError::InvalidFilter(msg) => CatalystError::Invalid(format!("ServiceBus: Invalid filter: {}", msg)),
            ServiceBusError::WebSocketError(msg) => CatalystError::Network(format!("ServiceBus WebSocket: {}", msg)),
            ServiceBusError::SerializationError(msg) => CatalystError::Serialization(format!("ServiceBus: {}", msg)),
            ServiceBusError::ConfigError(msg) => CatalystError::Config(format!("ServiceBus: {}", msg)),
            ServiceBusError::NetworkError(msg) => CatalystError::Network(format!("ServiceBus: {}", msg)),
            ServiceBusError::InternalError(msg) => CatalystError::Internal(format!("ServiceBus: {}", msg)),
        }
    }
}

impl From<serde_json::Error> for ServiceBusError {
    fn from(err: serde_json::Error) -> Self {
        ServiceBusError::SerializationError(err.to_string())
    }
}

impl From<axum::Error> for ServiceBusError {
    fn from(err: axum::Error) -> Self {
        ServiceBusError::WebSocketError(err.to_string())
    }
}

impl From<tokio_tungstenite::tungstenite::Error> for ServiceBusError {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        ServiceBusError::WebSocketError(err.to_string())
    }
}

impl From<ServiceBusError> for axum::response::Response {
    fn from(err: ServiceBusError) -> Self {
        use axum::{http::StatusCode, response::Json};
        use axum::response::IntoResponse;
        
        let (status, error_code) = match &err {
            ServiceBusError::InvalidMessage(_) => (StatusCode::BAD_REQUEST, "INVALID_MESSAGE"),
            ServiceBusError::ConnectionClosed => (StatusCode::BAD_REQUEST, "CONNECTION_CLOSED"),
            ServiceBusError::AuthenticationFailed(_) => (StatusCode::UNAUTHORIZED, "AUTH_FAILED"),
            ServiceBusError::RateLimitExceeded => (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMIT"),
            ServiceBusError::InvalidEventType(_) => (StatusCode::BAD_REQUEST, "INVALID_EVENT_TYPE"),
            ServiceBusError::InvalidAddress(_) => (StatusCode::BAD_REQUEST, "INVALID_ADDRESS"),
            ServiceBusError::SubscriptionNotFound(_) => (StatusCode::NOT_FOUND, "SUBSCRIPTION_NOT_FOUND"),
            ServiceBusError::MaxConnectionsReached => (StatusCode::SERVICE_UNAVAILABLE, "MAX_CONNECTIONS"),
            ServiceBusError::InvalidFilter(_) => (StatusCode::BAD_REQUEST, "INVALID_FILTER"),
            ServiceBusError::WebSocketError(_) => (StatusCode::BAD_REQUEST, "WEBSOCKET_ERROR"),
            ServiceBusError::SerializationError(_) => (StatusCode::BAD_REQUEST, "SERIALIZATION_ERROR"),
            ServiceBusError::ConfigError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "CONFIG_ERROR"),
            ServiceBusError::NetworkError(_) => (StatusCode::BAD_GATEWAY, "NETWORK_ERROR"),
            ServiceBusError::InternalError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        };
        
        let error_response = serde_json::json!({
            "error": err.to_string(),
            "code": error_code,
            "timestamp": catalyst_utils::utils::current_timestamp()
        });
        
        (status, Json(error_response)).into_response()
    }
}