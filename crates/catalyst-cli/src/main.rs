// Replace crates/catalyst-cli/src/main.rs with this simplified version

use anyhow::Result;
use catalyst_config::CatalystConfig;
use catalyst_node::{NodeBuilder, CatalystNode};
use catalyst_rpc::RpcConfig;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, warn, error};

#[derive(Parser)]
#[command(name = "catalyst-node")]
#[command(about = "Catalyst blockchain node")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    
    /// Configuration file path
    #[arg(short, long)]
    config: Option<PathBuf>,
    
    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Catalyst node
    Start {
        /// Run as validator
        #[arg(long)]
        validator: bool,
        
        /// RPC port
        #[arg(long)]
        rpc_port: Option<u16>,
        
        /// Data directory
        #[arg(long)]
        data_dir: Option<PathBuf>,
        
        /// Network port
        #[arg(long)]
        network_port: Option<u16>,
    },
    
    /// Stop the Catalyst node
    Stop,
    
    /// Get node status
    Status {
        /// Show verbose status
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// Initialize configuration
    Init {
        /// Output configuration file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    // Initialize logging
    init_logging(&cli.log_level)?;
    
    match cli.command {
        Commands::Start { validator, rpc_port, data_dir, network_port } => {
            start_node(cli.config, validator, rpc_port, data_dir, network_port).await
        }
        Commands::Stop => stop_node().await,
        Commands::Status { verbose } => show_status(verbose).await,
        Commands::Init { output } => init_config(output).await,
    }
}

async fn start_node(
    config_path: Option<PathBuf>,  // Remove the underscore
    _validator: bool,
    rpc_port: Option<u16>,
    _data_dir: Option<PathBuf>,
    _network_port: Option<u16>,
) -> Result<()> {
    info!("🚀 Starting Catalyst node...");
    
    // Now this will work:
    let config = match config_path {
        Some(path) => {
            info!("Loading configuration from: {}", path.display());
            // For now, use default config until we implement load
            CatalystConfig::default()
        }
        None => {
            warn!("No configuration file specified, using defaults");
            CatalystConfig::default()
        }
    };
    
    // Create RPC configuration
    let rpc_config = RpcConfig {
        enabled: true,
        port: rpc_port.unwrap_or(9933),
        address: "127.0.0.1".to_string(),
        max_connections: 100,
        cors_enabled: true,
        cors_origins: vec!["*".to_string()],
    };
    
    // Build the node using the new architecture
    let mut node = NodeBuilder::new()
        .with_config(config)
        .with_rpc(rpc_config)
        .build()
        .await?;
    
    // Start the node
    node.start().await?;
    
    info!("✅ Catalyst node started successfully!");
    info!("Node ID: catalyst-node");
    
    if let Some(port) = rpc_port {
        info!("RPC endpoint: http://127.0.0.1:{}", port);
        info!("Test with: curl -X POST http://127.0.0.1:{} -H 'Content-Type: application/json' -d '{{\"jsonrpc\":\"2.0\",\"method\":\"catalyst_version\",\"params\":[],\"id\":1}}'", port);
    }
    
    // Keep the node running
    info!("Node is running. Press Ctrl+C to stop.");
    
    // Set up graceful shutdown
    let shutdown_signal = async {
        tokio::signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
    };
    
    // Wait for shutdown signal
    tokio::select! {
        _ = shutdown_signal => {
            info!("Received shutdown signal, stopping node...");
        }
        _ = node.wait_for_shutdown() => {
            info!("Node stopped");
        }
    }
    
    // Clean shutdown
    node.stop().await?;
    
    info!("✅ Node stopped successfully");
    Ok(())
}

async fn stop_node() -> Result<()> {
    info!("Stopping Catalyst node...");
    warn!("Node stop command not yet implemented");
    warn!("Use Ctrl+C in the node terminal to stop");
    Ok(())
}

async fn show_status(verbose: bool) -> Result<()> {
    info!("Checking Catalyst node status...");
    
    let client = reqwest::Client::new();
    let response = client
        .post("http://127.0.0.1:9933")
        .header("Content-Type", "application/json")
        .body(r#"{"jsonrpc":"2.0","method":"catalyst_status","params":[],"id":1}"#)
        .send()
        .await;
    
    match response {
        Ok(resp) => {
            if resp.status().is_success() {
                let status: serde_json::Value = resp.json().await?;
                
                println!("✅ Catalyst node is running");
                
                if verbose {
                    println!("Status details:");
                    println!("{}", serde_json::to_string_pretty(&status)?);
                } else {
                    if let Some(result) = status.get("result") {
                        if let Some(block_height) = result.get("block_height") {
                            println!("Block height: {}", block_height);
                        }
                        if let Some(peer_count) = result.get("peer_count") {
                            println!("Peers: {}", peer_count);
                        }
                        if let Some(sync_status) = result.get("sync_status") {
                            println!("Sync status: {}", sync_status.as_str().unwrap_or("unknown"));
                        }
                    }
                }
            } else {
                error!("❌ Node responded with error: {}", resp.status());
            }
        }
        Err(_) => {
            error!("❌ Cannot connect to Catalyst node");
            error!("Make sure the node is running with RPC enabled");
        }
    }
    
    Ok(())
}

async fn init_config(output: Option<PathBuf>) -> Result<()> {
    let output_path = output.unwrap_or_else(|| PathBuf::from("catalyst.toml"));
    
    info!("Generating default configuration...");
    
    // Simple config file for now
    let config_content = r#"# Catalyst Node Configuration
# This is a placeholder configuration file

[network]
listen_port = 30333

[rpc]
enabled = true
port = 9933
address = "127.0.0.1"

[storage]
data_dir = "data"

[consensus]
algorithm = "catalyst-pos"
"#;
    
    std::fs::write(&output_path, config_content)?;
    
    info!("✅ Configuration written to: {}", output_path.display());
    info!("Edit the configuration file and start the node with:");
    info!("catalyst-node start --config {}", output_path.display());
    
    Ok(())
}

fn init_logging(level: &str) -> Result<()> {
    use tracing_subscriber::{EnvFilter, FmtSubscriber};
    
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(level));
    
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)?;
    
    Ok(())
}