//! Client SDK for connecting to the Catalyst Service Bus

use crate::{
    events::{BlockchainEvent, EventFilter, EventType},
    websocket::WsMessage,
    error::ServiceBusError,
};
use catalyst_utils::{CatalystResult, logging::LogCategory};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{broadcast, mpsc, RwLock},
    time::{interval, timeout},
};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

/// Service Bus client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Service Bus WebSocket URL
    pub url: String,
    
    /// Authentication token
    pub auth_token: Option<String>,
    
    /// API key for authentication
    pub api_key: Option<String>,
    
    /// Connection timeout
    pub connect_timeout: Duration,
    
    /// Reconnection settings
    pub reconnect: ReconnectConfig,
    
    /// Message buffer size
    pub buffer_size: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            url: "ws://localhost:8080/ws".to_string(),
            auth_token: None,
            api_key: None,
            connect_timeout: Duration::from_secs(30),
            reconnect: ReconnectConfig::default(),
            buffer_size: 1000,
        }
    }
}

/// Reconnection configuration
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Enable automatic reconnection
    pub enabled: bool,
    
    /// Maximum number of reconnection attempts
    pub max_attempts: usize,
    
    /// Initial delay between reconnection attempts
    pub initial_delay: Duration,
    
    /// Maximum delay between reconnection attempts
    pub max_delay: Duration,
    
    /// Backoff multiplier
    pub backoff_multiplier: f64,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 10,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
        }
    }
}

/// Event callback function type
pub type EventCallback = Arc<dyn Fn(BlockchainEvent) + Send + Sync>;

/// Service Bus client
pub struct ServiceBusClient {
    /// Client configuration
    config: ClientConfig,
    
    /// Active subscriptions
    subscriptions: Arc<RwLock<HashMap<Uuid, EventCallback>>>,
    
    /// Connection state
    connection_state: Arc<RwLock<ConnectionState>>,
    
    /// Command sender for internal communication
    command_tx: mpsc::UnboundedSender<ClientCommand>,
    
    /// Event receiver
    event_rx: Arc<RwLock<Option<broadcast::Receiver<BlockchainEvent>>>>,
}

/// Connection state
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting { attempt: usize },
    Failed { error: String },
}

/// Internal client commands
#[derive(Debug)]
enum ClientCommand {
    Subscribe {
        filter: EventFilter,
        callback: EventCallback,
        response_tx: tokio::sync::oneshot::Sender<CatalystResult<Uuid>>,
    },
    Unsubscribe {
        subscription_id: Uuid,
        response_tx: tokio::sync::oneshot::Sender<CatalystResult<()>>,
    },
    GetHistory {
        filter: EventFilter,
        limit: Option<usize>,
        response_tx: tokio::sync::oneshot::Sender<CatalystResult<Vec<BlockchainEvent>>>,
    },
    Disconnect,
}

impl ServiceBusClient {
    /// Create a new Service Bus client
    pub fn new(config: ClientConfig) -> Self {
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let subscriptions = Arc::new(RwLock::new(HashMap::new()));
        let connection_state = Arc::new(RwLock::new(ConnectionState::Disconnected));
        
        let client = Self {
            config: config.clone(),
            subscriptions: Arc::clone(&subscriptions),
            connection_state: Arc::clone(&connection_state),
            command_tx,
            event_rx: Arc::new(RwLock::new(None)),
        };
        
        // Spawn the client task
        tokio::spawn(Self::client_task(
            config,
            command_rx,
            subscriptions,
            connection_state,
        ));
        
        client
    }
    
    /// Connect to the Service Bus
    pub async fn connect(&self) -> CatalystResult<()> {
        // Connection is handled automatically by the client task
        // Just wait for connection to be established
        let mut attempts = 0;
        let max_attempts = 30; // 30 seconds
        
        loop {
            let state = self.connection_state.read().await.clone();
            match state {
                ConnectionState::Connected => return Ok(()),
                ConnectionState::Failed { error } => {
                    return Err(ServiceBusError::ConnectionClosed.into());
                }
                _ => {
                    if attempts >= max_attempts {
                        return Err(ServiceBusError::NetworkError("Connection timeout".to_string()).into());
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    attempts += 1;
                }
            }
        }
    }
    
    /// Subscribe to events with a callback
    pub async fn subscribe<F>(&self, filter: EventFilter, callback: F) -> CatalystResult<Uuid>
    where
        F: Fn(BlockchainEvent) + Send + Sync + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let callback = Arc::new(callback);
        
        self.command_tx.send(ClientCommand::Subscribe {
            filter,
            callback,
            response_tx: tx,
        }).map_err(|_| ServiceBusError::InternalError("Client task not running".to_string()))?;
        
        rx.await.map_err(|_| ServiceBusError::InternalError("Response channel closed".to_string()))?
    }
    
    /// Unsubscribe from events
    pub async fn unsubscribe(&self, subscription_id: Uuid) -> CatalystResult<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        self.command_tx.send(ClientCommand::Unsubscribe {
            subscription_id,
            response_tx: tx,
        }).map_err(|_| ServiceBusError::InternalError("Client task not running".to_string()))?;
        
        rx.await.map_err(|_| ServiceBusError::InternalError("Response channel closed".to_string()))?
    }
    
    /// Get historical events
    pub async fn get_history(&self, filter: EventFilter, limit: Option<usize>) -> CatalystResult<Vec<BlockchainEvent>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        self.command_tx.send(ClientCommand::GetHistory {
            filter,
            limit,
            response_tx: tx,
        }).map_err(|_| ServiceBusError::InternalError("Client task not running".to_string()))?;
        
        rx.await.map_err(|_| ServiceBusError::InternalError("Response channel closed".to_string()))?
    }
    
    /// Get current connection state
    pub async fn get_connection_state(&self) -> ConnectionState {
        self.connection_state.read().await.clone()
    }
    
    /// Disconnect from the Service Bus
    pub async fn disconnect(&self) -> CatalystResult<()> {
        self.command_tx.send(ClientCommand::Disconnect)
            .map_err(|_| ServiceBusError::InternalError("Client task not running".to_string()))?;
        
        // Wait for disconnection
        let mut attempts = 0;
        while attempts < 10 {
            let state = self.connection_state.read().await.clone();
            if matches!(state, ConnectionState::Disconnected) {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            attempts += 1;
        }
        
        Ok(())
    }
    
    /// Main client task
    async fn client_task(
        config: ClientConfig,
        mut command_rx: mpsc::UnboundedReceiver<ClientCommand>,
        subscriptions: Arc<RwLock<HashMap<Uuid, EventCallback>>>,
        connection_state: Arc<RwLock<ConnectionState>>,
    ) {
        let mut reconnect_attempts = 0;
        let mut reconnect_delay = config.reconnect.initial_delay;
        
        loop {
            // Update connection state
            *connection_state.write().await = if reconnect_attempts == 0 {
                ConnectionState::Connecting
            } else {
                ConnectionState::Reconnecting { attempt: reconnect_attempts }
            };
            
            // Attempt to connect
            match Self::connect_websocket(&config).await {
                Ok((ws_stream, _)) => {
                    catalyst_utils::logging::log_info!(
                        LogCategory::ServiceBus,
                        "Connected to Service Bus at {}",
                        config.url
                    );
                    
                    *connection_state.write().await = ConnectionState::Connected;
                    reconnect_attempts = 0;
                    reconnect_delay = config.reconnect.initial_delay;
                    
                    // Handle the connection
                    let disconnect_reason = Self::handle_connection(
                        ws_stream,
                        &mut command_rx,
                        &subscriptions,
                        &connection_state,
                    ).await;
                    
                    catalyst_utils::logging::log_info!(
                        LogCategory::ServiceBus,
                        "Disconnected from Service Bus: {:?}",
                        disconnect_reason
                    );
                    
                    // Check if disconnection was intentional
                    if matches!(disconnect_reason, DisconnectReason::Intentional) {
                        *connection_state.write().await = ConnectionState::Disconnected;
                        break;
                    }
                }
                Err(e) => {
                    catalyst_utils::logging::log_error!(
                        LogCategory::ServiceBus,
                        "Failed to connect to Service Bus: {}",
                        e
                    );
                    
                    reconnect_attempts += 1;
                    
                    if !config.reconnect.enabled || reconnect_attempts > config.reconnect.max_attempts {
                        *connection_state.write().await = ConnectionState::Failed {
                            error: e.to_string(),
                        };
                        break;
                    }
                }
            }
            
            // Wait before reconnecting
            if config.reconnect.enabled && reconnect_attempts <= config.reconnect.max_attempts {
                catalyst_utils::logging::log_info!(
                    LogCategory::ServiceBus,
                    "Reconnecting in {:?} (attempt {}/{})",
                    reconnect_delay,
                    reconnect_attempts,
                    config.reconnect.max_attempts
                );
                
                tokio::time::sleep(reconnect_delay).await;
                
                // Exponential backoff
                reconnect_delay = std::cmp::min(
                    Duration::from_millis(
                        (reconnect_delay.as_millis() as f64 * config.reconnect.backoff_multiplier) as u64
                    ),
                    config.reconnect.max_delay,
                );
            } else {
                break;
            }
        }
    }
    
    /// Connect to WebSocket
    async fn connect_websocket(
        config: &ClientConfig,
    ) -> CatalystResult<(tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, tokio_tungstenite::tungstenite::http::Response<Option<Vec<u8>>>)> {
        let mut request = tokio_tungstenite::tungstenite::http::Request::builder()
            .uri(&config.url);
        
        // Add authentication headers
        if let Some(ref token) = config.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        
        if let Some(ref api_key) = config.api_key {
            request = request.header("X-API-Key", api_key);
        }
        
        let request = request.body(()).map_err(|e| {
            ServiceBusError::NetworkError(format!("Failed to build request: {}", e))
        })?;
        
        timeout(config.connect_timeout, connect_async(request))
            .await
            .map_err(|_| ServiceBusError::NetworkError("Connection timeout".to_string()))?
            .map_err(|e| ServiceBusError::NetworkError(format!("WebSocket connection failed: {}", e)))
    }
    
    /// Handle an active WebSocket connection
    async fn handle_connection(
        ws_stream: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
        command_rx: &mut mpsc::UnboundedReceiver<ClientCommand>,
        subscriptions: &Arc<RwLock<HashMap<Uuid, EventCallback>>>,
        connection_state: &Arc<RwLock<ConnectionState>>,
    ) -> DisconnectReason {
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        let mut ping_interval = interval(Duration::from_secs(30));
        let mut pending_responses: HashMap<String, tokio::sync::oneshot::Sender<WsMessage>> = HashMap::new();
        
        loop {
            tokio::select! {
                // Handle incoming WebSocket messages
                msg = ws_receiver.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<WsMessage>(&text) {
                                Ok(ws_message) => {
                                    if let Err(_) = Self::handle_ws_message(
                                        ws_message,
                                        subscriptions,
                                        &mut pending_responses,
                                    ).await {
                                        return DisconnectReason::Error;
                                    }
                                }
                                Err(e) => {
                                    catalyst_utils::logging::log_error!(
                                        LogCategory::ServiceBus,
                                        "Failed to parse WebSocket message: {}",
                                        e
                                    );
                                }
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            if ws_sender.send(Message::Pong(data)).await.is_err() {
                                return DisconnectReason::Error;
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            return DisconnectReason::ServerClosed;
                        }
                        Some(Err(e)) => {
                            catalyst_utils::logging::log_error!(
                                LogCategory::ServiceBus,
                                "WebSocket error: {}",
                                e
                            );
                            return DisconnectReason::Error;
                        }
                        None => {
                            return DisconnectReason::Error;
                        }
                        _ => {
                            // Ignore other message types
                        }
                    }
                }
                
                // Handle client commands
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(ClientCommand::Subscribe { filter, callback, response_tx }) => {
                            let result = Self::send_subscribe(
                                &mut ws_sender,
                                filter,
                                callback,
                                subscriptions,
                                &mut pending_responses,
                            ).await;
                            let _ = response_tx.send(result);
                        }
                        Some(ClientCommand::Unsubscribe { subscription_id, response_tx }) => {
                            let result = Self::send_unsubscribe(
                                &mut ws_sender,
                                subscription_id,
                                subscriptions,
                            ).await;
                            let _ = response_tx.send(result);
                        }
                        Some(ClientCommand::GetHistory { filter, limit, response_tx }) => {
                            let result = Self::send_get_history(
                                &mut ws_sender,
                                filter,
                                limit,
                                &mut pending_responses,
                            ).await;
                            let _ = response_tx.send(result);
                        }
                        Some(ClientCommand::Disconnect) => {
                            let _ = ws_sender.send(Message::Close(None)).await;
                            return DisconnectReason::Intentional;
                        }
                        None => {
                            return DisconnectReason::Error;
                        }
                    }
                }
                
                // Send periodic pings
                _ = ping_interval.tick() => {
                    let ping_msg = WsMessage::Ping;
                    if let Ok(json) = serde_json::to_string(&ping_msg) {
                        if ws_sender.send(Message::Text(json)).await.is_err() {
                            return DisconnectReason::Error;
                        }
                    }
                }
            }
        }
    }
    
    /// Handle incoming WebSocket message
    async fn handle_ws_message(
        message: WsMessage,
        subscriptions: &Arc<RwLock<HashMap<Uuid, EventCallback>>>,
        pending_responses: &mut HashMap<String, tokio::sync::oneshot::Sender<WsMessage>>,
    ) -> CatalystResult<()> {
        match message {
            WsMessage::Event { subscription_id, event } => {
                let subs = subscriptions.read().await;
                if let Some(callback) = subs.get(&subscription_id) {
                    callback(event);
                }
            }
            WsMessage::SubscriptionCreated { .. } |
            WsMessage::SubscriptionRemoved { .. } |
            WsMessage::HistoricalEvents { .. } |
            WsMessage::Error { .. } => {
                // These would be handled by pending response handlers
                // For now, just log them
                catalyst_utils::logging::log_debug!(
                    LogCategory::ServiceBus,
                    "Received response message: {:?}",
                    message
                );
            }
            WsMessage::Pong => {
                // Pong received, connection is alive
            }
            _ => {
                // Ignore other message types
            }
        }
        
        Ok(())
    }
    
    /// Send subscribe message
    async fn send_subscribe(
        ws_sender: &mut futures::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>,
        filter: EventFilter,
        callback: EventCallback,
        subscriptions: &Arc<RwLock<HashMap<Uuid, EventCallback>>>,
        pending_responses: &mut HashMap<String, tokio::sync::oneshot::Sender<WsMessage>>,
    ) -> CatalystResult<Uuid> {
        let subscribe_msg = WsMessage::Subscribe {
            filter,
            replay: Some(false),
        };
        
        let json = serde_json::to_string(&subscribe_msg)?;
        ws_sender.send(Message::Text(json)).await?;
        
        // For simplicity, generate a client-side subscription ID
        // In a full implementation, you'd wait for the server response
        let subscription_id = Uuid::new_v4();
        subscriptions.write().await.insert(subscription_id, callback);
        
        Ok(subscription_id)
    }
    
    /// Send unsubscribe message
    async fn send_unsubscribe(
        ws_sender: &mut futures::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>,
        subscription_id: Uuid,
        subscriptions: &Arc<RwLock<HashMap<Uuid, EventCallback>>>,
    ) -> CatalystResult<()> {
        let unsubscribe_msg = WsMessage::Unsubscribe { subscription_id };
        
        let json = serde_json::to_string(&unsubscribe_msg)?;
        ws_sender.send(Message::Text(json)).await?;
        
        subscriptions.write().await.remove(&subscription_id);
        
        Ok(())
    }
    
    /// Send get history message
    async fn send_get_history(
        ws_sender: &mut futures::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, Message>,
        filter: EventFilter,
        limit: Option<usize>,
        pending_responses: &mut HashMap<String, tokio::sync::oneshot::Sender<WsMessage>>,
    ) -> CatalystResult<Vec<BlockchainEvent>> {
        let history_msg = WsMessage::GetHistory { filter, limit };
        
        let json = serde_json::to_string(&history_msg)?;
        ws_sender.send(Message::Text(json)).await?;
        
        // For simplicity, return empty vector
        // In a full implementation, you'd wait for the server response
        Ok(Vec::new())
    }
}

/// Reason for disconnection
#[derive(Debug)]
enum DisconnectReason {
    Intentional,
    ServerClosed,
    Error,
}

/// Simple client for basic usage
pub struct SimpleClient {
    client: ServiceBusClient,
    event_tx: broadcast::Sender<BlockchainEvent>,
    _event_rx: broadcast::Receiver<BlockchainEvent>,
}

impl SimpleClient {
    /// Create a new simple client
    pub fn new(url: &str) -> Self {
        let config = ClientConfig {
            url: url.to_string(),
            ..Default::default()
        };
        
        let client = ServiceBusClient::new(config);
        let (event_tx, event_rx) = broadcast::channel(1000);
        
        Self {
            client,
            event_tx,
            _event_rx: event_rx,
        }
    }
    
    /// Connect to the service bus
    pub async fn connect(&self) -> CatalystResult<()> {
        self.client.connect().await
    }
    
    /// Subscribe to events of a specific type
    pub async fn subscribe_to_event_type(&self, event_type: EventType) -> CatalystResult<broadcast::Receiver<BlockchainEvent>> {
        let rx = self.event_tx.subscribe();
        let tx = self.event_tx.clone();
        
        let filter = EventFilter::new().with_event_types(vec![event_type]);
        
        self.client.subscribe(filter, move |event| {
            let _ = tx.send(event);
        }).await?;
        
        Ok(rx)
    }
    
    /// Subscribe to token transfers
    pub async fn subscribe_to_token_transfers(&self) -> CatalystResult<broadcast::Receiver<BlockchainEvent>> {
        self.subscribe_to_event_type(EventType::TokenTransfer).await
    }
    
    /// Subscribe to transaction confirmations
    pub async fn subscribe_to_transactions(&self) -> CatalystResult<broadcast::Receiver<BlockchainEvent>> {
        self.subscribe_to_event_type(EventType::TransactionConfirmed).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_client_config_default() {
        let config = ClientConfig::default();
        assert_eq!(config.url, "ws://localhost:8080/ws");
        assert!(config.reconnect.enabled);
        assert_eq!(config.buffer_size, 1000);
    }

    #[test]
    fn test_reconnect_config() {
        let config = ReconnectConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_attempts, 10);
        assert_eq!(config.backoff_multiplier, 2.0);
    }

    #[tokio::test]
    async fn test_client_creation() {
        let config = ClientConfig::default();
        let client = ServiceBusClient::new(config);
        
        let state = client.get_connection_state().await;
        // Initially should be disconnected or connecting
        assert!(matches!(state, ConnectionState::Disconnected | ConnectionState::Connecting));
    }

    #[tokio::test]
    async fn test_simple_client() {
        let client = SimpleClient::new("ws://localhost:8080/ws");
        
        // Test subscription creation (won't actually connect in test)
        // This mainly tests the interface
        let result = client.subscribe_to_token_transfers().await;
        // Will fail due to no actual server, but tests the API
    }

    #[test]
    fn test_connection_state() {
        assert_eq!(ConnectionState::Disconnected, ConnectionState::Disconnected);
        assert_ne!(ConnectionState::Connected, ConnectionState::Disconnected);
        
        let reconnecting = ConnectionState::Reconnecting { attempt: 5 };
        match reconnecting {
            ConnectionState::Reconnecting { attempt } => assert_eq!(attempt, 5),
            _ => panic!("Wrong state type"),
        }
    }
}