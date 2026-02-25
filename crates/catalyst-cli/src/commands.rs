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
use catalyst_storage::{StorageConfig as StorageConfigLib, StorageManager};
use tokio::io::AsyncWriteExt;
use std::net::SocketAddr;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SnapshotMetaV1 {
    version: u32,
    created_at_ms: u64,
    chain_id: u64,
    network_id: String,
    genesis_hash: String,
    applied_cycle: u64,
    applied_lsu_hash: String,
    applied_state_root: String,
    last_lsu_cid: Option<String>,
}

fn decode_u64_le_opt(bytes: Option<Vec<u8>>) -> u64 {
    let Some(b) = bytes else { return 0 };
    if b.len() != 8 {
        return 0;
    }
    let mut arr = [0u8; 8];
    arr.copy_from_slice(&b);
    u64::from_le_bytes(arr)
}

fn parse_hex_32(s: &str) -> anyhow::Result<[u8; 32]> {
    let s = s.trim().strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s)?;
    anyhow::ensure!(bytes.len() == 32, "expected 32-byte hex");
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn parse_u64_hex(s: &str) -> Option<u64> {
    let s = s.trim().strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(s, 16).ok()
}

async fn fetch_chain_domain(rpc_url: &str) -> Option<(u64, [u8; 32])> {
    let client = HttpClientBuilder::default().build(rpc_url).ok()?;
    let chain_id_hex: String = client
        .request("catalyst_chainId", jsonrpsee::rpc_params![])
        .await
        .ok()?;
    let genesis_hex: String = client
        .request("catalyst_genesisHash", jsonrpsee::rpc_params![])
        .await
        .ok()?;
    let chain_id = parse_u64_hex(&chain_id_hex)?;
    let genesis_hash = parse_hex_32(&genesis_hex).ok().unwrap_or([0u8; 32]);
    Some((chain_id, genesis_hash))
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
    if let Some(ok) = r.success {
        println!("success: {ok}");
    }
    if let Some(err) = r.error {
        println!("error: {err}");
    }
    if let Some(g) = r.gas_used {
        println!("gas_used: {g}");
    }
    if let Some(ret) = r.return_data {
        println!("return_data: {ret}");
    }
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

pub async fn db_backup(data_dir: &Path, out_dir: &Path, archive: Option<&Path>) -> Result<()> {
    let mut cfg = StorageConfigLib::default();
    cfg.data_dir = data_dir.to_path_buf();
    let store = StorageManager::new(cfg).await?;
    store.backup_to_directory(out_dir).await?;
    
    // Write chain identity + head metadata to the snapshot directory for fast-sync verification.
    let chain_id = decode_u64_le_opt(store.get_metadata("protocol:chain_id").await.ok().flatten());
    let network_id = store
        .get_metadata("protocol:network_id")
        .await
        .ok()
        .flatten()
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let genesis_hash = store
        .get_metadata("protocol:genesis_hash")
        .await
        .ok()
        .flatten()
        .map(|b| format!("0x{}", hex::encode(b)))
        .unwrap_or_else(|| "0x0".to_string());
    let applied_cycle =
        decode_u64_le_opt(store.get_metadata("consensus:last_applied_cycle").await.ok().flatten());
    let applied_lsu_hash = store
        .get_metadata("consensus:last_applied_lsu_hash")
        .await
        .ok()
        .flatten()
        .map(|b| format!("0x{}", hex::encode(b)))
        .unwrap_or_else(|| "0x0".to_string());
    let applied_state_root = store
        .get_metadata("consensus:last_applied_state_root")
        .await
        .ok()
        .flatten()
        .map(|b| format!("0x{}", hex::encode(b)))
        .unwrap_or_else(|| "0x0".to_string());
    let last_lsu_cid = store
        .get_metadata("consensus:last_lsu_cid")
        .await
        .ok()
        .flatten()
        .and_then(|b| String::from_utf8(b).ok());

    let meta = SnapshotMetaV1 {
        version: 1,
        created_at_ms: catalyst_utils::utils::current_timestamp_ms(),
        chain_id,
        network_id,
        genesis_hash,
        applied_cycle,
        applied_lsu_hash,
        applied_state_root,
        last_lsu_cid,
    };
    let meta_path = out_dir.join("catalyst_snapshot.json");
    std::fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;
    println!("meta_path: {}", meta_path.display());

    if let Some(archive_path) = archive {
        // Package the snapshot directory into a tar archive for distribution.
        let f = std::fs::File::create(archive_path)?;
        let mut builder = tar::Builder::new(f);
        builder.append_dir_all(".", out_dir)?;
        builder.finish()?;

        // Compute sha256 + bytes for operator publishing.
        use sha2::Digest;
        let bytes = std::fs::read(archive_path)?;
        let mut h = sha2::Sha256::new();
        h.update(&bytes);
        let sum = h.finalize();
        println!("archive_path: {}", archive_path.display());
        println!("archive_bytes: {}", bytes.len());
        println!("archive_sha256: 0x{}", hex::encode(sum));
    }

    println!("backup_ok: true");
    println!("out_dir: {}", out_dir.display());
    Ok(())
}

/// Print RocksDB/storage statistics for an existing node data directory.
pub async fn db_stats(data_dir: &Path) -> Result<()> {
    let mut cfg = StorageConfigLib::default();
    cfg.data_dir = data_dir.to_path_buf();
    let store = StorageManager::new(cfg).await?;

    let st = store.get_statistics().await?;
    println!(
        "state_root: {}",
        st.current_state_root
            .map(|h| format!("0x{}", hex::encode(h)))
            .unwrap_or_else(|| "null".to_string())
    );
    println!("pending_transactions: {}", st.pending_transactions);

    // Column-family key counts are a simple proxy for growth hotspots.
    let mut cfs: Vec<(String, u64)> = st.column_family_stats.into_iter().collect();
    cfs.sort_by(|a, b| a.0.cmp(&b.0));
    for (cf, n) in cfs {
        println!("cf_keys.{}: {}", cf, n);
    }

    // Memory usage estimates.
    let mut mem: Vec<(String, u64)> = st.memory_usage.into_iter().collect();
    mem.sort_by(|a, b| a.0.cmp(&b.0));
    for (k, v) in mem {
        println!("mem.{}: {}", k, v);
    }

    if let Some(stats) = st.database_stats {
        println!("rocksdb_stats:\n{}", stats);
    }
    Ok(())
}

/// Run maintenance (flush + manual compaction + snapshot cleanup).
pub async fn db_maintenance(data_dir: &Path) -> Result<()> {
    let mut cfg = StorageConfigLib::default();
    cfg.data_dir = data_dir.to_path_buf();
    let store = StorageManager::new(cfg).await?;

    store.maintenance().await?;
    println!("maintenance_ok: true");
    Ok(())
}

pub async fn db_restore(data_dir: &Path, from_dir: &Path) -> Result<()> {
    // Optional pre-flight: if metadata is present, load it for post-restore verification.
    let meta_path = from_dir.join("catalyst_snapshot.json");
    let meta: Option<SnapshotMetaV1> = std::fs::read_to_string(&meta_path)
        .ok()
        .and_then(|s| serde_json::from_str::<SnapshotMetaV1>(&s).ok());

    // Restore into a sibling temp directory, then atomically swap into place to avoid corrupting
    // an existing data dir if restore fails partway.
    let parent = data_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid data_dir (no parent): {}", data_dir.display()))?;
    std::fs::create_dir_all(parent)?;
    let ts = catalyst_utils::utils::current_timestamp_ms();
    let tmp_dir = parent.join(format!(
        ".catalyst-restore-tmp-{}",
        ts
    ));
    if tmp_dir.exists() {
        std::fs::remove_dir_all(&tmp_dir)?;
    }
    std::fs::create_dir_all(&tmp_dir)?;

    {
        let mut cfg = StorageConfigLib::default();
        cfg.data_dir = tmp_dir.clone();
        let store = StorageManager::new(cfg).await?;
        store.restore_from_directory(from_dir).await?;

        if let Some(m) = &meta {
            let chain_id =
                decode_u64_le_opt(store.get_metadata("protocol:chain_id").await.ok().flatten());
            let genesis_hash = store
                .get_metadata("protocol:genesis_hash")
                .await
                .ok()
                .flatten()
                .map(|b| format!("0x{}", hex::encode(b)))
                .unwrap_or_else(|| "0x0".to_string());
            anyhow::ensure!(
                chain_id == m.chain_id,
                "restore verification failed: chain_id mismatch (snapshot={} restored={})",
                m.chain_id,
                chain_id
            );
            anyhow::ensure!(
                genesis_hash == m.genesis_hash,
                "restore verification failed: genesis_hash mismatch (snapshot={} restored={})",
                m.genesis_hash,
                genesis_hash
            );
        }
    } // drop store to release DB locks before renames

    // Swap directories (best-effort cleanup).
    if data_dir.exists() {
        let backup_dir = parent.join(format!(".catalyst-restore-backup-{}", ts));
        // Rename old data dir out of the way (atomic within same filesystem).
        std::fs::rename(data_dir, &backup_dir)?;
        // Rename restored tmp into place.
        std::fs::rename(&tmp_dir, data_dir)?;
        // Cleanup old backup dir (optional; if it fails, leave it for manual inspection).
        let _ = std::fs::remove_dir_all(&backup_dir);
    } else {
        std::fs::rename(&tmp_dir, data_dir)?;
    }

    println!("restore_ok: true");
    println!("from_dir: {}", from_dir.display());
    Ok(())
}

pub async fn snapshot_publish(
    data_dir: &Path,
    snapshot_dir: &Path,
    archive_url: &str,
    archive_path: &Path,
    ttl_seconds: Option<u64>,
) -> Result<()> {
    // Load snapshot meta file produced by db-backup.
    let meta_path = snapshot_dir.join("catalyst_snapshot.json");
    let meta: SnapshotMetaV1 = serde_json::from_str(&std::fs::read_to_string(&meta_path)?)?;

    // Compute sha256 + bytes of archive.
    use sha2::Digest;
    let bytes = std::fs::read(archive_path)?;
    let mut h = sha2::Sha256::new();
    h.update(&bytes);
    let sum = h.finalize();

    let published_at_ms = catalyst_utils::utils::current_timestamp_ms();
    let expires_at_ms = ttl_seconds.map(|s| published_at_ms.saturating_add(s.saturating_mul(1000)));

    let info = catalyst_rpc::RpcSnapshotInfo {
        version: 1,
        created_at_ms: meta.created_at_ms,
        published_at_ms,
        expires_at_ms,
        chain_id: format!("0x{:x}", meta.chain_id),
        network_id: meta.network_id.clone(),
        genesis_hash: meta.genesis_hash.clone(),
        applied_cycle: meta.applied_cycle,
        applied_lsu_hash: meta.applied_lsu_hash.clone(),
        applied_state_root: meta.applied_state_root.clone(),
        last_lsu_cid: meta.last_lsu_cid.clone(),
        archive_url: archive_url.to_string(),
        archive_sha256: Some(format!("0x{}", hex::encode(sum))),
        archive_bytes: Some(bytes.len() as u64),
    };

    let mut cfg = StorageConfigLib::default();
    cfg.data_dir = data_dir.to_path_buf();
    let store = StorageManager::new(cfg).await?;
    let payload = serde_json::to_vec(&info)?;
    store.set_metadata("snapshot:latest", &payload).await?;

    println!("published: true");
    println!("archive_url: {}", info.archive_url);
    println!("archive_sha256: {}", info.archive_sha256.clone().unwrap_or_else(|| "null".to_string()));
    println!("archive_bytes: {}", info.archive_bytes.unwrap_or(0));
    Ok(())
}

pub async fn sync_from_snapshot(
    rpc_url: &str,
    data_dir: &Path,
    work_dir: Option<&Path>,
) -> Result<()> {
    let client = HttpClientBuilder::default().build(rpc_url)?;
    let snap: Option<catalyst_rpc::RpcSnapshotInfo> = client
        .request("catalyst_getSnapshotInfo", jsonrpsee::rpc_params![])
        .await?;
    let Some(s) = snap else {
        anyhow::bail!("no snapshot published by RPC node");
    };

    let base = work_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("catalyst-sync"));
    std::fs::create_dir_all(&base)?;
    let archive_path = base.join("snapshot.tar");
    let extract_dir = base.join("snapshot");
    if extract_dir.exists() {
        std::fs::remove_dir_all(&extract_dir)?;
    }
    std::fs::create_dir_all(&extract_dir)?;

    // Download archive (stream to disk).
    let resp = reqwest::get(&s.archive_url).await?;
    anyhow::ensure!(resp.status().is_success(), "download failed: {}", resp.status());
    let mut file = tokio::fs::File::create(&archive_path).await?;
    let mut stream = resp.bytes_stream();
    use futures::StreamExt;
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    let mut written: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        written = written.saturating_add(chunk.len() as u64);
        h.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;

    // Verify byte size if provided.
    if let Some(expect_bytes) = s.archive_bytes {
        anyhow::ensure!(
            written == expect_bytes,
            "archive size mismatch: expected_bytes={} got_bytes={}",
            expect_bytes,
            written
        );
    }

    // Verify sha256 if provided.
    if let Some(expect) = &s.archive_sha256 {
        let got = format!("0x{}", hex::encode(h.finalize()));
        anyhow::ensure!(
            got.to_lowercase() == expect.to_lowercase(),
            "sha256 mismatch: expected={} got={}",
            expect,
            got
        );
    }

    // Extract tar to directory.
    let f = std::fs::File::open(&archive_path)?;
    let mut ar = tar::Archive::new(f);
    ar.unpack(&extract_dir)?;

    // Restore into data_dir.
    db_restore(data_dir, &extract_dir).await?;

    println!("restored: true");
    println!("expected_chain_id: {}", s.chain_id);
    println!("expected_genesis_hash: {}", s.genesis_hash);
    println!("snapshot_cycle: {}", s.applied_cycle);
    Ok(())
}

pub async fn snapshot_make_latest(
    data_dir: &Path,
    out_base_dir: &Path,
    archive_url_base: &str,
    retain: usize,
    ttl_seconds: Option<u64>,
) -> Result<()> {
    anyhow::ensure!(retain >= 1, "--retain must be >= 1");
    std::fs::create_dir_all(out_base_dir)?;

    let created_at_ms = catalyst_utils::utils::current_timestamp_ms();
    let snapshot_dirname = format!("catalyst-snapshot-{created_at_ms}");
    let snapshot_dir = out_base_dir.join(&snapshot_dirname);

    let archive_name = format!("catalyst-snapshot-{created_at_ms}.tar");
    let archive_path = out_base_dir.join(&archive_name);
    let url_base = archive_url_base.trim_end_matches('/');
    let archive_url = format!("{url_base}/{archive_name}");

    db_backup(data_dir, &snapshot_dir, Some(&archive_path)).await?;
    snapshot_publish(data_dir, &snapshot_dir, &archive_url, &archive_path, ttl_seconds).await?;

    // Retention: keep newest N snapshots/archives (by timestamp in filename).
    #[derive(Debug)]
    struct Item {
        ts: u64,
        path: std::path::PathBuf,
        is_dir: bool,
    }

    fn parse_ts(name: &str) -> Option<u64> {
        if let Some(rest) = name.strip_prefix("catalyst-snapshot-") {
            if let Some(num) = rest.strip_suffix(".tar") {
                return num.parse::<u64>().ok();
            }
            return rest.parse::<u64>().ok();
        }
        None
    }

    let mut items: Vec<Item> = Vec::new();
    for entry in std::fs::read_dir(out_base_dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy().to_string();
        let Some(ts) = parse_ts(&name) else { continue };
        let ft = entry.file_type()?;
        items.push(Item { ts, path, is_dir: ft.is_dir() });
    }

    // Determine timestamps to keep (newest first).
    let mut timestamps: Vec<u64> = items.iter().map(|i| i.ts).collect();
    timestamps.sort_unstable();
    timestamps.dedup();
    timestamps.sort_unstable_by(|a, b| b.cmp(a));
    let keep: std::collections::HashSet<u64> = timestamps.into_iter().take(retain).collect();

    let mut deleted = 0usize;
    for item in items {
        if keep.contains(&item.ts) {
            continue;
        }
        if item.is_dir {
            let _ = std::fs::remove_dir_all(&item.path);
        } else {
            let _ = std::fs::remove_file(&item.path);
        }
        deleted += 1;
    }

    println!("snapshot_ok: true");
    println!("snapshot_dir: {}", snapshot_dir.display());
    println!("archive_path: {}", archive_path.display());
    println!("archive_url: {}", archive_url);
    println!("retained: {}", retain);
    println!("deleted: {}", deleted);
    Ok(())
}

pub async fn show_receipt(tx_hash: &str, rpc_url: &str) -> Result<()> {
    get_receipt(tx_hash, rpc_url).await
}

pub async fn snapshot_serve(dir: &Path, bind: &str) -> Result<()> {
    use bytes::Bytes;
    use http_body_util::{BodyExt, Empty, Full, StreamBody, combinators::BoxBody};
    use hyper::{Method, Request, Response, StatusCode};
    use hyper::body::Frame;
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use hyper_util::rt::TokioIo;
    use tokio::io::{AsyncSeekExt, AsyncReadExt};
    use tokio::net::TcpListener;
    use tokio_util::io::ReaderStream;
    use futures::TryStreamExt;
    use std::convert::Infallible;
    use std::io;
    use std::path::PathBuf;

    fn boxed_empty() -> BoxBody<Bytes, io::Error> {
        Empty::new()
            .map_err(|_: Infallible| io::Error::new(io::ErrorKind::Other, "infallible"))
            .boxed()
    }

    fn boxed_text(status: StatusCode, s: &str) -> Response<BoxBody<Bytes, io::Error>> {
        let body = Full::new(Bytes::from(s.to_string()))
            .map_err(|_: Infallible| io::Error::new(io::ErrorKind::Other, "infallible"))
            .boxed();
        Response::builder()
            .status(status)
            .header("content-type", "text/plain; charset=utf-8")
            .header("cache-control", "no-store")
            .body(body)
            .unwrap()
    }

    fn reject(status: StatusCode, msg: &str) -> Response<BoxBody<Bytes, io::Error>> {
        boxed_text(status, msg)
    }

    fn sanitize_filename(path: &str) -> Option<String> {
        let p = path.trim_start_matches('/');
        if p.is_empty() {
            return None;
        }
        if p.contains('/') || p.contains('\\') || p.contains("..") {
            return None;
        }
        if !p.ends_with(".tar") {
            return None;
        }
        Some(p.to_string())
    }

    async fn handle_request(
        req: Request<hyper::body::Incoming>,
        base_dir: PathBuf,
    ) -> Result<Response<BoxBody<Bytes, io::Error>>, io::Error> {
        let method = req.method().clone();
        if method != Method::GET && method != Method::HEAD {
            return Ok(reject(StatusCode::METHOD_NOT_ALLOWED, "method not allowed\n"));
        }

        let Some(name) = sanitize_filename(req.uri().path()) else {
            return Ok(reject(StatusCode::NOT_FOUND, "not found\n"));
        };
        let full_path = base_dir.join(&name);
        let meta = match tokio::fs::metadata(&full_path).await {
            Ok(m) => m,
            Err(_) => return Ok(reject(StatusCode::NOT_FOUND, "not found\n")),
        };
        if !meta.is_file() {
            return Ok(reject(StatusCode::NOT_FOUND, "not found\n"));
        }
        let total_len = meta.len();
        if total_len == 0 {
            return Ok(reject(StatusCode::NOT_FOUND, "empty file\n"));
        }

        // Parse Range: bytes=start-end (single range only).
        let mut start: u64 = 0;
        let mut end: u64 = total_len.saturating_sub(1);
        let mut partial = false;
        if let Some(range) = req.headers().get("range").and_then(|v| v.to_str().ok()) {
            if let Some(spec) = range.strip_prefix("bytes=") {
                if let Some((a, b)) = spec.split_once('-') {
                    let a = a.trim();
                    let b = b.trim();
                    if !a.is_empty() {
                        if let Ok(s) = a.parse::<u64>() {
                            start = s;
                        } else {
                            start = total_len; // force 416
                        }
                    }
                    if !b.is_empty() {
                        if let Ok(e) = b.parse::<u64>() {
                            end = e;
                        } else {
                            end = 0;
                        }
                    }
                    partial = true;
                }
            }
        }

        if start >= total_len || end >= total_len || start > end {
            let body = boxed_empty();
            let resp = Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header("accept-ranges", "bytes")
                .header("content-range", format!("bytes */{total_len}"))
                .body(body)
                .unwrap();
            return Ok(resp);
        }

        let content_len = end - start + 1;
        let mut builder = Response::builder()
            .header("content-type", "application/x-tar")
            .header("accept-ranges", "bytes")
            .header("content-length", content_len.to_string())
            .header("cache-control", "public, max-age=60");

        if partial {
            builder = builder
                .status(StatusCode::PARTIAL_CONTENT)
                .header("content-range", format!("bytes {start}-{end}/{total_len}"));
        } else {
            builder = builder.status(StatusCode::OK);
        }

        if method == Method::HEAD {
            return Ok(builder.body(boxed_empty()).unwrap());
        }

        let mut file = tokio::fs::File::open(&full_path).await?;
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let reader = file.take(content_len);
        let stream = ReaderStream::new(reader).map_ok(Frame::data);
        let body = http_body_util::BodyExt::boxed(StreamBody::new(stream));
        Ok(builder.body(body).unwrap())
    }

    let addr: SocketAddr = bind.parse()?;
    let listener = TcpListener::bind(addr).await?;
    println!("snapshot_serve_ok: true");
    println!("dir: {}", dir.display());
    println!("bind: {}", bind);

    let base_dir = dir.to_path_buf();
    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let bd = base_dir.clone();
        tokio::spawn(async move {
            let svc = service_fn(move |req| handle_request(req, bd.clone()));
            let _ = http1::Builder::new().serve_connection(io, svc).await;
        });
    }
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
    // `lock_time` is interpreted as unix seconds "not before".
    //
    // Default to 0 (immediately valid) to avoid liveness issues when client/server
    // clocks are skewed.
    let lock_time_secs: u32 = 0;

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
            lock_time: lock_time_secs,
            fees: 0,
            data: Vec::new(),
        },
        signature_scheme: catalyst_core::protocol::sig_scheme::SCHNORR_V1,
        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
        sender_pubkey: None,
        timestamp: now_ms,
    };
    tx.core.fees = catalyst_core::protocol::min_fee(&tx);

    // Real signature: prefer v2 domain-separated payload; fall back to v1 + legacy.
    let payload = if let Some((chain_id, genesis_hash)) = fetch_chain_domain(rpc_url).await {
        tx.signing_payload_v2(chain_id, genesis_hash)
            .or_else(|_| tx.signing_payload_v1(chain_id, genesis_hash))
            .map_err(anyhow::Error::msg)?
    } else {
        tx.signing_payload().map_err(anyhow::Error::msg)?
    };
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

    let hex_data = match catalyst_core::protocol::encode_wire_tx_v2(&tx) {
        Ok(bytes) => format!("0x{}", hex::encode(bytes)),
        Err(_) => {
            let bytes = bincode::serialize(&tx)?;
            format!("0x{}", hex::encode(bytes))
        }
    };

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
    let lock_time_secs: u32 = 0;

    let mut tx = catalyst_core::protocol::Transaction {
        core: catalyst_core::protocol::TransactionCore {
            tx_type: catalyst_core::protocol::TransactionType::WorkerRegistration,
            entries: vec![catalyst_core::protocol::TransactionEntry {
                public_key: pk,
                amount: catalyst_core::protocol::EntryAmount::NonConfidential(0),
            }],
            nonce,
            lock_time: lock_time_secs,
            fees: 0,
            data: Vec::new(),
        },
        signature_scheme: catalyst_core::protocol::sig_scheme::SCHNORR_V1,
        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
        sender_pubkey: None,
        timestamp: now_ms,
    };
    tx.core.fees = catalyst_core::protocol::min_fee(&tx);

    // Real signature: prefer v2 domain-separated payload; fall back to v1 + legacy.
    let payload = if let Some((chain_id, genesis_hash)) = fetch_chain_domain(rpc_url).await {
        tx.signing_payload_v2(chain_id, genesis_hash)
            .or_else(|_| tx.signing_payload_v1(chain_id, genesis_hash))
            .map_err(anyhow::Error::msg)?
    } else {
        tx.signing_payload().map_err(anyhow::Error::msg)?
    };
    let mut rng = rand::rngs::OsRng;
    let scheme = SignatureScheme::new();
    let sig: Signature = scheme.sign(&mut rng, &sk, &payload)?;
    tx.signature = catalyst_core::protocol::AggregatedSignature(sig.to_bytes().to_vec());

    let hex_data = match catalyst_core::protocol::encode_wire_tx_v2(&tx) {
        Ok(bytes) => format!("0x{}", hex::encode(bytes)),
        Err(_) => {
            let bytes = bincode::serialize(&tx)?;
            format!("0x{}", hex::encode(bytes))
        }
    };
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
    let lock_time_secs: u32 = 0;
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
            lock_time: lock_time_secs,
            fees: 0,
            data,
        },
        signature_scheme: catalyst_core::protocol::sig_scheme::SCHNORR_V1,
        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
        sender_pubkey: None,
        timestamp: now_ms,
    };
    tx.core.fees = catalyst_core::protocol::min_fee(&tx);

    let payload = if let Some((chain_id, genesis_hash)) = fetch_chain_domain(rpc_url).await {
        tx.signing_payload_v2(chain_id, genesis_hash)
            .or_else(|_| tx.signing_payload_v1(chain_id, genesis_hash))
            .map_err(anyhow::Error::msg)?
    } else {
        tx.signing_payload().map_err(anyhow::Error::msg)?
    };
    let mut rng = rand::rngs::OsRng;
    let scheme = SignatureScheme::new();
    let sig: Signature = scheme.sign(&mut rng, &sk, &payload)?;
    tx.signature = catalyst_core::protocol::AggregatedSignature(sig.to_bytes().to_vec());

    let hex_data = match catalyst_core::protocol::encode_wire_tx_v2(&tx) {
        Ok(bytes) => format!("0x{}", hex::encode(bytes)),
        Err(_) => {
            let bytes = bincode::serialize(&tx)?;
            format!("0x{}", hex::encode(bytes))
        }
    };
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
    let lock_time_secs: u32 = 0;
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
            lock_time: lock_time_secs,
            fees: 0,
            data,
        },
        signature_scheme: catalyst_core::protocol::sig_scheme::SCHNORR_V1,
        signature: catalyst_core::protocol::AggregatedSignature(vec![0u8; 64]),
        sender_pubkey: None,
        timestamp: now_ms,
    };
    tx.core.fees = catalyst_core::protocol::min_fee(&tx);

    let payload = if let Some((chain_id, genesis_hash)) = fetch_chain_domain(rpc_url).await {
        tx.signing_payload_v2(chain_id, genesis_hash)
            .or_else(|_| tx.signing_payload_v1(chain_id, genesis_hash))
            .map_err(anyhow::Error::msg)?
    } else {
        tx.signing_payload().map_err(anyhow::Error::msg)?
    };
    let mut rng = rand::rngs::OsRng;
    let scheme = SignatureScheme::new();
    let sig: Signature = scheme.sign(&mut rng, &sk, &payload)?;
    tx.signature = catalyst_core::protocol::AggregatedSignature(sig.to_bytes().to_vec());

    let hex_data = match catalyst_core::protocol::encode_wire_tx_v2(&tx) {
        Ok(bytes) => format!("0x{}", hex::encode(bytes)),
        Err(_) => {
            let bytes = bincode::serialize(&tx)?;
            format!("0x{}", hex::encode(bytes))
        }
    };
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

