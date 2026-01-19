use std::path::PathBuf;

use catalyst_dfs::ContentId;
use tokio::fs;

/// Minimal, multi-process-safe, content-addressed store.
///
/// We intentionally avoid `catalyst-dfs`'s `LocalDfsStorage` here because it uses RocksDB for
/// metadata, which takes an exclusive lock and prevents multiple local testnet processes from
/// sharing a single storage directory.
#[derive(Debug, Clone)]
pub struct LocalContentStore {
    root: PathBuf,
}

impl LocalContentStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn content_path_for(&self, cid_str: &str) -> PathBuf {
        let prefix = &cid_str[0..2.min(cid_str.len())];
        self.root.join("content").join(prefix).join(cid_str)
    }

    pub async fn put(&self, bytes: Vec<u8>) -> Result<String, String> {
        let cid = ContentId::from_data(&bytes).map_err(|e| e.to_string())?;
        let cid_str = cid.to_string();

        let path = self.content_path_for(&cid_str);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("Failed to create DFS dir: {e}"))?;
        }

        // Write only if missing.
        if fs::metadata(&path).await.is_err() {
            fs::write(&path, &bytes)
                .await
                .map_err(|e| format!("Failed to write DFS object: {e}"))?;
        }

        Ok(cid_str)
    }

    pub async fn get(&self, cid: &str) -> Result<Vec<u8>, String> {
        let path = self.content_path_for(cid);
        fs::read(&path)
            .await
            .map_err(|e| format!("Failed to read DFS object {cid}: {e}"))
    }
}

