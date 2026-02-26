//! Network configuration structures - Section 1: Core Structures and Imports

use libp2p::{Multiaddr, PeerId, identity::Keypair};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    path::PathBuf,
    time::Duration,
};
use crate::error::{NetworkResult, NetworkError};

// Duration serialization helper module
mod duration_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let secs = duration.as_secs();
        serializer.serialize_u64(secs)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
}

// Helper modules for serializing complex types
mod multiaddr_serde {
    use libp2p::Multiaddr;
    use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(addrs: &Vec<Multiaddr>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let addr_strings: Vec<String> = addrs.iter().map(|addr| addr.to_string()).collect();
        addr_strings.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Multiaddr>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let addr_strings: Vec<String> = Vec::deserialize(deserializer)?;
        let mut addrs = Vec::new();
        for addr_str in addr_strings {
            match addr_str.parse::<Multiaddr>() {
                Ok(addr) => addrs.push(addr),
                Err(_) => return Err(serde::de::Error::custom(format!("Invalid multiaddr: {}", addr_str))),
            }
        }
        Ok(addrs)
    }
}

mod bootstrap_peers_serde {
    use libp2p::{Multiaddr, PeerId};
    use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(peers: &Vec<(PeerId, Multiaddr)>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let peer_strings: Vec<(String, String)> = peers.iter()
            .map(|(peer_id, addr)| (peer_id.to_string(), addr.to_string()))
            .collect();
        peer_strings.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<(PeerId, Multiaddr)>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let peer_strings: Vec<(String, String)> = Vec::deserialize(deserializer)?;
        let mut peers = Vec::new();
        for (peer_str, addr_str) in peer_strings {
            let peer_id = peer_str.parse::<PeerId>()
                .map_err(|_| serde::de::Error::custom(format!("Invalid PeerId: {}", peer_str)))?;
            let addr = addr_str.parse::<Multiaddr>()
                .map_err(|_| serde::de::Error::custom(format!("Invalid Multiaddr: {}", addr_str)))?;
            peers.push((peer_id, addr));
        }
        Ok(peers)
    }
}

/// Main network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Local peer configuration
    pub peer: PeerConfig,
    
    /// Transport configuration
    pub transport: TransportConfig,
    
    /// Gossip protocol configuration
    pub gossip: GossipConfig,

    /// Safety limits (DoS bounding) applied by transports.
    pub safety_limits: SafetyLimitsConfig,
    
    /// Discovery configuration
    pub discovery: DiscoveryConfig,
    
    /// Security configuration
    pub security: SecurityConfig,
    
    /// Bandwidth management
    pub bandwidth: BandwidthConfig,
    
    /// Reputation system
    pub reputation: ReputationConfig,
    
    /// Monitoring configuration
    pub monitoring: MonitoringConfig,
}

/// Transport-agnostic DoS safety limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyLimitsConfig {
    /// Maximum gossipsub message bytes accepted by the libp2p transport.
    pub max_gossip_message_bytes: usize,

    /// Per-peer message rate limit (msgs/sec) for libp2p transport.
    pub per_peer_max_msgs_per_sec: u32,

    /// Per-peer bandwidth cap (bytes/sec) for libp2p transport.
    pub per_peer_max_bytes_per_sec: usize,

    /// Maximum TCP frame bytes accepted by the simple transport.
    pub max_tcp_frame_bytes: usize,

    /// Per-connection message rate limit (msgs/sec) for simple transport.
    pub per_conn_max_msgs_per_sec: u32,

    /// Per-connection bandwidth cap (bytes/sec) for simple transport.
    pub per_conn_max_bytes_per_sec: usize,

    /// Maximum hops for multi-hop rebroadcast.
    pub max_hops: u8,

    /// Maximum number of recently seen envelope ids stored for deduplication.
    pub dedup_cache_max_entries: usize,

    /// Maximum dial jitter (milliseconds) applied to backoff scheduling.
    pub dial_jitter_max_ms: u64,

    /// Maximum backoff cap (milliseconds) applied to exponential dial backoff.
    pub dial_backoff_max_ms: u64,
}

/// Peer-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    /// Local peer keypair (loaded from file or generated)
    #[serde(skip)]
    pub keypair: Option<Keypair>,
    
    /// Path to store/load keypair
    pub keypair_path: Option<PathBuf>,
    
    /// Listen addresses
    #[serde(with = "multiaddr_serde")]
    pub listen_addresses: Vec<Multiaddr>,
    
    /// External addresses to announce
    #[serde(with = "multiaddr_serde")]
    pub external_addresses: Vec<Multiaddr>,
    
    /// Bootstrap peers
    #[serde(with = "bootstrap_peers_serde")]
    pub bootstrap_peers: Vec<(PeerId, Multiaddr)>,
    
    /// Maximum number of peers
    pub max_peers: usize,
    
    /// Minimum number of peers to maintain
    pub min_peers: usize,
    
    /// Connection timeout
    #[serde(with = "duration_serde")]
    pub connection_timeout: Duration,
    
    /// Keep-alive interval
    #[serde(with = "duration_serde")]
    pub keep_alive_interval: Duration,
    
    /// Idle connection timeout
    #[serde(with = "duration_serde")]
    pub idle_timeout: Duration,
    
    /// Maximum connections per IP
    pub max_connections_per_ip: usize,
    
    /// Connection retry attempts
    pub max_retry_attempts: u32,
    
    /// Retry backoff base duration
    #[serde(with = "duration_serde")]
    pub retry_backoff: Duration,
    
    /// Enable connection metrics
    pub enable_connection_metrics: bool,
}

/// Transport layer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Enable TCP transport
    pub enable_tcp: bool,
    
    /// Enable WebSocket transport
    pub enable_websocket: bool,
    
    /// Enable QUIC transport
    pub enable_quic: bool,
    
    /// TCP port range
    pub tcp_port_range: (u16, u16),
    
    /// WebSocket port
    pub websocket_port: Option<u16>,
    
    /// QUIC port
    pub quic_port: Option<u16>,
    
    /// Enable noise encryption
    pub enable_noise: bool,
    
    /// Enable yamux multiplexing
    pub enable_yamux: bool,
    
    /// Connection limits per IP
    pub max_connections_per_ip: usize,
    
    /// Global connection limit
    pub max_connections: usize,
    
    /// Connection upgrade timeout
    #[serde(with = "duration_serde")]
    pub upgrade_timeout: Duration,
    
    /// Keep-alive configuration
    pub keep_alive: KeepAliveConfig,
    
    /// Buffer sizes
    pub buffer_config: BufferConfig,
}

/// Keep-alive configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeepAliveConfig {
    /// Enable keep-alive
    pub enabled: bool,
    
    /// Keep-alive interval
    #[serde(with = "duration_serde")]
    pub interval: Duration,
    
    /// Keep-alive timeout
    #[serde(with = "duration_serde")]
    pub timeout: Duration,
    
    /// Maximum failures before disconnect
    pub max_failures: u32,
}

/// Buffer configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferConfig {
    /// Send buffer size
    pub send_buffer_size: usize,
    
    /// Receive buffer size
    pub receive_buffer_size: usize,
    
    /// Connection buffer size
    pub connection_buffer_size: usize,
    
    /// Message queue size
    pub message_queue_size: usize,
}

/// Gossip protocol configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipConfig {
    /// Topic name for Catalyst messages
    pub topic_name: String,
    
    /// Message cache size
    pub message_cache_size: usize,
    
    /// Message cache duration
    #[serde(with = "duration_serde")]
    pub message_cache_duration: Duration,
    
    /// Heartbeat interval
    #[serde(with = "duration_serde")]
    pub heartbeat_interval: Duration,
    
    /// Gossip factor (D in paper)
    pub gossip_factor: usize,
    
    /// Mesh maintenance parameters
    pub mesh_n: usize,      // Target mesh size
    pub mesh_n_low: usize,  // Low watermark
    pub mesh_n_high: usize, // High watermark
    
    /// Message validation timeout
    #[serde(with = "duration_serde")]
    pub validation_timeout: Duration,
    
    /// Enable message signing
    pub enable_signing: bool,
    
    /// Duplicate message detection window
    #[serde(with = "duration_serde")]
    pub duplicate_detection_window: Duration,
    
    /// Flood publish configuration
    pub flood_publish: bool,
    
    /// Message propagation parameters
    pub propagation: PropagationConfig,
    
    /// Gossip scoring parameters
    pub scoring: GossipScoringConfig,
}

/// Message propagation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationConfig {
    /// Publish threshold
    pub publish_threshold: f64,
    
    /// Gossip threshold
    pub gossip_threshold: f64,
    
    /// Accept PX threshold
    pub accept_px_threshold: f64,
    
    /// Opportunistic graft threshold
    pub opportunistic_graft_threshold: f64,
}

/// Gossip scoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GossipScoringConfig {
    /// Enable peer scoring
    pub enabled: bool,
    
    /// Topic weight
    pub topic_weight: f64,
    
    /// Time in mesh weight
    pub time_in_mesh_weight: f64,
    
    /// Time in mesh quantum
    #[serde(with = "duration_serde")]
    pub time_in_mesh_quantum: Duration,
    
    /// Time in mesh cap
    pub time_in_mesh_cap: f64,
    
    /// First message deliveries weight
    pub first_message_deliveries_weight: f64,
    
    /// First message deliveries decay
    pub first_message_deliveries_decay: f64,
    
    /// First message deliveries cap
    pub first_message_deliveries_cap: f64,
    
    /// Mesh message deliveries weight
    pub mesh_message_deliveries_weight: f64,
    
    /// Mesh message deliveries decay
    pub mesh_message_deliveries_decay: f64,
    
    /// Mesh message deliveries cap
    pub mesh_message_deliveries_cap: f64,
    
    /// Mesh message deliveries threshold
    pub mesh_message_deliveries_threshold: f64,
    
    /// Mesh message deliveries window
    #[serde(with = "duration_serde")]
    pub mesh_message_deliveries_window: Duration,
    
    /// Mesh message deliveries activation
    #[serde(with = "duration_serde")]
    pub mesh_message_deliveries_activation: Duration,
    
    /// Mesh failure penalty weight
    pub mesh_failure_penalty_weight: f64,
    
    /// Mesh failure penalty decay
    pub mesh_failure_penalty_decay: f64,
    
    /// Invalid message deliveries weight
    pub invalid_message_deliveries_weight: f64,
    
    /// Invalid message deliveries decay
    pub invalid_message_deliveries_decay: f64,
}

/// Peer discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryConfig {
    /// Enable mDNS discovery
    pub enable_mdns: bool,
    
    /// Enable Kademlia DHT
    pub enable_kademlia: bool,
    
    /// Enable random walk discovery
    pub enable_random_walk: bool,
    
    /// Kademlia replication factor
    pub kademlia_replication: usize,
    
    /// Bootstrap interval
    #[serde(with = "duration_serde")]
    pub bootstrap_interval: Duration,
    
    /// Discovery interval
    #[serde(with = "duration_serde")]
    pub discovery_interval: Duration,
    
    /// Maximum discovery attempts
    pub max_discovery_attempts: usize,
    
    /// Discovery timeout
    #[serde(with = "duration_serde")]
    pub discovery_timeout: Duration,
    
    /// Enable AutoNAT for external address discovery
    pub enable_autonat: bool,
    
    /// Enable relay for NAT traversal
    pub enable_relay: bool,
    
    /// Enable circuit relay v2
    pub enable_relay_v2: bool,
    
    /// Enable DCUtR (Direct Connection Upgrade through Relay)
    pub enable_dcutr: bool,
    
    /// mDNS configuration
    pub mdns: MdnsConfig,
    
    /// Kademlia configuration
    pub kademlia: KademliaConfig,
    
    /// AutoNAT configuration
    pub autonat: AutonatConfig,
    
    /// Relay configuration
    pub relay: RelayConfig,
}

/// mDNS discovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MdnsConfig {
    /// Service name
    pub service_name: String,
    
    /// Query interval
    #[serde(with = "duration_serde")]
    pub query_interval: Duration,
    
    /// TTL for mDNS records
    #[serde(with = "duration_serde")]
    pub ttl: Duration,
    
    /// Enable IPv6
    pub enable_ipv6: bool,
}

/// Kademlia DHT configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KademliaConfig {
    /// Protocol name
    pub protocol_name: String,
    
    /// Replication factor (K)
    pub replication_factor: usize,
    
    /// Query timeout
    #[serde(with = "duration_serde")]
    pub query_timeout: Duration,
    
    /// Replication interval
    #[serde(with = "duration_serde")]
    pub replication_interval: Duration,
    
    /// Publication interval
    #[serde(with = "duration_serde")]
    pub publication_interval: Duration,
    
    /// Record TTL
    #[serde(with = "duration_serde")]
    pub record_ttl: Duration,
    
    /// Provider record TTL
    #[serde(with = "duration_serde")]
    pub provider_record_ttl: Duration,
    
    /// Maximum packet size
    pub max_packet_size: usize,
    
    /// Enable automatic mode switching
    pub automatic_mode: bool,
}

/// AutoNAT configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonatConfig {
    /// Only global IPs
    pub only_global_ips: bool,
    
    /// Throttle server creation
    #[serde(with = "duration_serde")]
    pub throttle_server_creation: Duration,
    
    /// Boot delay
    #[serde(with = "duration_serde")]
    pub boot_delay: Duration,
    
    /// Refresh interval
    #[serde(with = "duration_serde")]
    pub refresh_interval: Duration,
    
    /// Retry interval
    #[serde(with = "duration_serde")]
    pub retry_interval: Duration,
    
    /// Confidence max
    pub confidence_max: usize,
}

/// Relay configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayConfig {
    /// Maximum circuits
    pub max_circuits: u32,
    
    /// Maximum circuit duration
    #[serde(with = "duration_serde")]
    pub max_circuit_duration: Duration,
    
    /// Maximum circuit bytes
    pub max_circuit_bytes: u64,
    
    /// Enable relay server
    pub enable_server: bool,
    
    /// Enable relay client
    pub enable_client: bool,
    
    /// Circuit src rate limiter
    pub circuit_src_rate_limiter: RateLimiterConfig,
}

/// Rate limiter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimiterConfig {
    /// Maximum requests
    pub max_requests: u32,
    
    /// Time window
    #[serde(with = "duration_serde")]
    pub window: Duration,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Allowed peer IDs (empty = allow all)
    pub allowed_peers: HashSet<String>, // PeerId as string
    
    /// Blocked peer IDs
    pub blocked_peers: HashSet<String>,
    
    /// Blocked IP addresses
    pub blocked_ips: HashSet<IpAddr>,
    
    /// Blocked subnets (CIDR notation)
    pub blocked_subnets: HashSet<String>,
    
    /// Message size limits
    pub max_message_size: usize,
    
    /// Enable message encryption
    pub enable_encryption: bool,
    
    /// Authentication timeout
    #[serde(with = "duration_serde")]
    pub auth_timeout: Duration,
    
    /// Enable peer identity verification
    pub verify_peer_identity: bool,
    
    /// Trust threshold for accepting connections
    pub trust_threshold: f64,
    
    /// Message validation configuration
    pub message_validation: MessageValidationConfig,
    
    /// Connection security configuration
    pub connection_security: ConnectionSecurityConfig,
    
    /// Rate limiting for security
    pub security_rate_limits: SecurityRateLimits,
}

/// Message validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageValidationConfig {
    /// Enable strict validation
    pub strict_validation: bool,
    
    /// Maximum message age
    #[serde(with = "duration_serde")]
    pub max_message_age: Duration,
    
    /// Require message signatures
    pub require_signatures: bool,
    
    /// Enable content filtering
    pub enable_content_filtering: bool,
    
    /// Blacklisted content patterns
    pub blacklisted_patterns: Vec<String>,
    
    /// Maximum message chain length
    pub max_message_chain_length: usize,
    
    /// Enable message deduplication
    pub enable_deduplication: bool,
    
    /// Deduplication window
    #[serde(with = "duration_serde")]
    pub deduplication_window: Duration,
}

/// Connection security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionSecurityConfig {
    /// Require secure connections
    pub require_secure_connections: bool,
    
    /// Minimum TLS version
    pub min_tls_version: String,
    
    /// Allowed cipher suites
    pub allowed_cipher_suites: Vec<String>,
    
    /// Certificate validation
    pub validate_certificates: bool,
    
    /// Enable perfect forward secrecy
    pub require_perfect_forward_secrecy: bool,
    
    /// Connection handshake timeout
    #[serde(with = "duration_serde")]
    pub handshake_timeout: Duration,
    
    /// Enable connection fingerprinting
    pub enable_fingerprinting: bool,
    
    /// Fingerprint cache size
    pub fingerprint_cache_size: usize,
    
    /// Fingerprint cache duration
    #[serde(with = "duration_serde")]
    pub fingerprint_cache_duration: Duration,
}

/// Security rate limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityRateLimits {
    /// Connection attempts per IP
    pub connections_per_ip: RateLimiterConfig,
    
    /// Messages per peer
    pub messages_per_peer: RateLimiterConfig,
    
    /// Authentication attempts
    pub auth_attempts: RateLimiterConfig,
    
    /// Invalid message threshold
    pub invalid_message_threshold: RateLimiterConfig,
    
    /// Bandwidth per IP
    pub bandwidth_per_ip: RateLimiterConfig,
    
    /// New peer introductions
    pub peer_introductions: RateLimiterConfig,
}

/// Bandwidth management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthConfig {
    /// Enable bandwidth limiting
    pub enable_limiting: bool,
    
    /// Upload bandwidth limit (bytes/sec)
    pub upload_limit: Option<u64>,
    
    /// Download bandwidth limit (bytes/sec)
    pub download_limit: Option<u64>,
    
    /// Per-peer bandwidth limit
    pub per_peer_limit: Option<u64>,
    
    /// Burst allowance
    pub burst_allowance: u64,
    
    /// Rate limiting window
    #[serde(with = "duration_serde")]
    pub rate_limit_window: Duration,
    
    /// Priority levels for different message types
    pub message_priorities: MessagePriorityConfig,
    
    /// Traffic shaping configuration
    pub traffic_shaping: TrafficShapingConfig,
    
    /// QoS configuration
    pub qos: QosConfig,
    
    /// Adaptive bandwidth management
    pub adaptive: AdaptiveBandwidthConfig,
}

/// Message priority configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePriorityConfig {
    /// Consensus messages priority
    pub consensus_priority: u8,
    
    /// Transaction messages priority
    pub transaction_priority: u8,
    
    /// Discovery messages priority
    pub discovery_priority: u8,
    
    /// System messages priority
    pub system_priority: u8,
    
    /// Default message priority
    pub default_priority: u8,
    
    /// Priority weights (0.0 to 1.0)
    pub priority_weights: Vec<f64>,
    
    /// Enable dynamic priority adjustment
    pub enable_dynamic_priority: bool,
    
    /// Priority adjustment interval
    #[serde(with = "duration_serde")]
    pub priority_adjustment_interval: Duration,
}

/// Traffic shaping configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficShapingConfig {
    /// Enable traffic shaping
    pub enabled: bool,
    
    /// Token bucket configuration
    pub token_bucket: TokenBucketConfig,
    
    /// Fair queuing
    pub fair_queuing: bool,
    
    /// Congestion control
    pub congestion_control: CongestionControlConfig,
    
    /// Traffic classification
    pub traffic_classification: TrafficClassificationConfig,
}

/// Token bucket configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBucketConfig {
    /// Bucket capacity
    pub capacity: u64,
    
    /// Token refill rate (tokens/sec)
    pub refill_rate: u64,
    
    /// Initial tokens
    pub initial_tokens: u64,
    
    /// Maximum burst size
    pub max_burst_size: u64,
    
    /// Token bucket algorithm variant
    pub algorithm: TokenBucketAlgorithm,
}

/// Token bucket algorithm variants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TokenBucketAlgorithm {
    /// Classic token bucket
    Classic,
    /// Leaky bucket
    LeakyBucket,
    /// Hierarchical token bucket
    Hierarchical,
}

/// Congestion control configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CongestionControlConfig {
    /// Algorithm type
    pub algorithm: CongestionControlAlgorithm,
    
    /// Algorithm-specific parameters
    pub parameters: HashMap<String, f64>,
    
    /// Enable ECN (Explicit Congestion Notification)
    pub enable_ecn: bool,
    
    /// Congestion window initial size
    pub initial_window_size: u32,
    
    /// Maximum window size
    pub max_window_size: u32,
    
    /// Slow start threshold
    pub slow_start_threshold: u32,
    
    /// RTT measurement window
    #[serde(with = "duration_serde")]
    pub rtt_measurement_window: Duration,
}

/// Congestion control algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CongestionControlAlgorithm {
    /// TCP Reno
    Reno,
    /// TCP Cubic
    Cubic,
    /// BBR (Bottleneck Bandwidth and RTT)
    Bbr,
    /// TCP Vegas
    Vegas,
    /// Custom algorithm
    Custom(String),
}

/// Traffic classification configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrafficClassificationConfig {
    /// Enable traffic classification
    pub enabled: bool,
    
    /// Classification rules
    pub rules: Vec<ClassificationRule>,
    
    /// Default traffic class
    pub default_class: String,
    
    /// Enable deep packet inspection
    pub enable_dpi: bool,
}

/// Traffic classification rule
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationRule {
    /// Rule name
    pub name: String,
    
    /// Message type patterns
    pub message_type_patterns: Vec<String>,
    
    /// Peer ID patterns
    pub peer_id_patterns: Vec<String>,
    
    /// Content patterns
    pub content_patterns: Vec<String>,
    
    /// Assigned traffic class
    pub traffic_class: String,
    
    /// Rule priority
    pub priority: u32,
}

/// Quality of Service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QosConfig {
    /// Enable QoS
    pub enabled: bool,
    
    /// Service classes
    pub service_classes: Vec<ServiceClass>,
    
    /// Default service class
    pub default_class: String,
    
    /// QoS scheduling algorithm
    pub scheduling_algorithm: QosSchedulingAlgorithm,
    
    /// Enable admission control
    pub enable_admission_control: bool,
    
    /// Admission control policy
    pub admission_control: AdmissionControlConfig,
}

/// Service class definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceClass {
    /// Class name
    pub name: String,
    
    /// Bandwidth allocation (percentage)
    pub bandwidth_allocation: f64,
    
    /// Maximum latency
    #[serde(with = "duration_serde")]
    pub max_latency: Duration,
    
    /// Maximum jitter
    #[serde(with = "duration_serde")]
    pub max_jitter: Duration,
    
    /// Packet loss threshold
    pub packet_loss_threshold: f64,
    
    /// Priority level
    pub priority: u8,
    
    /// Guaranteed bandwidth (bytes/sec)
    pub guaranteed_bandwidth: Option<u64>,
    
    /// Maximum bandwidth (bytes/sec)
    pub max_bandwidth: Option<u64>,
}

/// QoS scheduling algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QosSchedulingAlgorithm {
    /// First In First Out
    Fifo,
    /// Priority Queuing
    PriorityQueuing,
    /// Weighted Fair Queuing
    WeightedFairQueuing,
    /// Class-Based Queuing
    ClassBasedQueuing,
    /// Deficit Round Robin
    DeficitRoundRobin,
}

/// Admission control configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdmissionControlConfig {
    /// Maximum connections per service class
    pub max_connections_per_class: HashMap<String, u32>,
    
    /// Bandwidth reservation thresholds
    pub bandwidth_reservation_thresholds: HashMap<String, f64>,
    
    /// Enable preemption
    pub enable_preemption: bool,
    
    /// Preemption policy
    pub preemption_policy: PreemptionPolicy,
}

/// Preemption policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreemptionPolicy {
    /// No preemption
    None,
    /// Preempt lower priority
    LowerPriority,
    /// Preempt least recently used
    LeastRecentlyUsed,
    /// Preempt oldest connection
    OldestConnection,
}

/// Adaptive bandwidth configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveBandwidthConfig {
    /// Enable adaptive bandwidth management
    pub enabled: bool,
    
    /// Monitoring interval
    #[serde(with = "duration_serde")]
    pub monitoring_interval: Duration,
    
    /// Adjustment sensitivity (0.0 to 1.0)
    pub adjustment_sensitivity: f64,
    
    /// Minimum adjustment threshold
    pub min_adjustment_threshold: f64,
    
    /// Maximum adjustment factor
    pub max_adjustment_factor: f64,
    
    /// Network condition factors
    pub network_condition_factors: NetworkConditionFactors,
    
    /// Learning algorithm
    pub learning_algorithm: LearningAlgorithm,
}

/// Network condition factors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConditionFactors {
    /// Latency weight
    pub latency_weight: f64,
    
    /// Packet loss weight
    pub packet_loss_weight: f64,
    
    /// Throughput weight
    pub throughput_weight: f64,
    
    /// Jitter weight
    pub jitter_weight: f64,
    
    /// Peer count weight
    pub peer_count_weight: f64,
}

/// Learning algorithm for adaptive bandwidth
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LearningAlgorithm {
    /// No learning
    None,
    /// Simple moving average
    MovingAverage { window_size: usize },
    /// Exponential weighted moving average
    Ewma { alpha: f64 },
    /// Reinforcement learning
    ReinforcementLearning { config: RlConfig },
}

/// Reinforcement learning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlConfig {
    /// Learning rate
    pub learning_rate: f64,
    
    /// Discount factor
    pub discount_factor: f64,
    
    /// Exploration rate
    pub exploration_rate: f64,
    
    /// State space size
    pub state_space_size: usize,
    
    /// Action space size
    pub action_space_size: usize,
}

/// Peer reputation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationConfig {
    /// Enable reputation system
    pub enable_reputation: bool,
    
    /// Initial reputation score
    pub initial_reputation: f64,
    
    /// Minimum reputation to maintain connection
    pub min_reputation: f64,
    
    /// Maximum reputation score
    pub max_reputation: f64,
    
    /// Reputation decay rate
    pub decay_rate: f64,
    
    /// Reputation update interval
    #[serde(with = "duration_serde")]
    pub update_interval: Duration,
    
    /// Score adjustments for different behaviors
    pub score_adjustments: ScoreAdjustments,
    
    /// Reputation persistence
    pub persistence: ReputationPersistence,
    
    /// Advanced scoring
    pub advanced_scoring: AdvancedScoringConfig,
    
    /// Reputation aggregation
    pub aggregation: ReputationAggregationConfig,
}

/// Reputation score adjustments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreAdjustments {
    /// Successful message delivery
    pub successful_delivery: f64,
    
    /// Failed message delivery
    pub failed_delivery: f64,
    
    /// Invalid message
    pub invalid_message: f64,
    
    /// Connection drop
    pub connection_drop: f64,
    
    /// Slow response
    pub slow_response: f64,
    
    /// Fast response
    pub fast_response: f64,
    
    /// Protocol violation
    pub protocol_violation: f64,
    
    /// Helpful behavior
    pub helpful_behavior: f64,
    
    /// Malicious behavior
    pub malicious_behavior: f64,
    
    /// Consensus participation
    pub consensus_participation: f64,
    
    /// Resource sharing
    pub resource_sharing: f64,
    
    /// Uptime contribution
    pub uptime_contribution: f64,
    
    /// Network stability
    pub network_stability: f64,
    
    /// Bandwidth sharing
    pub bandwidth_sharing: f64,
    
    /// Security contribution
    pub security_contribution: f64,
}

/// Reputation persistence configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationPersistence {
    /// Enable persistence
    pub enabled: bool,
    
    /// Storage path
    pub storage_path: Option<PathBuf>,
    
    /// Save interval
    #[serde(with = "duration_serde")]
    pub save_interval: Duration,
    
    /// Maximum entries to store
    pub max_entries: usize,
    
    /// Entry expiration
    #[serde(with = "duration_serde")]
    pub entry_expiration: Duration,
    
    /// Compression enabled
    pub enable_compression: bool,
    
    /// Backup enabled
    pub enable_backup: bool,
    
    /// Backup interval
    #[serde(with = "duration_serde")]
    pub backup_interval: Duration,
    
    /// Maximum backup files
    pub max_backup_files: usize,
}

/// Advanced scoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedScoringConfig {
    /// Enable machine learning scoring
    pub enable_ml_scoring: bool,
    
    /// Behavioral analysis
    pub behavioral_analysis: BehavioralAnalysisConfig,
    
    /// Pattern recognition
    pub pattern_recognition: PatternRecognitionConfig,
    
    /// Anomaly detection
    pub anomaly_detection: AnomalyDetectionConfig,
    
    /// Trust propagation
    pub trust_propagation: TrustPropagationConfig,
    
    /// Temporal scoring
    pub temporal_scoring: TemporalScoringConfig,
}

/// Behavioral analysis configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehavioralAnalysisConfig {
    /// Enable behavioral analysis
    pub enabled: bool,
    
    /// Analysis window
    #[serde(with = "duration_serde")]
    pub analysis_window: Duration,
    
    /// Behavior categories to track
    pub tracked_behaviors: Vec<BehaviorCategory>,
    
    /// Behavior weights
    pub behavior_weights: HashMap<String, f64>,
    
    /// Minimum sample size
    pub min_sample_size: usize,
    
    /// Update frequency
    #[serde(with = "duration_serde")]
    pub update_frequency: Duration,
}

/// Behavior categories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BehaviorCategory {
    /// Message sending patterns
    MessagePatterns,
    /// Connection patterns
    ConnectionPatterns,
    /// Response time patterns
    ResponseTimePatterns,
    /// Resource usage patterns
    ResourceUsagePatterns,
    /// Consensus participation patterns
    ConsensusPatterns,
    /// Custom behavior pattern
    Custom(String),
}

/// Pattern recognition configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternRecognitionConfig {
    /// Enable pattern recognition
    pub enabled: bool,
    
    /// Pattern types to recognize
    pub pattern_types: Vec<PatternType>,
    
    /// Recognition algorithms
    pub algorithms: Vec<RecognitionAlgorithm>,
    
    /// Pattern matching threshold
    pub matching_threshold: f64,
    
    /// Pattern history size
    pub history_size: usize,
    
    /// Pattern update interval
    #[serde(with = "duration_serde")]
    pub update_interval: Duration,
}

/// Pattern types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternType {
    /// Periodic patterns
    Periodic,
    /// Sequential patterns
    Sequential,
    /// Correlation patterns
    Correlation,
    /// Clustering patterns
    Clustering,
    /// Custom pattern
    Custom(String),
}

/// Recognition algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecognitionAlgorithm {
    /// Statistical analysis
    Statistical,
    /// Machine learning
    MachineLearning,
    /// Rule-based
    RuleBased,
    /// Hybrid approach
    Hybrid,
}

/// Anomaly detection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnomalyDetectionConfig {
    /// Enable anomaly detection
    pub enabled: bool,
    
    /// Detection algorithms
    pub algorithms: Vec<AnomalyDetectionAlgorithm>,
    
    /// Anomaly threshold
    pub anomaly_threshold: f64,
    
    /// Detection window
    #[serde(with = "duration_serde")]
    pub detection_window: Duration,
    
    /// Baseline establishment period
    #[serde(with = "duration_serde")]
    pub baseline_period: Duration,
    
    /// Anomaly response actions
    pub response_actions: Vec<AnomalyResponseAction>,
}

/// Anomaly detection algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnomalyDetectionAlgorithm {
    /// Statistical outlier detection
    StatisticalOutlier,
    /// Isolation forest
    IsolationForest,
    /// Local outlier factor
    LocalOutlierFactor,
    /// One-class SVM
    OneClassSvm,
    /// Custom algorithm
    Custom(String),
}

/// Anomaly response actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnomalyResponseAction {
    /// Log the anomaly
    Log,
    /// Reduce reputation score
    ReduceReputation(f64),
    /// Temporary quarantine
    Quarantine(Duration),
    /// Alert administrators
    Alert,
    /// Custom action
    Custom(String),
}

/// Trust propagation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustPropagationConfig {
    /// Enable trust propagation
    pub enabled: bool,
    
    /// Propagation depth
    pub max_depth: usize,
    
    /// Trust decay factor
    pub decay_factor: f64,
    
    /// Minimum trust threshold
    pub min_trust_threshold: f64,
    
    /// Propagation algorithm
    pub algorithm: TrustPropagationAlgorithm,
    
    /// Update frequency
    #[serde(with = "duration_serde")]
    pub update_frequency: Duration,
    
    /// Trust sources
    pub trust_sources: Vec<TrustSource>,
}

/// Trust propagation algorithms
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrustPropagationAlgorithm {
    /// Simple transitive trust
    Transitive,
    /// PageRank-based trust
    PageRank,
    /// Eigentrust algorithm
    EigenTrust,
    /// Beta reputation system
    BetaReputation,
    /// Custom algorithm
    Custom(String),
}

/// Trust sources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrustSource {
    /// Direct interactions
    DirectInteractions,
    /// Witness recommendations
    WitnessRecommendations,
    /// Historical behavior
    HistoricalBehavior,
    /// Network consensus
    NetworkConsensus,
    /// External validators
    ExternalValidators,
}

/// Temporal scoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalScoringConfig {
    /// Enable temporal scoring
    pub enabled: bool,
    
    /// Time-based weights
    pub time_weights: TimeWeights,
    
    /// Scoring intervals
    pub scoring_intervals: Vec<ScoringInterval>,
    
    /// Temporal aggregation method
    pub aggregation_method: TemporalAggregationMethod,
    
    /// Historical data retention
    #[serde(with = "duration_serde")]
    pub historical_retention: Duration,
}

/// Time-based weights
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeWeights {
    /// Recent activity weight (last hour)
    pub recent_weight: f64,
    
    /// Short-term weight (last day)
    pub short_term_weight: f64,
    
    /// Medium-term weight (last week)
    pub medium_term_weight: f64,
    
    /// Long-term weight (last month)
    pub long_term_weight: f64,
    
    /// Historical weight (older than month)
    pub historical_weight: f64,
}

/// Scoring intervals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringInterval {
    /// Interval name
    pub name: String,
    
    /// Interval duration
    #[serde(with = "duration_serde")]
    pub duration: Duration,
    
    /// Weight in final score
    pub weight: f64,
    
    /// Decay function
    pub decay_function: DecayFunction,
}

/// Decay functions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecayFunction {
    /// Linear decay
    Linear,
    /// Exponential decay
    Exponential,
    /// Logarithmic decay
    Logarithmic,
    /// Step function
    Step,
    /// Custom function
    Custom(String),
}

/// Temporal aggregation methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TemporalAggregationMethod {
    /// Weighted average
    WeightedAverage,
    /// Exponential moving average
    ExponentialMovingAverage,
    /// Median
    Median,
    /// Maximum
    Maximum,
    /// Minimum
    Minimum,
    /// Custom aggregation
    Custom(String),
}

/// Reputation aggregation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationAggregationConfig {
    /// Aggregation method
    pub method: AggregationMethod,
    
    /// Component weights
    pub component_weights: ComponentWeights,
    
    /// Normalization method
    pub normalization: NormalizationMethod,
    
    /// Outlier handling
    pub outlier_handling: OutlierHandling,
    
    /// Confidence calculation
    pub confidence_calculation: ConfidenceCalculation,
}

/// Aggregation methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregationMethod {
    /// Weighted sum
    WeightedSum,
    /// Weighted average
    WeightedAverage,
    /// Geometric mean
    GeometricMean,
    /// Harmonic mean
    HarmonicMean,
    /// Median
    Median,
    /// Custom aggregation
    Custom(String),
}

/// Component weights for reputation calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentWeights {
    /// Direct interaction weight
    pub direct_interaction: f64,
    
    /// Behavioral analysis weight
    pub behavioral_analysis: f64,
    
    /// Pattern recognition weight
    pub pattern_recognition: f64,
    
    /// Anomaly detection weight
    pub anomaly_detection: f64,
    
    /// Trust propagation weight
    pub trust_propagation: f64,
    
    /// Temporal scoring weight
    pub temporal_scoring: f64,
    
    /// Network contribution weight
    pub network_contribution: f64,
}

/// Normalization methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NormalizationMethod {
    /// Min-max normalization
    MinMax,
    /// Z-score normalization
    ZScore,
    /// Robust normalization
    Robust,
    /// No normalization
    None,
}

/// Outlier handling methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OutlierHandling {
    /// Remove outliers
    Remove,
    /// Cap outliers
    Cap,
    /// Transform outliers
    Transform,
    /// Include outliers
    Include,
}

/// Confidence calculation methods
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfidenceCalculation {
    /// Sample size based
    SampleSize,
    /// Variance based
    Variance,
    /// Agreement based
    Agreement,
    /// Time decay based
    TimeDecay,
    /// Combined method
    Combined,
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    /// Enable monitoring
    pub enabled: bool,
    
    /// Metrics collection interval
    #[serde(with = "duration_serde")]
    pub metrics_interval: Duration,
    
    /// Health check interval
    #[serde(with = "duration_serde")]
    pub health_check_interval: Duration,
    
    /// Enable detailed logging
    pub enable_detailed_logging: bool,
    
    /// Log level
    pub log_level: String,
    
    /// Metrics to collect
    pub metrics: Vec<MetricType>,
    
    /// Performance monitoring
    pub performance: PerformanceMonitoringConfig,
    
    /// Network monitoring
    pub network: NetworkMonitoringConfig,
    
    /// Resource monitoring
    pub resource: ResourceMonitoringConfig,
}

/// Metric types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricType {
    /// Connection metrics
    Connections,
    /// Message metrics
    Messages,
    /// Bandwidth metrics
    Bandwidth,
    /// Latency metrics
    Latency,
    /// Error metrics
    Errors,
    /// Resource usage metrics
    ResourceUsage,
    /// Reputation metrics
    Reputation,
    /// Custom metric
    Custom(String),
}

/// Performance monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMonitoringConfig {
    /// Enable performance monitoring
    pub enabled: bool,
    
    /// CPU usage monitoring
    pub monitor_cpu: bool,
    
    /// Memory usage monitoring
    pub monitor_memory: bool,
    
    /// Disk I/O monitoring
    pub monitor_disk_io: bool,
    
    /// Network I/O monitoring
    pub monitor_network_io: bool,
    
    /// Performance alert thresholds
    pub alert_thresholds: PerformanceThresholds,
}

/// Performance thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceThresholds {
    /// CPU usage threshold (percentage)
    pub cpu_threshold: f64,
    
    /// Memory usage threshold (percentage)
    pub memory_threshold: f64,
    
    /// Disk usage threshold (percentage)
    pub disk_threshold: f64,
    
    /// Network latency threshold (milliseconds)
    pub latency_threshold: u64,
    
    /// Error rate threshold (percentage)
    pub error_rate_threshold: f64,
}

/// Network monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMonitoringConfig {
    /// Enable network monitoring
    pub enabled: bool,
    
    /// Monitor peer connections
    pub monitor_connections: bool,
    
    /// Monitor message flow
    pub monitor_messages: bool,
    
    /// Monitor gossip health
    pub monitor_gossip: bool,
    
    /// Monitor discovery health
    pub monitor_discovery: bool,
    
    /// Network health check interval
    #[serde(with = "duration_serde")]
    pub health_check_interval: Duration,
}

/// Resource monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMonitoringConfig {
    /// Enable resource monitoring
    pub enabled: bool,
    
    /// Monitor file descriptors
    pub monitor_file_descriptors: bool,
    
    /// Monitor thread count
    pub monitor_threads: bool,
    
    /// Monitor heap usage
    pub monitor_heap: bool,
    
    /// Resource limits
    pub resource_limits: ResourceLimits,
}

/// Resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Maximum file descriptors
    pub max_file_descriptors: Option<u64>,
    
    /// Maximum threads
    pub max_threads: Option<u64>,
    
    /// Maximum heap size (bytes)
    pub max_heap_size: Option<u64>,
    
    /// Maximum connections
    pub max_connections: Option<u64>,
}

// Default implementations for all configuration structures
impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            peer: PeerConfig::default(),
            transport: TransportConfig::default(),
            gossip: GossipConfig::default(),
            safety_limits: SafetyLimitsConfig::default(),
            discovery: DiscoveryConfig::default(),
            security: SecurityConfig::default(),
            bandwidth: BandwidthConfig::default(),
            reputation: ReputationConfig::default(),
            monitoring: MonitoringConfig::default(),
        }
    }
}

impl Default for SafetyLimitsConfig {
    fn default() -> Self {
        Self {
            // Match the previously hard-coded defaults in the transports.
            max_gossip_message_bytes: 8 * 1024 * 1024,
            per_peer_max_msgs_per_sec: 200,
            per_peer_max_bytes_per_sec: 8 * 1024 * 1024,
            max_tcp_frame_bytes: 8 * 1024 * 1024,
            per_conn_max_msgs_per_sec: 200,
            per_conn_max_bytes_per_sec: 8 * 1024 * 1024,
            max_hops: 10,
            dedup_cache_max_entries: 20_000,
            dial_jitter_max_ms: 250,
            dial_backoff_max_ms: 60_000,
        }
    }
}

impl Default for PeerConfig {
    fn default() -> Self {
        Self {
            keypair: None,
            keypair_path: Some(PathBuf::from("./network_keypair")),
            listen_addresses: vec![
                "/ip4/0.0.0.0/tcp/9000".parse().unwrap(),
                "/ip4/0.0.0.0/tcp/9001/ws".parse().unwrap(),
            ],
            external_addresses: Vec::new(),
            bootstrap_peers: Vec::new(),
            max_peers: 50,
            min_peers: 10,
            connection_timeout: Duration::from_secs(30),
            keep_alive_interval: Duration::from_secs(60),
            idle_timeout: Duration::from_secs(300),
            max_connections_per_ip: 5,
            max_retry_attempts: 3,
            retry_backoff: Duration::from_secs(5),
            enable_connection_metrics: true,
        }
    }
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            enable_tcp: true,
            enable_websocket: true,
            enable_quic: false,
            tcp_port_range: (9000, 9100),
            websocket_port: Some(9001),
            quic_port: Some(9002),
            enable_noise: true,
            enable_yamux: true,
            max_connections_per_ip: 5,
            max_connections: 100,
            upgrade_timeout: Duration::from_secs(10),
            keep_alive: KeepAliveConfig::default(),
            buffer_config: BufferConfig::default(),
        }
    }
}

impl Default for KeepAliveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(10),
            max_failures: 3,
        }
    }
}

impl Default for BufferConfig {
    fn default() -> Self {
        Self {
            send_buffer_size: 64 * 1024,      // 64 KB
            receive_buffer_size: 64 * 1024,   // 64 KB
            connection_buffer_size: 16 * 1024, // 16 KB
            message_queue_size: 1000,
        }
    }
}

impl Default for GossipConfig {
    fn default() -> Self {
        Self {
            topic_name: "catalyst".to_string(),
            message_cache_size: 1000,
            message_cache_duration: Duration::from_secs(300),
            heartbeat_interval: Duration::from_secs(1),
            gossip_factor: 3,
            mesh_n: 6,
            mesh_n_low: 4,
            mesh_n_high: 12,
            validation_timeout: Duration::from_secs(5),
            enable_signing: true,
            duplicate_detection_window: Duration::from_secs(60),
            flood_publish: false,
            propagation: PropagationConfig::default(),
            scoring: GossipScoringConfig::default(),
        }
    }
}

impl Default for PropagationConfig {
    fn default() -> Self {
        Self {
            publish_threshold: -4000.0,
            gossip_threshold: -2000.0,
            accept_px_threshold: 100.0,
            opportunistic_graft_threshold: 5.0,
        }
    }
}

impl Default for GossipScoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            topic_weight: 1.0,
            time_in_mesh_weight: 1.0,
            time_in_mesh_quantum: Duration::from_secs(1),
            time_in_mesh_cap: 3600.0,
            first_message_deliveries_weight: 1.0,
            first_message_deliveries_decay: 0.5,
            first_message_deliveries_cap: 2000.0,
            mesh_message_deliveries_weight: -1.0,
            mesh_message_deliveries_decay: 0.5,
            mesh_message_deliveries_cap: 100.0,
            mesh_message_deliveries_threshold: 20.0,
            mesh_message_deliveries_window: Duration::from_secs(10),
            mesh_message_deliveries_activation: Duration::from_secs(5),
            mesh_failure_penalty_weight: -1.0,
            mesh_failure_penalty_decay: 0.5,
            invalid_message_deliveries_weight: -1.0,
            invalid_message_deliveries_decay: 0.3,
        }
    }
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            enable_mdns: true,
            enable_kademlia: true,
            enable_random_walk: false,
            kademlia_replication: 20,
            bootstrap_interval: Duration::from_secs(30),
            discovery_interval: Duration::from_secs(60),
            max_discovery_attempts: 5,
            discovery_timeout: Duration::from_secs(10),
            enable_autonat: true,
            enable_relay: false,
            enable_relay_v2: false,
            enable_dcutr: false,
            mdns: MdnsConfig::default(),
            kademlia: KademliaConfig::default(),
            autonat: AutonatConfig::default(),
            relay: RelayConfig::default(),
        }
    }
}

impl Default for MdnsConfig {
    fn default() -> Self {
        Self {
            service_name: "_catalyst._tcp.local".to_string(),
            query_interval: Duration::from_secs(60),
            ttl: Duration::from_secs(300),
            enable_ipv6: true,
        }
    }
}

impl Default for KademliaConfig {
    fn default() -> Self {
        Self {
            protocol_name: "/catalyst/kad/1.0.0".to_string(),
            replication_factor: 20,
            query_timeout: Duration::from_secs(60),
            replication_interval: Duration::from_secs(3600),
            publication_interval: Duration::from_secs(86400),
            record_ttl: Duration::from_secs(36 * 3600),
            provider_record_ttl: Duration::from_secs(24 * 3600),
            max_packet_size: 4096,
            automatic_mode: true,
        }
    }
}

impl Default for AutonatConfig {
    fn default() -> Self {
        Self {
            only_global_ips: false,
            throttle_server_creation: Duration::from_secs(2),
            boot_delay: Duration::from_secs(15),
            refresh_interval: Duration::from_secs(15),
            retry_interval: Duration::from_secs(90),
            confidence_max: 3,
        }
    }
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            max_circuits: 16,
            max_circuit_duration: Duration::from_secs(120),
            max_circuit_bytes: 1 << 17, // 128 KB
            enable_server: false,
            enable_client: true,
            circuit_src_rate_limiter: RateLimiterConfig::default(),
        }
    }
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window: Duration::from_secs(60),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            allowed_peers: HashSet::new(),
            blocked_peers: HashSet::new(),
            blocked_ips: HashSet::new(),
            blocked_subnets: HashSet::new(),
            max_message_size: 1024 * 1024, // 1MB
            enable_encryption: true,
            auth_timeout: Duration::from_secs(10),
            verify_peer_identity: true,
            trust_threshold: 0.5,
            message_validation: MessageValidationConfig::default(),
            connection_security: ConnectionSecurityConfig::default(),
            security_rate_limits: SecurityRateLimits::default(),
        }
    }
}

impl Default for MessageValidationConfig {
    fn default() -> Self {
        Self {
            strict_validation: true,
            max_message_age: Duration::from_secs(300),
            require_signatures: false,
            enable_content_filtering: false,
            blacklisted_patterns: Vec::new(),
            max_message_chain_length: 100,
            enable_deduplication: true,
            deduplication_window: Duration::from_secs(300),
        }
    }
}

impl Default for ConnectionSecurityConfig {
    fn default() -> Self {
        Self {
            require_secure_connections: true,
            min_tls_version: "1.3".to_string(),
            allowed_cipher_suites: vec![
                "TLS_AES_256_GCM_SHA384".to_string(),
                "TLS_CHACHA20_POLY1305_SHA256".to_string(),
                "TLS_AES_128_GCM_SHA256".to_string(),
            ],
            validate_certificates: false,
            require_perfect_forward_secrecy: true,
            handshake_timeout: Duration::from_secs(30),
            enable_fingerprinting: false,
            fingerprint_cache_size: 1000,
            fingerprint_cache_duration: Duration::from_secs(3600),
        }
    }
}

impl Default for SecurityRateLimits {
    fn default() -> Self {
        Self {
            connections_per_ip: RateLimiterConfig {
                max_requests: 10,
                window: Duration::from_secs(60),
            },
            messages_per_peer: RateLimiterConfig {
                max_requests: 1000,
                window: Duration::from_secs(60),
            },
            auth_attempts: RateLimiterConfig {
                max_requests: 5,
                window: Duration::from_secs(300),
            },
            invalid_message_threshold: RateLimiterConfig {
                max_requests: 10,
                window: Duration::from_secs(60),
            },
            bandwidth_per_ip: RateLimiterConfig {
                max_requests: 10485760, // 10 MB in 60 seconds
                window: Duration::from_secs(60),
            },
            peer_introductions: RateLimiterConfig {
                max_requests: 5,
                window: Duration::from_secs(60),
            },
        }
    }
}

impl Default for BandwidthConfig {
    fn default() -> Self {
        Self {
            enable_limiting: true,
            upload_limit: Some(1024 * 1024 * 10), // 10 MB/s
            download_limit: Some(1024 * 1024 * 20), // 20 MB/s
            per_peer_limit: Some(1024 * 1024), // 1 MB/s per peer
            burst_allowance: 1024 * 1024 * 5, // 5 MB burst
            rate_limit_window: Duration::from_secs(1),
            message_priorities: MessagePriorityConfig::default(),
            traffic_shaping: TrafficShapingConfig::default(),
            qos: QosConfig::default(),
            adaptive: AdaptiveBandwidthConfig::default(),
        }
    }
}

impl Default for MessagePriorityConfig {
    fn default() -> Self {
        Self {
            consensus_priority: 10,
            transaction_priority: 8,
            discovery_priority: 5,
            system_priority: 7,
            default_priority: 3,
            priority_weights: vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0],
            enable_dynamic_priority: false,
            priority_adjustment_interval: Duration::from_secs(60),
        }
    }
}

impl Default for TrafficShapingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token_bucket: TokenBucketConfig::default(),
            fair_queuing: false,
            congestion_control: CongestionControlConfig::default(),
            traffic_classification: TrafficClassificationConfig::default(),
        }
    }
}

impl Default for TokenBucketConfig {
    fn default() -> Self {
        Self {
            capacity: 1000,
            refill_rate: 100,
            initial_tokens: 500,
            max_burst_size: 2000,
            algorithm: TokenBucketAlgorithm::Classic,
        }
    }
}

impl Default for CongestionControlConfig {
    fn default() -> Self {
        Self {
            algorithm: CongestionControlAlgorithm::Bbr,
            parameters: HashMap::new(),
            enable_ecn: false,
            initial_window_size: 10,
            max_window_size: 65535,
            slow_start_threshold: 65535,
            rtt_measurement_window: Duration::from_secs(10),
        }
    }
}

impl Default for TrafficClassificationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            rules: Vec::new(),
            default_class: "normal".to_string(),
            enable_dpi: false,
        }
    }
}

impl Default for QosConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            service_classes: vec![
                ServiceClass {
                    name: "critical".to_string(),
                    bandwidth_allocation: 40.0,
                    max_latency: Duration::from_millis(10),
                    max_jitter: Duration::from_millis(2),
                    packet_loss_threshold: 0.001,
                    priority: 10,
                    guaranteed_bandwidth: Some(1024 * 1024 * 4), // 4 MB/s
                    max_bandwidth: Some(1024 * 1024 * 8), // 8 MB/s
                },
                ServiceClass {
                    name: "high".to_string(),
                    bandwidth_allocation: 30.0,
                    max_latency: Duration::from_millis(50),
                    max_jitter: Duration::from_millis(10),
                    packet_loss_threshold: 0.01,
                    priority: 8,
                    guaranteed_bandwidth: Some(1024 * 1024 * 3), // 3 MB/s
                    max_bandwidth: Some(1024 * 1024 * 6), // 6 MB/s
                },
                ServiceClass {
                    name: "normal".to_string(),
                    bandwidth_allocation: 20.0,
                    max_latency: Duration::from_millis(200),
                    max_jitter: Duration::from_millis(50),
                    packet_loss_threshold: 0.05,
                    priority: 5,
                    guaranteed_bandwidth: Some(1024 * 1024 * 2), // 2 MB/s
                    max_bandwidth: Some(1024 * 1024 * 4), // 4 MB/s
                },
                ServiceClass {
                    name: "low".to_string(),
                    bandwidth_allocation: 10.0,
                    max_latency: Duration::from_secs(1),
                    max_jitter: Duration::from_millis(200),
                    packet_loss_threshold: 0.1,
                    priority: 2,
                    guaranteed_bandwidth: Some(1024 * 1024), // 1 MB/s
                    max_bandwidth: Some(1024 * 1024 * 2), // 2 MB/s
                },
            ],
            default_class: "normal".to_string(),
            scheduling_algorithm: QosSchedulingAlgorithm::WeightedFairQueuing,
            enable_admission_control: false,
            admission_control: AdmissionControlConfig::default(),
        }
    }
}

impl Default for AdmissionControlConfig {
    fn default() -> Self {
        let mut max_connections = HashMap::new();
        max_connections.insert("critical".to_string(), 10);
        max_connections.insert("high".to_string(), 20);
        max_connections.insert("normal".to_string(), 50);
        max_connections.insert("low".to_string(), 100);
        
        let mut bandwidth_thresholds = HashMap::new();
        bandwidth_thresholds.insert("critical".to_string(), 0.8);
        bandwidth_thresholds.insert("high".to_string(), 0.6);
        bandwidth_thresholds.insert("normal".to_string(), 0.4);
        bandwidth_thresholds.insert("low".to_string(), 0.2);
        
        Self {
            max_connections_per_class: max_connections,
            bandwidth_reservation_thresholds: bandwidth_thresholds,
            enable_preemption: false,
            preemption_policy: PreemptionPolicy::None,
        }
    }
}

impl Default for AdaptiveBandwidthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            monitoring_interval: Duration::from_secs(30),
            adjustment_sensitivity: 0.1,
            min_adjustment_threshold: 0.05,
            max_adjustment_factor: 2.0,
            network_condition_factors: NetworkConditionFactors::default(),
            learning_algorithm: LearningAlgorithm::None,
        }
    }
}

impl Default for NetworkConditionFactors {
    fn default() -> Self {
        Self {
            latency_weight: 0.3,
            packet_loss_weight: 0.3,
            throughput_weight: 0.2,
            jitter_weight: 0.1,
            peer_count_weight: 0.1,
        }
    }
}

impl Default for ReputationConfig {
    fn default() -> Self {
        Self {
            enable_reputation: true,
            initial_reputation: 50.0,
            min_reputation: 10.0,
            max_reputation: 100.0,
            decay_rate: 0.01, // 1% decay per update
            update_interval: Duration::from_secs(60),
            score_adjustments: ScoreAdjustments::default(),
            persistence: ReputationPersistence::default(),
            advanced_scoring: AdvancedScoringConfig::default(),
            aggregation: ReputationAggregationConfig::default(),
        }
    }
}

impl Default for ScoreAdjustments {
    fn default() -> Self {
        Self {
            successful_delivery: 1.0,
            failed_delivery: -2.0,
            invalid_message: -5.0,
            connection_drop: -3.0,
            slow_response: -0.5,
            fast_response: 0.5,
            protocol_violation: -10.0,
            helpful_behavior: 2.0,
            malicious_behavior: -20.0,
            consensus_participation: 3.0,
            resource_sharing: 1.5,
            uptime_contribution: 1.0,
            network_stability: 2.0,
            bandwidth_sharing: 1.5,
            security_contribution: 2.5,
        }
    }
}

impl Default for ReputationPersistence {
    fn default() -> Self {
        Self {
            enabled: true,
            storage_path: Some(PathBuf::from("./reputation.db")),
            save_interval: Duration::from_secs(300), // 5 minutes
            max_entries: 10000,
            entry_expiration: Duration::from_secs(7 * 24 * 3600), // 1 week
            enable_compression: true,
            enable_backup: true,
            backup_interval: Duration::from_secs(24 * 3600), // 24 hours
            max_backup_files: 7,
        }
    }
}

impl Default for AdvancedScoringConfig {
    fn default() -> Self {
        Self {
            enable_ml_scoring: false,
            behavioral_analysis: BehavioralAnalysisConfig::default(),
            pattern_recognition: PatternRecognitionConfig::default(),
            anomaly_detection: AnomalyDetectionConfig::default(),
            trust_propagation: TrustPropagationConfig::default(),
            temporal_scoring: TemporalScoringConfig::default(),
        }
    }
}

impl Default for BehavioralAnalysisConfig {
    fn default() -> Self {
        let mut behavior_weights = HashMap::new();
        behavior_weights.insert("message_patterns".to_string(), 0.3);
        behavior_weights.insert("connection_patterns".to_string(), 0.2);
        behavior_weights.insert("response_time_patterns".to_string(), 0.2);
        behavior_weights.insert("resource_usage_patterns".to_string(), 0.15);
        behavior_weights.insert("consensus_patterns".to_string(), 0.15);
        
        Self {
            enabled: false,
            analysis_window: Duration::from_secs(3600), // 1 hour
            tracked_behaviors: vec![
                BehaviorCategory::MessagePatterns,
                BehaviorCategory::ConnectionPatterns,
                BehaviorCategory::ResponseTimePatterns,
            ],
            behavior_weights,
            min_sample_size: 10,
            update_frequency: Duration::from_secs(300), // 5 minutes
        }
    }
}

impl Default for PatternRecognitionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            pattern_types: vec![PatternType::Periodic, PatternType::Sequential],
            algorithms: vec![RecognitionAlgorithm::Statistical],
            matching_threshold: 0.8,
            history_size: 100,
            update_interval: Duration::from_secs(600), // 10 minutes
        }
    }
}

impl Default for AnomalyDetectionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            algorithms: vec![AnomalyDetectionAlgorithm::StatisticalOutlier],
            anomaly_threshold: 0.95,
            detection_window: Duration::from_secs(3600), // 1 hour
            baseline_period: Duration::from_secs(24 * 3600), // 24 hours
            response_actions: vec![
                AnomalyResponseAction::Log,
                AnomalyResponseAction::ReduceReputation(5.0),
            ],
        }
    }
}

impl Default for TrustPropagationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_depth: 3,
            decay_factor: 0.5,
            min_trust_threshold: 0.1,
            algorithm: TrustPropagationAlgorithm::Transitive,
            update_frequency: Duration::from_secs(3600), // 1 hour
            trust_sources: vec![
                TrustSource::DirectInteractions,
                TrustSource::HistoricalBehavior,
            ],
        }
    }
}

impl Default for TemporalScoringConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            time_weights: TimeWeights::default(),
            scoring_intervals: vec![
                ScoringInterval {
                    name: "recent".to_string(),
                    duration: Duration::from_secs(3600), // 1 hour
                    weight: 0.4,
                    decay_function: DecayFunction::Exponential,
                },
                ScoringInterval {
                    name: "short_term".to_string(),
                    duration: Duration::from_secs(24 * 3600), // 1 day
                    weight: 0.3,
                    decay_function: DecayFunction::Linear,
                },
                ScoringInterval {
                    name: "long_term".to_string(),
                    duration: Duration::from_secs(7 * 24 * 3600), // 1 week
                    weight: 0.3,
                    decay_function: DecayFunction::Linear,
                },
            ],
            aggregation_method: TemporalAggregationMethod::WeightedAverage,
            historical_retention: Duration::from_secs(30 * 24 * 3600), // 30 days
        }
    }
}

impl Default for TimeWeights {
    fn default() -> Self {
        Self {
            recent_weight: 0.4,
            short_term_weight: 0.3,
            medium_term_weight: 0.2,
            long_term_weight: 0.1,
            historical_weight: 0.05,
        }
    }
}

impl Default for ReputationAggregationConfig {
    fn default() -> Self {
        Self {
            method: AggregationMethod::WeightedAverage,
            component_weights: ComponentWeights::default(),
            normalization: NormalizationMethod::MinMax,
            outlier_handling: OutlierHandling::Cap,
            confidence_calculation: ConfidenceCalculation::SampleSize,
        }
    }
}

impl Default for ComponentWeights {
    fn default() -> Self {
        Self {
            direct_interaction: 0.4,
            behavioral_analysis: 0.15,
            pattern_recognition: 0.1,
            anomaly_detection: 0.1,
            trust_propagation: 0.1,
            temporal_scoring: 0.1,
            network_contribution: 0.05,
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            metrics_interval: Duration::from_secs(30),
            health_check_interval: Duration::from_secs(60),
            enable_detailed_logging: false,
            log_level: "info".to_string(),
            metrics: vec![
                MetricType::Connections,
                MetricType::Messages,
                MetricType::Bandwidth,
                MetricType::Latency,
                MetricType::Errors,
            ],
            performance: PerformanceMonitoringConfig::default(),
            network: NetworkMonitoringConfig::default(),
            resource: ResourceMonitoringConfig::default(),
        }
    }
}

impl Default for PerformanceMonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            monitor_cpu: true,
            monitor_memory: true,
            monitor_disk_io: false,
            monitor_network_io: true,
            alert_thresholds: PerformanceThresholds::default(),
        }
    }
}

impl Default for PerformanceThresholds {
    fn default() -> Self {
        Self {
            cpu_threshold: 80.0,
            memory_threshold: 85.0,
            disk_threshold: 90.0,
            latency_threshold: 1000, // 1 second
            error_rate_threshold: 5.0,
        }
    }
}

impl Default for NetworkMonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            monitor_connections: true,
            monitor_messages: true,
            monitor_gossip: true,
            monitor_discovery: true,
            health_check_interval: Duration::from_secs(30),
        }
    }
}

impl Default for ResourceMonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            monitor_file_descriptors: true,
            monitor_threads: true,
            monitor_heap: true,
            resource_limits: ResourceLimits::default(),
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_file_descriptors: Some(65536),
            max_threads: Some(1000),
            max_heap_size: Some(4 * 1024 * 1024 * 1024), // 4 GB
            max_connections: Some(1000),
        }
    }
}

// Implementation methods for NetworkConfig
impl NetworkConfig {
    /// Create a new network configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Load configuration from file
    pub fn load_from_file(path: &std::path::Path) -> NetworkResult<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| NetworkError::ConfigError(format!("Failed to read config file: {}", e)))?;
        let config: NetworkConfig = toml::from_str(&content)
            .map_err(|e| NetworkError::ConfigError(format!("Failed to parse config: {}", e)))?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn save_to_file(&self, path: &std::path::Path) -> NetworkResult<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| NetworkError::ConfigError(format!("Failed to serialize config: {}", e)))?;
        std::fs::write(path, content)
            .map_err(|e| NetworkError::ConfigError(format!("Failed to write config file: {}", e)))?;
        Ok(())
    }

    /// Validate the configuration
    pub fn validate(&self) -> NetworkResult<()> {
        // Validate peer configuration
        if self.peer.max_peers < self.peer.min_peers {
            return Err(NetworkError::ConfigError(
                "max_peers must be greater than or equal to min_peers".to_string()
            ));
        }

        // Validate safety limits
        let sl = &self.safety_limits;
        if sl.max_gossip_message_bytes == 0 || sl.max_tcp_frame_bytes == 0 {
            return Err(NetworkError::ConfigError(
                "safety_limits max message/frame bytes must be > 0".to_string(),
            ));
        }
        if sl.per_peer_max_msgs_per_sec == 0
            || sl.per_conn_max_msgs_per_sec == 0
            || sl.per_peer_max_bytes_per_sec == 0
            || sl.per_conn_max_bytes_per_sec == 0
        {
            return Err(NetworkError::ConfigError(
                "safety_limits per-peer/per-conn budgets must be > 0".to_string(),
            ));
        }
        if sl.max_hops == 0 {
            return Err(NetworkError::ConfigError(
                "safety_limits.max_hops must be > 0".to_string(),
            ));
        }
        if sl.dedup_cache_max_entries == 0 {
            return Err(NetworkError::ConfigError(
                "safety_limits.dedup_cache_max_entries must be > 0".to_string(),
            ));
        }
        if sl.dial_backoff_max_ms == 0 {
            return Err(NetworkError::ConfigError(
                "safety_limits.dial_backoff_max_ms must be > 0".to_string(),
            ));
        }

        // Validate port ranges
        if self.transport.tcp_port_range.0 >= self.transport.tcp_port_range.1 {
            return Err(NetworkError::ConfigError(
                "TCP port range start must be less than end".to_string()
            ));
        }

        // Validate bandwidth limits
        if let (Some(upload), Some(download)) = (self.bandwidth.upload_limit, self.bandwidth.download_limit) {
            if upload > download {
                log::warn!("Upload limit is higher than download limit, this may cause issues");
            }
        }

        // Validate reputation settings
        if self.reputation.min_reputation >= self.reputation.max_reputation {
            return Err(NetworkError::ConfigError(
                "min_reputation must be less than max_reputation".to_string()
            ));
        }

        Ok(())
    }

    /// Get a configuration optimized for testing
    pub fn test_config() -> Self {
        let mut config = Self::default();
        
        // Use different ports for testing
        config.peer.listen_addresses = vec![
            "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
        ];
        config.transport.tcp_port_range = (10000, 10100);
        
        // Reduce timeouts for faster tests
        config.peer.connection_timeout = Duration::from_secs(5);
        config.discovery.discovery_timeout = Duration::from_secs(3);
        config.gossip.validation_timeout = Duration::from_secs(1);
        
        // Disable some features for simpler testing
        config.security.enable_encryption = false;
        config.reputation.enable_reputation = false;
        config.bandwidth.enable_limiting = false;
        
        config
    }

    /// Get a configuration optimized for development
    pub fn development_config() -> Self {
        let mut config = Self::default();
        
        // More verbose logging
        config.monitoring.enable_detailed_logging = true;
        config.monitoring.log_level = "debug".to_string();
        
        // Shorter intervals for faster development feedback
        config.discovery.discovery_interval = Duration::from_secs(30);
        config.gossip.heartbeat_interval = Duration::from_millis(500);
        
        // Allow more connections for testing
        config.peer.max_peers = 100;
        config.transport.max_connections = 200;
        
        config
    }

    /// Get a configuration optimized for production
    pub fn production_config() -> Self {
        let mut config = Self::default();
        
        // Enable all security features
        config.security.enable_encryption = true;
        config.security.verify_peer_identity = true;
        config.security.message_validation.strict_validation = true;
        
        // Enable reputation system
        config.reputation.enable_reputation = true;
        config.reputation.advanced_scoring.enable_ml_scoring = false; // Keep disabled until stable
        
        // Enable bandwidth limiting
        config.bandwidth.enable_limiting = true;
        config.bandwidth.adaptive.enabled = true;
        
        // Enable comprehensive monitoring
        config.monitoring.enabled = true;
        config.monitoring.performance.enabled = true;
        config.monitoring.network.enabled = true;
        config.monitoring.resource.enabled = true;
        
        // Conservative connection limits
        config.peer.max_peers = 50;
        config.transport.max_connections = 100;
        config.peer.max_connections_per_ip = 3;
        
        config
    }
}