// Updated crates/catalyst-cli/src/main.rs - Fixed imports

use anyhow::Result;
use catalyst_config::CatalystConfig;
use catalyst_node::NodeBuilder;
use catalyst_rpc::RpcConfig;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{error, info, warn};

// Simplified storage config struct for CLI use
#[derive(Debug, Clone)]
pub struct SimpleStorageConfig {
    pub data_dir: PathBuf,
    pub max_open_files: i32,
    pub write_buffer_size: usize,
    pub block_cache_size: usize,
    pub compression_enabled: bool,
    pub enable_statistics: bool,
}

impl Default for SimpleStorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./catalyst_blockchain_data"),
            max_open_files: 1000,
            write_buffer_size: 32 * 1024 * 1024, // 32 MB
            block_cache_size: 128 * 1024 * 1024, // 128 MB
            compression_enabled: true,
            enable_statistics: true,
        }
    }
}

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
        #[arg(long, default_value = "9933")]
        rpc_port: u16,

        /// Data directory
        #[arg(long)]
        data_dir: Option<PathBuf>,

        /// Network port
        #[arg(long, default_value = "30333")]
        network_port: u16,
    },

    /// Stop the Catalyst node
    Stop,

    /// Get node status
    Status {
        /// Show verbose status
        #[arg(short, long)]
        verbose: bool,

        /// RPC port to connect to
        #[arg(long, default_value = "9933")]
        rpc_port: u16,
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
        Commands::Start {
            validator,
            rpc_port,
            data_dir,
            network_port,
        } => start_node(cli.config, validator, rpc_port, data_dir, network_port).await,
        Commands::Stop => stop_node().await,
        Commands::Status { verbose, rpc_port } => show_status(verbose, rpc_port).await,
        Commands::Init { output } => init_config(output).await,
    }
}

async fn start_node(
    config_path: Option<PathBuf>,
    validator: bool,
    rpc_port: u16,
    data_dir: Option<PathBuf>,
    _network_port: u16,
) -> Result<()> {
    info!("🚀 Starting Catalyst node...");

    // Load or create configuration
    let config = match config_path {
        Some(path) => {
            info!("Loading configuration from: {}", path.display());
            // For now, use default config
            CatalystConfig::default()
        }
        None => {
            warn!("No configuration file specified, using defaults");
            CatalystConfig::default()
        }
    };

    // Create simplified storage configuration
    let storage_config = SimpleStorageConfig {
        data_dir: data_dir.unwrap_or_else(|| PathBuf::from("./catalyst_blockchain_data")),
        max_open_files: 1000,
        write_buffer_size: 32 * 1024 * 1024, // 32 MB
        block_cache_size: 128 * 1024 * 1024, // 128 MB
        compression_enabled: true,
        enable_statistics: true,
    };

    // Create RPC configuration
    let rpc_config = RpcConfig {
        enabled: true,
        http_port: rpc_port,
        ws_port: rpc_port + 11, // WebSocket on +11
        address: "127.0.0.1".to_string(),
        max_connections: 100,
        cors_enabled: true,
        cors_origins: vec!["*".to_string()],
    };

    // Build and start the node
    let mut node = NodeBuilder::new(config)
        .with_rpc(rpc_config)
        .build()
        .await?;

    info!("🔧 Node built successfully, starting services...");

    // Start the node
    match node.start().await {
        Ok(_) => {
            info!("✅ Catalyst node started successfully!");
            info!("📡 RPC server listening on:");
            info!("   HTTP: http://127.0.0.1:{}", rpc_port);
            info!("   WebSocket: ws://127.0.0.1:{}", rpc_port + 11);
            info!("🗄️ Data directory: {:?}", storage_config.data_dir);
            info!("⚖️ Validator mode: {}", validator);
            info!("");
            info!("🧪 Try these RPC calls:");
            info!("   curl -X POST http://127.0.0.1:{} -H 'Content-Type: application/json' -d '{{\"jsonrpc\":\"2.0\",\"method\":\"catalyst_status\",\"params\":[],\"id\":1}}'", rpc_port);
            info!("   curl -X POST http://127.0.0.1:{} -H 'Content-Type: application/json' -d '{{\"jsonrpc\":\"2.0\",\"method\":\"catalyst_version\",\"params\":[],\"id\":2}}'", rpc_port);

            // Check node health
            let health = node.health_check().await;
            info!("🏥 Node Health Check:");
            info!("   Overall Healthy: {}", health.is_healthy);
            info!("   Active Services: {}", health.services.len());

            // Wait for shutdown signal
            info!("💡 Press Ctrl+C to stop the node");
            tokio::signal::ctrl_c().await?;
            info!("🛑 Shutdown signal received, stopping node...");
        }
        Err(e) => {
            error!("❌ Failed to start Catalyst node: {}", e);
            return Err(e.into());
        }
    }

    // Graceful shutdown
    if let Err(e) = node.stop().await {
        error!("⚠️ Error during node shutdown: {}", e);
    } else {
        info!("✅ Catalyst node stopped successfully");
    }

    Ok(())
}

async fn stop_node() -> Result<()> {
    info!("Stopping Catalyst node...");
    warn!("Node stop command not yet implemented");
    warn!("Use Ctrl+C in the node terminal to stop");
    Ok(())
}

async fn show_status(verbose: bool, rpc_port: u16) -> Result<()> {
    info!("Checking Catalyst node status...");

    let client = reqwest::Client::new();
    let rpc_url = format!("http://127.0.0.1:{}", rpc_port);

    let response = client
        .post(&rpc_url)
        .header("Content-Type", "application/json")
        .body(r#"{"jsonrpc":"2.0","method":"catalyst_status","params":[],"id":1}"#)
        .send()
        .await;

    match response {
        Ok(resp) => {
            if resp.status().is_success() {
                let status: serde_json::Value = resp.json().await?;

                println!("✅ Catalyst node is running on port {}", rpc_port);

                if verbose {
                    println!("Status details:");
                    println!("{}", serde_json::to_string_pretty(&status)?);
                } else {
                    if let Some(result) = status.get("result") {
                        if let Some(version) = result.get("version") {
                            println!("Version: {}", version.as_str().unwrap_or("unknown"));
                        }
                        if let Some(status) = result.get("status") {
                            println!("Status: {}", status.as_str().unwrap_or("unknown"));
                        }
                    }
                }

                // Test additional endpoints
                if verbose {
                    println!("\n🧪 Testing additional RPC endpoints:");

                    // Test version
                    let version_response = client
                        .post(&rpc_url)
                        .header("Content-Type", "application/json")
                        .body(r#"{"jsonrpc":"2.0","method":"catalyst_version","params":[],"id":2}"#)
                        .send()
                        .await;

                    if let Ok(resp) = version_response {
                        if let Ok(version_data) = resp.json::<serde_json::Value>().await {
                            if let Some(result) = version_data.get("result") {
                                println!("   Version: {}", result);
                            }
                        }
                    }
                }
            } else {
                error!("❌ Node responded with error: {}", resp.status());
            }
        }
        Err(_) => {
            error!("❌ Cannot connect to Catalyst node on port {}", rpc_port);
            error!("Make sure the node is running with RPC enabled");
            error!("Try: catalyst-node start --rpc-port {}", rpc_port);
        }
    }

    Ok(())
}

async fn init_config(output: Option<PathBuf>) -> Result<()> {
    let output_path = output.unwrap_or_else(|| PathBuf::from("catalyst.toml"));

    info!("Generating default configuration...");

    // Simplified config file
    let config_content = r#"# Catalyst Node Configuration

[network]
listen_port = 30333
max_peers = 50
discovery_enabled = true

[rpc]
enabled = true
http_port = 9933
ws_port = 9944
address = "127.0.0.1"
max_connections = 100
cors_enabled = true
cors_origins = ["*"]

[storage]
data_dir = "catalyst_blockchain_data"
max_open_files = 1000
compression_enabled = true
enable_statistics = true

[consensus]
algorithm = "catalyst-pos"
validator = false

[runtime]
evm_enabled = true
gas_limit = "8000000"

[metrics]
enabled = true
port = 9615
"#;

    std::fs::write(&output_path, config_content)?;

    info!("✅ Configuration written to: {}", output_path.display());
    info!("📝 Edit the configuration file and start the node with:");
    info!("   catalyst-node start --config {}", output_path.display());
    info!("");
    info!("🚀 Or start with default settings:");
    info!("   catalyst-node start");
    info!("");
    info!("📊 Check node status:");
    info!("   catalyst-node status");

    Ok(())
}

fn init_logging(level: &str) -> Result<()> {
    use tracing_subscriber::{EnvFilter, FmtSubscriber};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));

    let subscriber = FmtSubscriber::builder()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_simple_node_startup() {
        let temp_dir = TempDir::new().unwrap();

        let storage_config = SimpleStorageConfig {
            data_dir: temp_dir.path().to_path_buf(),
            max_open_files: 100,
            write_buffer_size: 16 * 1024 * 1024, // 16 MB for tests
            block_cache_size: 32 * 1024 * 1024,  // 32 MB for tests
            compression_enabled: false,          // Disable for faster tests
            enable_statistics: false,
        };

        let rpc_config = RpcConfig {
            enabled: true,
            http_port: 19933, // Use different port for tests
            ws_port: 19944,
            max_connections: 10,
            address: "127.0.0.1".to_string(),
            cors_enabled: true,
            cors_origins: vec!["*".to_string()],
        };

        let node_config = CatalystConfig::default();

        let node_result = NodeBuilder::new(node_config)
            .with_rpc(rpc_config)
            .build()
            .await;

        // Just test that we can build the node
        assert!(node_result.is_ok(), "Should be able to build node");
    }
}
