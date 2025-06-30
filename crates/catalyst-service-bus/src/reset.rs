//! REST API endpoints for the Service Bus

use crate::{
    config::RestApiConfig,
    events::{BlockchainEvent, EventFilter},
    filters::{FilterEngine, FilterEngineStats},
    error::ServiceBusError,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use catalyst_utils::{CatalystResult, logging::LogCategory};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use uuid::Uuid;

/// REST API state
#[derive(Clone)]
pub struct RestApiState {
    pub filter_engine: Arc<FilterEngine>,
    pub config: RestApiConfig,
}

/// Query parameters for event history
#[derive(Debug, Deserialize)]
pub struct EventHistoryQuery {
    /// Filter by event types (comma-separated)
    pub event_types: Option<String>,
    
    /// Filter by addresses (comma-separated hex strings)
    pub addresses: Option<String>,
    
    /// Filter by contract addresses (comma-separated hex strings)
    pub contracts: Option<String>,
    
    /// Filter by topics (comma-separated)
    pub topics: Option<String>,
    
    /// From block number
    pub from_block: Option<u64>,
    
    /// To block number
    pub to_block: Option<u64>,
    
    /// Limit number of results
    pub limit: Option<usize>,
    
    /// Offset for pagination
    pub offset: Option<usize>,
}

/// Request body for creating event subscriptions
#[derive(Debug, Deserialize)]
pub struct CreateSubscriptionRequest {
    pub filter: EventFilter,
    pub connection_id: String,
}

/// Response for subscription creation
#[derive(Debug, Serialize)]
pub struct CreateSubscriptionResponse {
    pub subscription_id: Uuid,
    pub filter: EventFilter,
    pub created_at: u64,
}

/// Response for event history
#[derive(Debug, Serialize)]
pub struct EventHistoryResponse {
    pub events: Vec<BlockchainEvent>,
    pub total_count: usize,
    pub has_more: bool,
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub timestamp: u64,
    pub stats: FilterEngineStats,
}

/// API error response
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: Option<String>,
    pub details: Option<serde_json::Value>,
}

/// Create REST API router
pub fn create_rest_router(state: RestApiState) -> Router {
    Router::new()
        // Health and status endpoints
        .route("/health", get(health_check))
        .route("/status", get(get_status))
        .route("/stats", get(get_stats))
        
        // Event endpoints
        .route("/events", get(get_events))
        .route("/events/history", get(get_event_history))
        
        // Subscription management endpoints
        .route("/subscriptions", post(create_subscription))
        .route("/subscriptions", get(list_subscriptions))
        .route("/subscriptions/:id", get(get_subscription))
        .route("/subscriptions/:id", axum::routing::delete(delete_subscription))
        
        // Connection management
        .route("/connections/:id/subscriptions", get(get_connection_subscriptions))
        
        // API documentation (if enabled)
        .route("/docs", get(api_documentation))
        
        .with_state(state)
}

/// Health check endpoint
async fn health_check(State(state): State<RestApiState>) -> Json<HealthResponse> {
    let stats = state.filter_engine.get_stats();
    
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: catalyst_utils::utils::current_timestamp(),
        stats,
    })
}

/// Get service status
async fn get_status(State(state): State<RestApiState>) -> Json<serde_json::Value> {
    let stats = state.filter_engine.get_stats();
    
    Json(serde_json::json!({
        "service": "catalyst-service-bus",
        "version": env!("CARGO_PKG_VERSION"),
        "api_version": crate::API_VERSION,
        "stats": stats,
        "config": {
            "rest_api_enabled": state.config.enabled,
            "max_body_size": state.config.max_body_size,
            "request_timeout_ms": state.config.request_timeout.as_millis(),
        }
    }))
}

/// Get filter engine statistics
async fn get_stats(State(state): State<RestApiState>) -> Json<FilterEngineStats> {
    Json(state.filter_engine.get_stats())
}

/// Get recent events (real-time endpoint)
async fn get_events(
    State(state): State<RestApiState>,
    Query(query): Query<EventHistoryQuery>,
) -> Result<Json<Vec<BlockchainEvent>>, (StatusCode, Json<ErrorResponse>)> {
    let filter = parse_event_filter_from_query(&query)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse {
            error: e.to_string(),
            code: Some("INVALID_FILTER".to_string()),
            details: None,
        })))?;
    
    let limit = query.limit.unwrap_or(100).min(1000); // Cap at 1000 events
    let limited_filter = filter.with_limit(limit);
    
    match state.filter_engine.get_historical_events(&limited_filter, Some(limit)).await {
        Ok(events) => Ok(Json(events)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
            error: e.to_string(),
            code: Some("QUERY_FAILED".to_string()),
            details: None,
        }))),
    }
}

/// Get event history with pagination
async fn get_event_history(
    State(state): State<RestApiState>,
    Query(query): Query<EventHistoryQuery>,
) -> Result<Json<EventHistoryResponse>, (StatusCode, Json<ErrorResponse>)> {
    let filter = parse_event_filter_from_query(&query)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ErrorResponse {
            error: e.to_string(),
            code: Some("INVALID_FILTER".to_string()),
            details: None,
        })))?;
    
    let limit = query.limit.unwrap_or(100).min(1000);
    let offset = query.offset.unwrap_or(0);
    
    match state.filter_engine.get_historical_events(&filter, Some(limit + offset + 1)).await {
        Ok(mut all_events) => {
            // Simple pagination - in production, you'd want more efficient pagination
            let total_count = all_events.len();
            let has_more = total_count > limit + offset;
            
            if offset < all_events.len() {
                all_events.drain(0..offset.min(all_events.len()));
            } else {
                all_events.clear();
            }
            
            if all_events.len() > limit {
                all_events.truncate(limit);
            }
            
            Ok(Json(EventHistoryResponse {
                events: all_events,
                total_count,
                has_more,
            }))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse {
            error: e.to_string(),
            code: Some("QUERY_FAILED".to_string()),
            details: None,
        }))),
    }
}

/// Create a new event subscription
async fn create_subscription(
    State(state): State<RestApiState>,
    Json(request): Json<CreateSubscriptionRequest>,
) -> Result<Json<CreateSubscriptionResponse>, (StatusCode, Json<ErrorResponse>)> {
    match state.filter_engine.add_subscription(request.connection_id, request.filter.clone()) {
        Ok((subscription_id, _)) => {
            catalyst_utils::logging::log_info!(
                LogCategory::ServiceBus,
                "Created subscription {} via REST API",
                subscription_id
            );
            
            Ok(Json(CreateSubscriptionResponse {
                subscription_id,
                filter: request.filter,
                created_at: catalyst_utils::utils::current_timestamp(),
            }))
        }
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(ErrorResponse {
            error: e.to_string(),
            code: Some("SUBSCRIPTION_FAILED".to_string()),
            details: None,
        }))),
    }
}

/// List all subscriptions (with optional connection filter)
async fn list_subscriptions(
    State(_state): State<RestApiState>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, Json<ErrorResponse>)> {
    // For security reasons, we don't expose all subscriptions
    // This endpoint would typically require authentication
    Err((StatusCode::NOT_IMPLEMENTED, Json(ErrorResponse {
        error: "Listing all subscriptions not implemented for security reasons".to_string(),
        code: Some("NOT_IMPLEMENTED".to_string()),
        details: None,
    })))
}

/// Get subscription by ID
async fn get_subscription(
    Path(_subscription_id): Path<Uuid>,
    State(_state): State<RestApiState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // This would require authentication and authorization
    Err((StatusCode::NOT_IMPLEMENTED, Json(ErrorResponse {
        error: "Get subscription not implemented".to_string(),
        code: Some("NOT_IMPLEMENTED".to_string()),
        details: None,
    })))
}

/// Delete subscription by ID
async fn delete_subscription(
    Path(subscription_id): Path<Uuid>,
    Query(query): Query<HashMap<String, String>>,
    State(state): State<RestApiState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let connection_id = query.get("connection_id")
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(ErrorResponse {
            error: "connection_id parameter required".to_string(),
            code: Some("MISSING_PARAMETER".to_string()),
            details: None,
        })))?;
    
    match state.filter_engine.remove_subscription(subscription_id, connection_id) {
        Ok(()) => {
            catalyst_utils::logging::log_info!(
                LogCategory::ServiceBus,
                "Removed subscription {} via REST API",
                subscription_id
            );
            
            Ok(Json(serde_json::json!({
                "success": true,
                "subscription_id": subscription_id
            })))
        }
        Err(e) => Err((StatusCode::NOT_FOUND, Json(ErrorResponse {
            error: e.to_string(),
            code: Some("SUBSCRIPTION_NOT_FOUND".to_string()),
            details: None,
        }))),
    }
}

/// Get subscriptions for a specific connection
async fn get_connection_subscriptions(
    Path(connection_id): Path<String>,
    State(state): State<RestApiState>,
) -> Json<Vec<serde_json::Value>> {
    let subscriptions = state.filter_engine.get_connection_subscriptions(&connection_id);
    
    let subscription_info: Vec<serde_json::Value> = subscriptions
        .into_iter()
        .map(|sub| serde_json::json!({
            "id": sub.id,
            "filter": sub.filter,
            "created_at": sub.created_at,
            "active": sub.active,
        }))
        .collect();
    
    Json(subscription_info)
}

/// API documentation endpoint
async fn api_documentation(
    State(state): State<RestApiState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    if !state.config.enable_docs {
        return Err((StatusCode::NOT_FOUND, Json(ErrorResponse {
            error: "API documentation is disabled".to_string(),
            code: Some("DOCS_DISABLED".to_string()),
            details: None,
        })));
    }
    
    Ok(Json(serde_json::json!({
        "title": "Catalyst Service Bus API",
        "version": crate::API_VERSION,
        "description": "REST API for Catalyst Network Service Bus - Web2 integration layer",
        "endpoints": {
            "health": {
                "method": "GET",
                "path": "/health",
                "description": "Health check endpoint"
            },
            "status": {
                "method": "GET", 
                "path": "/status",
                "description": "Service status and configuration"
            },
            "stats": {
                "method": "GET",
                "path": "/stats", 
                "description": "Filter engine statistics"
            },
            "events": {
                "method": "GET",
                "path": "/events",
                "description": "Get recent events",
                "query_params": {
                    "event_types": "Comma-separated event types",
                    "addresses": "Comma-separated addresses",
                    "contracts": "Comma-separated contract addresses",
                    "topics": "Comma-separated topics",
                    "from_block": "Start block number",
                    "to_block": "End block number",
                    "limit": "Maximum number of results (max 1000)"
                }
            },
            "event_history": {
                "method": "GET",
                "path": "/events/history",
                "description": "Get paginated event history",
                "query_params": {
                    "event_types": "Comma-separated event types",
                    "addresses": "Comma-separated addresses", 
                    "contracts": "Comma-separated contract addresses",
                    "topics": "Comma-separated topics",
                    "from_block": "Start block number",
                    "to_block": "End block number",
                    "limit": "Maximum number of results (max 1000)",
                    "offset": "Pagination offset"
                }
            },
            "create_subscription": {
                "method": "POST",
                "path": "/subscriptions",
                "description": "Create event subscription",
                "body": {
                    "filter": "EventFilter object",
                    "connection_id": "Connection identifier"
                }
            },
            "delete_subscription": {
                "method": "DELETE",
                "path": "/subscriptions/{id}",
                "description": "Delete event subscription",
                "query_params": {
                    "connection_id": "Connection identifier (required)"
                }
            },
            "connection_subscriptions": {
                "method": "GET",
                "path": "/connections/{id}/subscriptions",
                "description": "Get subscriptions for a connection"
            }
        },
        "websocket": {
            "path": "/ws",
            "description": "WebSocket endpoint for real-time event streaming",
            "message_types": [
                "Subscribe", "Unsubscribe", "ListSubscriptions", 
                "GetHistory", "Ping", "Pong", "Event", 
                "SubscriptionCreated", "SubscriptionRemoved",
                "HistoricalEvents", "SubscriptionList", "Error"
            ]
        }
    })))
}

/// Parse event filter from query parameters
fn parse_event_filter_from_query(query: &EventHistoryQuery) -> CatalystResult<EventFilter> {
    let mut filter = EventFilter::new();
    
    // Parse event types
    if let Some(ref types_str) = query.event_types {
        let event_types = types_str
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| parse_event_type(s))
            .collect::<CatalystResult<Vec<_>>>()?;
        
        if !event_types.is_empty() {
            filter = filter.with_event_types(event_types);
        }
    }
    
    // Parse addresses
    if let Some(ref addresses_str) = query.addresses {
        let addresses = parse_address_list(addresses_str)?;
        if !addresses.is_empty() {
            filter = filter.with_addresses(addresses);
        }
    }
    
    // Parse contract addresses
    if let Some(ref contracts_str) = query.contracts {
        let contracts = parse_address_list(contracts_str)?;
        if !contracts.is_empty() {
            filter = filter.with_contracts(contracts);
        }
    }
    
    // Parse topics
    if let Some(ref topics_str) = query.topics {
        let topics: Vec<String> = topics_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        if !topics.is_empty() {
            filter = filter.with_topics(topics);
        }
    }
    
    // Set block range
    filter = filter.with_block_range(query.from_block, query.to_block);
    
    if let Some(limit) = query.limit {
        filter = filter.with_limit(limit);
    }
    
    Ok(filter)
}

/// Parse event type from string
fn parse_event_type(type_str: &str) -> CatalystResult<crate::events::EventType> {
    use crate::events::EventType;
    
    match type_str.to_lowercase().as_str() {
        "blockfinalized" | "block_finalized" => Ok(EventType::BlockFinalized),
        "transactionconfirmed" | "transaction_confirmed" => Ok(EventType::TransactionConfirmed),
        "transactionpending" | "transaction_pending" => Ok(EventType::TransactionPending),
        "contractevent" | "contract_event" => Ok(EventType::ContractEvent),
        "tokentransfer" | "token_transfer" => Ok(EventType::TokenTransfer),
        "balanceupdate" | "balance_update" => Ok(EventType::BalanceUpdate),
        "consensusphase" | "consensus_phase" => Ok(EventType::ConsensusPhase),
        "peerconnected" | "peer_connected" => Ok(EventType::PeerConnected),
        "peerdisconnected" | "peer_disconnected" => Ok(EventType::PeerDisconnected),
        "networkstats" | "network_stats" => Ok(EventType::NetworkStats),
        custom if custom.starts_with("custom:") => {
            let custom_name = custom.strip_prefix("custom:").unwrap();
            Ok(EventType::Custom(custom_name.to_string()))
        }
        _ => Err(ServiceBusError::InvalidEventType(type_str.to_string()).into()),
    }
}

/// Parse comma-separated address list
fn parse_address_list(addresses_str: &str) -> CatalystResult<Vec<catalyst_utils::Address>> {
    addresses_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|addr_str| {
            catalyst_utils::utils::hex_to_bytes(addr_str)
                .and_then(|bytes| {
                    if bytes.len() == 21 {
                        let mut addr = [0u8; 21];
                        addr.copy_from_slice(&bytes);
                        Ok(addr)
                    } else {
                        Err(ServiceBusError::InvalidAddress(addr_str.to_string()).into())
                    }
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::ServiceBusConfig, events::EventType};
    use axum::http::StatusCode;

    #[test]
    fn test_parse_event_type() {
        assert!(matches!(parse_event_type("transaction_confirmed").unwrap(), EventType::TransactionConfirmed));
        assert!(matches!(parse_event_type("TransactionConfirmed").unwrap(), EventType::TransactionConfirmed));
        assert!(matches!(parse_event_type("custom:my_event").unwrap(), EventType::Custom(_)));
        assert!(parse_event_type("invalid_type").is_err());
    }

    #[test]
    fn test_parse_address_list() {
        let valid_address = "0x1234567890123456789012345678901234567890ab";
        let addresses = parse_address_list(valid_address).unwrap();
        assert_eq!(addresses.len(), 1);
        
        let multiple_addresses = "0x1234567890123456789012345678901234567890ab,0x0987654321098765432109876543210987654321ba";
        let addresses = parse_address_list(multiple_addresses).unwrap();
        assert_eq!(addresses.len(), 2);
        
        // Invalid address should fail
        assert!(parse_address_list("invalid_address").is_err());
    }

    #[tokio::test]
    async fn test_health_check() {
        let filter_engine = Arc::new(FilterEngine::new(10, true, 100));
        let config = RestApiConfig::default();
        let state = RestApiState { filter_engine, config };
        
        let response = health_check(State(state)).await;
        assert_eq!(response.0.status, "healthy");
    }
}