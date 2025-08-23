use crate::loader::FileLoader;
use crate::{CatalystConfig, ConfigError, ConfigResult};
use std::path::{Path, PathBuf};

/// Configuration utility functions
pub struct ConfigUtils;

impl ConfigUtils {
    /// Find configuration file in standard locations
    pub fn find_config_file(filename: &str) -> ConfigResult<PathBuf> {
        let search_paths = vec![
            // Current directory
            PathBuf::from("."),
            // Config subdirectory
            PathBuf::from("config"),
            PathBuf::from("configs"),
            // User config directory
            dirs::config_dir()
                .map(|d| d.join("catalyst"))
                .unwrap_or_else(|| PathBuf::from(".catalyst")),
            // System config directory
            PathBuf::from("/etc/catalyst"),
            // Application directory
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
                .map(|p| p.join("config"))
                .unwrap_or_else(|| PathBuf::from("config")),
        ];

        for search_path in search_paths.into_iter() {
            let config_path = search_path.join(filename);
            if config_path.exists() && config_path.is_file() {
                return Ok(config_path);
            }

            // Also try with common extensions
            for ext in &["toml", "json", "yaml"] {
                let config_with_ext = search_path.join(format!("{}.{}", filename, ext));
                if config_with_ext.exists() && config_with_ext.is_file() {
                    return Ok(config_with_ext);
                }
            }
        }

        Err(ConfigError::FileNotFound(format!(
            "Configuration file '{}' not found in standard locations",
            filename
        )))
    }

    /// Get default config file for a network
    pub fn get_default_config_file(network: &str) -> String {
        format!("{}.toml", network)
    }

    /// Create configuration directory if it doesn't exist
    pub fn ensure_config_directory<P: AsRef<Path>>(path: P) -> ConfigResult<()> {
        let path = path.as_ref();
        if !path.exists() {
            std::fs::create_dir_all(path).map_err(|e| ConfigError::Io(e))?;
        } else if !path.is_dir() {
            return Err(ConfigError::ValidationFailed(format!(
                "Path exists but is not a directory: {}",
                path.display()
            )));
        }
        Ok(())
    }

    /// Create data directories based on configuration
    pub fn create_data_directories(config: &crate::CatalystConfig) -> ConfigResult<()> {
        // Create storage directories
        Self::ensure_config_directory(&config.storage.database.data_directory)?;
        Self::ensure_config_directory(&config.storage.dfs.cache_directory)?;

        // Create backup directory if backups are enabled
        if config.storage.backup.enabled {
            Self::ensure_config_directory(&config.storage.backup.backup_directory)?;
        }

        // Create metrics storage directory if using local storage
        if let crate::config::StorageBackend::Local = config.metrics.storage.backend {
            Self::ensure_config_directory(&config.metrics.storage.local.directory)?;
        }

        // Create log directories
        for output in &config.logging.outputs {
            if let crate::config::OutputType::File { path, .. } = &output.output_type {
                if let Some(parent) = path.parent() {
                    Self::ensure_config_directory(parent)?;
                }
            }
        }

        Ok(())
    }

    /// Validate file permissions for security-sensitive files
    pub fn validate_file_permissions<P: AsRef<Path>>(path: P) -> ConfigResult<()> {
        let path = path.as_ref();

        if !path.exists() {
            return Ok(()); // File doesn't exist yet
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = path.metadata().map_err(|e| ConfigError::Io(e))?;
            let permissions = metadata.permissions();
            let mode = permissions.mode();

            // Check if file is readable by others (security risk)
            if mode & 0o044 != 0 {
                return Err(ConfigError::ValidationFailed(format!(
                    "Configuration file {} has overly permissive permissions",
                    path.display()
                )));
            }
        }

        Ok(())
    }

    /// Backup existing configuration before overwriting
    pub fn backup_config<P: AsRef<Path>>(config_path: P) -> ConfigResult<PathBuf> {
        let config_path = config_path.as_ref();

        if !config_path.exists() {
            return Err(ConfigError::FileNotFound(config_path.display().to_string()));
        }

        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let backup_path = config_path.with_extension(format!("backup_{}.toml", timestamp));

        std::fs::copy(config_path, &backup_path).map_err(|e| ConfigError::Io(e))?;

        Ok(backup_path)
    }

    /// Get configuration file extension
    pub fn get_file_extension<P: AsRef<Path>>(path: P) -> Option<String> {
        path.as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_lowercase())
    }

    /// Merge two configurations (overlay second onto first)
    pub fn merge_configs(
        base: &mut crate::CatalystConfig,
        overlay: &crate::CatalystConfig,
    ) -> ConfigResult<()> {
        // This is a simplified merge - in practice, you'd want more sophisticated merging
        // For now, we'll just replace major sections if they differ significantly

        // Network settings
        if overlay.network.name != base.network.name {
            base.network = overlay.network.clone();
        }

        // Consensus settings
        if overlay.consensus.cycle_duration_ms != base.consensus.cycle_duration_ms {
            base.consensus = overlay.consensus.clone();
        }

        // Storage settings
        if overlay.storage.database.data_directory != base.storage.database.data_directory {
            base.storage = overlay.storage.clone();
        }

        Ok(())
    }

    /// Generate a minimal configuration template
    pub fn generate_template(network: &str) -> ConfigResult<String> {
        let template = match network {
            "devnet" => include_str!("../configs/devnet.toml"), // Fixed path
            "testnet" => include_str!("../configs/testnet.toml"), // Fixed path
            "mainnet" => include_str!("../configs/mainnet.toml"), // Fixed path
            "example" => include_str!("../configs/example.toml"), // Fixed path
            "custom" => {
                let config = CatalystConfig::default();
                return Ok(toml::to_string_pretty(&config).map_err(|e| {
                    ConfigError::InvalidFormat(format!("Template generation failed: {}", e))
                })?);
            }
            _ => {
                return Err(ConfigError::InvalidNetwork(format!(
                    "Unknown network template: {}",
                    network
                )))
            }
        };
        Ok(template.to_string())
    }

    /// Validate configuration file syntax without loading
    pub fn validate_syntax<P: AsRef<Path>>(path: P) -> ConfigResult<()> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path).map_err(|e| ConfigError::Io(e))?;

        match Self::get_file_extension(path).as_deref() {
            Some("toml") => {
                toml::from_str::<toml::Value>(&content).map_err(|e| ConfigError::Toml(e))?;
            }
            Some("json") => {
                serde_json::from_str::<serde_json::Value>(&content)
                    .map_err(|e| ConfigError::Json(e))?;
            }
            _ => {
                // Try both formats
                if toml::from_str::<toml::Value>(&content).is_err()
                    && serde_json::from_str::<serde_json::Value>(&content).is_err()
                {
                    return Err(ConfigError::InvalidFormat(
                        "File is neither valid TOML nor JSON".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Convert between configuration formats
    pub async fn convert_format<P: AsRef<Path>>(
        input_path: P,
        output_path: P,
        output_format: ConfigFormat,
    ) -> ConfigResult<()> {
        let config = crate::loader::FileLoader::load_auto(input_path).await?;

        match output_format {
            ConfigFormat::Toml => {
                crate::loader::FileLoader::save_toml(&config, output_path).await?;
            }
            ConfigFormat::Json => {
                crate::loader::FileLoader::save_json(&config, output_path).await?;
            }
        }

        Ok(())
    }

    /// Check if running in Docker/container environment
    pub fn is_containerized() -> bool {
        std::path::Path::new("/.dockerenv").exists()
            || std::env::var("DOCKER_CONTAINER").is_ok()
            || std::env::var("KUBERNETES_SERVICE_HOST").is_ok()
    }

    /// Get appropriate configuration for the runtime environment
    pub fn get_environment_config() -> ConfigResult<crate::CatalystConfig> {
        let network = crate::loader::EnvLoader::get_env_var::<String>("CATALYST_NETWORK")?
            .unwrap_or_else(|| {
                if Self::is_containerized() {
                    "testnet".to_string()
                } else {
                    "devnet".to_string()
                }
            });

        let network_type = network.parse()?;
        Ok(crate::CatalystConfig::new_for_network(network_type))
    }

    /// Calculate configuration hash for change detection
    pub fn calculate_config_hash(config: &crate::CatalystConfig) -> ConfigResult<String> {
        let serialized = toml::to_string(config)
            .map_err(|e| ConfigError::InvalidFormat(format!("Serialization failed: {}", e)))?;

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        serialized.hash(&mut hasher);
        let hash = hasher.finish();

        Ok(format!("{:x}", hash))
    }

    /// Load configuration with fallback chain
    pub async fn load_with_fallbacks(
        primary_path: Option<PathBuf>,
    ) -> ConfigResult<crate::CatalystConfig> {
        // Try primary path first
        if let Some(path) = primary_path {
            if path.exists() {
                match crate::loader::FileLoader::load_auto(&path).await {
                    Ok(config) => return Ok(config),
                    Err(e) => eprintln!(
                        "Warning: Failed to load primary config {}: {}",
                        path.display(),
                        e
                    ),
                }
            }
        }

        // Try environment variables
        match crate::loader::EnvLoader::load_from_env() {
            Ok(config) => return Ok(config),
            Err(e) => eprintln!("Warning: Failed to load from environment: {}", e),
        }

        // Try default network-specific config
        let network = std::env::var("CATALYST_NETWORK").unwrap_or_else(|_| "devnet".to_string());
        let default_config_file = Self::get_default_config_file(&network);

        match Self::find_config_file(&default_config_file) {
            Ok(path) => match crate::loader::FileLoader::load_auto(&path).await {
                Ok(config) => return Ok(config),
                Err(e) => eprintln!(
                    "Warning: Failed to load default config {}: {}",
                    path.display(),
                    e
                ),
            },
            Err(e) => eprintln!("Warning: Default config not found: {}", e),
        }

        // Final fallback - generate default configuration
        let network_type = network.parse().unwrap_or(crate::NetworkType::Devnet);
        Ok(crate::CatalystConfig::new_for_network(network_type))
    }
}

/// Configuration file format enumeration
#[derive(Debug, Clone)]
pub enum ConfigFormat {
    Toml,
    Json,
}

impl std::str::FromStr for ConfigFormat {
    type Err = ConfigError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "toml" => Ok(ConfigFormat::Toml),
            "json" => Ok(ConfigFormat::Json),
            _ => Err(ConfigError::InvalidFormat(format!("Unknown format: {}", s))),
        }
    }
}

/// Configuration change detection
pub struct ConfigWatcher {
    config_hash: String,
    config_path: PathBuf,
}

impl ConfigWatcher {
    pub fn new<P: AsRef<Path>>(
        config_path: P,
        config: &crate::CatalystConfig,
    ) -> ConfigResult<Self> {
        let config_hash = ConfigUtils::calculate_config_hash(config)?;
        Ok(Self {
            config_hash,
            config_path: config_path.as_ref().to_path_buf(),
        })
    }

    pub async fn check_for_changes(&mut self) -> ConfigResult<bool> {
        if !self.config_path.exists() {
            return Ok(false);
        }

        let config = crate::loader::FileLoader::load_auto(&self.config_path).await?;
        let new_hash = ConfigUtils::calculate_config_hash(&config)?;

        if new_hash != self.config_hash {
            self.config_hash = new_hash;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

// Fix the default config directory function:
pub fn default_config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join("catalyst"))
        .unwrap_or_else(|| PathBuf::from(".catalyst"))
}

// Fix async functions that were missing async keyword:
pub async fn convert_config_format(
    input_path: &Path,
    output_path: &Path,
    output_format: &str,
) -> ConfigResult<()> {
    let config = FileLoader::load_auto(input_path).await?;

    match output_format.to_lowercase().as_str() {
        "toml" => {
            FileLoader::save_toml(&config, output_path).await?;
        }
        "json" => {
            FileLoader::save_json(&config, output_path).await?;
        }
        _ => {
            return Err(ConfigError::InvalidFormat(format!(
                "Unsupported output format: {}",
                output_format
            )))
        }
    }
    Ok(())
}

// Fix the template generation function:
pub fn generate_config_template(network: &str) -> ConfigResult<String> {
    let template = match network {
        "mainnet" => include_str!("../configs/mainnet.toml"),
        "testnet" => include_str!("../configs/testnet.toml"),
        "devnet" => include_str!("../configs/devnet.toml"),
        "example" => include_str!("../configs/example.toml"),
        "custom" => {
            let config = CatalystConfig::default();
            return Ok(toml::to_string_pretty(&config).map_err(|e| {
                ConfigError::InvalidFormat(format!("Template generation failed: {}", e))
            })?);
        }
        _ => {
            return Err(ConfigError::InvalidFormat(format!(
                "Unknown network template: {}",
                network
            )))
        }
    };
    Ok(template.to_string())
}
