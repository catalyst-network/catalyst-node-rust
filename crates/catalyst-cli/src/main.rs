use clap::{Parser, Subcommand};
use anyhow::Result;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use std::path::PathBuf;
use std::path::Path;
use serde::Deserialize;

mod node;
mod commands;
mod config;
mod tx;
mod sync;
mod dfs_store;
mod identity;
mod evm;
mod evm_revm;

use node::CatalystNode;
use config::NodeConfig;

fn is_testnet_config_path(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str().to_string_lossy().eq_ignore_ascii_case("testnet"))
}

#[derive(Debug, Deserialize)]
struct ValidatorsFile {
    #[serde(default)]
    validator_worker_ids: Vec<String>,
}

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

        /// RPC bind address (use 0.0.0.0 to expose externally)
        #[arg(long, default_value = "127.0.0.1")]
        rpc_address: String,

        /// Bootstrap peers (comma-separated multiaddrs)
        #[arg(long)]
        bootstrap_peers: Option<String>,

        /// Generate and gossip dummy transactions periodically (dev/testnet helper)
        #[arg(long, default_value_t = false)]
        generate_txs: bool,

        /// Interval (ms) between generated transactions when --generate-txs is set
        #[arg(long, default_value_t = 500)]
        tx_interval_ms: u64,
    },
    /// Generate a new node identity
    GenerateIdentity {
        /// Output file for the identity
        #[arg(short, long, default_value = "identity.json")]
        output: PathBuf,
    },
    /// Print the public key for a given 32-byte hex private key file
    Pubkey {
        /// Private key file (32 bytes hex)
        #[arg(long)]
        key_file: PathBuf,
    },
    /// Get a balance along with an inclusion proof (and verify it client-side)
    BalanceProof {
        /// Address (32-byte hex public key)
        address: String,
        /// RPC endpoint
        #[arg(long, default_value = "http://localhost:8545")]
        rpc_url: String,
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
    /// Backup node database to a directory (snapshot export)
    DbBackup {
        /// Data directory (same as config.storage.data_dir)
        #[arg(long)]
        data_dir: PathBuf,
        /// Output directory for the backup
        #[arg(long)]
        out_dir: PathBuf,
        /// Optional tar archive output path for distribution
        #[arg(long)]
        archive: Option<PathBuf>,
    },
    /// Restore node database from a backup directory (snapshot import)
    DbRestore {
        /// Data directory (same as config.storage.data_dir)
        #[arg(long)]
        data_dir: PathBuf,
        /// Backup directory to restore from
        #[arg(long)]
        from_dir: PathBuf,
    },
    /// Publish snapshot metadata into the node DB (served via RPC for fast-sync tooling)
    SnapshotPublish {
        /// Data directory (same as config.storage.data_dir) of the RPC node
        #[arg(long)]
        data_dir: PathBuf,
        /// Snapshot directory created by `db-backup` (contains *.snapshot and *_data)
        #[arg(long)]
        snapshot_dir: PathBuf,
        /// URL where the snapshot archive can be downloaded
        #[arg(long)]
        archive_url: String,
        /// Path to the tar archive file (used to compute sha256/bytes)
        #[arg(long)]
        archive_path: PathBuf,
    },
    /// Download the published snapshot archive and restore it locally
    SyncFromSnapshot {
        /// RPC endpoint to fetch snapshot info from
        #[arg(long, default_value = "http://localhost:8545")]
        rpc_url: String,
        /// Data directory to restore into (same as config.storage.data_dir)
        #[arg(long)]
        data_dir: PathBuf,
        /// Directory to download/extract into (defaults to /tmp)
        #[arg(long)]
        work_dir: Option<PathBuf>,
    },
    /// Create+archive a new snapshot and publish it to RPC (with retention cleanup)
    SnapshotMakeLatest {
        /// Data directory (same as config.storage.data_dir) of the RPC node
        #[arg(long)]
        data_dir: PathBuf,
        /// Base directory to write snapshot directories and tar archives into
        #[arg(long)]
        out_base_dir: PathBuf,
        /// Base URL that serves files from out_base_dir (no trailing slash required)
        #[arg(long)]
        archive_url_base: String,
        /// How many snapshots/archives to retain in out_base_dir
        #[arg(long, default_value_t = 3)]
        retain: usize,
    },
    /// Show a transaction receipt/status (and inclusion proof when applied)
    Receipt {
        /// Transaction hash (tx_id)
        tx_hash: String,
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
    /// Register this node as a worker/validator candidate (on-chain)
    RegisterWorker {
        /// Private key file
        #[arg(long, default_value = "wallet.key")]
        key_file: PathBuf,
        /// RPC endpoint
        #[arg(long, default_value = "http://localhost:8545")]
        rpc_url: String,
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

    // Execute command
    match cli.command {
        Commands::Start {
            validator,
            storage,
            storage_capacity,
            rpc,
            rpc_port,
            rpc_address,
            bootstrap_peers,
            generate_txs,
            tx_interval_ms,
        } => {
            // Load configuration (or auto-generate a local default if missing).
            let mut config = if cli.config.exists() {
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

            // Testnet-only consensus speedup: keep dev/mainnet defaults intact, but make the local
            // `make testnet` loop iterate quickly.
            if is_testnet_config_path(&cli.config) {
                // Testnet should iterate quickly but still be stable under local scheduling jitter.
                // 20s cycle, 4s per phase (16s total), 1s freeze window.
                config.consensus.cycle_duration_seconds = 20;
                config.consensus.freeze_time_seconds = 1;
                config.consensus.min_producer_count = 2;
                config.consensus.phase_timeouts.construction_timeout = 4;
                config.consensus.phase_timeouts.campaigning_timeout = 4;
                config.consensus.phase_timeouts.voting_timeout = 4;
                config.consensus.phase_timeouts.synchronization_timeout = 4;

                // Testnet-only DFS: use a shared local DFS directory so CID fetch works across
                // the 3 local processes even without a P2P DFS layer.
                config.dfs.cache_dir = PathBuf::from("testnet/shared_dfs");

                // Ensure per-node identity is NOT shared across nodes when configs already exist.
                // (Older configs may have `node.key`, which would collide in CWD.)
                if let Some(dir) = cli.config.parent() {
                    config.node.private_key_file = dir.join("node.key");
                }

                // Deterministic validator set for testnet: if present, load from `testnet/validators.toml`.
                // This replaces the older NodeStatus-based ad-hoc discovery.
                if config.consensus.validator_worker_ids.is_empty() {
                    let validators_path = PathBuf::from("testnet/validators.toml");
                    if let Ok(s) = std::fs::read_to_string(&validators_path) {
                        if let Ok(vf) = toml::from_str::<ValidatorsFile>(&s) {
                            if !vf.validator_worker_ids.is_empty() {
                                config.consensus.validator_worker_ids = vf.validator_worker_ids;
                                info!(
                                    "Loaded testnet validator worker pool from {:?} (n={})",
                                    validators_path,
                                    config.consensus.validator_worker_ids.len()
                                );
                            }
                        }
                    }
                }

                // Persist so the config dump/logs match runtime behavior.
                config.save(&cli.config)?;
                info!("Applied fast consensus timings for testnet config {:?}", cli.config);
            }

            let mut node_config = config;
            node_config.validator = validator;
            node_config.storage.enabled = storage;
            node_config.storage.capacity_gb = storage_capacity;
            node_config.rpc.enabled = rpc;
            node_config.rpc.port = rpc_port;
            node_config.rpc.address = rpc_address;

            if let Some(peers) = bootstrap_peers {
                node_config.network.bootstrap_peers = peers
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect();
            }

            start_node(node_config, generate_txs, tx_interval_ms).await?;
        }
        Commands::GenerateIdentity { output } => {
            commands::generate_identity(&output).await?;
        }
        Commands::Pubkey { key_file } => {
            let sk = crate::identity::load_or_generate_private_key(&key_file, false)?;
            let pk = crate::identity::public_key_bytes(&sk);
            println!("{}", hex::encode(pk));
        }
        Commands::BalanceProof { address, rpc_url } => {
            commands::balance_proof(&address, &rpc_url).await?;
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
        Commands::DbBackup { data_dir, out_dir, archive } => {
            commands::db_backup(&data_dir, &out_dir, archive.as_deref()).await?;
        }
        Commands::DbRestore { data_dir, from_dir } => {
            commands::db_restore(&data_dir, &from_dir).await?;
        }
        Commands::SnapshotPublish { data_dir, snapshot_dir, archive_url, archive_path } => {
            commands::snapshot_publish(&data_dir, &snapshot_dir, &archive_url, &archive_path).await?;
        }
        Commands::SyncFromSnapshot { rpc_url, data_dir, work_dir } => {
            commands::sync_from_snapshot(&rpc_url, &data_dir, work_dir.as_deref()).await?;
        }
        Commands::SnapshotMakeLatest { data_dir, out_base_dir, archive_url_base, retain } => {
            commands::snapshot_make_latest(&data_dir, &out_base_dir, &archive_url_base, retain).await?;
        }
        Commands::Receipt { tx_hash, rpc_url } => {
            commands::show_receipt(&tx_hash, &rpc_url).await?;
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
        Commands::RegisterWorker { key_file, rpc_url } => {
            commands::register_worker(&key_file, &rpc_url).await?;
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

async fn start_node(config: NodeConfig, generate_txs: bool, tx_interval_ms: u64) -> Result<()> {
    info!("Starting Catalyst node with config: {:#?}", config);

    let mut node = CatalystNode::new(config, generate_txs, tx_interval_ms).await?;

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