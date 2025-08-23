use crate::{CatalystConfig, ConfigError, ConfigResult};
use std::path::Path;
use tokio::fs;

/// File-based configuration loader
pub struct FileLoader;

impl FileLoader {
    /// Load configuration from a TOML file
    pub async fn load_toml<P: AsRef<Path>>(path: P) -> ConfigResult<CatalystConfig> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| ConfigError::Io(e))?;

        let config: CatalystConfig = toml::from_str(&content).map_err(|e| ConfigError::Toml(e))?;

        config.validate()?;
        Ok(config)
    }

    /// Load configuration from a JSON file
    pub async fn load_json<P: AsRef<Path>>(path: P) -> ConfigResult<CatalystConfig> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .await
            .map_err(|e| ConfigError::Io(e))?;

        let config: CatalystConfig =
            serde_json::from_str(&content).map_err(|e| ConfigError::Json(e))?;

        config.validate()?;
        Ok(config)
    }

    /// Auto-detect file format and load configuration
    pub async fn load_auto<P: AsRef<Path>>(path: P) -> ConfigResult<CatalystConfig> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(ConfigError::FileNotFound(path.display().to_string()));
        }

        match path.extension().and_then(|ext| ext.to_str()) {
            Some("toml") => Self::load_toml(path).await,
            Some("json") => Self::load_json(path).await,
            Some(ext) => Err(ConfigError::InvalidFormat(format!(
                "Unsupported file extension: {}",
                ext
            ))),
            None => {
                // Try TOML first, then JSON
                match Self::load_toml(path).await {
                    Ok(config) => Ok(config),
                    Err(_) => Self::load_json(path).await,
                }
            }
        }
    }

    /// Save configuration to a TOML file
    pub async fn save_toml<P: AsRef<Path>>(config: &CatalystConfig, path: P) -> ConfigResult<()> {
        let content = toml::to_string_pretty(config)
            .map_err(|e| ConfigError::InvalidFormat(format!("TOML serialization failed: {}", e)))?;

        fs::write(path, content)
            .await
            .map_err(|e| ConfigError::Io(e))?;

        Ok(())
    }

    /// Save configuration to a JSON file
    pub async fn save_json<P: AsRef<Path>>(config: &CatalystConfig, path: P) -> ConfigResult<()> {
        let content = serde_json::to_string_pretty(config).map_err(|e| ConfigError::Json(e))?;

        fs::write(path, content)
            .await
            .map_err(|e| ConfigError::Io(e))?;

        Ok(())
    }
}
