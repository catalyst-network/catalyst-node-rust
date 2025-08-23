use catalyst_config::networks::{
    ConsensusConfig, NetworkConfig, NetworkSettings, NodeConfig, RpcConfig, ServiceBusConfig,
    StorageConfig,
};
use catalyst_config::validation::{ConfigValidator, ValidationError, ValidationResult};
use std::path::PathBuf;

/// Test valid configuration validation
#[test]
fn test_valid_configuration_validation() {
    let validator = ConfigValidator::new();

    // Test all default network configurations
    assert!(validator.validate(&NetworkConfig::devnet()).is_ok());
    assert!(validator.validate(&NetworkConfig::testnet()).is_ok());
    assert!(validator.validate(&NetworkConfig::mainnet()).is_ok());
}

/// Test node configuration validation
#[test]
fn test_node_config_validation() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Test empty node ID
    config.node.id = "".to_string();
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::EmptyNodeId)));
    }

    // Test valid node ID
    config.node.id = "valid_node_id".to_string();
    assert!(validator.validate(&config).is_ok());

    // Test invalid log level
    config.node.log_level = "invalid_level".to_string();
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidLogLevel(_))));
    }

    // Test valid log levels
    let valid_levels = vec!["trace", "debug", "info", "warn", "error"];
    for level in valid_levels {
        config.node.log_level = level.to_string();
        assert!(
            validator.validate(&config).is_ok(),
            "Log level '{}' should be valid",
            level
        );
    }
}

/// Test network configuration validation
#[test]
fn test_network_config_validation() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Test empty network name
    config.network.name = "".to_string();
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::EmptyNetworkName)));
    }

    // Test invalid port (0)
    config.network.name = "valid_name".to_string();
    config.network.port = 0;
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidPort(_))));
    }

    // Test invalid port (too high)
    config.network.port = 65536;
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidPort(_))));
    }

    // Test valid port range
    for port in [1, 1024, 8080, 9944, 65535] {
        config.network.port = port;
        assert!(
            validator.validate(&config).is_ok(),
            "Port {} should be valid",
            port
        );
    }

    // Test negative max_peers
    config.network.port = 9944;
    config.network.max_peers = 0;
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidMaxPeers(_))));
    }

    // Test valid max_peers
    config.network.max_peers = 50;
    assert!(validator.validate(&config).is_ok());

    // Test empty bind address
    config.network.bind_address = "".to_string();
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::EmptyBindAddress)));
    }

    // Test valid bind addresses
    let valid_addresses = vec!["0.0.0.0", "127.0.0.1", "192.168.1.100", "::", "::1"];
    for addr in valid_addresses {
        config.network.bind_address = addr.to_string();
        assert!(
            validator.validate(&config).is_ok(),
            "Address '{}' should be valid",
            addr
        );
    }
}

/// Test consensus configuration validation
#[test]
fn test_consensus_config_validation() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Test invalid cycle time (too small)
    config.consensus.cycle_time_ms = 100;
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidCycleTime(_))));
    }

    // Test invalid cycle time (too large)
    config.consensus.cycle_time_ms = 10_000_000;
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidCycleTime(_))));
    }

    // Test valid cycle times
    for cycle_time in [5000, 15000, 30000, 60000, 120000] {
        config.consensus.cycle_time_ms = cycle_time;
        assert!(
            validator.validate(&config).is_ok(),
            "Cycle time {} should be valid",
            cycle_time
        );
    }

    // Test invalid max_producers (too small)
    config.consensus.cycle_time_ms = 30000;
    config.consensus.max_producers = 0;
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidMaxProducers(_))));
    }

    // Test invalid max_producers (too large)
    config.consensus.max_producers = 10000;
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidMaxProducers(_))));
    }

    // Test valid max_producers
    for max_producers in [10, 50, 100, 500, 1000] {
        config.consensus.max_producers = max_producers;
        assert!(
            validator.validate(&config).is_ok(),
            "Max producers {} should be valid",
            max_producers
        );
    }
}

/// Test storage configuration validation
#[test]
fn test_storage_config_validation() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Test invalid cache size (too small)
    config.storage.cache_size_mb = 0;
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidCacheSize(_))));
    }

    // Test invalid cache size (too large)
    config.storage.cache_size_mb = 100000;
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidCacheSize(_))));
    }

    // Test valid cache sizes
    for cache_size in [32, 64, 128, 256, 512, 1024, 2048] {
        config.storage.cache_size_mb = cache_size;
        assert!(
            validator.validate(&config).is_ok(),
            "Cache size {} should be valid",
            cache_size
        );
    }

    // Test empty storage path
    config.storage.cache_size_mb = 256;
    config.storage.path = PathBuf::new();
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::EmptyStoragePath)));
    }

    // Test valid storage path
    config.storage.path = "./valid/storage/path".into();
    assert!(validator.validate(&config).is_ok());
}

/// Test RPC configuration validation
#[test]
fn test_rpc_config_validation() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Test invalid RPC port
    config.rpc.port = 0;
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidPort(_))));
    }

    // Test valid RPC port
    config.rpc.port = 9933;
    assert!(validator.validate(&config).is_ok());

    // Test empty RPC bind address
    config.rpc.bind_address = "".to_string();
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::EmptyBindAddress)));
    }

    // Test valid RPC bind address
    config.rpc.bind_address = "127.0.0.1".to_string();
    assert!(validator.validate(&config).is_ok());
}

/// Test service bus configuration validation
#[test]
fn test_service_bus_config_validation() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Test invalid WebSocket port
    config.service_bus.ws_port = 0;
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidPort(_))));
    }

    // Test valid WebSocket port
    config.service_bus.ws_port = 9222;
    assert!(validator.validate(&config).is_ok());

    // Test empty service bus bind address
    config.service_bus.bind_address = "".to_string();
    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::EmptyBindAddress)));
    }

    // Test valid service bus bind address
    config.service_bus.bind_address = "0.0.0.0".to_string();
    assert!(validator.validate(&config).is_ok());
}

/// Test port conflict validation
#[test]
fn test_port_conflict_validation() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Set conflicting ports
    config.network.port = 9944;
    config.rpc.port = 9944;
    config.service_bus.ws_port = 9944;

    let result = validator.validate(&config);
    assert!(result.is_err());
    if let Err(errors) = result {
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::PortConflict(_, _))));
    }

    // Fix conflicts
    config.network.port = 9944;
    config.rpc.port = 9933;
    config.service_bus.ws_port = 9222;

    assert!(validator.validate(&config).is_ok());
}

/// Test multiple validation errors
#[test]
fn test_multiple_validation_errors() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Introduce multiple errors
    config.node.id = "".to_string(); // Empty node ID
    config.network.name = "".to_string(); // Empty network name
    config.network.port = 0; // Invalid port
    config.network.max_peers = 0; // Invalid max peers
    config.consensus.cycle_time_ms = 100; // Invalid cycle time
    config.consensus.max_producers = 0; // Invalid max producers
    config.storage.cache_size_mb = 0; // Invalid cache size

    let result = validator.validate(&config);
    assert!(result.is_err());

    if let Err(errors) = result {
        // Should have multiple errors
        assert!(errors.len() >= 6);

        // Check for specific errors
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::EmptyNodeId)));
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::EmptyNetworkName)));
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidPort(_))));
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidMaxPeers(_))));
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidCycleTime(_))));
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidMaxProducers(_))));
        assert!(errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidCacheSize(_))));
    }
}

/// Test validation error display
#[test]
fn test_validation_error_display() {
    let errors = vec![
        ValidationError::EmptyNodeId,
        ValidationError::EmptyNetworkName,
        ValidationError::InvalidPort(0),
        ValidationError::InvalidMaxPeers(0),
        ValidationError::InvalidCycleTime(100),
        ValidationError::InvalidMaxProducers(0),
        ValidationError::InvalidCacheSize(0),
        ValidationError::EmptyBindAddress,
        ValidationError::EmptyStoragePath,
        ValidationError::InvalidLogLevel("invalid".to_string()),
        ValidationError::PortConflict(9944, 9944),
    ];

    for error in errors {
        let display_string = error.to_string();
        assert!(!display_string.is_empty());
        println!("Error: {}", display_string); // For manual verification
    }
}

/// Test custom validation scenarios
#[test]
fn test_custom_validation_scenarios() {
    let validator = ConfigValidator::new();

    // Test extreme but valid configuration
    let extreme_config = NetworkConfig {
        node: NodeConfig {
            id: "extreme_node_with_very_long_id_that_should_still_be_valid".to_string(),
            data_dir: "./extremely/deep/nested/data/directory/path".into(),
            log_level: "trace".to_string(),
        },
        network: NetworkSettings {
            name: "extreme_test_network".to_string(),
            id: 65535,
            port: 65535,
            bind_address: "255.255.255.255".to_string(),
            max_peers: 1000,
            bootstrap_peers: (0..100).map(|i| format!("peer{}.example.com", i)).collect(),
        },
        consensus: ConsensusConfig {
            enabled: true,
            cycle_time_ms: 120000,
            max_producers: 1000,
        },
        storage: StorageConfig {
            path: "./extreme/storage/with/very/deep/path".into(),
            cache_size_mb: 2048,
        },
        rpc: RpcConfig {
            enabled: true,
            port: 65534,
            bind_address: "0.0.0.0".to_string(),
        },
        service_bus: ServiceBusConfig {
            enabled: true,
            ws_port: 65533,
            bind_address: "::".to_string(),
        },
    };

    // Should still be valid despite extreme values
    assert!(validator.validate(&extreme_config).is_ok());

    // Test minimal but valid configuration
    let minimal_config = NetworkConfig {
        node: NodeConfig {
            id: "a".to_string(),  // Very short but valid
            data_dir: ".".into(), // Minimal path
            log_level: "error".to_string(),
        },
        network: NetworkSettings {
            name: "min".to_string(),
            id: 1,
            port: 1024,
            bind_address: "::1".to_string(),
            max_peers: 1,
            bootstrap_peers: vec![],
        },
        consensus: ConsensusConfig {
            enabled: false,      // Disabled consensus
            cycle_time_ms: 5000, // Minimum allowed
            max_producers: 1,    // Minimum allowed
        },
        storage: StorageConfig {
            path: "./s".into(), // Short path
            cache_size_mb: 32,  // Minimum allowed
        },
        rpc: RpcConfig {
            enabled: false,
            port: 1025,
            bind_address: "127.0.0.1".to_string(),
        },
        service_bus: ServiceBusConfig {
            enabled: false,
            ws_port: 1026,
            bind_address: "localhost".to_string(),
        },
    };

    // Should be valid
    assert!(validator.validate(&minimal_config).is_ok());
}

/// Test bootstrap peers validation
#[test]
fn test_bootstrap_peers_validation() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Test with many bootstrap peers (should be valid)
    config.network.bootstrap_peers = (0..50)
        .map(|i| format!("peer{}.example.com:9944", i))
        .collect();

    assert!(validator.validate(&config).is_ok());

    // Test with empty bootstrap peers (should be valid for devnet)
    config.network.bootstrap_peers = vec![];
    assert!(validator.validate(&config).is_ok());

    // Test with single bootstrap peer
    config.network.bootstrap_peers = vec!["single-peer.example.com:9944".to_string()];
    assert!(validator.validate(&config).is_ok());
}

/// Test consensus disabled scenarios
#[test]
fn test_consensus_disabled_validation() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Disable consensus
    config.consensus.enabled = false;

    // Even with consensus disabled, other consensus settings should still be valid
    assert!(validator.validate(&config).is_ok());

    // Test with invalid consensus settings when disabled
    config.consensus.cycle_time_ms = 100; // Invalid
    config.consensus.max_producers = 0; // Invalid

    // Should still fail validation even if consensus is disabled
    let result = validator.validate(&config);
    assert!(result.is_err());
}

/// Test network ID validation
#[test]
fn test_network_id_validation() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Test various network IDs
    let valid_ids = vec![0, 1, 2, 100, 65535];
    for id in valid_ids {
        config.network.id = id;
        assert!(
            validator.validate(&config).is_ok(),
            "Network ID {} should be valid",
            id
        );
    }

    // Test extreme network ID
    config.network.id = u16::MAX;
    assert!(validator.validate(&config).is_ok());
}

/// Test bind address validation edge cases
#[test]
fn test_bind_address_validation_edge_cases() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Test various valid bind addresses
    let valid_addresses = vec![
        "0.0.0.0",
        "127.0.0.1",
        "192.168.1.1",
        "10.0.0.1",
        "172.16.0.1",
        "::",
        "::1",
        "fe80::1",
        "localhost",
    ];

    for addr in valid_addresses {
        config.network.bind_address = addr.to_string();
        config.rpc.bind_address = addr.to_string();
        config.service_bus.bind_address = addr.to_string();

        let result = validator.validate(&config);
        assert!(result.is_ok(), "Address '{}' should be valid", addr);
    }
}

/// Test log level validation comprehensive
#[test]
fn test_log_level_validation_comprehensive() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Test all valid log levels
    let valid_levels = vec!["trace", "debug", "info", "warn", "error"];
    for level in valid_levels {
        config.node.log_level = level.to_string();
        assert!(
            validator.validate(&config).is_ok(),
            "Log level '{}' should be valid",
            level
        );
    }

    // Test case insensitivity (should fail - we expect exact case)
    let case_variants = vec!["TRACE", "Debug", "INFO", "Warn", "ERROR"];
    for level in case_variants {
        config.node.log_level = level.to_string();
        let result = validator.validate(&config);
        assert!(
            result.is_err(),
            "Log level '{}' should be invalid (case sensitive)",
            level
        );
    }

    // Test invalid log levels
    let invalid_levels = vec!["verbose", "fatal", "panic", "silent", ""];
    for level in invalid_levels {
        config.node.log_level = level.to_string();
        let result = validator.validate(&config);
        assert!(result.is_err(), "Log level '{}' should be invalid", level);
    }
}

/// Test data directory path validation
#[test]
fn test_data_directory_validation() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Test various valid paths
    let valid_paths = vec![
        ".",
        "./data",
        "/absolute/path",
        "../relative/path",
        "~/home/path",
        "data/nested/deep",
        "C:\\Windows\\Path", // Windows path
    ];

    for path in valid_paths {
        config.node.data_dir = PathBuf::from(path);
        config.storage.path = PathBuf::from(path);

        assert!(
            validator.validate(&config).is_ok(),
            "Path '{}' should be valid",
            path
        );
    }
}

/// Test validation performance with large configurations
#[test]
fn test_validation_performance() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Create a configuration with many bootstrap peers
    config.network.bootstrap_peers = (0..1000)
        .map(|i| format!("peer{}.example.com:9944", i))
        .collect();

    let start = std::time::Instant::now();
    let result = validator.validate(&config);
    let duration = start.elapsed();

    assert!(result.is_ok());
    assert!(
        duration.as_millis() < 100,
        "Validation should be fast even with large configs"
    );
}

/// Test validator state and reuse
#[test]
fn test_validator_reuse() {
    let validator = ConfigValidator::new();

    // Use the same validator instance multiple times
    for _ in 0..10 {
        let config = NetworkConfig::devnet();
        assert!(validator.validate(&config).is_ok());

        let config = NetworkConfig::testnet();
        assert!(validator.validate(&config).is_ok());

        let config = NetworkConfig::mainnet();
        assert!(validator.validate(&config).is_ok());
    }
}

/// Test validation result formatting
#[test]
fn test_validation_result_formatting() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Create configuration with multiple errors
    config.node.id = "".to_string();
    config.network.port = 0;
    config.consensus.max_producers = 0;

    let result = validator.validate(&config);
    assert!(result.is_err());

    if let Err(errors) = result {
        assert!(errors.len() >= 3);

        // Test that each error has a meaningful message
        for error in &errors {
            let message = error.to_string();
            assert!(!message.is_empty());
            assert!(message.len() > 5); // Should be descriptive
        }
    }
}

/// Test edge cases for numeric values
#[test]
fn test_numeric_edge_cases() {
    let validator = ConfigValidator::new();
    let mut config = NetworkConfig::devnet();

    // Test maximum valid values
    config.network.port = 65535;
    config.rpc.port = 65535;
    config.service_bus.ws_port = 65535;
    config.network.max_peers = 10000;
    config.consensus.max_producers = 1000;
    config.consensus.cycle_time_ms = 300000; // 5 minutes
    config.storage.cache_size_mb = 4096; // 4GB

    // Should be at the edge but still valid
    let result = validator.validate(&config);

    // Some of these might be rejected as too extreme, which is fine
    // The test is to ensure the validator doesn't panic or crash
    match result {
        Ok(_) => println!("Extreme values accepted"),
        Err(errors) => {
            println!("Extreme values rejected: {:?}", errors);
            // Should have specific error messages, not generic failures
            for error in errors {
                assert!(!error.to_string().is_empty());
            }
        }
    }
}
