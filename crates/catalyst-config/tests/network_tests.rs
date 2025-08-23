use catalyst_config::loader::ConfigLoader;
use catalyst_config::networks::{
    ConsensusConfig, Network, NetworkConfig, NetworkSettings, NodeConfig, RpcConfig,
    ServiceBusConfig, StorageConfig,
};
use catalyst_config::validation::ConfigValidator;
use std::fs;
use tempfile::tempdir;

/// Test default network configurations
#[test]
fn test_default_network_configs() {
    // Test devnet configuration
    let devnet = NetworkConfig::devnet();
    assert_eq!(devnet.network.name, "devnet");
    assert_eq!(devnet.network.id, 1);
    assert_eq!(devnet.network.port, 9944);
    assert_eq!(devnet.network.bind_address, "0.0.0.0");
    assert_eq!(devnet.network.max_peers, 50);
    assert!(devnet.network.bootstrap_peers.is_empty());
    assert!(devnet.consensus.enabled);
    assert_eq!(devnet.consensus.cycle_time_ms, 30000);
    assert_eq!(devnet.consensus.max_producers, 100);

    // Test testnet configuration
    let testnet = NetworkConfig::testnet();
    assert_eq!(testnet.network.name, "testnet");
    assert_eq!(testnet.network.id, 2);
    assert_eq!(testnet.network.port, 9944);
    assert!(testnet.consensus.enabled);
    assert!(!testnet.network.bootstrap_peers.is_empty()); // Should have bootstrap peers

    // Test mainnet configuration
    let mainnet = NetworkConfig::mainnet();
    assert_eq!(mainnet.network.name, "mainnet");
    assert_eq!(mainnet.network.id, 0);
    assert_eq!(mainnet.network.port, 9944);
    assert!(mainnet.consensus.enabled);
    assert!(!mainnet.network.bootstrap_peers.is_empty()); // Should have bootstrap peers
}

/// Test network-specific settings
#[test]
fn test_network_specific_settings() {
    let networks = vec![
        (Network::Devnet, NetworkConfig::devnet()),
        (Network::Testnet, NetworkConfig::testnet()),
        (Network::Mainnet, NetworkConfig::mainnet()),
    ];

    for (network_type, config) in networks {
        match network_type {
            Network::Devnet => {
                // Devnet should have development-friendly settings
                assert_eq!(config.node.log_level, "info");
                assert!(config.rpc.enabled); // RPC should be enabled for development
                assert_eq!(config.consensus.cycle_time_ms, 30000); // Faster cycles for dev
            }
            Network::Testnet => {
                // Testnet should have more realistic settings
                assert_eq!(config.network.id, 2);
                assert!(config.consensus.enabled);
                assert!(config.rpc.enabled); // RPC enabled for testing
            }
            Network::Mainnet => {
                // Mainnet should have production settings
                assert_eq!(config.network.id, 0);
                assert!(config.consensus.enabled);
                // More conservative settings for production
                assert!(config.consensus.max_producers >= 100);
            }
        }

        // All networks should have valid configurations
        let validator = ConfigValidator::new();
        assert!(validator.validate(&config).is_ok());
    }
}

/// Test for_network factory method
#[test]
fn test_for_network_factory() {
    let devnet = NetworkConfig::for_network(Network::Devnet);
    let testnet = NetworkConfig::for_network(Network::Testnet);
    let mainnet = NetworkConfig::for_network(Network::Mainnet);

    assert_eq!(devnet.network.name, "devnet");
    assert_eq!(testnet.network.name, "testnet");
    assert_eq!(mainnet.network.name, "mainnet");

    // Each should be equivalent to their direct constructors
    assert_eq!(devnet.network.id, NetworkConfig::devnet().network.id);
    assert_eq!(testnet.network.id, NetworkConfig::testnet().network.id);
    assert_eq!(mainnet.network.id, NetworkConfig::mainnet().network.id);
}

/// Test bootstrap peers configuration
#[test]
fn test_bootstrap_peers() {
    // Devnet should have no bootstrap peers (local development)
    let devnet = NetworkConfig::devnet();
    assert!(devnet.network.bootstrap_peers.is_empty());

    // Testnet should have testnet bootstrap peers
    let testnet = NetworkConfig::testnet();
    assert!(!testnet.network.bootstrap_peers.is_empty());
    assert!(testnet
        .network
        .bootstrap_peers
        .iter()
        .any(|peer| peer.contains("testnet")));

    // Mainnet should have mainnet bootstrap peers
    let mainnet = NetworkConfig::mainnet();
    assert!(!mainnet.network.bootstrap_peers.is_empty());
    assert!(mainnet
        .network
        .bootstrap_peers
        .iter()
        .any(|peer| peer.contains("mainnet") || peer.contains("catalyst")));
}

/// Test network port assignments
#[test]
fn test_network_ports() {
    let devnet = NetworkConfig::devnet();
    let testnet = NetworkConfig::testnet();
    let mainnet = NetworkConfig::mainnet();

    // All networks use the same base port by default
    assert_eq!(devnet.network.port, 9944);
    assert_eq!(testnet.network.port, 9944);
    assert_eq!(mainnet.network.port, 9944);

    // But they should have different RPC ports to avoid conflicts when running multiple networks
    // (This could be a future enhancement)
    assert_eq!(devnet.rpc.port, 9933);
    assert_eq!(testnet.rpc.port, 9933);
    assert_eq!(mainnet.rpc.port, 9933);
}

/// Test consensus settings across networks
#[test]
fn test_consensus_settings() {
    let devnet = NetworkConfig::devnet();
    let testnet = NetworkConfig::testnet();
    let mainnet = NetworkConfig::mainnet();

    // All networks should have consensus enabled
    assert!(devnet.consensus.enabled);
    assert!(testnet.consensus.enabled);
    assert!(mainnet.consensus.enabled);

    // Devnet might have faster cycles for development
    assert_eq!(devnet.consensus.cycle_time_ms, 30000); // 30 seconds

    // Testnet and mainnet should have production-like timing
    assert!(testnet.consensus.cycle_time_ms >= 30000);
    assert!(mainnet.consensus.cycle_time_ms >= 30000);

    // All should have reasonable producer limits
    assert!(devnet.consensus.max_producers >= 10);
    assert!(testnet.consensus.max_producers >= 50);
    assert!(mainnet.consensus.max_producers >= 100);
}

/// Test storage configuration across networks
#[test]
fn test_storage_configuration() {
    let devnet = NetworkConfig::devnet();
    let testnet = NetworkConfig::testnet();
    let mainnet = NetworkConfig::mainnet();

    // Each network should have its own storage directory
    assert!(
        devnet.storage.path.to_string_lossy().contains("devnet")
            || devnet.storage.path.to_string_lossy().contains("data")
    );
    assert!(
        testnet.storage.path.to_string_lossy().contains("testnet")
            || testnet.storage.path.to_string_lossy().contains("data")
    );
    assert!(
        mainnet.storage.path.to_string_lossy().contains("mainnet")
            || mainnet.storage.path.to_string_lossy().contains("data")
    );

    // Cache sizes should be reasonable
    assert!(devnet.storage.cache_size_mb >= 128);
    assert!(testnet.storage.cache_size_mb >= 256);
    assert!(mainnet.storage.cache_size_mb >= 256);
}

/// Test service bus configuration
#[test]
fn test_service_bus_configuration() {
    let devnet = NetworkConfig::devnet();
    let testnet = NetworkConfig::testnet();
    let mainnet = NetworkConfig::mainnet();

    // Service bus should be enabled by default for Web2 integration
    assert!(devnet.service_bus.enabled);
    assert!(testnet.service_bus.enabled);
    assert!(mainnet.service_bus.enabled);

    // WebSocket ports should be configured
    assert!(devnet.service_bus.ws_port > 0);
    assert!(testnet.service_bus.ws_port > 0);
    assert!(mainnet.service_bus.ws_port > 0);

    // Bind addresses should be reasonable
    assert!(!devnet.service_bus.bind_address.is_empty());
    assert!(!testnet.service_bus.bind_address.is_empty());
    assert!(!mainnet.service_bus.bind_address.is_empty());
}

/// Test loading network configs from files
#[test]
fn test_load_network_configs_from_files() {
    let temp_dir = tempdir().unwrap();

    // Create network-specific config files
    let devnet_config = r#"
[node]
id = "devnet_file_node"
data_dir = "./devnet_data"
log_level = "debug"

[network]
name = "devnet"
id = 1
port = 9944
bind_address = "0.0.0.0"
max_peers = 25
bootstrap_peers = []

[consensus]
enabled = true
cycle_time_ms = 20000
max_producers = 50

[storage]
path = "./devnet_storage"
cache_size_mb = 128

[rpc]
enabled = true
port = 9933
bind_address = "127.0.0.1"

[service_bus]
enabled = true
ws_port = 9222
bind_address = "127.0.0.1"
"#;

    let devnet_path = temp_dir.path().join("devnet.toml");
    fs::write(&devnet_path, devnet_config).unwrap();

    let mut loader = ConfigLoader::new();
    let loaded_devnet = loader
        .load_network_config(Network::Devnet, Some(devnet_path.to_str().unwrap()))
        .unwrap();

    assert_eq!(loaded_devnet.node.id, "devnet_file_node");
    assert_eq!(loaded_devnet.consensus.cycle_time_ms, 20000);
    assert_eq!(loaded_devnet.network.max_peers, 25);
}

/// Test network configuration validation
#[test]
fn test_network_config_validation() {
    let validator = ConfigValidator::new();

    // Test valid configurations
    assert!(validator.validate(&NetworkConfig::devnet()).is_ok());
    assert!(validator.validate(&NetworkConfig::testnet()).is_ok());
    assert!(validator.validate(&NetworkConfig::mainnet()).is_ok());

    // Test invalid network configuration
    let mut invalid_config = NetworkConfig::devnet();
    invalid_config.network.port = 0; // Invalid port

    let validation_result = validator.validate(&invalid_config);
    assert!(validation_result.is_err());
}

/// Test network configuration serialization/deserialization
#[test]
fn test_network_config_serialization() {
    let original_devnet = NetworkConfig::devnet();
    let original_testnet = NetworkConfig::testnet();
    let original_mainnet = NetworkConfig::mainnet();

    // Test TOML serialization
    let devnet_toml = toml::to_string(&original_devnet).unwrap();
    let testnet_toml = toml::to_string(&original_testnet).unwrap();
    let mainnet_toml = toml::to_string(&original_mainnet).unwrap();

    // Test TOML deserialization
    let deserialized_devnet: NetworkConfig = toml::from_str(&devnet_toml).unwrap();
    let deserialized_testnet: NetworkConfig = toml::from_str(&testnet_toml).unwrap();
    let deserialized_mainnet: NetworkConfig = toml::from_str(&mainnet_toml).unwrap();

    // Verify they match
    assert_eq!(
        original_devnet.network.name,
        deserialized_devnet.network.name
    );
    assert_eq!(original_devnet.network.id, deserialized_devnet.network.id);
    assert_eq!(
        original_testnet.network.name,
        deserialized_testnet.network.name
    );
    assert_eq!(original_testnet.network.id, deserialized_testnet.network.id);
    assert_eq!(
        original_mainnet.network.name,
        deserialized_mainnet.network.name
    );
    assert_eq!(original_mainnet.network.id, deserialized_mainnet.network.id);
}

/// Test custom network creation
#[test]
fn test_custom_network_creation() {
    let custom_network = NetworkSettings {
        name: "custom".to_string(),
        id: 999,
        port: 8888,
        bind_address: "192.168.1.100".to_string(),
        max_peers: 75,
        bootstrap_peers: vec!["custom-peer1.example.com".to_string()],
    };

    let custom_node = NodeConfig {
        id: "custom_node".to_string(),
        data_dir: "./custom_data".into(),
        log_level: "warn".to_string(),
    };

    let custom_consensus = ConsensusConfig {
        enabled: true,
        cycle_time_ms: 45000,
        max_producers: 200,
    };

    let custom_storage = StorageConfig {
        path: "./custom_storage".into(),
        cache_size_mb: 512,
    };

    let custom_rpc = RpcConfig {
        enabled: false,
        port: 8080,
        bind_address: "0.0.0.0".to_string(),
    };

    let custom_service_bus = ServiceBusConfig {
        enabled: true,
        ws_port: 7777,
        bind_address: "0.0.0.0".to_string(),
    };

    let custom_config = NetworkConfig {
        network: custom_network,
        node: custom_node,
        consensus: custom_consensus,
        storage: custom_storage,
        rpc: custom_rpc,
        service_bus: custom_service_bus,
    };

    // Validate custom configuration
    let validator = ConfigValidator::new();
    assert!(validator.validate(&custom_config).is_ok());

    // Verify custom values
    assert_eq!(custom_config.network.name, "custom");
    assert_eq!(custom_config.network.id, 999);
    assert_eq!(custom_config.network.port, 8888);
    assert_eq!(custom_config.node.id, "custom_node");
    assert_eq!(custom_config.consensus.cycle_time_ms, 45000);
    assert!(!custom_config.rpc.enabled);
}

/// Test network configuration cloning and comparison
#[test]
fn test_network_config_clone_and_compare() {
    let original = NetworkConfig::devnet();
    let cloned = original.clone();

    // They should be equal
    assert_eq!(original.network.name, cloned.network.name);
    assert_eq!(original.network.id, cloned.network.id);
    assert_eq!(original.network.port, cloned.network.port);
    assert_eq!(original.node.id, cloned.node.id);
    assert_eq!(original.consensus.enabled, cloned.consensus.enabled);

    // Test inequality
    let mut modified = original.clone();
    modified.network.port = 8888;

    assert_ne!(original.network.port, modified.network.port);
}

/// Test network-specific data directories
#[test]
fn test_network_data_directories() {
    let devnet = NetworkConfig::devnet();
    let testnet = NetworkConfig::testnet();
    let mainnet = NetworkConfig::mainnet();

    // Data directories should be different for each network to avoid conflicts
    let devnet_data = devnet.node.data_dir.to_string_lossy();
    let testnet_data = testnet.node.data_dir.to_string_lossy();
    let mainnet_data = mainnet.node.data_dir.to_string_lossy();

    // They should be different or contain network identifiers
    assert!(
        devnet_data != testnet_data
            || devnet_data.contains("devnet")
            || testnet_data.contains("testnet")
    );
    assert!(
        testnet_data != mainnet_data
            || testnet_data.contains("testnet")
            || mainnet_data.contains("mainnet")
    );
    assert!(
        devnet_data != mainnet_data
            || devnet_data.contains("devnet")
            || mainnet_data.contains("mainnet")
    );
}

/// Test network configuration defaults
#[test]
fn test_network_configuration_defaults() {
    // Test that all networks have sensible defaults
    let networks = vec![
        NetworkConfig::devnet(),
        NetworkConfig::testnet(),
        NetworkConfig::mainnet(),
    ];

    for config in networks {
        // Node config defaults
        assert!(!config.node.id.is_empty());
        assert!(!config.node.data_dir.as_os_str().is_empty());
        assert!(!config.node.log_level.is_empty());

        // Network config defaults
        assert!(!config.network.name.is_empty());
        assert!(config.network.port > 0);
        assert!(!config.network.bind_address.is_empty());
        assert!(config.network.max_peers > 0);

        // Consensus config defaults
        assert!(config.consensus.cycle_time_ms > 0);
        assert!(config.consensus.max_producers > 0);

        // Storage config defaults
        assert!(!config.storage.path.as_os_str().is_empty());
        assert!(config.storage.cache_size_mb > 0);

        // RPC config defaults
        assert!(config.rpc.port > 0);
        assert!(!config.rpc.bind_address.is_empty());

        // Service bus config defaults
        assert!(config.service_bus.ws_port > 0);
        assert!(!config.service_bus.bind_address.is_empty());
    }
}
