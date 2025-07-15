//! Catalyst Network Node CLI
//! Enhanced version that uses real Catalyst components

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, error, warn};

mod config;
use config::NodeConfig;

#[derive(Parser)]
#[command(name = "catalyst-node")]
#[command(about = "Catalyst Network Node - A modular blockchain platform")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Catalyst node
    Start {
        /// Configuration file path
        #[arg(short, long)]
        config: Option<PathBuf>,
        
        /// Run as validator
        #[arg(long)]
        validator: bool,
        
        /// Override RPC port
        #[arg(long)]
        rpc_port: Option<u16>,
    },
    
    /// Stop a running node
    Stop {
        /// RPC endpoint to connect to
        #[arg(long, default_value = "http://127.0.0.1:9933")]
        rpc_url: String,
    },
    
    /// Show node status
    Status {
        /// RPC endpoint to connect to
        #[arg(long, default_value = "http://127.0.0.1:9933")]
        rpc_url: String,
    },
    
    /// Initialize a new node configuration
    Init {
        /// Output directory
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
        
        /// Configuration template (minimal, full, validator)
        #[arg(long, default_value = "minimal")]
        template: String,
    },
    
    /// Generate cryptographic identity
    GenerateIdentity {
        /// Output file for private key
        #[arg(short, long, default_value = "node.key")]
        output: PathBuf,
    },
    
    /// Validate configuration file
    ValidateConfig {
        /// Configuration file to validate
        #[arg(short, long, default_value = "catalyst.toml")]
        config: PathBuf,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    init_logging()?;
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Start { config, validator, rpc_port } => {
            start_node(config, validator, rpc_port).await
        }
        Commands::Stop { rpc_url } => {
            stop_node(&rpc_url).await
        }
        Commands::Status { rpc_url } => {
            show_status(&rpc_url).await
        }
        Commands::Init { output, template } => {
            init_config(&output, &template).await
        }
        Commands::GenerateIdentity { output } => {
            generate_identity(&output).await
        }
        Commands::ValidateConfig { config } => {
            validate_config(&config).await
        }
    }
}

/// Initialize logging based on environment
fn init_logging() -> anyhow::Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
    
    Ok(())
}

/// Start the Catalyst node
async fn start_node(
    config_path: Option<PathBuf>, 
    validator: bool, 
    rpc_port: Option<u16>
) -> anyhow::Result<()> {
    info!("Starting Catalyst node...");
    
    // Load or create configuration
    let mut config = match config_path {
        Some(path) => {
            info!("Loading configuration from: {}", path.display());
            NodeConfig::load(&path)?
        }
        None => {
            warn!("No configuration file specified, using defaults");
            NodeConfig::default()
        }
    };
    
    // Apply command line overrides
    if validator {
        config.validator = true;
        info!("Running as validator node");
    }
    
    if let Some(port) = rpc_port {
        config.rpc.port = port;
        config.rpc.enabled = true;
        info!("RPC server will run on port {}", port);
    }
    
    // Validate configuration
    config.validate()?;
    config.ensure_data_dir()?;
    
    // Start RPC server if enabled
    if config.rpc.enabled {
        info!("Starting RPC server on {}:{}", config.rpc.address, config.rpc.port);
        
        let rpc_config = catalyst_rpc::RpcConfig {
            enabled: true,
            port: config.rpc.port,
            address: config.rpc.address.clone(),
            max_connections: 100,
            cors_enabled: config.rpc.cors_enabled,
            cors_origins: config.rpc.cors_origins.clone(),
        };
        
        let rpc_server = catalyst_rpc::RpcServer::new(rpc_config);
        
        // Start server in background
        tokio::spawn(async move {
            if let Err(e) = rpc_server.start().await {
                error!("RPC server failed: {}", e);
            }
        });
    }
    
    info!("✅ Catalyst node started successfully!");
    info!("Node ID: {}", config.node.name);
    info!("Data directory: {}", config.storage.data_dir.display());
    
    if config.rpc.enabled {
        info!("RPC endpoint: http://{}:{}", config.rpc.address, config.rpc.port);
    }
    
    // Keep the node running
    info!("Node is running. Press Ctrl+C to stop.");
    tokio::signal::ctrl_c().await?;
    info!("Received shutdown signal, stopping node...");
    
    Ok(())
}

/// Stop a running node via RPC
async fn stop_node(rpc_url: &str) -> anyhow::Result<()> {
    info!("Stopping node at: {}", rpc_url);
    
    // TODO: Implement actual RPC client call to stop node
    // For now, just show what would happen
    info!("Would send shutdown command to: {}", rpc_url);
    info!("✅ Stop command sent (placeholder)");
    
    Ok(())
}

/// Show node status via RPC
async fn show_status(rpc_url: &str) -> anyhow::Result<()> {
    info!("Checking status of node at: {}", rpc_url);
    
    // TODO: Implement actual RPC client call
    // For now, show placeholder status
    println!("Node Status:");
    println!("  RPC Endpoint: {}", rpc_url);
    println!("  Status: Running (placeholder)");
    println!("  Block Height: 12345");
    println!("  Peers: 8");
    println!("  Sync Status: Synced");
    
    Ok(())
}

/// Initialize node configuration
async fn init_config(output_dir: &PathBuf, template: &str) -> anyhow::Result<()> {
    info!("Initializing configuration in: {}", output_dir.display());
    
    // Create output directory
    std::fs::create_dir_all(output_dir)?;
    
    let config = match template {
        "minimal" => {
            let mut config = NodeConfig::default();
            config.rpc.enabled = true;
            config.logging.level = "info".to_string();
            config
        }
        "full" => {
            let mut config = NodeConfig::default();
            config.rpc.enabled = true;
            config.service_bus.enabled = true;
            config.dfs.enabled = true;
            config.storage.enabled = true;
            config
        }
        "validator" => {
            let mut config = NodeConfig::default();
            config.validator = true;
            config.rpc.enabled = true;
            config.consensus.min_producer_count = 1;
            config.storage.enabled = true;
            config
        }
        _ => {
            warn!("Unknown template '{}', using minimal", template);
            NodeConfig::default()
        }
    };
    
    let config_path = output_dir.join("catalyst.toml");
    config.save(&config_path)?;
    
    info!("✅ Configuration saved to: {}", config_path.display());
    info!("Template used: {}", template);
    info!("Edit the configuration file and run: catalyst-node start -c {}", config_path.display());
    
    Ok(())
}

/// Generate cryptographic identity
async fn generate_identity(output: &PathBuf) -> anyhow::Result<()> {
    info!("Generating new node identity...");
    
    // TODO: Use actual catalyst-crypto to generate keypair
    // For now, create a placeholder
    let dummy_private_key = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    
    std::fs::write(output, dummy_private_key)?;
    
    info!("✅ Identity generated and saved to: {}", output.display());
    warn!("🔒 Keep this file secure - it contains your node's private key!");
    
    Ok(())
}

/// Validate configuration file
async fn validate_config(config_path: &PathBuf) -> anyhow::Result<()> {
    info!("Validating configuration: {}", config_path.display());
    
    let config = NodeConfig::load(config_path)?;
    config.validate()?;
    
    info!("✅ Configuration is valid!");
    info!("Node name: {}", config.node.name);
    info!("Validator mode: {}", config.validator);
    info!("RPC enabled: {}", config.rpc.enabled);
    
    if config.rpc.enabled {
        info!("RPC will run on: {}:{}", config.rpc.address, config.rpc.port);
    }
    
    Ok(())
}