//! Catalyst DFS: local content-addressed store with optional IPFS HTTP backend.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use async_trait::async_trait;
// NOTE: use cid's re-exported multihash to avoid version conflicts.
use cid::Cid;
use multihash_codetable::{Code, MultihashDigest};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::{fs, io::AsyncWriteExt};

#[cfg(feature = "ipfs-http")]
use {reqwest::Client, reqwest::Url};

/// Result alias
pub type DfsResult<T> = Result<T, DfsError>;

/// DFS error type
#[derive(Debug, Error)]
pub enum DfsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[cfg(feature = "ipfs-http")]
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid content id")]
    InvalidContentId,

    #[error("IPFS disabled")]
    IpfsDisabled,

    #[error("other: {0}")]
    Other(String),
}

/// Replication levels for content
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum ReplicationLevel {
    /// Keep only locally (no IPFS pin)
    None,
    /// Attempt single remote replica
    Low,
    /// Attempt a few replicas
    Medium,
    /// Aggressively replicate/pin
    High,
}

impl Default for ReplicationLevel {
    fn default() -> Self {
        ReplicationLevel::Low
    }
}

/// DFS configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfsConfig {
    /// Local directory for content-addressed files
    pub local_dir: PathBuf,
    /// Optional IPFS API endpoint (e.g. `http://127.0.0.1:5001`)
    pub ipfs_api: Option<String>,
    /// Desired replication policy
    pub replication: ReplicationLevel,
}

impl Default for DfsConfig {
    fn default() -> Self {
        Self {
            local_dir: PathBuf::from("./.catalyst/dfs"),
            ipfs_api: None,
            replication: ReplicationLevel::Low,
        }
    }
}

/// ContentId is a CIDv1 string over raw bytes (codec 0x55) using sha2-256 multihash.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ContentId(String);

impl ContentId {
    /// Compute a CID from raw data.
    pub fn from_data(data: &[u8]) -> DfsResult<Self> {
        // Multicodec "raw" = 0x55
        const RAW_CODEC: u64 = 0x55;
        let mh = Code::Sha2_256.digest(data);
        let cid = Cid::new_v1(RAW_CODEC, mh);
        Ok(Self(cid.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_path_component(&self) -> &str {
        &self.0
    }
}

impl From<String> for ContentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}
impl From<&str> for ContentId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl std::fmt::Display for ContentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where content belongs for policy/retention
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum ContentCategory {
    Recent,
    Hot,
    Historical,
}

/// The generic DFS trait used across the node.
#[async_trait]
pub trait DistributedFileSystem: Send + Sync {
    /// Store bytes, return a content id (CID).
    async fn put(&self, data: Vec<u8>) -> DfsResult<ContentId>;

    /// Retrieve bytes for a given content id.
    async fn get(&self, cid: &ContentId) -> DfsResult<Vec<u8>>;

    /// (Best-effort) pin content to prevent GC.
    async fn pin(&self, _cid: &ContentId) -> DfsResult<()> {
        Ok(())
    }

    /// (Best-effort) unpin content.
    async fn unpin(&self, _cid: &ContentId) -> DfsResult<()> {
        Ok(())
    }
}

/// Alias used by existing tests/imports
pub type DfsService = dyn DistributedFileSystem;

/// A factory for creating DFS instances.
pub trait DfsFactory: Send + Sync {
    fn create(&self, cfg: &DfsConfig) -> Arc<dyn DistributedFileSystem>;
}

/// IPFS+local implementation factory.
#[derive(Debug, Default)]
pub struct IpfsFactory;

impl DfsFactory for IpfsFactory {
    fn create(&self, cfg: &DfsConfig) -> Arc<dyn DistributedFileSystem> {
        Arc::new(IpfsDfs::new(cfg.clone()))
    }
}

/// Simple categorized wrapper that can route by `ContentCategory`.
#[derive(Clone)]
pub struct CategorizedStorage {
    map: Arc<RwLock<HashMap<ContentCategory, Arc<dyn DistributedFileSystem>>>>,
    default: Arc<dyn DistributedFileSystem>,
}

impl CategorizedStorage {
    pub fn new(default: Arc<dyn DistributedFileSystem>) -> Self {
        Self {
            map: Arc::new(RwLock::new(HashMap::new())),
            default,
        }
    }

    pub fn with_backend(self, category: ContentCategory, backend: Arc<dyn DistributedFileSystem>) -> Self {
        self.map.write().insert(category, backend);
        self
    }

    pub fn for_category(&self, category: ContentCategory) -> Arc<dyn DistributedFileSystem> {
        self.map
            .read()
            .get(&category)
            .cloned()
            .unwrap_or_else(|| self.default.clone())
    }
}

/// Content provider discovery (stubbed for now).
#[async_trait]
pub trait ContentProvider: Send + Sync {
    async fn announce(&self, _cid: &ContentId) -> DfsResult<()>;
    async fn providers(&self, _cid: &ContentId) -> DfsResult<Vec<String>>;
}

/// DHT-based provider (no-op placeholder).
#[derive(Debug, Default)]
pub struct DhtContentProvider;

#[async_trait]
impl ContentProvider for DhtContentProvider {
    async fn announce(&self, _cid: &ContentId) -> DfsResult<()> {
        Ok(())
    }
    async fn providers(&self, _cid: &ContentId) -> DfsResult<Vec<String>> {
        Ok(vec![])
    }
}

/// Content replication controller (no-op placeholder).
#[derive(Debug, Default)]
pub struct ContentReplicator {
    level: ReplicationLevel,
}

impl ContentReplicator {
    pub fn new(level: ReplicationLevel) -> Self {
        Self { level }
    }

    /// Best-effort pin/replicate according to desired level.
    pub async fn replicate(&self, dfs: &dyn DistributedFileSystem, cid: &ContentId) -> DfsResult<()> {
        match self.level {
            ReplicationLevel::None => Ok(()),
            _ => dfs.pin(cid).await,
        }
    }
}

/// Concrete DFS: local CAS + optional IPFS HTTP.
#[derive(Clone)]
pub struct IpfsDfs {
    cfg: DfsConfig,
    #[cfg(feature = "ipfs-http")]
    client: Option<Client>,
    #[cfg(feature = "ipfs-http")]
    api_base: Option<Url>,
}

impl IpfsDfs {
    pub fn new(cfg: DfsConfig) -> Self {
        #[cfg(feature = "ipfs-http")]
        {
            let client = reqwest::Client::builder().tcp_nodelay(true).build().ok();
            let api_base = cfg.ipfs_api.as_ref().and_then(|s| Url::parse(s).ok());
            Self { cfg, client, api_base }
        }
        #[cfg(not(feature = "ipfs-http"))]
        {
            Self { cfg }
        }
    }

    fn local_path_for(&self, cid: &ContentId) -> PathBuf {
        self.cfg.local_dir.join(cid.to_path_component())
    }

    async fn ensure_local_dir<P: AsRef<Path>>(&self, p: P) -> DfsResult<()> {
        fs::create_dir_all(p.as_ref()).await?;
        Ok(())
    }

    #[cfg(feature = "ipfs-http")]
    async fn ipfs_add(&self, data: &[u8]) -> DfsResult<String> {
        let (client, base) = match (&self.client, &self.api_base) {
            (Some(c), Some(b)) => (c, b),
            _ => return Err(DfsError::IpfsDisabled),
        };

        let url = base
            .join("api/v0/add")
            .map_err(|e| DfsError::Other(format!("invalid URL: {e}")))?;
        let part = reqwest::multipart::Part::bytes(data.to_vec()).file_name("data.bin");
        let form = reqwest::multipart::Form::new().part("file", part);

        let resp = client.post(url).query(&[("pin", "true")]).multipart(form).send().await?;

        let v: serde_json::Value = resp.json().await?;
        let Some(hash) = v.get("Hash").and_then(|h| h.as_str()) else {
            return Err(DfsError::Other("IPFS add response missing Hash".into()));
        };
        Ok(hash.to_owned())
    }

    #[cfg(feature = "ipfs-http")]
    async fn ipfs_cat(&self, cid: &str) -> DfsResult<Vec<u8>> {
        let (client, base) = match (&self.client, &self.api_base) {
            (Some(c), Some(b)) => (c, b),
            _ => return Err(DfsError::IpfsDisabled),
        };
        let url = base
            .join("api/v0/cat")
            .map_err(|e| DfsError::Other(format!("invalid URL: {e}")))?;
        let resp = client.get(url).query(&[("arg", cid)]).send().await?;
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }
}

#[async_trait]
impl DistributedFileSystem for IpfsDfs {
    async fn put(&self, data: Vec<u8>) -> DfsResult<ContentId> {
        self.ensure_local_dir(&self.cfg.local_dir).await?;

        // Always compute CID and write locally first.
        let cid = ContentId::from_data(&data)?;
        let path = self.local_path_for(&cid);
        if !path.exists() {
            let mut f = fs::File::create(&path).await?;
            f.write_all(&data).await?;
            f.flush().await?;
        }

        // Optionally attempt to add to IPFS (best-effort).
        #[cfg(feature = "ipfs-http")]
        if self.api_base.is_some() {
            let _ = self.ipfs_add(&data).await;
        }

        // Respect replication policy (best-effort pin)
        if matches!(
            self.cfg.replication,
            ReplicationLevel::Low | ReplicationLevel::Medium | ReplicationLevel::High
        ) {
            let _ = self.pin(&cid).await;
        }

        Ok(cid)
    }

    async fn get(&self, cid: &ContentId) -> DfsResult<Vec<u8>> {
        let path = self.local_path_for(cid);
        if path.exists() {
            return Ok(fs::read(path).await?);
        }

        // Fallback to IPFS if available
        #[cfg(feature = "ipfs-http")]
        if self.api_base.is_some() {
            if let Ok(bytes) = self.ipfs_cat(cid.as_str()).await {
                // Cache locally
                self.ensure_local_dir(&self.cfg.local_dir).await?;
                let path = self.local_path_for(cid);
                let mut f = fs::File::create(&path).await?;
                f.write_all(&bytes).await?;
                f.flush().await?;
                return Ok(bytes);
            }
        }

        Err(DfsError::NotFound(cid.to_string()))
    }

    async fn pin(&self, _cid: &ContentId) -> DfsResult<()> {
        // With local CAS, "pin" is implicit. If IPFS exists we could call `pin/add`,
        // but `/api/v0/add?pin=true` already pins on upload. No-op here.
        Ok(())
    }

    async fn unpin(&self, _cid: &ContentId) -> DfsResult<()> {
        Ok(())
    }
}
