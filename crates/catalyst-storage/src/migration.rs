//! Database migration system for schema evolution

use std::path::Path;

use catalyst_utils::{
    // Import macros from root level
    log_info, log_warn, log_error,
    
    // Import types normally
    Hash,
    // Only import types from modules, not macros
    logging::LogCategory,
    utils::current_timestamp,
};

use crate::{StorageError, StorageResult, RocksEngine};

use std::sync::Arc;
use serde::{Serialize, Deserialize};
use async_trait::async_trait;

/// Database migration trait
#[async_trait]
pub trait Migration: Send + Sync {
    /// Get migration version
    fn version(&self) -> u32;
    
    /// Get migration name
    fn name(&self) -> &str;
    
    /// Get migration description
    fn description(&self) -> &str;
    
    /// Execute the migration
    async fn up(&self, engine: &RocksEngine) -> StorageResult<()>;
    
    /// Rollback the migration (optional)
    async fn down(&self, engine: &RocksEngine) -> StorageResult<()> {
        Err(StorageError::migration(format!("Migration '{}' does not support rollback", self.name())))
    }
    
    /// Check if migration can be safely applied
    async fn can_apply(&self, engine: &RocksEngine) -> StorageResult<bool> {
        // Default implementation - always allow
        let _ = engine;
        Ok(true)
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
        self.add_migration(Box::new(InitialSchemaV1));
        // Add more migrations here as the schema evolves
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
                log_info!(LogCategory::Storage, "Migration tracking already initialized");
            }
            None => {
                log_info!(LogCategory::Storage, "Initializing migration tracking");
                
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
        log_info!(LogCategory::Storage, "Current migration version: {}", current_version);
        
        // Find pending migrations
        let pending_migrations: Vec<_> = self.migrations
            .iter()
            .filter(|m| m.version() > current_version)
            .collect();
        
        if pending_migrations.is_empty() {
            log_info!(LogCategory::Storage, "No pending migrations");
            return Ok(());
        }
        
        log_info!(
            LogCategory::Storage,
            "Found {} pending migrations",
            pending_migrations.len()
        );
        
        // Apply each migration
        for migration in pending_migrations {
            self.apply_migration(migration.as_ref()).await?;
        }
        
        log_info!(LogCategory::Storage, "All migrations applied successfully");
        Ok(())
    }
    
    /// Apply a single migration
    async fn apply_migration(&self, migration: &dyn Migration) -> StorageResult<()> {
        log_info!(
            LogCategory::Storage,
            "Applying migration v{}: {}",
            migration.version(),
            migration.name()
        );
        
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
                    applied_at: current_timestamp(),
                    checksum,
                };
                
                self.add_to_history(record).await?;
                
                log_info!(
                    LogCategory::Storage,
                    "Migration v{} applied successfully",
                    migration.version()
                );
                
                Ok(())
            }
            Err(e) => {
                log_error!(
                    LogCategory::Storage,
                    "Migration v{} failed: {}",
                    migration.version(),
                    e
                );
                Err(e)
            }
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
        
        log_warn!(
            LogCategory::Storage,
            "Rolling back from version {} to version {}",
            current_version,
            target_version
        );
        
        // Find migrations to rollback (in reverse order)
        let rollback_migrations: Vec<_> = self.migrations
            .iter()
            .filter(|m| m.version() > target_version && m.version() <= current_version)
            .rev()
            .collect();
        
        // Apply rollbacks
        for migration in rollback_migrations {
            log_info!(
                LogCategory::Storage,
                "Rolling back migration v{}: {}",
                migration.version(),
                migration.name()
            );
            
            migration.down(&self.engine).await?;
            
            log_info!(
                LogCategory::Storage,
                "Migration v{} rolled back successfully",
                migration.version()
            );
        }
        
        // Update version
        self.set_current_version(target_version).await?;
        
        log_info!(
            LogCategory::Storage,
            "Rollback to version {} completed",
            target_version
        );
        
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

// Built-in migrations

/// Initial schema migration (v1)
struct InitialSchemaV1;

#[async_trait]
impl Migration for InitialSchemaV1 {
    fn version(&self) -> u32 {
        1
    }
    
    fn name(&self) -> &str {
        "initial_schema"
    }
    
    fn description(&self) -> &str {
        "Create initial database schema with all required column families"
    }
    
    async fn up(&self, engine: &RocksEngine) -> StorageResult<()> {
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
        
        for (cf_name, cf_config) in required_cfs {
            if !engine.cf_names().contains(&cf_name.to_string()) {
                log_info!(LogCategory::Storage, "Creating column family: {}", cf_name);
                engine.create_cf(cf_name, cf_config)?;
            }
        }
        
        // Set initial metadata values
        engine.put("metadata", b"schema_version", b"1")?;
        engine.put("metadata", b"created_at", &current_timestamp().to_le_bytes())?;
        
        Ok(())
    }
    
    async fn can_apply(&self, engine: &RocksEngine) -> StorageResult<bool> {
        // Check if this is a fresh database or if we can safely apply
        let existing_cfs = engine.cf_names();
        
        // If we already have the required column families, don't apply
        let required_cfs = ["accounts", "transactions", "consensus", "metadata", "contracts", "dfs_refs"];
        let has_all_cfs = required_cfs.iter().all(|cf| existing_cfs.contains(&cf.to_string()));
        
        Ok(!has_all_cfs)
    }
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
        
        let migration = &InitialSchemaV1;
        let checksum1 = manager.calculate_migration_checksum(migration);
        let checksum2 = manager.calculate_migration_checksum(migration);
        
        // Checksums should be consistent
        assert_eq!(checksum1, checksum2);
        assert!(!checksum1.is_empty());
    }
    
    // Test custom migration
    struct TestMigration;
    
    #[async_trait]
    impl Migration for TestMigration {
        fn version(&self) -> u32 {
            2
        }
        
        fn name(&self) -> &str {
            "test_migration"
        }
        
        fn description(&self) -> &str {
            "A test migration for unit tests"
        }
        
        async fn up(&self, engine: &RocksEngine) -> StorageResult<()> {
            engine.put("metadata", b"test_migration_applied", b"true")?;
            Ok(())
        }
        
        async fn down(&self, engine: &RocksEngine) -> StorageResult<()> {
            engine.delete("metadata", b"test_migration_applied")?;
            Ok(())
        }
    }
    
    #[tokio::test]
    async fn test_custom_migration() {
        let engine = create_test_engine();
        let mut manager = MigrationManager::new(engine.clone()).await.unwrap();
        
        // Add custom migration
        manager.add_migration(Box::new(TestMigration));
        
        // Run all migrations
        manager.run_migrations().await.unwrap();
        
        // Check that both migrations were applied
        let version = manager.get_current_version().await.unwrap();
        assert_eq!(version, 2);
        
        // Check that test migration actually ran
        let test_value = engine.get("metadata", b"test_migration_applied").unwrap();
        assert_eq!(test_value, Some(b"true".to_vec()));
    }
    
    #[tokio::test]
    async fn test_migration_rollback() {
        let engine = create_test_engine();
        let mut manager = MigrationManager::new(engine.clone()).await.unwrap();
        
        // Add custom migration that supports rollback
        manager.add_migration(Box::new(TestMigration));
        
        // Run all migrations
        manager.run_migrations().await.unwrap();
        assert_eq!(manager.get_current_version().await.unwrap(), 2);
        
        // Rollback to version 1
        manager.rollback_to_version(1).await.unwrap();
        assert_eq!(manager.get_current_version().await.unwrap(), 1);
        
        // Check that test migration was rolled back
        let test_value = engine.get("metadata", b"test_migration_applied").unwrap();
        assert_eq!(test_value, None);
    }
    
    #[tokio::test]
    async fn test_migration_rollback_error() {
        let engine = create_test_engine();
        let manager = MigrationManager::new(engine).await.unwrap();
        
        // Try to rollback to a higher version
        let result = manager.rollback_to_version(5).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not lower than current"));
    }
    
    #[tokio::test]
    async fn test_no_pending_migrations() {
        let engine = create_test_engine();
        let manager = MigrationManager::new(engine).await.unwrap();
        
        // Run migrations first
        manager.run_migrations().await.unwrap();
        
        // Running again should be a no-op
        manager.run_migrations().await.unwrap();
        
        let status = manager.get_status().await.unwrap();
        assert!(status.pending_migrations.is_empty());
    }
}