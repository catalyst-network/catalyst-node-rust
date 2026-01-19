//! CLI subcommand implementations.
//!
//! The CLI is still evolving; these handlers are currently lightweight stubs
//! to keep the workspace compiling while the RPC and node subsystems mature.

use anyhow::Result;
use std::path::Path;

use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::core::client::ClientT;

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
    let client = HttpClientBuilder::default().build(rpc_url)?;

    let head: catalyst_rpc::RpcHead = client.request("catalyst_head", jsonrpsee::rpc_params![]).await?;
    let block_number: u64 = client.request("catalyst_blockNumber", jsonrpsee::rpc_params![]).await?;
    let version: String = client.request("catalyst_version", jsonrpsee::rpc_params![]).await?;

    println!("version: {version}");
    println!("applied_cycle: {}", head.applied_cycle);
    println!("block_number: {block_number}");
    println!("applied_lsu_hash: {}", head.applied_lsu_hash);
    println!("applied_state_root: {}", head.applied_state_root);
    if let Some(cid) = head.last_lsu_cid {
        println!("last_lsu_cid: {cid}");
    }
    Ok(())
}

pub async fn get_peers(rpc_url: &str) -> Result<()> {
    let client = HttpClientBuilder::default().build(rpc_url)?;
    let peers: u64 = client.request("catalyst_peerCount", jsonrpsee::rpc_params![]).await?;
    println!("peer_count: {peers}");
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
    let client = HttpClientBuilder::default().build(rpc_url)?;
    let bal: String = client.request("catalyst_getBalance", jsonrpsee::rpc_params![address]).await?;
    println!("balance: {bal}");
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

