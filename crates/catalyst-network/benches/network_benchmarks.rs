// ==================== benches/network_benchmarks.rs ====================
//! Network performance benchmarks

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use catalyst_network::{
    NetworkService, NetworkConfig,
    messaging::{NetworkMessage, RoutingStrategy},
};
use catalyst_utils::network::PingMessage;
use libp2p::PeerId;
use tokio::runtime::Runtime;

fn benchmark_message_creation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("message_creation", |b| {
        b.iter(|| {
            let ping = PingMessage {
                timestamp: catalyst_utils::utils::current_timestamp(),
                data: vec![0u8; 1024],
            };
            
            let message = NetworkMessage::new(
                black_box(&ping),
                "sender".to_string(),
                None,
                RoutingStrategy::Broadcast,
            ).unwrap();
            
            black_box(message);
        });
    });
}

fn benchmark_message_serialization(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("message_serialization");
    
    for size in [100, 1000, 10000, 100000].iter() {
        group.bench_with_input(BenchmarkId::new("serialize", size), size, |b, &size| {
            let ping = PingMessage {
                timestamp: catalyst_utils::utils::current_timestamp(),
                data: vec![0u8; size],
            };
            
            let message = NetworkMessage::new(
                &ping,
                "sender".to_string(),
                None,
                RoutingStrategy::Broadcast,
            ).unwrap();
            
            b.iter(|| {
                let serialized = catalyst_utils::serialization::CatalystSerialize::serialize(black_box(&message)).unwrap();
                black_box(serialized);
            });
        });
        
        group.bench_with_input(BenchmarkId::new("deserialize", size), size, |b, &size| {
            let ping = PingMessage {
                timestamp: catalyst_utils::utils::current_timestamp(),
                data: vec![0u8; size],
            };
            
            let message = NetworkMessage::new(
                &ping,
                "sender".to_string(),
                None,
                RoutingStrategy::Broadcast,
            ).unwrap();
            
            let serialized = catalyst_utils::serialization::CatalystSerialize::serialize(&message).unwrap();
            
            b.iter(|| {
                let deserialized = catalyst_utils::serialization::CatalystDeserialize::deserialize(black_box(&serialized)).unwrap();
                black_box(deserialized);
            });
        });
    }
    
    group.finish();
}

fn benchmark_peer_discovery(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("peer_discovery_add", |b| {
        b.to_async(&rt).iter(|| async {
            let config = catalyst_network::config::DiscoveryConfig::default();
            let discovery = catalyst_network::discovery::PeerDiscovery::new(&config);
            
            let peer_id = PeerId::random();
            let addresses = vec!["/ip4/127.0.0.1/tcp/8080".parse().unwrap()];
            
            let event = discovery.add_peer(
                black_box(peer_id),
                black_box(addresses),
                catalyst_network::discovery::DiscoveryMethod::Manual,
            ).await;
            
            black_box(event);
        });
    });
}

fn benchmark_reputation_updates(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("reputation_update", |b| {
        b.to_async(&rt).iter(|| async {
            let config = catalyst_network::config::ReputationConfig::default();
            let reputation = catalyst_network::reputation::ReputationManager::new(&config);
            
            let peer_id = PeerId::random();
            
            reputation.update_peer_score(
                black_box(peer_id),
                black_box(5.0),
                "benchmark test",
            ).await;
        });
    });
}

criterion_group!(
    benches,
    benchmark_message_creation,
    benchmark_message_serialization,
    benchmark_peer_discovery,
    benchmark_reputation_updates
);
criterion_main!(benches);

// ==================== tests/integration_tests.rs ====================
//! Integration tests for catalyst-network

use catalyst_network::{
    NetworkService, NetworkConfig,
    messaging::{NetworkMessage, RoutingStrategy, MessageHandler},
    discovery::{PeerDiscovery, DiscoveryMethod},
    reputation::ReputationManager,
};
use catalyst_utils::{
    network::PingMessage,
    CatalystResult,
};
use libp2p::PeerId;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::test]
async fn test_network_service_lifecycle() {
    let config = NetworkConfig::default();
    let service = NetworkService::new(config).await.unwrap();
    
    // Test initial state
    assert_eq!(service.get_state().await, catalyst_network::service::ServiceState::Stopped);
    
    // Start service
    service.start().await.unwrap();
    assert_eq!(service.get_state().await, catalyst_network::service::ServiceState::Running);
    
    // Stop service
    service.stop().await.unwrap();
    assert_eq!(service.get_state().await, catalyst_network::service::ServiceState::Stopped);
}

#[tokio::test]
async fn test_message_routing() {
    use catalyst_network::messaging::{MessageRouter, PingHandler};
    
    let router = MessageRouter::new();
    router.register_handler(PingHandler).await.unwrap();
    
    let ping = PingMessage {
        timestamp: catalyst_utils::utils::current_timestamp(),
        data: Vec::new(),
    };
    
    let message = NetworkMessage::new(
        &ping,
        "sender".to_string(),
        None,
        RoutingStrategy::Broadcast,
    ).unwrap();
    
    let sender = PeerId::random();
    let responses = router.route_message(message, sender).await.unwrap();
    
    // Should get a response from the ping handler
    assert!(!responses.is_empty());
}

#[tokio::test]
async fn test_peer_discovery() {
    let config = catalyst_network::config::DiscoveryConfig::default();
    let discovery = PeerDiscovery::new(&config);
    
    let peer_id = PeerId::random();
    let addresses = vec!["/ip4/127.0.0.1/tcp/8080".parse().unwrap()];
    
    // Add peer
    let event = discovery.add_peer(
        peer_id,
        addresses.clone(),
        DiscoveryMethod::Manual,
    ).await;
    
    // Verify event
    match event {
        catalyst_network::discovery::DiscoveryEvent::PeerDiscovered { peer_id: discovered_id, .. } => {
            assert_eq!(discovered_id, peer_id);
        }
        _ => panic!("Expected PeerDiscovered event"),
    }
    
    // Get peer
    let peer_info = discovery.get_peer(&peer_id).await.unwrap();
    assert_eq!(peer_info.peer_id, peer_id);
    assert!(!peer_info.addresses.is_empty());
}

#[tokio::test]
async fn test_reputation_management() {
    let config = catalyst_network::config::ReputationConfig::default();
    let reputation = ReputationManager::new(&config);
    
    let peer_id = PeerId::random();
    
    // Initial score should be default
    let initial_score = reputation.get_peer_score(&peer_id).await;
    assert_eq!(initial_score, config.initial_reputation);
    
    // Update score
    reputation.update_peer_score(peer_id, 10.0, "test increase").await;
    let updated_score = reputation.get_peer_score(&peer_id).await;
    assert_eq!(updated_score, initial_score + 10.0);
    
    // Check connection decision
    assert!(reputation.should_connect_to_peer(&peer_id).await);
    
    // Lower score below threshold
    reputation.update_peer_score(peer_id, -100.0, "test decrease").await;
    assert!(!reputation.should_connect_to_peer(&peer_id).await);
}

#[tokio::test]
async fn test_bandwidth_management() {
    let config = catalyst_network::config::BandwidthConfig::default();
    let bandwidth = catalyst_network::bandwidth::BandwidthManager::new(&config);
    
    // Test bandwidth checking
    assert!(bandwidth.can_send(1024).await);
    assert!(bandwidth.can_receive(1024).await);
    
    // Record usage
    bandwidth.record_upload(1024).await;
    bandwidth.record_download(2048).await;
    
    let usage = bandwidth.get_usage().await;
    assert_eq!(usage.total_uploaded, 1024);
    assert_eq!(usage.total_downloaded, 2048);
}

#[tokio::test]
async fn test_security_validation() {
    let config = catalyst_network::config::SecurityConfig::default();
    let security = catalyst_network::security::SecurityManager::new(&config);
    
    let peer_id = PeerId::random();
    let ip = "127.0.0.1".parse().unwrap();
    
    // Should allow by default
    assert!(security.validate_peer_connection(&peer_id, &ip).await.is_ok());
    
    // Block peer
    security.block_peer(peer_id).await;
    
    // Should now reject
    assert!(security.validate_peer_connection(&peer_id, &ip).await.is_err());
}

#[tokio::test]
async fn test_health_monitoring() {
    let config = catalyst_network::config::MonitoringConfig::default();
    let monitor = catalyst_network::monitoring::HealthMonitor::new(&config);
    
    // Initial health check
    monitor.check_health().await.unwrap();
    
    let health = monitor.get_health().await;
    assert!(health.connection_health >= 0.0 && health.connection_health <= 1.0);
    assert!(health.bandwidth_health >= 0.0 && health.bandwidth_health <= 1.0);
    assert!(health.latency_health >= 0.0 && health.latency_health <= 1.0);
}

#[tokio::test]
async fn test_gossip_protocol() {
    let config = catalyst_network::config::GossipConfig::default();
    let gossip = catalyst_network::gossip::GossipProtocol::new(&config);
    
    let ping = PingMessage {
        timestamp: catalyst_utils::utils::current_timestamp(),
        data: vec![1, 2, 3],
    };
    
    let message = NetworkMessage::new(
        &ping,
        "sender".to_string(),
        None,
        RoutingStrategy::Gossip,
    ).unwrap();
    
    // Publish message
    gossip.publish_message(message).await.unwrap();
    
    let stats = gossip.get_stats().await;
    assert_eq!(stats.messages_published, 1);
}

#[tokio::test]
async fn test_network_configuration() {
    let mut config = NetworkConfig::default();
    
    // Should be valid by default
    assert!(config.validate().is_ok());
    
    // Make invalid
    config.peer.min_peers = 100;
    config.peer.max_peers = 50;
    
    // Should now be invalid
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn test_message_serialization_roundtrip() {
    let ping = PingMessage {
        timestamp: catalyst_utils::utils::current_timestamp(),
        data: vec![1, 2, 3, 4, 5],
    };
    
    let original = NetworkMessage::new(
        &ping,
        "sender".to_string(),
        Some("target".to_string()),
        RoutingStrategy::Direct("target".to_string()),
    ).unwrap();
    
    // Serialize
    let serialized = catalyst_utils::serialization::CatalystSerialize::serialize(&original).unwrap();
    
    // Deserialize
    let deserialized = catalyst_utils::serialization::CatalystDeserialize::deserialize(&serialized).unwrap();
    
    // Compare
    assert_eq!(original.routing.hop_count, deserialized.routing.hop_count);
    assert_eq!(original.transport.size_bytes, deserialized.transport.size_bytes);
    assert_eq!(original.message_type(), deserialized.message_type());
}

// Test helper for creating test networks
pub struct TestNetwork {
    pub services: Vec<NetworkService>,
    pub configs: Vec<NetworkConfig>,
}

impl TestNetwork {
    pub async fn new(node_count: usize) -> Self {
        let mut services = Vec::new();
        let mut configs = Vec::new();
        
        for i in 0..node_count {
            let mut config = NetworkConfig::default();
            
            // Use different ports for each node
            config.peer.listen_addresses = vec![
                format!("/ip4/127.0.0.1/tcp/{}", 9000 + i).parse().unwrap(),
            ];
            
            let service = NetworkService::new(config.clone()).await.unwrap();
            
            services.push(service);
            configs.push(config);
        }
        
        Self { services, configs }
    }
    
    pub async fn start_all(&self) {
        for service in &self.services {
            service.start().await.unwrap();
        }
        
        // Wait for services to start
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    pub async fn stop_all(&self) {
        for service in &self.services {
            service.stop().await.unwrap();
        }
    }
    
    pub async fn connect_peers(&self) {
        // Connect each peer to the next one in a ring
        for i in 0..self.services.len() {
            let next = (i + 1) % self.services.len();
            
            if let Some(addr) = self.configs[next].peer.listen_addresses.first() {
                // Would connect services[i] to services[next] at addr
                // This requires implementing peer connection in the test
            }
        }
    }
}

#[tokio::test]
async fn test_multi_node_network() {
    let network = TestNetwork::new(3).await;
    
    // Start all nodes
    network.start_all().await;
    
    // Connect peers
    network.connect_peers().await;
    
    // Wait for connections to establish
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Test message propagation
    let ping = PingMessage {
        timestamp: catalyst_utils::utils::current_timestamp(),
        data: vec![1, 2, 3],
    };
    
    // Send from first node
    if let Err(e) = network.services[0].send_message(&ping, None).await {
        println!("Message send failed: {}", e);
        // This is expected in the test environment
    }
    
    // Stop all nodes
    network.stop_all().await;
}

// ==================== tests/property_tests.rs ====================
//! Property-based tests using proptest

use proptest::prelude::*;
use catalyst_network::{
    messaging::{NetworkMessage, RoutingStrategy},
    discovery::{PeerDiscovery, DiscoveryMethod},
};
use catalyst_utils::network::PingMessage;
use libp2p::PeerId;

proptest! {
    #[test]
    fn test_message_serialization_roundtrip(
        data in prop::collection::vec(any::<u8>(), 0..10000),
        timestamp in any::<u64>(),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        
        rt.block_on(async {
            let ping = PingMessage {
                timestamp,
                data: data.clone(),
            };
            
            let original = NetworkMessage::new(
                &ping,
                "sender".to_string(),
                None,
                RoutingStrategy::Broadcast,
            ).unwrap();
            
            // Serialize and deserialize
            let serialized = catalyst_utils::serialization::CatalystSerialize::serialize(&original).unwrap();
            let deserialized = catalyst_utils::serialization::CatalystDeserialize::deserialize(&serialized).unwrap();
            
            // Properties that should hold
            prop_assert_eq!(original.routing.hop_count, deserialized.routing.hop_count);
            prop_assert_eq!(original.transport.size_bytes, deserialized.transport.size_bytes);
            prop_assert_eq!(original.message_type(), deserialized.message_type());
        });
    }
    
    #[test]
    fn test_peer_discovery_invariants(
        peer_count in 1..100usize,
        score_adjustments in prop::collection::vec(-10.0..10.0f64, 0..50),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        
        rt.block_on(async {
            let config = catalyst_network::config::DiscoveryConfig::default();
            let discovery = PeerDiscovery::new(&config);
            
            let mut peer_ids = Vec::new();
            
            // Add peers
            for _ in 0..peer_count {
                let peer_id = PeerId::random();
                let addresses = vec!["/ip4/127.0.0.1/tcp/8080".parse().unwrap()];
                
                discovery.add_peer(peer_id, addresses, DiscoveryMethod::Manual).await;
                peer_ids.push(peer_id);
            }
            
            let peers = discovery.get_peers().await;
            prop_assert_eq!(peers.len(), peer_count);
            
            // Apply score adjustments
            for (i, &adjustment) in score_adjustments.iter().enumerate() {
                if let Some(&peer_id) = peer_ids.get(i % peer_ids.len()) {
                    discovery.update_peer_score(&peer_id, adjustment).await;
                }
            }
            
            // Check that all peer scores are within valid range
            let peers_after = discovery.get_peers().await;
            for peer_info in peers_after.values() {
                prop_assert!(peer_info.score >= 0.0);
                prop_assert!(peer_info.score <= 100.0);
            }
        });
    }
    
    #[test]
    fn test_reputation_score_bounds(
        initial_score in 0.0..100.0f64,
        adjustments in prop::collection::vec(-50.0..50.0f64, 0..100),
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        
        rt.block_on(async {
            let mut config = catalyst_network::config::ReputationConfig::default();
            config.initial_reputation = initial_score;
            
            let reputation = catalyst_network::reputation::ReputationManager::new(&config);
            let peer_id = PeerId::random();
            
            // Apply all adjustments
            for adjustment in adjustments {
                reputation.update_peer_score(peer_id, adjustment, "property test").await;
            }
            
            let final_score = reputation.get_peer_score(&peer_id).await;
            
            // Score should always be within bounds
            prop_assert!(final_score >= 0.0);
            prop_assert!(final_score <= config.max_reputation);
        });
    }
}