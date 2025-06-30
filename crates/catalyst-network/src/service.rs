//! Main network service implementation

use crate::{
    config::NetworkConfig,
    error::{NetworkError, NetworkResult},
    swarm::CatalystSwarm,
    messaging::{MessageRouter, MessageHandler, NetworkMessage},
    discovery::PeerDiscovery,
    reputation::ReputationManager,
    monitoring::HealthMonitor,
    bandwidth::BandwidthManager,
};

use catalyst_utils::{
    CatalystResult,
    logging::*,
    metrics::*,
    network::{NetworkMessage as CatalystNetworkMessage, MessageType},
};

use libp2p::{PeerId, Multiaddr, identity::Keypair, swarm::SwarmEvent};
use std::{
    collections::HashMap,
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{mpsc, RwLock, Mutex, oneshot},
    task::JoinHandle,
    time::{interval, timeout},
};

/// Main network service that coordinates all networking components
pub struct NetworkService {
    /// Network configuration
    config: NetworkConfig,
    
    /// libp2p swarm manager
    swarm: Arc<Mutex<CatalystSwarm>>,
    
    /// Message router
    router: Arc<MessageRouter>,
    
    /// Peer discovery service
    discovery: Arc<PeerDiscovery>,
    
    /// Reputation manager
    reputation: Arc<ReputationManager>,
    
    /// Health monitor
    health_monitor: Arc<HealthMonitor>,
    
    /// Bandwidth manager
    bandwidth_manager: Arc<BandwidthManager>,
    
    /// Service state
    state: Arc<RwLock<ServiceState>>,
    
    /// Command channel for external control
    command_tx: mpsc::UnboundedSender<NetworkCommand>,
    command_rx: Mutex<Option<mpsc::UnboundedReceiver<NetworkCommand>>>,
    
    /// Event channel for external notifications
    event_tx: Arc<RwLock<Vec<mpsc::UnboundedSender<NetworkEvent>>>>,
    
    /// Background task handles
    task_handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

/// Network service state
#[derive(Debug, Clone, PartialEq)]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error(String),
}

/// Commands for controlling the network service
#[derive(Debug)]
pub enum NetworkCommand {
    /// Start the network service
    Start,
    
    /// Stop the network service
    Stop,
    
    /// Connect to a specific peer
    Connect { 
        peer_id: PeerId, 
        address: Multiaddr,
        response: oneshot::Sender<NetworkResult<()>>,
    },
    
    /// Disconnect from a peer
    Disconnect { 
        peer_id: PeerId,
        response: oneshot::Sender<NetworkResult<()>>,
    },
    
    /// Send message to peer(s)
    SendMessage { 
        message: NetworkMessage,
        response: oneshot::Sender<NetworkResult<()>>,
    },
    
    /// Register message handler
    RegisterHandler {
        handler: Box<dyn MessageHandler>,
        response: oneshot::Sender<NetworkResult<()>>,
    },
    
    /// Get network statistics
    GetStats {
        response: oneshot::Sender<NetworkResult<NetworkStats>>,
    },
    
    /// Update configuration
    UpdateConfig {
        config: NetworkConfig,
        response: oneshot::Sender<NetworkResult<()>>,
    },
}

/// Network events for external subscribers
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// Peer connected
    PeerConnected { peer_id: PeerId, address: Multiaddr },
    
    /// Peer disconnected
    PeerDisconnected { peer_id: PeerId, reason: String },
    
    /// Message received
    MessageReceived { message: NetworkMessage, sender: PeerId },
    
    /// Message sent
    MessageSent { message_id: String, target: PeerId },
    
    /// Network error occurred
    Error { error: NetworkError },
    
    /// Service state changed
    StateChanged { old_state: ServiceState, new_state: ServiceState },
    
    /// Discovery event
    DiscoveryEvent { event: String, peer_id: Option<PeerId> },
    
    /// Reputation update
    ReputationUpdate { peer_id: PeerId, old_score: f64, new_score: f64 },
}

/// Network statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub connected_peers: usize,
    pub total_connections: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors: u64,
    pub uptime: Duration,
    pub average_latency: Duration,
    pub bandwidth_usage: BandwidthUsage,
}

#[derive(Debug, Clone)]
pub struct BandwidthUsage {
    pub upload_rate: f64,   // bytes/sec
    pub download_rate: f64, // bytes/sec
    pub total_uploaded: u64,
    pub total_downloaded: u64,
}

impl NetworkService {
    /// Create new network service
    pub async fn new(config: NetworkConfig) -> NetworkResult<Self> {
        log_info!(LogCategory::Network, "Creating new network service");
        
        // Validate configuration
        config.validate()?;
        
        // Create libp2p swarm
        let swarm = CatalystSwarm::new(&config).await?;
        let swarm = Arc::new(Mutex::new(swarm));
        
        // Create components
        let router = Arc::new(MessageRouter::new());
        let discovery = Arc::new(PeerDiscovery::new(&config.discovery));
        let reputation = Arc::new(ReputationManager::new(&config.reputation));
        let health_monitor = Arc::new(HealthMonitor::new(&config.monitoring));
        let bandwidth_manager = Arc::new(BandwidthManager::new(&config.bandwidth));
        
        // Create command channel
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        
        let service = Self {
            config,
            swarm,
            router,
            discovery,
            reputation,
            health_monitor,
            bandwidth_manager,
            state: Arc::new(RwLock::new(ServiceState::Stopped)),
            command_tx,
            command_rx: Mutex::new(Some(command_rx)),
            event_tx: Arc::new(RwLock::new(Vec::new())),
            task_handles: Arc::new(Mutex::new(Vec::new())),
        };
        
        log_info!(LogCategory::Network, "Network service created successfully");
        Ok(service)
    }
    
    /// Start the network service
    pub async fn start(&self) -> NetworkResult<()> {
        log_info!(LogCategory::Network, "Starting network service");
        
        // Check if already running
        {
            let state = self.state.read().await;
            if *state == ServiceState::Running {
                return Ok(());
            }
        }
        
        // Set starting state
        self.set_state(ServiceState::Starting).await;
        
        // Take command receiver
        let command_rx = self.command_rx.lock().await.take()
            .ok_or_else(|| NetworkError::NotInitialized)?;
        
        // Start background tasks
        let mut handles = self.task_handles.lock().await;
        
        // Start main event loop
        handles.push(tokio::spawn(self.clone().run_event_loop(command_rx)));
        
        // Start discovery service
        handles.push(tokio::spawn(self.clone().run_discovery()));
        
        // Start reputation manager
        handles.push(tokio::spawn(self.clone().run_reputation_manager()));
        
        // Start health monitor
        handles.push(tokio::spawn(self.clone().run_health_monitor()));
        
        // Start bandwidth monitor
        handles.push(tokio::spawn(self.clone().run_bandwidth_monitor()));
        
        // Start metrics collection
        handles.push(tokio::spawn(self.clone().run_metrics_collection()));
        
        // Set running state
        self.set_state(ServiceState::Running).await;
        
        log_info!(LogCategory::Network, "Network service started successfully");
        increment_counter!("network_service_starts_total", 1);
        
        Ok(())
    }
    
    /// Stop the network service
    pub async fn stop(&self) -> NetworkResult<()> {
        log_info!(LogCategory::Network, "Stopping network service");
        
        // Set stopping state
        self.set_state(ServiceState::Stopping).await;
        
        // Cancel all background tasks
        let mut handles = self.task_handles.lock().await;
        for handle in handles.drain(..) {
            handle.abort();
        }
        
        // Close swarm
        {
            let mut swarm = self.swarm.lock().await;
            // Disconnect all peers gracefully
            // swarm.disconnect_all().await;
        }
        
        // Set stopped state
        self.set_state(ServiceState::Stopped).await;
        
        log_info!(LogCategory::Network, "Network service stopped");
        increment_counter!("network_service_stops_total", 1);
        
        Ok(())
    }
    
    /// Get service command sender
    pub fn command_sender(&self) -> mpsc::UnboundedSender<NetworkCommand> {
        self.command_tx.clone()
    }
    
    /// Subscribe to network events
    pub async fn subscribe_events(&self) -> mpsc::UnboundedReceiver<NetworkEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.event_tx.write().await.push(tx);
        rx
    }
    
    /// Send message to network
    pub async fn send_message<T: CatalystNetworkMessage>(
        &self,
        message: &T,
        target: Option<PeerId>,
    ) -> NetworkResult<()> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let network_message = NetworkMessage::new(
            message,
            "local_node".to_string(),
            target.map(|p| p.to_string()),
            match target {
                Some(peer) => crate::messaging::RoutingStrategy::Direct(peer.to_string()),
                None => crate::messaging::RoutingStrategy::Broadcast,
            },
        )?;
        
        let command = NetworkCommand::SendMessage {
            message: network_message,
            response: response_tx,
        };
        
        self.command_tx.send(command)
            .map_err(|_| NetworkError::NotInitialized)?;
        
        timeout(Duration::from_secs(10), response_rx)
            .await
            .map_err(|_| NetworkError::Timeout { duration: Duration::from_secs(10) })?
            .map_err(|_| NetworkError::NotInitialized)?
    }
    
    /// Register message handler
    pub async fn register_handler<H>(&self, handler: H) -> NetworkResult<()>
    where
        H: MessageHandler + 'static,
    {
        let (response_tx, response_rx) = oneshot::channel();
        
        let command = NetworkCommand::RegisterHandler {
            handler: Box::new(handler),
            response: response_tx,
        };
        
        self.command_tx.send(command)
            .map_err(|_| NetworkError::NotInitialized)?;
        
        timeout(Duration::from_secs(5), response_rx)
            .await
            .map_err(|_| NetworkError::Timeout { duration: Duration::from_secs(5) })?
            .map_err(|_| NetworkError::NotInitialized)?
    }
    
    /// Connect to a peer
    pub async fn connect_peer(&self, peer_id: PeerId, address: Multiaddr) -> NetworkResult<()> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let command = NetworkCommand::Connect {
            peer_id,
            address,
            response: response_tx,
        };
        
        self.command_tx.send(command)
            .map_err(|_| NetworkError::NotInitialized)?;
        
        timeout(Duration::from_secs(30), response_rx)
            .await
            .map_err(|_| NetworkError::Timeout { duration: Duration::from_secs(30) })?
            .map_err(|_| NetworkError::NotInitialized)?
    }
    
    /// Disconnect from a peer
    pub async fn disconnect_peer(&self, peer_id: PeerId) -> NetworkResult<()> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let command = NetworkCommand::Disconnect {
            peer_id,
            response: response_tx,
        };
        
        self.command_tx.send(command)
            .map_err(|_| NetworkError::NotInitialized)?;
        
        timeout(Duration::from_secs(10), response_rx)
            .await
            .map_err(|_| NetworkError::Timeout { duration: Duration::from_secs(10) })?
            .map_err(|_| NetworkError::NotInitialized)?
    }
    
    /// Get network statistics
    pub async fn get_stats(&self) -> NetworkResult<NetworkStats> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let command = NetworkCommand::GetStats {
            response: response_tx,
        };
        
        self.command_tx.send(command)
            .map_err(|_| NetworkError::NotInitialized)?;
        
        timeout(Duration::from_secs(5), response_rx)
            .await
            .map_err(|_| NetworkError::Timeout { duration: Duration::from_secs(5) })?
            .map_err(|_| NetworkError::NotInitialized)?
    }
    
    /// Get current service state
    pub async fn get_state(&self) -> ServiceState {
        self.state.read().await.clone()
    }
    
    /// Set service state and notify subscribers
    async fn set_state(&self, new_state: ServiceState) {
        let old_state = {
            let mut state = self.state.write().await;
            let old = state.clone();
            *state = new_state.clone();
            old
        };
        
        if old_state != new_state {
            let event = NetworkEvent::StateChanged { old_state, new_state };
            self.emit_event(event).await;
        }
    }
    
    /// Emit event to all subscribers
    async fn emit_event(&self, event: NetworkEvent) {
        let event_senders = self.event_tx.read().await;
        let mut failed_senders = Vec::new();
        
        for (index, sender) in event_senders.iter().enumerate() {
            if sender.send(event.clone()).is_err() {
                failed_senders.push(index);
            }
        }
        
        // Remove failed senders
        if !failed_senders.is_empty() {
            drop(event_senders);
            let mut event_senders = self.event_tx.write().await;
            for &index in failed_senders.iter().rev() {
                event_senders.remove(index);
            }
        }
    }
    
    /// Main event loop
    async fn run_event_loop(
        self: Arc<Self>,
        mut command_rx: mpsc::UnboundedReceiver<NetworkCommand>,
    ) {
        log_info!(LogCategory::Network, "Starting network event loop");
        
        let mut swarm_interval = interval(Duration::from_millis(100));
        let mut stats_interval = interval(Duration::from_secs(30));
        
        loop {
            tokio::select! {
                // Handle commands
                command = command_rx.recv() => {
                    match command {
                        Some(cmd) => {
                            if let Err(e) = self.handle_command(cmd).await {
                                log_error!(LogCategory::Network, "Command handling error: {}", e);
                                self.emit_event(NetworkEvent::Error { error: e }).await;
                            }
                        }
                        None => {
                            log_info!(LogCategory::Network, "Command channel closed, stopping event loop");
                            break;
                        }
                    }
                }
                
                // Poll swarm events
                _ = swarm_interval.tick() => {
                    if let Err(e) = self.poll_swarm_events().await {
                        log_error!(LogCategory::Network, "Swarm polling error: {}", e);
                    }
                }
                
                // Update statistics
                _ = stats_interval.tick() => {
                    self.update_metrics().await;
                }
            }
        }
        
        log_info!(LogCategory::Network, "Network event loop stopped");
    }
    
    /// Handle network commands
    async fn handle_command(&self, command: NetworkCommand) -> NetworkResult<()> {
        match command {
            NetworkCommand::Start => {
                // Already handled in start() method
                Ok(())
            }
            
            NetworkCommand::Stop => {
                self.stop().await
            }
            
            NetworkCommand::Connect { peer_id, address, response } => {
                let result = self.handle_connect(peer_id, address).await;
                let _ = response.send(result);
                Ok(())
            }
            
            NetworkCommand::Disconnect { peer_id, response } => {
                let result = self.handle_disconnect(peer_id).await;
                let _ = response.send(result);
                Ok(())
            }
            
            NetworkCommand::SendMessage { message, response } => {
                let result = self.handle_send_message(message).await;
                let _ = response.send(result);
                Ok(())
            }
            
            NetworkCommand::RegisterHandler { handler, response } => {
                let result = self.router.register_handler(*handler).await
                    .map_err(|e| NetworkError::RoutingFailed(e.to_string()));
                let _ = response.send(result);
                Ok(())
            }
            
            NetworkCommand::GetStats { response } => {
                let result = self.collect_stats().await;
                let _ = response.send(Ok(result));
                Ok(())
            }
            
            NetworkCommand::UpdateConfig { config, response } => {
                let result = self.update_config(config).await;
                let _ = response.send(result);
                Ok(())
            }
        }
    }
    
    /// Handle peer connection
    async fn handle_connect(&self, peer_id: PeerId, address: Multiaddr) -> NetworkResult<()> {
        log_info!(LogCategory::Network, "Connecting to peer {} at {}", peer_id, address);
        
        let mut swarm = self.swarm.lock().await;
        swarm.connect(peer_id, address.clone()).await?;
        
        increment_counter!("network_connections_attempted_total", 1);
        
        Ok(())
    }
    
    /// Handle peer disconnection
    async fn handle_disconnect(&self, peer_id: PeerId) -> NetworkResult<()> {
        log_info!(LogCategory::Network, "Disconnecting from peer {}", peer_id);
        
        let mut swarm = self.swarm.lock().await;
        swarm.disconnect(peer_id).await?;
        
        increment_counter!("network_disconnections_total", 1);
        
        Ok(())
    }
    
    /// Handle message sending
    async fn handle_send_message(&self, message: NetworkMessage) -> NetworkResult<()> {
        log_debug!(
            LogCategory::Network,
            "Sending message type {:?}",
            message.message_type()
        );
        
        // Check bandwidth limits
        if !self.bandwidth_manager.can_send(message.transport.size_bytes).await {
            return Err(NetworkError::BandwidthExceeded { 
                peer_id: PeerId::random() // TODO: Get actual peer
            });
        }
        
        let mut swarm = self.swarm.lock().await;
        swarm.send_message(message).await?;
        
        increment_counter!("network_messages_sent_total", 1);
        
        Ok(())
    }
    
    /// Poll swarm for events
    async fn poll_swarm_events(&self) -> NetworkResult<()> {
        let mut swarm = self.swarm.lock().await;
        
        if let Some(event) = swarm.poll_next().await {
            self.handle_swarm_event(event).await?;
        }
        
        Ok(())
    }
    
    /// Handle swarm events
    async fn handle_swarm_event(&self, event: SwarmEvent<(), std::io::Error>) -> NetworkResult<()> {
        match event {
            SwarmEvent::ConnectionEstablished { peer_id, endpoint, .. } => {
                log_info!(LogCategory::Network, "Connection established with {}", peer_id);
                
                let event = NetworkEvent::PeerConnected {
                    peer_id,
                    address: endpoint.get_remote_address().clone(),
                };
                self.emit_event(event).await;
                
                increment_counter!("network_connections_established_total", 1);
                set_gauge!("network_connected_peers", self.get_connected_peer_count().await as f64);
            }
            
            SwarmEvent::ConnectionClosed { peer_id, cause, .. } => {
                log_info!(LogCategory::Network, "Connection closed with {}: {:?}", peer_id, cause);
                
                let event = NetworkEvent::PeerDisconnected {
                    peer_id,
                    reason: format!("{:?}", cause),
                };
                self.emit_event(event).await;
                
                increment_counter!("network_connections_closed_total", 1);
                set_gauge!("network_connected_peers", self.get_connected_peer_count().await as f64);
            }
            
            SwarmEvent::IncomingConnection { .. } => {
                increment_counter!("network_incoming_connections_total", 1);
            }
            
            SwarmEvent::IncomingConnectionError { error, .. } => {
                log_warn!(LogCategory::Network, "Incoming connection error: {}", error);
                increment_counter!("network_connection_errors_total", 1);
            }
            
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                log_warn!(LogCategory::Network, "Outgoing connection error to {:?}: {}", peer_id, error);
                increment_counter!("network_connection_errors_total", 1);
            }
            
            _ => {}
        }
        
        Ok(())
    }
    
    /// Get connected peer count
    async fn get_connected_peer_count(&self) -> usize {
        let swarm = self.swarm.lock().await;
        swarm.connected_peers().len()
    }
    
    /// Update configuration
    async fn update_config(&self, _config: NetworkConfig) -> NetworkResult<()> {
        // TODO: Implement config updates
        log_info!(LogCategory::Network, "Configuration update requested");
        Ok(())
    }
    
    /// Collect network statistics
    async fn collect_stats(&self) -> NetworkStats {
        let swarm = self.swarm.lock().await;
        let connected_peers = swarm.connected_peers().len();
        
        // Get bandwidth usage
        let bandwidth_usage = self.bandwidth_manager.get_usage().await;
        
        // Get message stats
        let message_stats = self.router.get_stats().await;
        
        NetworkStats {
            connected_peers,
            total_connections: 0, // TODO: Track this
            messages_sent: message_stats.messages_sent,
            messages_received: message_stats.messages_received,
            bytes_sent: message_stats.bytes_sent,
            bytes_received: message_stats.bytes_received,
            errors: message_stats.errors,
            uptime: Duration::from_secs(0), // TODO: Track uptime
            average_latency: message_stats.avg_processing_time,
            bandwidth_usage,
        }
    }
    
    /// Update metrics
    async fn update_metrics(&self) {
        let stats = self.collect_stats().await;
        
        set_gauge!("network_connected_peers", stats.connected_peers as f64);
        set_gauge!("network_messages_sent_total", stats.messages_sent as f64);
        set_gauge!("network_messages_received_total", stats.messages_received as f64);
        set_gauge!("network_bytes_sent_total", stats.bytes_sent as f64);
        set_gauge!("network_bytes_received_total", stats.bytes_received as f64);
        set_gauge!("network_bandwidth_upload_rate", stats.bandwidth_usage.upload_rate);
        set_gauge!("network_bandwidth_download_rate", stats.bandwidth_usage.download_rate);
    }
    
    /// Run discovery service
    async fn run_discovery(self: Arc<Self>) {
        log_info!(LogCategory::Network, "Starting peer discovery service");
        
        let mut interval = interval(self.config.discovery.discovery_interval);
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.discovery.discover_peers().await {
                log_warn!(LogCategory::Network, "Discovery error: {}", e);
            }
        }
    }
    
    /// Run reputation manager
    async fn run_reputation_manager(self: Arc<Self>) {
        log_info!(LogCategory::Network, "Starting reputation manager");
        
        let mut interval = interval(self.config.reputation.update_interval);
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.reputation.update_scores().await {
                log_warn!(LogCategory::Network, "Reputation update error: {}", e);
            }
        }
    }
    
    /// Run health monitor
    async fn run_health_monitor(self: Arc<Self>) {
        log_info!(LogCategory::Network, "Starting health monitor");
        
        let mut interval = interval(self.config.monitoring.health_check_interval);
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.health_monitor.check_health().await {
                log_warn!(LogCategory::Network, "Health check error: {}", e);
            }
        }
    }
    
    /// Run bandwidth monitor
    async fn run_bandwidth_monitor(self: Arc<Self>) {
        log_info!(LogCategory::Network, "Starting bandwidth monitor");
        
        let mut interval = interval(Duration::from_secs(1));
        
        loop {
            interval.tick().await;
            self.bandwidth_manager.update_rates().await;
        }
    }
    
    /// Run metrics collection
    async fn run_metrics_collection(self: Arc<Self>) {
        log_info!(LogCategory::Network, "Starting metrics collection");
        
        let mut interval = interval(self.config.monitoring.metrics_interval);
        
        loop {
            interval.tick().await;
            self.update_metrics().await;
        }
    }
}

impl Clone for NetworkService {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            swarm: self.swarm.clone(),
            router: self.router.clone(),
            discovery: self.discovery.clone(),
            reputation: self.reputation.clone(),
            health_monitor: self.health_monitor.clone(),
            bandwidth_manager: self.bandwidth_manager.clone(),
            state: self.state.clone(),
            command_tx: self.command_tx.clone(),
            command_rx: Mutex::new(None), // Only original has receiver
            event_tx: self.event_tx.clone(),
            task_handles: self.task_handles.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_service_creation() {
        let config = NetworkConfig::default();
        let service = NetworkService::new(config).await.unwrap();
        
        assert_eq!(service.get_state().await, ServiceState::Stopped);
    }
    
    #[tokio::test]
    async fn test_service_lifecycle() {
        let config = NetworkConfig::default();
        let service = NetworkService::new(config).await.unwrap();
        
        // Start service
        service.start().await.unwrap();
        assert_eq!(service.get_state().await, ServiceState::Running);
        
        // Stop service
        service.stop().await.unwrap();
        assert_eq!(service.get_state().await, ServiceState::Stopped);
    }
    
    #[tokio::test]
    async fn test_event_subscription() {
        let config = NetworkConfig::default();
        let service = NetworkService::new(config).await.unwrap();
        
        let mut events = service.subscribe_events().await;
        
        // Start service to generate state change event
        service.start().await.unwrap();
        
        // Should receive state change event
        let event = timeout(Duration::from_millis(100), events.recv()).await;
        assert!(event.is_ok());
    }
}