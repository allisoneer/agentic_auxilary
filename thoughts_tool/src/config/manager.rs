use super::types::*;
use crate::error::{Result, ThoughtsError};
use crate::utils::paths;
use atomicwrites::{AllowOverwrite, AtomicFile};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Result<Self> {
        let config_path = paths::get_config_path()?;
        Ok(Self { config_path })
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self { config_path: path }
    }

    pub fn load(&self) -> Result<Config> {
        if !self.config_path.exists() {
            return Err(ThoughtsError::ConfigNotFound {
                path: self.config_path.clone(),
            });
        }

        debug!("Loading config from {:?}", self.config_path);
        let contents = fs::read_to_string(&self.config_path)?;
        let config: Config = serde_json::from_str(&contents)?;

        self.validate(&config)?;
        Ok(config)
    }

    pub fn load_or_default(&self) -> Config {
        match self.load() {
            Ok(config) => config,
            Err(e) => {
                warn!("Failed to load config: {}, using default", e);
                Config::default()
            }
        }
    }

    /// Create a backup of the current config file
    pub fn backup(&self) -> Result<()> {
        if self.config_path.exists() {
            let backup_path = self.config_path.with_extension("json.bak");
            fs::copy(&self.config_path, backup_path)?;
            debug!("Created config backup");
        }
        Ok(())
    }

    /// Save config with automatic backup
    pub fn save_with_backup(&self, config: &Config) -> Result<()> {
        self.backup()?;
        self.save(config)
    }

    pub fn save(&self, config: &Config) -> Result<()> {
        self.validate(config)?;

        // Ensure config directory exists
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Create backup if file exists
        if self.config_path.exists() {
            let backup_path = self.config_path.with_extension("json.bak");
            fs::copy(&self.config_path, backup_path)?;
        }

        // Write atomically
        debug!("Saving config to {:?}", self.config_path);
        let json = serde_json::to_string_pretty(config)?;

        let af = AtomicFile::new(&self.config_path, AllowOverwrite);
        af.write(|f| f.write_all(json.as_bytes()))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

        info!("Configuration saved successfully");
        Ok(())
    }

    /// Enhanced validation with detailed error messages
    pub fn validate(&self, config: &Config) -> Result<()> {
        // Validate mounts using MountValidator
        for (_name, mount) in &config.mounts {
            crate::config::validation::MountValidator::validate_mount(mount)?;
        }

        Ok(())
    }

    pub fn config_exists(&self) -> bool {
        self.config_path.exists()
    }

    pub fn get_config_path(&self) -> &Path {
        &self.config_path
    }

    /// Get or create default configuration
    pub fn get_or_create_default(&self) -> Result<Config> {
        if self.config_exists() {
            self.load()
        } else {
            // Create config directory if needed
            if let Some(parent) = self.config_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let default_config = Config::default();
            self.save(&default_config)?;
            Ok(default_config)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_config_round_trip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let manager = ConfigManager::with_path(config_path);

        let config = Config::default();
        manager.save(&config).unwrap();

        let loaded = manager.load().unwrap();
        assert_eq!(loaded.version, config.version);
    }

    #[test]
    fn test_backup_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.json");
        let manager = ConfigManager::with_path(config_path.clone());

        // Create initial config
        let config = Config::default();
        manager.save(&config).unwrap();

        // Create backup
        manager.backup().unwrap();

        // Verify backup exists
        let backup_path = config_path.with_extension("json.bak");
        assert!(backup_path.exists());
    }
}
