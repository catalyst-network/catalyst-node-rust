//! CLI subcommand implementations.
//!
//! The CLI is still evolving; these handlers are currently lightweight stubs
//! to keep the workspace compiling while the RPC and node subsystems mature.

use anyhow::Result;
use std::path::Path;

use jsonrpsee::http_client::HttpClientBuilder;
use jsonrpsee::core::client::ClientT;

use catalyst_crypto::signatures::{Signature, SignatureScheme};
use crate::evm::EvmTxKind;
use alloy_primitives::Address as EvmAddress;

fn parse_hex_32(s: &str) -> anyhow::Result<[u8; 32]> {
    let s = s.trim().strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s)?;
    anyhow::ensure!(bytes.len() == 32, "expected 32-byte hex");
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn hash_leaf(key: &[u8], value: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update([0u8]);
    h.update((key.len() as u64).to_le_bytes());
    h.update(key);
    h.update((value.len() as u64).to_le_bytes());
    h.update(value);
    h.finalize().into()
}

fn hash_node(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update([1u8]);
    h.update(left);
    h.update(right);
    h.finalize().into()
}

fn verify_merkle_proof(root: &[u8; 32], leaf: &[u8; 32], steps: &[String]) -> anyhow::Result<bool> {
    let mut cur = *leaf;
    for s in steps {
        let (dir, hexh) = s
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("bad proof step (expected L:0x.. or R:0x..): {s}"))?;
        let hexh = hexh.strip_prefix("0x").unwrap_or(hexh);
        let b = hex::decode(hexh)?;
        anyhow::ensure!(b.len() == 32, "bad proof hash len");
        let mut sib = [0u8; 32];
        sib.copy_from_slice(&b);
        cur = match dir {
            "L" => hash_node(&sib, &cur),
            "R" => hash_node(&cur, &sib),
            _ => return Err(anyhow::anyhow!("bad proof direction: {dir}")),
        };
    }
    Ok(&cur == root)
}

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

pub async fn get_receipt(tx_hash: &str, rpc_url: &str) -> Result<()> {
    let client = HttpClientBuilder::default().build(rpc_url)?;
    let receipt: Option<catalyst_rpc::RpcTxReceipt> = client
        .request("catalyst_getTransactionReceipt", jsonrpsee::rpc_params![tx_hash])
        .await?;
    let Some(r) = receipt else {
        println!("receipt: null");
        return Ok(());
    };
    println!("tx_hash: {}", r.tx_hash);
    println!("status: {}", r.status);
    println!("received_at_ms: {}", r.received_at_ms);
    if let Some(from) = r.from {
        println!("from: {from}");
    }
    println!("nonce: {}", r.nonce);
    println!("fees: {}", r.fees);
    if let Some(c) = r.selected_cycle {
        println!("selected_cycle: {c}");
    }
    if let Some(c) = r.applied_cycle {
        println!("applied_cycle: {c}");
    }
    if let Some(h) = r.applied_lsu_hash {
        println!("applied_lsu_hash: {h}");
    }
    if let Some(h) = r.applied_state_root {
        println!("applied_state_root: {h}");
    }

    if r.applied_cycle.is_some() {
        let proof: Option<catalyst_rpc::RpcTxInclusionProof> = client
            .request(
                "catalyst_getTransactionInclusionProof",
                jsonrpsee::rpc_params![tx_hash],
            )
            .await
            .unwrap_or(None);
        if let Some(p) = proof {
            println!("inclusion_cycle: {}", p.cycle);
            println!("inclusion_tx_index: {}", p.tx_index);
            println!("inclusion_merkle_root: {}", p.merkle_root);
            println!("inclusion_proof_len: {}", p.proof.len());
        }
    }
    Ok(())
}

pub async fn balance_proof(address: &str, rpc_url: &str) -> Result<()> {
    let client = HttpClientBuilder::default().build(rpc_url)?;
    let p: catalyst_rpc::RpcBalanceProof =
        client.request("catalyst_getBalanceProof", jsonrpsee::rpc_params![address]).await?;
    println!("balance: {}", p.balance);
    println!("state_root: {}", p.state_root);
    println!("proof_len: {}", p.proof.len());

    let pk = parse_hex_32(address)?;
    let mut key = b"bal:".to_vec();
    key.extend_from_slice(&pk);

    let bal_i64: i64 = p.balance.parse().unwrap_or(0);
    let leaf = hash_leaf(&key, &bal_i64.to_le_bytes());

    let root_bytes = {
        let b = hex::decode(p.state_root.trim().strip_prefix("0x").unwrap_or(&p.state_root))?;
        anyhow::ensure!(b.len() == 32, "bad state_root len");
        let mut r = [0u8; 32];
        r.copy_from_slice(&b);
        r
    };

    let ok = verify_merkle_proof(&root_bytes, &leaf, &p.proof)?;
    println!("proof_ok: {}", ok);
    Ok(())
}

// Backwards-compatible names used by `main.rs`.
pub async fn show_status(rpc_url: &str) -> Result<()> {
    get_status(rpc_url).await
}

pub async fn show_peers(rpc_url: &str) -> Result<()> {
    get_peers(rpc_url).await
}

pub async fn show_receipt(tx_hash: &str, rpc_url: &str) -> Result<()> {
    get_receipt(tx_hash, rpc_url).await
}

pub async fn send_transaction(
    to: &str,
    amount: &str,
    key_file: &Path,
    rpc_url: &str,
    confidential: bool,
) -> Result<()> {
    let _ = confidential;
    let client = HttpClientBuilder::default().build(rpc_url)?;

    // Dev-only: treat `to` as a 32-byte pubkey hex string (node-id), and send from faucet.
    let to_hex = to.strip_prefix("0x").unwrap_or(to);
    let to_bytes = hex::decode(to_hex)?;
    anyhow::ensure!(to_bytes.len() == 32, "to must be 32-byte hex pubkey");
    let mut to_pk = [0u8; 32];
    to_pk.copy_from_slice(&to_bytes);

    // Load (or create) sender private key and derive sender public key.
    let sk = crate::identity::load_or_generate_private_key(key_file, true)?;
    let from_pk = crate::identity::public_key_bytes(&sk);

    // Fetch current nonce for sender.
    let from_hex = format!("0x{}", hex::encode(from_pk));
    let cur_nonce: u64 = client
        .request("catalyst_getNonce", jsonrpsee::rpc_params![from_hex])
        .await
        .unwrap_or(0);
    let nonce = cur_nonce.saturating_add(1);
    let nonce = std::env::var("CATALYST_FORCE_NONCE")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(nonce);

    let amt: i64 = amount.parse()?;
    anyhow::ensure!(amt > 0, "amount must be positive");

    let now_ms = catalyst_utils::utils::current_timestamp_ms();
    let now_secs = (now_ms / 1000) as u32;

    let mut tx = catalyst_core::protocol::Transaction {
        core: catalyst_core::protocol::TransactionCore {
            tx_type: catalyst_core::protocol::TransactionType::NonConfidentialTransfer,
            entries: vec![
                catalyst_core::protocol::TransactionEntry {
                    public_key: from_pk,
                    amount: catalyst_core::protocol::EntryAmount::NonConfidential(-amt),
                },
                catalyst_core::protocol::TransactionEntry {
                    public_key: to_pk,
                    amount: catalyst_core::protocol::EntryAmount::NonConfidential(amt),
                },
            ],
            nonce,
            lock_time: now_secs,
            fees: 0,
            data: Vec::new(),
        },
        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
        timestamp: now_ms,
    };
    tx.core.fees = catalyst_core::protocol::min_fee(&tx);

    // Real signature: Schnorr over canonical payload.
    let payload = tx.signing_payload().map_err(anyhow::Error::msg)?;
    let mut rng = rand::rngs::OsRng;
    let scheme = SignatureScheme::new();
    let sig: Signature = scheme.sign(&mut rng, &sk, &payload)?;
    tx.signature = catalyst_core::protocol::AggregatedSignature(sig.to_bytes().to_vec());

    // Dev-only: allow easy end-to-end validation that signature checks are enforced.
    // Example:
    //   CATALYST_TAMPER_SIG=1 cargo run -p catalyst-cli -- send ... --rpc-url ...
    if std::env::var("CATALYST_TAMPER_SIG").ok().as_deref() == Some("1") {
        if let Some(b) = tx.signature.0.first_mut() {
            *b ^= 0x01;
        }
    }

    let bytes = bincode::serialize(&tx)?;
    let hex_data = format!("0x{}", hex::encode(bytes));

    let tx_id: String = client
        .request("catalyst_sendRawTransaction", jsonrpsee::rpc_params![hex_data])
        .await?;
    println!("tx_id: {tx_id}");
    Ok(())
}

pub async fn register_worker(key_file: &Path, rpc_url: &str) -> Result<()> {
    let client = HttpClientBuilder::default().build(rpc_url)?;

    // Load (or create) sender private key and derive sender public key.
    let sk = crate::identity::load_or_generate_private_key(key_file, true)?;
    let pk = crate::identity::public_key_bytes(&sk);

    // Fetch current nonce for sender.
    let pk_hex = format!("0x{}", hex::encode(pk));
    let cur_nonce: u64 = client
        .request("catalyst_getNonce", jsonrpsee::rpc_params![pk_hex])
        .await
        .unwrap_or(0);
    let nonce = cur_nonce.saturating_add(1);

    let now_ms = catalyst_utils::utils::current_timestamp_ms();
    let now_secs = (now_ms / 1000) as u32;

    let mut tx = catalyst_core::protocol::Transaction {
        core: catalyst_core::protocol::TransactionCore {
            tx_type: catalyst_core::protocol::TransactionType::WorkerRegistration,
            entries: vec![catalyst_core::protocol::TransactionEntry {
                public_key: pk,
                amount: catalyst_core::protocol::EntryAmount::NonConfidential(0),
            }],
            nonce,
            lock_time: now_secs,
            fees: 0,
            data: Vec::new(),
        },
        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
        timestamp: now_ms,
    };
    tx.core.fees = catalyst_core::protocol::min_fee(&tx);

    // Real signature: Schnorr over canonical payload.
    let payload = tx.signing_payload().map_err(anyhow::Error::msg)?;
    let mut rng = rand::rngs::OsRng;
    let scheme = SignatureScheme::new();
    let sig: Signature = scheme.sign(&mut rng, &sk, &payload)?;
    tx.signature = catalyst_core::protocol::AggregatedSignature(sig.to_bytes().to_vec());

    let bytes = bincode::serialize(&tx)?;
    let hex_data = format!("0x{}", hex::encode(bytes));
    let tx_id: String = client
        .request("catalyst_sendRawTransaction", jsonrpsee::rpc_params![hex_data])
        .await?;
    println!("tx_id: {tx_id}");
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
    let _ = (args, runtime);
    let client = HttpClientBuilder::default().build(rpc_url)?;

    // Load sender key
    let sk = crate::identity::load_or_generate_private_key(key_file, true)?;
    let from_pk = crate::identity::public_key_bytes(&sk);

    // Nonce
    let from_hex = format!("0x{}", hex::encode(from_pk));
    let cur_nonce: u64 = client
        .request("catalyst_getNonce", jsonrpsee::rpc_params![from_hex])
        .await
        .unwrap_or(0);
    let nonce = cur_nonce.saturating_add(1);

    // Read bytecode: accept a file containing hex (0x...) or raw bytes.
    let raw = std::fs::read(contract)?;
    let bytecode = if let Ok(s) = std::str::from_utf8(&raw) {
        let s = s.trim();
        let s = s.strip_prefix("0x").unwrap_or(s);
        if s.chars().all(|c| c.is_ascii_hexdigit()) && s.len() % 2 == 0 {
            hex::decode(s)?
        } else {
            raw
        }
    } else {
        raw
    };

    let now_ms = catalyst_utils::utils::current_timestamp_ms();
    let now_secs = (now_ms / 1000) as u32;
    let kind = EvmTxKind::Deploy { bytecode: bytecode.clone() };
    let data = bincode::serialize(&kind)?;

    let mut tx = catalyst_core::protocol::Transaction {
        core: catalyst_core::protocol::TransactionCore {
            tx_type: catalyst_core::protocol::TransactionType::SmartContract,
            entries: vec![catalyst_core::protocol::TransactionEntry {
                public_key: from_pk,
                amount: catalyst_core::protocol::EntryAmount::NonConfidential(0),
            }],
            nonce,
            lock_time: now_secs,
            fees: 0,
            data,
        },
        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
        timestamp: now_ms,
    };
    tx.core.fees = catalyst_core::protocol::min_fee(&tx);

    let payload = tx.signing_payload().map_err(anyhow::Error::msg)?;
    let mut rng = rand::rngs::OsRng;
    let scheme = SignatureScheme::new();
    let sig: Signature = scheme.sign(&mut rng, &sk, &payload)?;
    tx.signature = catalyst_core::protocol::AggregatedSignature(sig.to_bytes().to_vec());

    let bytes = bincode::serialize(&tx)?;
    let hex_data = format!("0x{}", hex::encode(bytes));
    let tx_id: String = client
        .request("catalyst_sendRawTransaction", jsonrpsee::rpc_params![hex_data])
        .await?;

    // Deterministic create address
    let mut from20 = [0u8; 20];
    from20.copy_from_slice(&from_pk[12..32]);
    let from_addr = EvmAddress::from_slice(&from20);
    // EVM nonce is derived from protocol nonce (protocol starts at 1; EVM starts at 0).
    let evm_nonce = nonce.saturating_sub(1);
    let created = catalyst_runtime_evm::utils::calculate_ethereum_create_address(&from_addr, evm_nonce);

    println!("tx_id: {tx_id}");
    println!("contract_address: 0x{}", hex::encode(created.as_slice()));
    Ok(())
}

pub async fn call_contract(
    contract: &str,
    function: &str,
    key_file: Option<&Path>,
    rpc_url: &str,
    value: &str,
) -> Result<()> {
    let _ = value;
    let client = HttpClientBuilder::default().build(rpc_url)?;

    let to_hex = contract.trim().strip_prefix("0x").unwrap_or(contract.trim());
    let to_bytes = hex::decode(to_hex)?;
    anyhow::ensure!(to_bytes.len() == 20, "contract must be 20-byte hex address");
    let mut to = [0u8; 20];
    to.copy_from_slice(&to_bytes);

    // Treat `function` as calldata hex for now.
    let data_hex = function.trim().strip_prefix("0x").unwrap_or(function.trim());
    let input = if data_hex.is_empty() { Vec::new() } else { hex::decode(data_hex)? };

    // Load sender key
    let key_file = key_file.unwrap_or_else(|| Path::new("wallet.key"));
    let sk = crate::identity::load_or_generate_private_key(key_file, true)?;
    let from_pk = crate::identity::public_key_bytes(&sk);

    // Nonce
    let from_hex = format!("0x{}", hex::encode(from_pk));
    let cur_nonce: u64 = client
        .request("catalyst_getNonce", jsonrpsee::rpc_params![from_hex])
        .await
        .unwrap_or(0);
    let nonce = cur_nonce.saturating_add(1);

    let now_ms = catalyst_utils::utils::current_timestamp_ms();
    let now_secs = (now_ms / 1000) as u32;
    let kind = EvmTxKind::Call { to, input };
    let data = bincode::serialize(&kind)?;

    let mut tx = catalyst_core::protocol::Transaction {
        core: catalyst_core::protocol::TransactionCore {
            tx_type: catalyst_core::protocol::TransactionType::SmartContract,
            entries: vec![catalyst_core::protocol::TransactionEntry {
                public_key: from_pk,
                amount: catalyst_core::protocol::EntryAmount::NonConfidential(0),
            }],
            nonce,
            lock_time: now_secs,
            fees: 0,
            data,
        },
        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
        timestamp: now_ms,
    };
    tx.core.fees = catalyst_core::protocol::min_fee(&tx);

    let payload = tx.signing_payload().map_err(anyhow::Error::msg)?;
    let mut rng = rand::rngs::OsRng;
    let scheme = SignatureScheme::new();
    let sig: Signature = scheme.sign(&mut rng, &sk, &payload)?;
    tx.signature = catalyst_core::protocol::AggregatedSignature(sig.to_bytes().to_vec());

    let bytes = bincode::serialize(&tx)?;
    let hex_data = format!("0x{}", hex::encode(bytes));
    let tx_id: String = client
        .request("catalyst_sendRawTransaction", jsonrpsee::rpc_params![hex_data])
        .await?;
    println!("tx_id: {tx_id}");
    Ok(())
}

pub async fn benchmark(rpc_url: &str, duration_secs: u64, concurrent: usize) -> Result<()> {
    let _ = (rpc_url, duration_secs, concurrent);
    // TODO: implement benchmark runner.
    Ok(())
}

