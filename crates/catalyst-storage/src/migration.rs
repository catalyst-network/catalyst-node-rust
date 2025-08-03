//! Database migration system for schema evolution

use crate::{RocksEngine, StorageError, StorageResult};
use std::sync::Arc;
use serde::{Serialize, Deserialize};
use std::future::Future;
use std::pin::Pin;

/// Database migration trait - made object-safe by using boxed futures
pub trait Migration: Send + Sync {
    /// Get migration version
    fn version(&self) -> u32;
    
    /// Get migration name
    fn name(&self) -> &str;
    
    /// Get migration description
    fn description(&self) -> &str;
    
    /// Execute the migration
    fn up<'a>(&'a self, engine: &'a RocksEngine) -> Pin<Box<dyn Future<Output = StorageResult<()>> + Send + 'a>>;
    
    /// Rollback the migration (optional)
    fn down<'a>(&'a self, engine: &'a RocksEngine) -> Pin<Box<dyn Future<Output = StorageResult<()>> + Send + 'a>> {
        Box::pin(async move {
            let _ = engine;
            Err(StorageError::migration(format!("Migration '{}' does not support rollback", self.name())))
        })
    }
    
    /// Check if migration can be safely applied
    fn can_apply<'a>(&'a self, engine: &'a RocksEngine) -> Pin<Box<dyn Future<Output = StorageResult<bool>> + Send + 'a>> {
        Box::pin(async move {
            let _ = engine;
            Ok(true)
        })
    }
}

/// Migration record for tracking applied migrations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    pub version: u32,
    pub name: String,
    pub description: String,
    pub applied_at: u64,
    pub checksum: String,
}

/// Concrete migration implementation
pub struct ConcreteMigration {
    version: u32,
    name: String,
    description: String,
    up_fn: Box<dyn Fn(&RocksEngine) -> Pin<Box<dyn Future<Output = StorageResult<()>> + Send + '_>> + Send + Sync>,
    down_fn: Option<Box<dyn Fn(&RocksEngine) -> Pin<Box<dyn Future<Output = StorageResult<()>> + Send + '_>> + Send + Sync>>,
    can_apply_fn: Option<Box<dyn Fn(&RocksEngine) -> Pin<Box<dyn Future<Output = StorageResult<bool>> + Send + '_>> + Send + Sync>>,
}

impl ConcreteMigration {
    pub fn new(
        version: u32,
        name: String,
        description: String,
        up_fn: Box<dyn Fn(&RocksEngine) -> Pin<Box<dyn Future<Output = StorageResult<()>> + Send + '_>> + Send + Sync>,
    ) -> Self {
        Self {
            version,
            name,
            description,
            up_fn,
            down_fn: None,
            can_apply_fn: None,
        }
    }
    
    pub fn with_down(
        mut self,
        down_fn: Box<dyn Fn(&RocksEngine) -> Pin<Box<dyn Future<Output = StorageResult<()>> + Send + '_>> + Send + Sync>,
    ) -> Self {
        self.down_fn = Some(down_fn);
        self
    }
    
    pub fn with_can_apply(
        mut self,
        can_apply_fn: Box<dyn Fn(&RocksEngine) -> Pin<Box<dyn Future<Output = StorageResult<bool>> + Send + '_>> + Send + Sync>,
    ) -> Self {
        self.can_apply_fn = Some(can_apply_fn);
        self
    }
}

impl Migration for ConcreteMigration {
    fn version(&self) -> u32 {
        self.version
    }
    
    fn name(&self) -> &str {
        &self.name
    }
    
    fn description(&self) -> &str {
        &self.description
    }
    
    fn up<'a>(&'a self, engine: &'a RocksEngine) -> Pin<Box<dyn Future<Output = StorageResult<()>> + Send + 'a>> {
        (self.up_fn)(engine)
    }
    
    fn down<'a>(&'a self, engine: &'a RocksEngine) -> Pin<Box<dyn Future<Output = StorageResult<()>> + Send + 'a>> {
        if let Some(down_fn) = &self.down_fn {
            down_fn(engine)
        } else {
            Box::pin(async move {
                Err(StorageError::migration(format!("Migration '{}' does not support rollback", self.name)))
            })
        }
    }
    
    fn can_apply<'a>(&'a self, engine: &'a RocksEngine) -> Pin<Box<dyn Future<Output = StorageResult<bool>> + Send + 'a>> {
        if let Some(can_apply_fn) = &self.can_apply_fn {
            can_apply_fn(engine)
        } else {
            Box::pin(async move { Ok(true) })
        }
    }
}

/// Migration manager
pub struct MigrationManager {
    engine: Arc<RocksEngine>,
    migrations: Vec<Box<dyn Migration>>,
}

impl MigrationManager {
    /// Create a new migration manager
    pub async fn new(engine: Arc<RocksEngine>) -> StorageResult<Self> {
        let mut manager = Self {
            engine,
            migrations: Vec::new(),
        };
        
        // Register built-in migrations
        manager.register_builtin_migrations();
        
        // Initialize migration tracking
        manager.initialize_migration_tracking().await?;
        
        Ok(manager)
    }
    
    /// Register built-in migrations
    fn register_builtin_migrations(&mut self) {
        // Initial schema migration
        let initial_migration = ConcreteMigration::new(
            1,
            "initial_schema".to_string(),
            "Create initial database schema with all required column families".to_string(),
            Box::new(|engine| {
                Box::pin(async move {
                    use crate::config::ColumnFamilyConfig;
                    
                    // Ensure all required column families exist
                    let required_cfs = &[
                        ("accounts", ColumnFamilyConfig::for_accounts()),
                        ("transactions", ColumnFamilyConfig::for_transactions()),
                        ("consensus", ColumnFamilyConfig::for_consensus()),
                        ("metadata", ColumnFamilyConfig::for_metadata()),
                        ("contracts", ColumnFamilyConfig::for_contracts()),
                        ("dfs_refs", ColumnFamilyConfig::for_dfs_refs()),
                    ];
                    
                    for (cf_name, _cf_config) in required_cfs {
                        if !engine.cf_names().contains(&cf_name.to_string()) {
                            eprintln!("Column family '{}' needs to be created", cf_name);
                            let opts = rocksdb::Options::default();
                            let _ = engine.create_cf(cf_name, &opts); // Ignore error since we expect it to fail
                        }
                    }
                    
                    // Set initial metadata values
                    engine.put("metadata", b"schema_version", b"1")?;
                    engine.put("metadata", b"created_at", &catalyst_utils::utils::current_timestamp().to_le_bytes())?;
                    
                    Ok(())
                })
            }),
        )
        .with_can_apply(Box::new(|engine| {
            Box::pin(async move {
                // Check if this is a fresh database or if we can safely apply
                let existing_cfs = engine.cf_names();
                
                // If we already have the required column families, don't apply
                let required_cfs = ["accounts", "transactions", "consensus", "metadata", "contracts", "dfs_refs"];
                let has_all_cfs = required_cfs.iter().all(|cf| existing_cfs.contains(&cf.to_string()));
                
                Ok(!has_all_cfs)
            })
        }));
        
        self.add_migration(Box::new(initial_migration));
    }
    
    /// Add a migration
    pub fn add_migration(&mut self, migration: Box<dyn Migration>) {
        self.migrations.push(migration);
        
        // Sort by version
        self.migrations.sort_by_key(|m| m.version());
    }
    
    /// Initialize migration tracking table
    async fn initialize_migration_tracking(&self) -> StorageResult<()> {
        // Check if migration metadata exists
        let migration_key = b"migration_version";
        
        match self.engine.get("metadata", migration_key)? {
            Some(_) => {
                // Migration tracking already initialized
            }
            None => {
                // Set initial version
                let version_bytes = 0u32.to_le_bytes();
                self.engine.put("metadata", migration_key, &version_bytes)?;
                
                // Create empty migration history
                let history_key = b"migration_history";
                let empty_history: Vec<MigrationRecord> = Vec::new();
                let history_data = serde_json::to_vec(&empty_history)
                    .map_err(|e| StorageError::serialization(e.to_string()))?;
                
                self.engine.put("metadata", history_key, &history_data)?;
            }
        }
        
        Ok(())
    }
    
    /// Get current migration version
    async fn get_current_version(&self) -> StorageResult<u32> {
        let migration_key = b"migration_version";
        
        match self.engine.get("metadata", migration_key)? {
            Some(data) => {
                if data.len() != 4 {
                    return Err(StorageError::migration("Invalid migration version data".to_string()));
                }
                
                let version = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                Ok(version)
            }
            None => Ok(0), // No migrations applied yet
        }
    }
    
    /// Set current migration version
    async fn set_current_version(&self, version: u32) -> StorageResult<()> {
        let migration_key = b"migration_version";
        let version_bytes = version.to_le_bytes();
        
        self.engine.put("metadata", migration_key, &version_bytes)?;
        Ok(())
    }
    
    /// Get migration history
    async fn get_migration_history(&self) -> StorageResult<Vec<MigrationRecord>> {
        let history_key = b"migration_history";
        
        match self.engine.get("metadata", history_key)? {
            Some(data) => {
                let history: Vec<MigrationRecord> = serde_json::from_slice(&data)
                    .map_err(|e| StorageError::serialization(e.to_string()))?;
                Ok(history)
            }
            None => Ok(Vec::new()),
        }
    }
    
    /// Add migration to history
    async fn add_to_history(&self, record: MigrationRecord) -> StorageResult<()> {
        let mut history = self.get_migration_history().await?;
        history.push(record);
        
        let history_key = b"migration_history";
        let history_data = serde_json::to_vec(&history)
            .map_err(|e| StorageError::serialization(e.to_string()))?;
        
        self.engine.put("metadata", history_key, &history_data)?;
        Ok(())
    }
    
    /// Run all pending migrations
    pub async fn run_migrations(&self) -> StorageResult<()> {
        let current_version = self.get_current_version().await?;
        
        // Find pending migrations
        let mut pending_migrations = Vec::new();
        for migration in &self.migrations {
            if migration.version() > current_version {
                pending_migrations.push(migration.as_ref());
            }
        }
        
        if pending_migrations.is_empty() {
            return Ok(());
        }
        
        // Apply each migration
        for migration in pending_migrations {
            self.apply_migration(migration).await?;
        }
        
        Ok(())
    }
    
    /// Apply a single migration
    async fn apply_migration(&self, migration: &dyn Migration) -> StorageResult<()> {
        // Check if migration can be applied
        if !migration.can_apply(&self.engine).await? {
            return Err(StorageError::migration(format!(
                "Migration '{}' cannot be safely applied",
                migration.name()
            )));
        }
        
        // Calculate migration checksum
        let checksum = self.calculate_migration_checksum(migration);
        
        // Apply the migration
        match migration.up(&self.engine).await {
            Ok(_) => {
                // Update version
                self.set_current_version(migration.version()).await?;
                
                // Add to history
                let record = MigrationRecord {
                    version: migration.version(),
                    name: migration.name().to_string(),
                    description: migration.description().to_string(),
                    applied_at: catalyst_utils::utils::current_timestamp(),
                    checksum,
                };
                
                self.add_to_history(record).await?;
                
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
    
    /// Calculate a checksum for a migration
    fn calculate_migration_checksum(&self, migration: &dyn Migration) -> String {
        use sha2::{Sha256, Digest};
        
        let mut hasher = Sha256::new();
        hasher.update(migration.version().to_le_bytes());
        hasher.update(migration.name().as_bytes());
        hasher.update(migration.description().as_bytes());
        
        hex::encode(hasher.finalize())
    }
    
    /// Rollback to a specific version
    pub async fn rollback_to_version(&self, target_version: u32) -> StorageResult<()> {
        let current_version = self.get_current_version().await?;
        
        if target_version >= current_version {
            return Err(StorageError::migration(format!(
                "Target version {} is not lower than current version {}",
                target_version,
                current_version
            )));
        }
        
        // Find migrations to rollback (in reverse order)
        let mut rollback_migrations = Vec::new();
        for migration in self.migrations.iter().rev() {
            if migration.version() > target_version && migration.version() <= current_version {
                rollback_migrations.push(migration.as_ref());
            }
        }
        
        // Apply rollbacks
        for migration in rollback_migrations {
            migration.down(&self.engine).await?;
        }
        
        // Update version
        self.set_current_version(target_version).await?;
        
        Ok(())
    }
    
    /// Get migration status
    pub async fn get_status(&self) -> StorageResult<MigrationStatus> {
        let current_version = self.get_current_version().await?;
        let history = self.get_migration_history().await?;
        
        let pending_migrations: Vec<_> = self.migrations
            .iter()
            .filter(|m| m.version() > current_version)
            .map(|m| PendingMigration {
                version: m.version(),
                name: m.name().to_string(),
                description: m.description().to_string(),
            })
            .collect();
        
        Ok(MigrationStatus {
            current_version,
            latest_available_version: self.migrations.iter().map(|m| m.version()).max().unwrap_or(0),
            applied_migrations: history,
            pending_migrations,
        })
    }
}

/// Migration status information
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    pub current_version: u32,
    pub latest_available_version: u32,
    pub applied_migrations: Vec<MigrationRecord>,
    pub pending_migrations: Vec<PendingMigration>,
}

/// Information about a pending migration
#[derive(Debug, Clone)]
pub struct PendingMigration {
    pub version: u32,
    pub name: String,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StorageConfig, RocksEngine};
    use tempfile::TempDir;
    
    fn create_test_engine() -> Arc<RocksEngine> {
        let temp_dir = TempDir::new().unwrap();
        let mut config = StorageConfig::default();
        config.data_dir = temp_dir.path().to_path_buf();
        Arc::new(RocksEngine::new(config).unwrap())
    }
    
    #[tokio::test]
    async fn test_migration_manager_creation() {
        let engine = create_test_engine();
        let manager = MigrationManager::new(engine).await.unwrap();
        
        assert!(!manager.migrations.is_empty());
    }
    
    #[tokio::test]
    async fn test_initial_migration_version() {
        let engine = create_test_engine();
        let manager = MigrationManager::new(engine).await.unwrap();
        
        let version = manager.get_current_version().await.unwrap();
        assert_eq!(version, 0); // No migrations applied initially
    }
    
    #[tokio::test]
    async fn test_run_migrations() {
        let engine = create_test_engine();
        let manager = MigrationManager::new(engine).await.unwrap();
        
        // Run migrations
        manager.run_migrations().await.unwrap();
        
        // Check that version was updated
        let version = manager.get_current_version().await.unwrap();
        assert_eq!(version, 1); // Initial schema migration applied
    }
    
    #[tokio::test]
    async fn test_migration_history() {
        let engine = create_test_engine();
        let manager = MigrationManager::new(engine).await.unwrap();
        
        // Initially empty
        let history = manager.get_migration_history().await.unwrap();
        assert!(history.is_empty());
        
        // Run migrations
        manager.run_migrations().await.unwrap();
        
        // Check history
        let history = manager.get_migration_history().await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].version, 1);
        assert_eq!(history[0].name, "initial_schema");
    }
    
    #[tokio::test]
    async fn test_migration_status() {
        let engine = create_test_engine();
        let manager = MigrationManager::new(engine).await.unwrap();
        
        // Get status before migrations
        let status = manager.get_status().await.unwrap();
        assert_eq!(status.current_version, 0);
        assert_eq!(status.latest_available_version, 1);
        assert!(status.applied_migrations.is_empty());
        assert_eq!(status.pending_migrations.len(), 1);
        
        // Run migrations
        manager.run_migrations().await.unwrap();
        
        // Get status after migrations
        let status = manager.get_status().await.unwrap();
        assert_eq!(status.current_version, 1);
        assert_eq!(status.latest_available_version, 1);
        assert_eq!(status.applied_migrations.len(), 1);
        assert!(status.pending_migrations.is_empty());
    }
    
    #[tokio::test]
    async fn test_migration_checksum() {
        let engine = create_test_engine();
        let manager = MigrationManager::new(engine).await.unwrap();
        
        let migration = &manager.migrations[0];
        let checksum1 = manager.calculate_migration_checksum(migration.as_ref());
        let checksum2 = manager.calculate_migration_checksum(migration.as_ref());
        
        // Checksums should be consistent
        assert_eq!(checksum1, checksum2);
        assert!(!checksum1.is_empty());
    }
}