//! WebSocket server implementation for real-time event streaming

use crate::{
    config::WebSocketConfig,
    events::{BlockchainEvent, EventFilter},
    filters::FilterEngine,
    error::ServiceBusError,
};
use catalyst_utils::{CatalystResult, logging::LogCategory};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State, Query,
    },
    response::Response,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::broadcast,
    time::{interval, timeout},
};
use uuid::Uuid;

/// WebSocket message types for client-server communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    /// Subscribe to events with filter
    Subscribe {
        filter: EventFilter,
        replay: Option<bool>,
    },
    
    /// Unsubscribe from events
    Unsubscribe {
        subscription_id: Uuid,
    },
    
    /// List active subscriptions
    ListSubscriptions,
    
    /// Get historical events
    GetHistory {
        filter: EventFilter,
        limit: Option<usize>,
    },
    
    /// Ping for connection health
    Ping,
    
    /// Response to ping
    Pong,
    
    /// Event data from server to client
    Event {
        subscription_id: Uuid,
        event: BlockchainEvent,
    },
    
    /// Subscription confirmation
    SubscriptionCreated {
        subscription_id: Uuid,
    },
    
    /// Subscription removed confirmation
    SubscriptionRemoved {
        subscription_id: Uuid,
    },
    
    /// Historical events response
    HistoricalEvents {
        events: Vec<BlockchainEvent>,
    },
    
    /// List of active subscriptions
    SubscriptionList {
        subscriptions: Vec<SubscriptionInfo>,
    },
    
    /// Error response
    Error {
        message: String,
        code: Option<String>,
    },
}

/// Subscription information for client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionInfo {
    pub id: Uuid,
    pub filter: EventFilter,
    pub created_at: u64,
}

/// WebSocket connection query parameters
#[derive(Debug, Deserialize)]
pub struct WsQuery {
    /// Authentication token
    pub token: Option<String>,
    
    /// Connection ID for tracking
    pub connection_id: Option<String>,
}

/// WebSocket connection state
pub struct WebSocketConnection {
    /// Connection ID
    pub id: String,
    
    /// Filter engine reference
    pub filter_engine: Arc<FilterEngine>,
    
    /// Active subscriptions for this connection
    pub subscription_receivers: HashMap<Uuid, broadcast::Receiver<BlockchainEvent>>,
    
    /// WebSocket configuration
    pub config: WebSocketConfig,
}

impl WebSocketConnection {
    /// Create a new WebSocket connection
    pub fn new(
        connection_id: String,
        filter_engine: Arc<FilterEngine>,
        config: WebSocketConfig,
    ) -> Self {
        Self {
            id: connection_id,
            filter_engine,
            subscription_receivers: HashMap::new(),
            config,
        }
    }
}

/// WebSocket handler state
#[derive(Clone)]
pub struct WebSocketState {
    pub filter_engine: Arc<FilterEngine>,
    pub config: WebSocketConfig,
}

/// Handle WebSocket upgrade
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsQuery>,
    State(state): State<WebSocketState>,
) -> Response {
    let connection_id = params.connection_id
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    
    catalyst_utils::logging::log_info!(
        LogCategory::ServiceBus,
        "WebSocket connection established: {}",
        connection_id
    );
    
    ws.on_upgrade(move |socket| {
        handle_websocket_connection(socket, connection_id, state)
    })
}

/// Handle individual WebSocket connection
async fn handle_websocket_connection(
    socket: WebSocket,
    connection_id: String,
    state: WebSocketState,
) {
    let mut connection = WebSocketConnection::new(
        connection_id.clone(),
        state.filter_engine,
        state.config,
    );
    
    let (mut sender, mut receiver) = socket.split();
    
    // Setup ping interval
    let mut ping_interval = interval(connection.config.ping_interval);
    
    // Connection handling loop
    loop {
        tokio::select! {
            // Handle incoming messages from client
            msg = receiver.next() => {
                match msg {
                    Some(Ok(msg)) => {
                        if let Err(e) = handle_client_message(&mut connection, &mut sender, msg).await {
                            catalyst_utils::logging::log_error!(
                                LogCategory::ServiceBus,
                                "Error handling client message for {}: {}",
                                connection_id,
                                e
                            );
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        catalyst_utils::logging::log_error!(
                            LogCategory::ServiceBus,
                            "WebSocket error for {}: {}",
                            connection_id,
                            e
                        );
                        break;
                    }
                    None => {
                        catalyst_utils::logging::log_info!(
                            LogCategory::ServiceBus,
                            "WebSocket connection closed: {}",
                            connection_id
                        );
                        break;
                    }
                }
            }
            
            // Handle events from subscriptions
            _ = handle_subscription_events(&mut connection, &mut sender) => {
                // Continue processing
            }
            
            // Send periodic pings
            _ = ping_interval.tick() => {
                let ping_msg = WsMessage::Ping;
                if let Ok(msg_json) = serde_json::to_string(&ping_msg) {
                    if sender.send(Message::Text(msg_json)).await.is_err() {
                        catalyst_utils::logging::log_warn!(
                            LogCategory::ServiceBus,
                            "Failed to send ping to {}",
                            connection_id
                        );
                        break;
                    }
                }
            }
        }
    }
    
    // Cleanup on disconnect
    connection.filter_engine.remove_connection_subscriptions(&connection_id);
    
    catalyst_utils::logging::log_info!(
        LogCategory::ServiceBus,
        "WebSocket connection cleaned up: {}",
        connection_id
    );
}

/// Handle incoming message from client
async fn handle_client_message(
    connection: &mut WebSocketConnection,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    message: Message,
) -> CatalystResult<()> {
    let response = match message {
        Message::Text(text) => {
            let ws_message: WsMessage = serde_json::from_str(&text)
                .map_err(|e| ServiceBusError::InvalidMessage(format!("Invalid JSON: {}", e)))?;
            
            handle_ws_message(connection, ws_message).await?
        }
        Message::Binary(_) => {
            Some(WsMessage::Error {
                message: "Binary messages not supported".to_string(),
                code: Some("BINARY_NOT_SUPPORTED".to_string()),
            })
        }
        Message::Ping(data) => {
            sender
                .send(Message::Pong(data))
                .await
                .map_err(ServiceBusError::from)?;
            None
        }
        Message::Pong(_) => {
            // Pong received, connection is alive
            None
        }
        Message::Close(_) => {
            return Err(ServiceBusError::ConnectionClosed.into());
        }
    };
    
    // Send response if any
    if let Some(response_msg) = response {
        let response_json = serde_json::to_string(&response_msg).map_err(ServiceBusError::from)?;
        sender
            .send(Message::Text(response_json))
            .await
            .map_err(ServiceBusError::from)?;
    }
    
    Ok(())
}

/// Handle WebSocket message
async fn handle_ws_message(
    connection: &mut WebSocketConnection,
    message: WsMessage,
) -> CatalystResult<Option<WsMessage>> {
    match message {
        WsMessage::Subscribe { filter, replay } => {
            let (subscription_id, receiver) = connection.filter_engine.add_subscription(
                connection.id.clone(),
                filter.clone(),
            )?;
            
            connection.subscription_receivers.insert(subscription_id, receiver);
            
            // Handle replay if requested
            if replay.unwrap_or(false) {
                let historical_events = connection.filter_engine
                    .get_historical_events(&filter, Some(1000))
                    .await?;
                
                if !historical_events.is_empty() {
                    return Ok(Some(WsMessage::HistoricalEvents {
                        events: historical_events,
                    }));
                }
            }
            
            Ok(Some(WsMessage::SubscriptionCreated { subscription_id }))
        }
        
        WsMessage::Unsubscribe { subscription_id } => {
            connection.filter_engine.remove_subscription(
                subscription_id,
                &connection.id,
            )?;
            
            connection.subscription_receivers.remove(&subscription_id);
            
            Ok(Some(WsMessage::SubscriptionRemoved { subscription_id }))
        }
        
        WsMessage::ListSubscriptions => {
            let subscriptions = connection.filter_engine
                .get_connection_subscriptions(&connection.id)
                .into_iter()
                .map(|sub| SubscriptionInfo {
                    id: sub.id,
                    filter: sub.filter,
                    created_at: sub.created_at,
                })
                .collect();
            
            Ok(Some(WsMessage::SubscriptionList { subscriptions }))
        }
        
        WsMessage::GetHistory { filter, limit } => {
            let events = connection.filter_engine
                .get_historical_events(&filter, limit)
                .await?;
            
            Ok(Some(WsMessage::HistoricalEvents { events }))
        }
        
        WsMessage::Ping => {
            Ok(Some(WsMessage::Pong))
        }
        
        WsMessage::Pong => {
            // Client responded to our ping
            Ok(None)
        }
        
        _ => {
            Ok(Some(WsMessage::Error {
                message: "Unsupported message type".to_string(),
                code: Some("UNSUPPORTED_MESSAGE".to_string()),
            }))
        }
    }
}

/// Handle events from active subscriptions
async fn handle_subscription_events(
    connection: &mut WebSocketConnection,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) -> CatalystResult<()> {
    // Check all subscription receivers for new events
    let mut events_to_send = Vec::new();
    let mut dead_subscriptions = Vec::new();
    
    for (subscription_id, receiver) in &mut connection.subscription_receivers {
        match timeout(Duration::from_millis(1), receiver.recv()).await {
            Ok(Ok(event)) => {
                events_to_send.push((*subscription_id, event));
            }
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => {
                catalyst_utils::logging::log_warn!(
                    LogCategory::ServiceBus,
                    "Subscription {} lagged, some events may be missed",
                    subscription_id
                );
            }
            Ok(Err(broadcast::error::RecvError::Closed)) => {
                dead_subscriptions.push(*subscription_id);
            }
            Err(_) => {
                // Timeout - no events available
            }
        }
    }
    
    // Remove dead subscriptions
    for subscription_id in dead_subscriptions {
        connection.subscription_receivers.remove(&subscription_id);
    }
    
    // Send events to client
    for (subscription_id, event) in events_to_send {
        let ws_message = WsMessage::Event {
            subscription_id,
            event,
        };
        
        let message_json = serde_json::to_string(&ws_message).map_err(ServiceBusError::from)?;
        
        if let Err(e) = sender.send(Message::Text(message_json)).await {
            catalyst_utils::logging::log_error!(
                LogCategory::ServiceBus,
                "Failed to send event to client {}: {}",
                connection.id,
                e
            );
            return Err(ServiceBusError::from(e).into());
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::ServiceBusConfig, events::EventType};
    use tokio_tungstenite::{connect_async, tungstenite::Message as TungsteniteMessage};

    #[tokio::test]
    async fn test_ws_message_serialization() {
        let filter = EventFilter::new()
            .with_event_types(vec![EventType::TransactionConfirmed]);
        
        let subscribe_msg = WsMessage::Subscribe {
            filter: filter.clone(),
            replay: Some(true),
        };
        
        let json = serde_json::to_string(&subscribe_msg).unwrap();
        let deserialized: WsMessage = serde_json::from_str(&json).unwrap();
        
        match deserialized {
            WsMessage::Subscribe { filter: f, replay } => {
                assert_eq!(replay, Some(true));
                assert!(f.event_types.is_some());
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[tokio::test]
    async fn test_websocket_connection_creation() {
        let config = WebSocketConfig::default();
        let filter_engine = Arc::new(FilterEngine::new(10, true, 100));
        
        let connection = WebSocketConnection::new(
            "test_connection".to_string(),
            filter_engine,
            config,
        );
        
        assert_eq!(connection.id, "test_connection");
        assert!(connection.subscription_receivers.is_empty());
    }

    #[tokio::test]
    async fn test_ws_message_handling() {
        let config = WebSocketConfig::default();
        let filter_engine = Arc::new(FilterEngine::new(10, true, 100));
        
        let mut connection = WebSocketConnection::new(
            "test_connection".to_string(),
            filter_engine,
            config,
        );
        
        let filter = EventFilter::new()
            .with_event_types(vec![EventType::TokenTransfer]);
        
        let subscribe_msg = WsMessage::Subscribe {
            filter,
            replay: Some(false),
        };
        
        let response = handle_ws_message(&mut connection, subscribe_msg).await.unwrap();
        
        match response {
            Some(WsMessage::SubscriptionCreated { subscription_id }) => {
                assert!(connection.subscription_receivers.contains_key(&subscription_id));
            }
            _ => panic!("Expected SubscriptionCreated response"),
        }
    }
}