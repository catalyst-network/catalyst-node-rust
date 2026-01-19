//! CLI subcommand implementations.
//!
//! The CLI is still evolving; these handlers are currently lightweight stubs
//! to keep the workspace compiling while the RPC and node subsystems mature.

use anyhow::Result;
use std::path::Path;

pub async fn generate_identity(output: &Path) -> Result<()> {
    let _ = output;
    // TODO: implement persistent identity generation.
    Ok(())
}

pub async fn create_genesis(output: &Path, accounts: Option<&Path>) -> Result<()> {
    let _ = (output, accounts);
    // TODO: implement genesis creation.
    Ok(())
}

pub async fn get_status(rpc_url: &str) -> Result<()> {
    let _ = rpc_url;
    // TODO: implement status query over RPC.
    Ok(())
}

pub async fn get_peers(rpc_url: &str) -> Result<()> {
    let _ = rpc_url;
    // TODO: implement peer query over RPC.
    Ok(())
}

// Backwards-compatible names used by `main.rs`.
pub async fn show_status(rpc_url: &str) -> Result<()> {
    get_status(rpc_url).await
}

pub async fn show_peers(rpc_url: &str) -> Result<()> {
    get_peers(rpc_url).await
}

pub async fn send_transaction(
    to: &str,
    amount: &str,
    key_file: &Path,
    rpc_url: &str,
    confidential: bool,
) -> Result<()> {
    let _ = (to, amount, key_file, rpc_url, confidential);
    // TODO: implement transaction submission.
    Ok(())
}

pub async fn check_balance(address: &str, rpc_url: &str) -> Result<()> {
    let _ = (address, rpc_url);
    // TODO: implement balance query.
    Ok(())
}

pub async fn deploy_contract(
    contract: &Path,
    args: Option<&str>,
    key_file: &Path,
    rpc_url: &str,
    runtime: &str,
) -> Result<()> {
    let _ = (contract, args, key_file, rpc_url, runtime);
    // TODO: implement contract deployment.
    Ok(())
}

pub async fn call_contract(
    contract: &str,
    function: &str,
    key_file: Option<&Path>,
    rpc_url: &str,
    value: &str,
) -> Result<()> {
    let _ = (contract, function, key_file, rpc_url, value);
    // TODO: implement contract call.
    Ok(())
}

pub async fn benchmark(rpc_url: &str, duration_secs: u64, concurrent: usize) -> Result<()> {
    let _ = (rpc_url, duration_secs, concurrent);
    // TODO: implement benchmark runner.
    Ok(())
}

