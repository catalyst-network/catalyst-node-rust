//! # Catalyst Storage
//!
//! Storage layer for Catalyst Network providing:
//! - RocksDB integration for persistent state
//! - Atomic transaction batching
//! - State snapshots and rollbacks
//! - Database migrations
//! - Performance optimization and monitoring
//!
//! ## Usage
//!
//! ```rust
//! use catalyst_storage::{StorageManager, StorageConfig};
//! use catalyst_utils::state::StateManager;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = StorageConfig::default();
//!     let storage = StorageManager::new(config).await?;
//!     
//!     // Use as StateManager
//!     storage.set_state(b"key", b"value".to_vec()).await?;
//!     let value = storage.get_state(b"key").await?;
//!     
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod engine;
pub mod error;
pub mod manager;
pub mod migration;
pub mod snapshot;
pub mod transaction;
pub mod utils;

// Re-exports
pub use config::{StorageConfig, ColumnFamilyConfig};
pub use engine::{RocksEngine, ColumnFamily};
pub use error::{StorageError, StorageResult};
pub use manager::StorageManager;
pub use migration::{Migration, MigrationManager};
pub use snapshot::{Snapshot, SnapshotManager};
pub use transaction::{StorageTransaction, TransactionBatch};

// Metrics integration
#[cfg(feature = "metrics")]
pub mod metrics;

use catalyst_utils::{CatalystResult, CatalystError};

/// Initialize the storage subsystem with metrics
pub async fn init_storage(config: StorageConfig) -> CatalystResult<StorageManager> {
    #[cfg(feature = "metrics")]
    {
        metrics::register_storage_metrics()
            .map_err(|e| CatalystError::Storage(format!("Failed to register metrics: {}", e)))?;
    }
    
    StorageManager::new(config)
        .await
        .map_err(|e| CatalystError::Storage(format!("Failed to initialize storage: {}", e)))
}

/// Storage version for migration compatibility
pub const STORAGE_VERSION: u32 = 1;

/// Default column families for Catalyst Network
pub const DEFAULT_COLUMN_FAMILIES: &[&str] = &[
    "accounts",           // Account states
    "transactions",       // Transaction data
    "consensus",          // Consensus state
    "metadata",          // System metadata
    "contracts",         // Smart contract data
    "dfs_refs",          // DFS reference data
];

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[tokio::test]
    async fn test_storage_initialization() {
        let temp_dir = TempDir::new().unwrap();
        let config = StorageConfig {
            data_dir: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        
        let storage = init_storage(config).await.unwrap();
        assert!(storage.is_healthy().await);
    }
}