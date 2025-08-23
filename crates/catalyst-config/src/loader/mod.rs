//! Configuration loading and environment handling

pub mod env;
pub mod file;
pub mod validation;

// Re-export the loaders that other code expects
pub use env::EnvLoader;
pub use file::FileLoader;
pub use validation::*;

use crate::{CatalystConfig, ConfigResult};
use std::path::Path;

/// Main configuration loader
pub struct ConfigLoader {
    // Configuration loading options
}

impl ConfigLoader {
    pub async fn load_config<P: AsRef<Path>>(&self, path: P) -> ConfigResult<CatalystConfig> {
        FileLoader::load_auto(path).await
    }

    pub fn new() -> Self {
        Self {}
    }

    /// Load configuration from file with environment variable overrides
    pub async fn load_from_file<P: AsRef<Path>>(&self, path: P) -> ConfigResult<CatalystConfig> {
        // Use the FileLoader
        FileLoader::load_auto(path).await
    }

    /// Load configuration from environment variables only
    pub fn load_from_env(&self) -> ConfigResult<CatalystConfig> {
        // Use the EnvLoader
        EnvLoader::load_from_env()
    }

    /// Load with precedence: CLI args > env vars > config file > defaults
    pub async fn load_with_overrides<P: AsRef<Path>>(
        &self,
        config_path: Option<P>,
        cli_overrides: Option<&str>,
    ) -> ConfigResult<CatalystConfig> {
        // Start with defaults
        let config = if let Some(path) = config_path {
            // Try to load from file first
            match FileLoader::load_auto(path).await {
                Ok(config) => config,
                Err(_) => {
                    // Fall back to environment if file loading fails
                    EnvLoader::load_from_env().unwrap_or_else(|_| {
                        // Final fallback to default
                        CatalystConfig::default()
                    })
                }
            }
        } else {
            // No file specified, try environment
            EnvLoader::load_from_env().unwrap_or_else(|_| {
                // Final fallback to default
                CatalystConfig::default()
            })
        };

        // Apply CLI overrides if provided
        if let Some(_overrides) = cli_overrides {
            // TODO: Parse and apply CLI overrides
            // For now, just return the config as-is
        }

        config.validate()?;
        Ok(config)
    }
}
