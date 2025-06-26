use catalyst_config::{CatalystConfig, NetworkType, ConfigLoader, ConfigUtils};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Catalyst Configuration Basic Usage Example");
    println!("==========================================\n");
    
    // Example 1: Create default configuration for different networks
    println!("1. Creating default configurations for different networks:");
    
    let devnet_config = CatalystConfig::new_for_network(NetworkType::Devnet);
    println!("   Devnet - P2P Port: {}, Consensus Cycle: {}ms", 
             devnet_config.network.p2p_port, 
             devnet_config.consensus.cycle_duration_ms);
    
    let testnet_config = CatalystConfig::new_for_network(NetworkType::Testnet);
    println!("   Testnet - P2P Port: {}, Consensus Cycle: {}ms", 
             testnet_config.network.p2p_port, 
             testnet_config.consensus.cycle_duration_ms);
    
    let mainnet_config = CatalystConfig::new_for_network(NetworkType::Mainnet);
    println!("   Mainnet - P2P Port: {}, Consensus Cycle: {}ms\n", 
             mainnet_config.network.p2p_port, 
             mainnet_config.consensus.cycle_duration_ms);
    
    // Example 2: Load configuration from file
    println!("2. Loading configuration from file:");
    
    // Create example config directory
    let config_dir = "./examples_output";
    ConfigUtils::ensure_config_directory(config_dir)?;
    
    // Save a sample configuration
    let sample_config_path = format!("{}/sample_devnet.toml", config_dir);
    catalyst_config::loader::FileLoader::save_toml(&devnet_config, &sample_config_path).await?;
    println!("   Saved sample configuration to: {}", sample_config_path);
    
    // Load it back
    let loaded_config = catalyst_config::loader::FileLoader::load_toml(&sample_config_path).await?;
    println!("   Loaded configuration - Network: {}", loaded_config.network.name);
    
    // Example 3: Load configuration with environment overrides
    println!("\n3. Loading configuration with environment variables:");
    
    // Set some environment variables (in practice, these would be set externally)
    std::env::set_var("CATALYST_NETWORK", "testnet");
    std::env::set_var("CATALYST_NETWORK_P2P_PORT", "7001");
    std::env::set_var("CATALYST_CONSENSUS_PRODUCER_COUNT", "25");
    
    let env_config = catalyst_config::loader::EnvLoader::load_from_env()?;
    println!("   Network from env: {}", env_config.network.name);
    println!("   P2P port from env: {}", env_config.network.p2p_port);
    println!("   Producer count from env: {}", env_config.consensus.producer_count);
    
    // Example 4: Configuration validation
    println!("\n4. Configuration validation:");
    
    match devnet_config.validate() {
        Ok(()) => println!("   Devnet configuration is valid ✓"),
        Err(e) => println!("   Devnet configuration error: {}", e),
    }
    
    // Example 5: Create data directories
    println!("\n5. Creating data directories:");
    
    match ConfigUtils::create_data_directories(&devnet_config) {
        Ok(()) => println!("   Data directories created successfully ✓"),
        Err(e) => println!("   Error creating directories: {}", e),
    }
    
    // Example 6: Generate configuration report
    println!("\n6. Configuration report:");
    let report = catalyst_config::loader::validation::ConfigValidator::generate_report(&devnet_config);
    println!("{}", report);
    
    // Example 7: Configuration templates
    println!("7. Configuration templates:");
    
    let template = ConfigUtils::generate_template("devnet")?;
    let template_path = format!("{}/devnet_template.toml", config_dir);
    tokio::fs::write(&template_path, template).await?;
    println!("   Generated devnet template: {}", template_path);
    
    // Example 8: Configuration format conversion
    println!("\n8. Configuration format conversion:");
    
    let json_path = format!("{}/sample_devnet.json", config_dir);
    catalyst_config::loader::FileLoader::save_json(&devnet_config, &json_path).await?;
    println!("   Converted TOML config to JSON: {}", json_path);
    
    // Clean up environment variables
    std::env::remove_var("CATALYST_NETWORK");
    std::env::remove_var("CATALYST_NETWORK_P2P_PORT");
    std::env::remove_var("CATALYST_CONSENSUS_PRODUCER_COUNT");
    
    println!("\nBasic usage examples completed successfully!");
    
    Ok(())
}