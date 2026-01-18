//! Main Service Bus server implementation

use crate::{
    config::ServiceBusConfig,
    websocket::{websocket_handler, WebSocketState},
    rest::{create_rest_router, RestApiState},
    filters::FilterEngine,
    auth::{AuthManager, middleware::{auth_middleware, AuthState}},
    events::BlockchainEvent,
    error::ServiceBusError,
};
use catalyst_utils::{CatalystResult, logging::LogCategory};
use axum::{
    routing::get,
    Router,
    middleware,
};
use std::{net::SocketAddr, sync::Arc};
use tokio::{net::TcpListener, sync::broadcast};
use tower_http::{
    cors::{CorsLayer, Any},
    trace::TraceLayer,
};

/// Main Service Bus server
pub struct ServiceBusServer {
    /// Server configuration
    config: ServiceBusConfig,
    
    /// Event filtering engine
    filter_engine: Arc<FilterEngine>,
    
    /// Authentication manager
    auth_manager: Arc<AuthManager>,
    
    /// Event broadcaster for new events
    event_broadcaster: broadcast::Sender<BlockchainEvent>,
    
    /// Event receiver for processing
    _event_receiver: broadcast::Receiver<BlockchainEvent>,
}

impl ServiceBusServer {
    /// Create a new Service Bus server
    pub async fn new(config: ServiceBusConfig) -> CatalystResult<Self> {
        // Validate configuration
        config.validate()?;
        
        catalyst_utils::logging::log_info!(
            LogCategory::ServiceBus,
            "Initializing Service Bus server on {}",
            config.bind_address()
        );
        
        // Initialize filter engine
        let filter_engine = Arc::new(FilterEngine::new(
            config.event_filter.max_filters_per_connection,
            config.event_filter.enable_replay,
            config.event_filter.replay_buffer_size,
        ));
        
        // Initialize authentication
        let auth_manager = Arc::new(AuthManager::new(config.auth.clone())?);
        
        // Create event broadcast channel
        let (event_tx, event_rx) = broadcast::channel(10000);
        
        catalyst_utils::logging::log_info!(
            LogCategory::ServiceBus,
            "Service Bus server initialized successfully"
        );
        
        Ok(Self {
            config,
            filter_engine,
            auth_manager,
            event_broadcaster: event_tx,
            _event_receiver: event_rx,
        })
    }
    
    /// Start the Service Bus server
    pub async fn start(self) -> CatalystResult<()> {
        let bind_addr: SocketAddr = self.config.bind_address()
            .parse()
            .map_err(|e| ServiceBusError::ConfigError(format!("Invalid bind address: {}", e)))?;
        
        catalyst_utils::logging::log_info!(
            LogCategory::ServiceBus,
            "Starting Service Bus server on {}",
            bind_addr
        );
        
        // Create the main application router
        let app = self.create_app_router().await?;
        
        // Create TCP listener
        let listener = TcpListener::bind(bind_addr)
            .await
            .map_err(|e| ServiceBusError::NetworkError(format!("Failed to bind to {}: {}", bind_addr, e)))?;
        
        catalyst_utils::logging::log_info!(
            LogCategory::ServiceBus,
            "Service Bus server listening on {}",
            bind_addr
        );
        
        // Start the server
        axum::serve(listener, app)
            .await
            .map_err(|e| ServiceBusError::NetworkError(format!("Server error: {}", e)))?;
        
        Ok(())
    }
    
    /// Process a new blockchain event
    pub async fn process_event(&self, event: BlockchainEvent) -> CatalystResult<()> {
        catalyst_utils::logging::log_debug!(
            LogCategory::ServiceBus,
            "Processing blockchain event: {} (type: {:?})",
            event.id,
            event.event_type
        );
        
        // Process through filter engine
        self.filter_engine.process_event(event.clone()).await?;
        
        // Broadcast to any other listeners
        if let Err(e) = self.event_broadcaster.send(event) {
            catalyst_utils::logging::log_warn!(
                LogCategory::ServiceBus,
                "Failed to broadcast event: {}",
                e
            );
        }
        
        Ok(())
    }
    
    /// Get server statistics
    pub fn get_stats(&self) -> ServiceBusStats {
        let filter_stats = self.filter_engine.get_stats();
        
        ServiceBusStats {
            total_subscriptions: filter_stats.total_subscriptions,
            total_connections: filter_stats.total_connections,
            replay_buffer_blocks: filter_stats.replay_buffer_blocks,
            replay_buffer_events: filter_stats.replay_buffer_events,
            auth_enabled: self.config.auth.enabled,
            websocket_enabled: self.config.websocket.enabled,
            rest_api_enabled: self.config.rest_api.enabled,
        }
    }
    
    /// Create the main application router
    async fn create_app_router(self) -> CatalystResult<Router> {
        // Keep the overall app state as `()` and provide state per-route with `.with_state(...)`.
        // This lets us mount routes that need different state types (WebSocketState vs RestApiState).
        let mut app: Router = Router::new();
        
        // Add WebSocket endpoint if enabled
        if self.config.websocket.enabled {
            let ws_state = WebSocketState {
                filter_engine: Arc::clone(&self.filter_engine),
                config: self.config.websocket.clone(),
            };
            
            app = app.route(
                &self.config.websocket.path,
                get(websocket_handler).with_state(ws_state),
            );
        }
        
        // Add REST API endpoints if enabled
        if self.config.rest_api.enabled {
            let rest_state = RestApiState {
                filter_engine: Arc::clone(&self.filter_engine),
                config: self.config.rest_api.clone(),
            };
            
            // Build a router that expects `RestApiState` and then "provide" that state,
            // turning it into a `Router<()>` that can be nested into the main app.
            let rest_router = create_rest_router().with_state(rest_state);
            app = app.nest(&self.config.rest_api.path_prefix, rest_router);
        }
        
        // Add root status endpoint
        app = app.route("/", get(root_handler));

        // Add CORS if enabled
        if self.config.cors.enabled {
            let cors = CorsLayer::new()
                .allow_origin(Any) // In production, configure this properly
                .allow_methods(Any)
                .allow_headers(Any);
            app = app.layer(cors);
        }

        // Add authentication middleware if enabled
        if self.config.auth.enabled {
            let auth_state = AuthState {
                auth_manager: Arc::clone(&self.auth_manager),
            };
            app = app.layer(middleware::from_fn_with_state(auth_state, auth_middleware));
        }

        // Add tracing
        app = app.layer(TraceLayer::new_for_http());

        Ok(app)
    }
}

/// Server statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceBusStats {
    pub total_subscriptions: usize,
    pub total_connections: usize,
    pub replay_buffer_blocks: usize,
    pub replay_buffer_events: usize,
    pub auth_enabled: bool,
    pub websocket_enabled: bool,
    pub rest_api_enabled: bool,
}

/// Root handler that provides basic service information
async fn root_handler() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "service": "catalyst-service-bus",
        "version": env!("CARGO_PKG_VERSION"),
        "api_version": crate::API_VERSION,
        "description": "Catalyst Network Service Bus - Web2 integration layer",
        "endpoints": {
            "websocket": crate::WS_PATH,
            "rest_api": crate::API_PATH,
            "health": format!("{}/health", crate::API_PATH),
            "docs": format!("{}/docs", crate::API_PATH)
        },
        "timestamp": catalyst_utils::utils::current_timestamp()
    }))
}

/// Event processor that can be used to inject events from other Catalyst components
pub struct EventProcessor {
    server: Arc<ServiceBusServer>,
}

impl EventProcessor {
    /// Create a new event processor
    pub fn new(server: Arc<ServiceBusServer>) -> Self {
        Self { server }
    }
    
    /// Process a blockchain event
    pub async fn process_event(&self, event: BlockchainEvent) -> CatalystResult<()> {
        self.server.process_event(event).await
    }
    
    /// Process multiple events in batch
    pub async fn process_events(&self, events: Vec<BlockchainEvent>) -> CatalystResult<()> {
        for event in events {
            self.process_event(event).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::ServiceBusConfig, events::{EventType, events}};
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_server_creation() {
        let config = ServiceBusConfig::default();
        let server = ServiceBusServer::new(config).await.unwrap();
        
        let stats = server.get_stats();
        assert_eq!(stats.total_subscriptions, 0);
        assert_eq!(stats.total_connections, 0);
    }

    #[tokio::test]
    async fn test_event_processing() {
        let config = ServiceBusConfig::default();
        let server = ServiceBusServer::new(config).await.unwrap();
        
        let event = events::transaction_confirmed(
            100,
            [1u8; 32],
            [2u8; 21],
            Some([3u8; 21]),
            1000,
            21000,
        );
        
        let result = server.process_event(event).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_event_processor() {
        let config = ServiceBusConfig::default();
        let server = Arc::new(ServiceBusServer::new(config).await.unwrap());
        let processor = EventProcessor::new(server);
        
        let events = vec![
            events::block_finalized(1, [1u8; 32]),
            events::block_finalized(2, [2u8; 32]),
        ];
        
        let result = processor.process_events(events).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_server_router_creation() {
        let config = ServiceBusConfig::default();
        let server = ServiceBusServer::new(config).await.unwrap();
        
        // This tests that the router can be created without errors
        let router = server.create_app_router().await;
        assert!(router.is_ok());
    }

    #[tokio::test]
    async fn test_disabled_endpoints() {
        let mut config = ServiceBusConfig::default();
        config.websocket.enabled = false;
        config.rest_api.enabled = false;
        
        let server = ServiceBusServer::new(config).await.unwrap();
        let router = server.create_app_router().await.unwrap();
        
        // Should still have root handler
        // Specific endpoint testing would require integration tests
    }
}