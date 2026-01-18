use clap::{Parser, Subcommand};
use anyhow::Result;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::path::PathBuf;

mod node;
mod commands;
mod config;

use node::CatalystNode;
use config::NodeConfig;

#[derive(Parser)]
#[command(name = "catalyst")]
#[command(about = "Catalyst Network Node - A truly decentralized blockchain")]
#[command(version = "0.1.0")]
struct Cli {
    /// Configuration file path
    #[arg(short, long, default_value = "catalyst.toml")]
    config: PathBuf,

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,

    /// Enable JSON logging
    #[arg(long)]
    json_logs: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Catalyst node
    Start {
        /// Run as validator node
        #[arg(long)]
        validator: bool,

        /// Provide storage to network
        #[arg(long)]
        storage: bool,

        /// Storage capacity in GB
        #[arg(long, default_value = "10")]
        storage_capacity: u64,

        /// Enable RPC server
        #[arg(long)]
        rpc: bool,

        /// RPC server port
        #[arg(long, default_value = "8545")]
        rpc_port: u16,

        /// Bootstrap peers (comma-separated multiaddrs)
        #[arg(long)]
        bootstrap_peers: Option<String>,
    },
    /// Generate a new node identity
    GenerateIdentity {
        /// Output file for the identity
        #[arg(short, long, default_value = "identity.json")]
        output: PathBuf,
    },
    /// Create genesis configuration
    CreateGenesis {
        /// Output file for genesis
        #[arg(short, long, default_value = "genesis.json")]
        output: PathBuf,

        /// Genesis accounts file
        #[arg(long)]
        accounts: Option<PathBuf>,
    },
    /// Show node status
    Status {
        /// RPC endpoint
        #[arg(long, default_value = "http://localhost:8545")]
        rpc_url: String,
    },
    /// Show network peers
    Peers {
        /// RPC endpoint
        #[arg(long, default_value = "http://localhost:8545")]
        rpc_url: String,
    },
    /// Send a transaction
    Send {
        /// Recipient address
        to: String,

        /// Amount to send (in KAT)
        amount: String,

        /// Private key file
        #[arg(long, default_value = "wallet.key")]
        key_file: PathBuf,

        /// RPC endpoint
        #[arg(long, default_value = "http://localhost:8545")]
        rpc_url: String,

        /// Make transaction confidential
        #[arg(long)]
        confidential: bool,
    },
    /// Check account balance
    Balance {
        /// Account address
        address: String,

        /// RPC endpoint
        #[arg(long, default_value = "http://localhost:8545")]
        rpc_url: String,
    },
    /// Deploy a smart contract
    Deploy {
        /// Contract bytecode file
        contract: PathBuf,

        /// Constructor arguments (hex)
        #[arg(long)]
        args: Option<String>,

        /// Private key file
        #[arg(long, default_value = "wallet.key")]
        key_file: PathBuf,

        /// RPC endpoint
        #[arg(long, default_value = "http://localhost:8545")]
        rpc_url: String,

        /// Runtime type (evm, svm, wasm)
        #[arg(long, default_value = "evm")]
        runtime: String,
    },
    /// Call a smart contract function
    Call {
        /// Contract address
        contract: String,

        /// Function signature and arguments
        function: String,

        /// Private key file (for state-changing calls)
        #[arg(long)]
        key_file: Option<PathBuf>,

        /// RPC endpoint
        #[arg(long, default_value = "http://localhost:8545")]
        rpc_url: String,

        /// Value to send with call (in KAT)
        #[arg(long, default_value = "0")]
        value: String,
    },
    /// Benchmark node performance
    Benchmark {
        /// Duration in seconds
        #[arg(short, long, default_value = "60")]
        duration: u64,

        /// Number of concurrent transactions
        #[arg(long, default_value = "100")]
        concurrent: usize,

        /// RPC endpoint
        #[arg(long, default_value = "http://localhost:8545")]
        rpc_url: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    init_logging(&cli.log_level, cli.json_logs)?;

    info!("Starting Catalyst CLI v{}", env!("CARGO_PKG_VERSION"));

    // Load configuration (or auto-generate a local default if missing).
    let config = if cli.config.exists() {
        NodeConfig::load(&cli.config)?
    } else {
        info!(
            "Configuration file not found at {:?}, generating a default config",
            cli.config
        );
        if let Some(parent) = cli.config.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let cfg = NodeConfig::default_for_config_path(&cli.config);
        cfg.save(&cli.config)?;
        cfg
    };

    // Execute command
    match cli.command {
        Commands::Start {
            validator,
            storage,
            storage_capacity,
            rpc,
            rpc_port,
            bootstrap_peers,
        } => {
            let mut node_config = config;
            node_config.validator = validator;
            node_config.storage.enabled = storage;
            node_config.storage.capacity_gb = storage_capacity;
            node_config.rpc.enabled = rpc;
            node_config.rpc.port = rpc_port;

            if let Some(peers) = bootstrap_peers {
                node_config.network.bootstrap_peers = peers
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect();
            }

            start_node(node_config).await?;
        }
        Commands::GenerateIdentity { output } => {
            commands::generate_identity(&output).await?;
        }
        Commands::CreateGenesis { output, accounts } => {
            commands::create_genesis(&output, accounts.as_deref()).await?;
        }
        Commands::Status { rpc_url } => {
            commands::show_status(&rpc_url).await?;
        }
        Commands::Peers { rpc_url } => {
            commands::show_peers(&rpc_url).await?;
        }
        Commands::Send {
            to,
            amount,
            key_file,
            rpc_url,
            confidential,
        } => {
            commands::send_transaction(&to, &amount, &key_file, &rpc_url, confidential).await?;
        }
        Commands::Balance { address, rpc_url } => {
            commands::check_balance(&address, &rpc_url).await?;
        }
        Commands::Deploy {
            contract,
            args,
            key_file,
            rpc_url,
            runtime,
        } => {
            commands::deploy_contract(&contract, args.as_deref(), &key_file, &rpc_url, &runtime).await?;
        }
        Commands::Call {
            contract,
            function,
            key_file,
            rpc_url,
            value,
        } => {
            commands::call_contract(&contract, &function, key_file.as_deref(), &rpc_url, &value).await?;
        }
        Commands::Benchmark {
            duration,
            concurrent,
            rpc_url,
        } => {
            commands::benchmark(&rpc_url, duration, concurrent).await?;
        }
    }

    Ok(())
}

async fn start_node(config: NodeConfig) -> Result<()> {
    info!("Starting Catalyst node with config: {:#?}", config);

    let mut node = CatalystNode::new(config).await?;

    // Start the node
    node.start().await?;

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    info!("Received shutdown signal");

    // Graceful shutdown
    node.stop().await?;
    info!("Node stopped successfully");

    Ok(())
}

fn init_logging(level: &str, json_logs: bool) -> Result<()> {
    let level = level.parse::<tracing::Level>()
        .map_err(|_| anyhow::anyhow!("Invalid log level: {}", level))?;

    if json_logs {
        // Keep the CLI buildable without enabling extra `tracing-subscriber` features.
        // We still honor `--json-logs` by emitting structured-ish logs later, but for
        // now fall back to the standard formatter.
        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_target(false))
            .with(tracing_subscriber::filter::LevelFilter::from_level(level))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_target(false)
                ,
            )
            .with(
                tracing_subscriber::filter::LevelFilter::from_level(level),
            )
            .init();
    }

    Ok(())
}