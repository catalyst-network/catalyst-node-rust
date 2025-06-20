//! Storage configuration structures

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use crate::ConfigError;

/// Storage configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// Data directory path
    pub data_dir: PathBuf,
    /// Database backend
    pub backend: StorageBackend,
    /// Cache configuration
    pub cache: CacheConfig,
    /// DFS configuration
    pub dfs: DfsConfig,
    /// Pruning configuration
    pub pruning: PruningConfig,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("./data"),
            backend: StorageBackend::RocksDb,
            cache: CacheConfig::default(),
            dfs: DfsConfig::default(),
            pruning: PruningConfig::default(),
        }
    }
}

/// Storage backend types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StorageBackend {
    RocksDb,
    Sled,
    Memory,
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Cache size in MB
    pub size_mb: usize,
    /// Enable compression
    pub enable_compression: bool,
    /// Cache eviction policy
    pub eviction_policy: EvictionPolicy,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            size_mb: 256,
            enable_compression: true,
            eviction_policy: EvictionPolicy::Lru,
        }
    }
}

/// Cache eviction policies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionPolicy {
    Lru,  // Least Recently Used
    Lfu,  // Least Frequently Used
    Fifo, // First In, First Out
}

/// Distributed File System configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DfsConfig {
    /// Enable DFS functionality
    pub enabled: bool,
    /// DFS storage directory
    pub storage_dir: PathBuf,
    /// Maximum storage size in GB
    pub max_storage_gb: u64,
    /// Enable garbage collection
    pub enable_gc: bool,
    /// GC interval in hours
    pub gc_interval_hours: u64,
    /// Replication factor
    pub replication_factor: usize,
}

impl Default for DfsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            storage_dir: PathBuf::from("./dfs"),
            max_storage_gb: 100,
            enable_gc: true,
            gc_interval_hours: 24,
            replication_factor: 3,
        }
    }
}

/// Data pruning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruningConfig {
    /// Enable pruning of old data
    pub enabled: bool,
    /// Keep recent blocks count
    pub keep_recent_blocks: u64,
    /// Keep finalized blocks for days
    pub keep_finalized_days: u64,
    /// Pruning interval in hours
    pub pruning_interval_hours: u64,
}

impl Default for PruningConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Conservative default
            keep_recent_blocks: 1000,
            keep_finalized_days: 30,
            pruning_interval_hours: 24,
        }
    }
}

impl StorageConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.data_dir.exists() {
            std::fs::create_dir_all(&self.data_dir)
                .map_err(|e| ConfigError::Invalid(
                    format!("Cannot create data directory: {}", e)
                ))?;
        }

        self.cache.validate()?;
        self.dfs.validate()?;
        self.pruning.validate()?;

        Ok(())
    }
}

impl CacheConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.size_mb == 0 {
            return Err(ConfigError::Invalid(
                "Cache size cannot be zero".to_string()
            ));
        }

        Ok(())
    }
}

impl DfsConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.enabled {
            if !self.storage_dir.exists() {
                std::fs::create_dir_all(&self.storage_dir)
                    .map_err(|e| ConfigError::Invalid(
                        format!("Cannot create DFS directory: {}", e)
                    ))?;
            }

            if self.max_storage_gb == 0 {
                return Err(ConfigError::Invalid(
                    "DFS max storage cannot be zero".to_string()
                ));
            }

            if self.replication_factor == 0 {
                return Err(ConfigError::Invalid(
                    "Replication factor cannot be zero".to_string()
                ));
            }
        }

        Ok(())
    }
}

impl PruningConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.enabled {
            if self.keep_recent_blocks == 0 {
                return Err(ConfigError::Invalid(
                    "Keep recent blocks cannot be zero".to_string()
                ));
            }

            if self.keep_finalized_days == 0 {
                return Err(ConfigError::Invalid(
                    "Keep finalized days cannot be zero".to_string()
                ));
            }
        }

        Ok(())
    }
}