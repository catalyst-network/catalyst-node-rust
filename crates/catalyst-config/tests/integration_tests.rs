use catalyst_config::env_override::EnvOverride;
use catalyst_config::events::{ConfigEvent, ConfigEventType};
use catalyst_config::hot_reload::HotReloadManager;
use catalyst_config::loader::ConfigLoader;
use catalyst_config::networks::{Network, NetworkConfig};
use catalyst_config::validation::ConfigValidator;
use catalyst_config::*;
use std::env;
use std::fs;
use std::time::Duration;
use tempfile::{tempdir, NamedTempFile};
use tokio::time::{sleep, timeout};

/// Test the complete configuration loading pipeline
#[tokio::test]
async fn test_complete_config_pipeline() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("test_config.toml");

    // Create a test configuration file
    let config_content = r#"
[node]
id = "integration_test_node"
data_dir = "./test_data"
log_level = "debug"

[network]
name = "testnet"
id = 999
port = 9999
bind_address = "127.0.0.1"
max_peers = 25
bootstrap_peers = ["peer1", "peer2"]

[consensus]
enabled = true
cycle_time_ms = 15000
max_producers = 50

[storage]
path = "./test_storage"
cache_size_mb = 128

[rpc]
enabled = false
port = 9933
bind_address = "127.0.0.1"

[service_bus]
enabled = true
ws_port = 9111
bind_address = "0.0.0.0"
"#;
    fs::write(&config_path, config_content).unwrap();

    // Load configuration
    let mut loader = ConfigLoader::new();
    let config = loader.load_config(&config_path).unwrap();

    // Validate loaded configuration
    assert_eq!(config.node.id, "integration_test_node");
    assert_eq!(config.network.port, 9999);
    assert_eq!(config.consensus.cycle_time_ms, 15000);
    assert!(!config.rpc.enabled);
    assert!(config.service_bus.enabled);
    assert_eq!(config.network.bootstrap_peers.len(), 2);

    // Test validation
    let validator = ConfigValidator::new();
    let validation_result = validator.validate(&config);
    assert!(validation_result.is_ok());
}

/// Test environment variable overrides integration
#[tokio::test]
async fn test_env_override_integration() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("test_config.toml");

    // Create base configuration
    let config_content = r#"
[node]
id = "base_node"
data_dir = "./data"
log_level = "info"

[network]
name = "devnet"
id = 1
port = 9944
bind_address = "0.0.0.0"
max_peers = 50
bootstrap_peers = []

[consensus]
enabled = true
cycle_time_ms = 30000
max_producers = 100

[storage]
path = "./storage"
cache_size_mb = 256

[rpc]
enabled = true
port = 9933
bind_address = "127.0.0.1"

[service_bus]
enabled = false
ws_port = 9222
bind_address = "127.0.0.1"
"#;
    fs::write(&config_path, config_content).unwrap();

    // Set environment variables
    env::set_var("CATALYST_NODE_ID", "env_override_node");
    env::set_var("CATALYST_NETWORK_PORT", "8888");
    env::set_var("CATALYST_RPC_ENABLED", "false");
    env::set_var("CATALYST_SERVICE_BUS_ENABLED", "true");

    // Load configuration with environment overrides
    let mut loader = ConfigLoader::new();
    let mut config = loader.load_config(&config_path).unwrap();

    let mut env_override = EnvOverride::new();
    env_override.load_env_vars().unwrap();
    env_override.apply_to_network_config(&mut config).unwrap();

    // Verify overrides were applied
    assert_eq!(config.node.id, "env_override_node");
    assert_eq!(config.network.port, 8888);
    assert!(!config.rpc.enabled);
    assert!(config.service_bus.enabled);

    // Clean up environment variables
    env::remove_var("CATALYST_NODE_ID");
    env::remove_var("CATALYST_NETWORK_PORT");
    env::remove_var("CATALYST_RPC_ENABLED");
    env::remove_var("CATALYST_SERVICE_BUS_ENABLED");
}

/// Test hot reload functionality
#[tokio::test]
async fn test_hot_reload_integration() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("hot_reload_config.toml");

    // Create initial configuration
    let initial_config = r#"
[node]
id = "hot_reload_node"
data_dir = "./data"
log_level = "info"

[network]
name = "devnet"
id = 1
port = 9944
bind_address = "0.0.0.0"
max_peers = 50
bootstrap_peers = []

[consensus]
enabled = true
cycle_time_ms = 30000
max_producers = 100

[storage]
path = "./storage"
cache_size_mb = 256

[rpc]
enabled = true
port = 9933
bind_address = "127.0.0.1"

[service_bus]
enabled = false
ws_port = 9222
bind_address = "127.0.0.1"
"#;
    fs::write(&config_path, initial_config).unwrap();

    // Load initial configuration
    let mut loader = ConfigLoader::new();
    let config = loader.load_config(&config_path).unwrap();

    // Create hot reload manager
    let mut hot_reload = HotReloadManager::new(config, config_path.clone(), Network::Devnet);
    let mut event_receiver = hot_reload.start_auto_reload().await.unwrap();

    // Wait a bit for initialization
    sleep(Duration::from_millis(100)).await;

    // Modify the configuration file
    let updated_config = r#"
[node]
id = "updated_hot_reload_node"
data_dir = "./data"
log_level = "debug"

[network]
name = "devnet"
id = 1
port = 8080
bind_address = "0.0.0.0"
max_peers = 75
bootstrap_peers = ["new_peer"]

[consensus]
enabled = false
cycle_time_ms = 25000
max_producers = 150

[storage]
path = "./storage"
cache_size_mb = 512

[rpc]
enabled = false
port = 9933
bind_address = "127.0.0.1"

[service_bus]
enabled = true
ws_port = 9333
bind_address = "127.0.0.1"
"#;
    fs::write(&config_path, updated_config).unwrap();

    // Wait for and verify hot reload events
    let timeout_duration = Duration::from_secs(5);

    // Should receive a file modified event and a reload complete event
    let file_modified_event = timeout(timeout_duration, event_receiver.recv()).await;
    assert!(file_modified_event.is_ok());

    // Get the updated configuration
    let reloaded_config = hot_reload.get_current_config();
    assert_eq!(reloaded_config.node.id, "updated_hot_reload_node");
    assert_eq!(reloaded_config.network.port, 8080);
    assert!(!reloaded_config.consensus.enabled);
    assert!(reloaded_config.service_bus.enabled);
    assert_eq!(reloaded_config.network.max_peers, 75);

    hot_reload.stop_auto_reload().await.unwrap();
}

/// Test configuration validation integration
#[tokio::test]
async fn test_validation_integration() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("validation_test.toml");

    // Create configuration with some invalid values
    let invalid_config = r#"
[node]
id = ""
data_dir = "./data"
log_level = "info"

[network]
name = "testnet"
id = 1
port = 0
bind_address = "0.0.0.0"
max_peers = -5
bootstrap_peers = []

[consensus]
enabled = true
cycle_time_ms = 100
max_producers = 0

[storage]
path = "./storage"
cache_size_mb = 0

[rpc]
enabled = true
port = 9933
bind_address = "127.0.0.1"

[service_bus]
enabled = true
ws_port = 9222
bind_address = "127.0.0.1"
"#;
    fs::write(&config_path, invalid_config).unwrap();

    // Try to load the configuration
    let result = std::panic::catch_unwind(|| {
        let mut loader = ConfigLoader::new();
        loader.load_config(&config_path)
    });

    // Should fail due to invalid port (0) and max_peers (-5) parsing
    assert!(result.is_err() || result.unwrap().is_err());

    // Create a valid configuration
    let valid_config = r#"
[node]
id = "valid_node"
data_dir = "./data"
log_level = "info"

[network]
name = "testnet"
id = 1
port = 9944
bind_address = "0.0.0.0"
max_peers = 50
bootstrap_peers = []

[consensus]
enabled = true
cycle_time_ms = 30000
max_producers = 100

[storage]
path = "./storage"
cache_size_mb = 256

[rpc]
enabled = true
port = 9933
bind_address = "127.0.0.1"

[service_bus]
enabled = true
ws_port = 9222
bind_address = "127.0.0.1"
"#;
    fs::write(&config_path, valid_config).unwrap();

    // Load and validate
    let mut loader = ConfigLoader::new();
    let config = loader.load_config(&config_path).unwrap();

    let validator = ConfigValidator::new();
    let validation_result = validator.validate(&config);
    assert!(validation_result.is_ok());
}

/// Test network-specific configuration loading
#[tokio::test]
async fn test_network_config_integration() {
    // Test loading different network configurations
    let networks = vec![Network::Devnet, Network::Testnet, Network::Mainnet];

    for network in networks {
        let config = NetworkConfig::for_network(network);

        // Verify network-specific settings
        match network {
            Network::Devnet => {
                assert_eq!(config.network.name, "devnet");
                assert_eq!(config.network.id, 1);
                assert!(config.consensus.enabled);
            }
            Network::Testnet => {
                assert_eq!(config.network.name, "testnet");
                assert_eq!(config.network.id, 2);
                assert!(config.consensus.enabled);
            }
            Network::Mainnet => {
                assert_eq!(config.network.name, "mainnet");
                assert_eq!(config.network.id, 0);
                assert!(config.consensus.enabled);
            }
        }

        // Validate each network configuration
        let validator = ConfigValidator::new();
        let validation_result = validator.validate(&config);
        assert!(
            validation_result.is_ok(),
            "Network {:?} config validation failed",
            network
        );
    }
}

/// Test error handling across components
#[tokio::test]
async fn test_error_handling_integration() {
    // Test loading non-existent file
    let non_existent_path = "/path/that/does/not/exist/config.toml";
    let mut loader = ConfigLoader::new();
    let result = loader.load_config(non_existent_path);
    assert!(result.is_err());

    // Test invalid TOML syntax
    let temp_file = NamedTempFile::new().unwrap();
    fs::write(temp_file.path(), "invalid toml syntax [[[").unwrap();

    let result = loader.load_config(temp_file.path());
    assert!(result.is_err());

    // Test hot reload with non-existent file
    let config = NetworkConfig::devnet();
    let non_existent_path_buf = std::path::PathBuf::from(non_existent_path);
    let mut hot_reload = HotReloadManager::new(config, non_existent_path_buf, Network::Devnet);

    let result = hot_reload.reload_now().await;
    assert!(result.is_err());
}

/// Test concurrent hot reload operations
#[tokio::test]
async fn test_concurrent_hot_reload() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("concurrent_test.toml");

    // Create initial configuration
    let initial_config = r#"
[node]
id = "concurrent_node"
data_dir = "./data"
log_level = "info"

[network]
name = "devnet"
id = 1
port = 9944
bind_address = "0.0.0.0"
max_peers = 50
bootstrap_peers = []

[consensus]
enabled = true
cycle_time_ms = 30000
max_producers = 100

[storage]
path = "./storage"
cache_size_mb = 256

[rpc]
enabled = true
port = 9933
bind_address = "127.0.0.1"

[service_bus]
enabled = false
ws_port = 9222
bind_address = "127.0.0.1"
"#;
    fs::write(&config_path, initial_config).unwrap();

    // Load initial configuration
    let mut loader = ConfigLoader::new();
    let config = loader.load_config(&config_path).unwrap();

    // Create hot reload manager
    let mut hot_reload = HotReloadManager::new(config, config_path.clone(), Network::Devnet);

    // Start multiple concurrent operations
    let reload_result1 = hot_reload.reload_now();
    let update_result = hot_reload.update_config_value("node.id", "updated_concurrent_node");

    // Both operations should complete successfully
    let (reload_res, update_res) = tokio::join!(reload_result1, update_result);
    assert!(reload_res.is_ok());
    assert!(update_res.is_ok());

    // Verify the configuration was updated
    let final_config = hot_reload.get_current_config();
    assert_eq!(final_config.node.id, "updated_concurrent_node");
}

/// Test configuration with all features enabled
#[tokio::test]
async fn test_full_feature_integration() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("full_feature_test.toml");

    // Create comprehensive configuration
    let full_config = r#"
[node]
id = "full_feature_node"
data_dir = "./full_test_data"
log_level = "trace"

[network]
name = "full_testnet"
id = 42
port = 7777
bind_address = "192.168.1.100"
max_peers = 200
bootstrap_peers = ["peer1.example.com", "peer2.example.com", "peer3.example.com"]

[consensus]
enabled = true
cycle_time_ms = 45000
max_producers = 250

[storage]
path = "./full_test_storage"
cache_size_mb = 1024

[rpc]
enabled = true
port = 8888
bind_address = "0.0.0.0"

[service_bus]
enabled = true
ws_port = 6666
bind_address = "0.0.0.0"
"#;
    fs::write(&config_path, full_config).unwrap();

    // Set environment overrides
    env::set_var("CATALYST_NODE_ID", "env_full_feature_node");
    env::set_var("CATALYST_NETWORK_MAX_PEERS", "300");

    // Load with all features
    let mut loader = ConfigLoader::new();
    let mut config = loader.load_config(&config_path).unwrap();

    // Apply environment overrides
    let mut env_override = EnvOverride::new();
    env_override.load_env_vars().unwrap();
    env_override.apply_to_network_config(&mut config).unwrap();

    // Validate
    let validator = ConfigValidator::new();
    validator.validate(&config).unwrap();

    // Start hot reload
    let mut hot_reload =
        HotReloadManager::new(config.clone(), config_path.clone(), Network::Devnet);
    let mut event_receiver = hot_reload.start_auto_reload().await.unwrap();

    // Verify environment overrides were applied
    assert_eq!(
        hot_reload.get_current_config().node.id,
        "env_full_feature_node"
    );
    assert_eq!(hot_reload.get_current_config().network.max_peers, 300);

    // Test manual config update
    hot_reload
        .update_config_value("consensus.cycle_time_ms", "50000")
        .await
        .unwrap();
    assert_eq!(
        hot_reload.get_current_config().consensus.cycle_time_ms,
        50000
    );

    // Clean up
    hot_reload.stop_auto_reload().await.unwrap();
    env::remove_var("CATALYST_NODE_ID");
    env::remove_var("CATALYST_NETWORK_MAX_PEERS");
}
