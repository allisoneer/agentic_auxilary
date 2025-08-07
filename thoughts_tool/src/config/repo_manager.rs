use crate::config::{RepoConfig, RequiredMount, MountDirs};
use crate::utils::paths;
use anyhow::{Result, Context};
use std::path::PathBuf;
use std::fs;
use std::io::Write;
use atomicwrites::{AtomicFile, OverwriteBehavior};

pub struct RepoConfigManager {
    repo_root: PathBuf,
}

impl RepoConfigManager {
    pub fn new(repo_root: PathBuf) -> Self {
        Self { repo_root }
    }

    pub fn load(&self) -> Result<Option<RepoConfig>> {
        let config_path = paths::get_repo_config_path(&self.repo_root);
        if !config_path.exists() {
            return Ok(None);
        }
        
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config from {:?}", config_path))?;
        let config: RepoConfig = serde_json::from_str(&content)
            .with_context(|| "Failed to parse repository configuration")?;
        
        self.validate(&config)?;
        Ok(Some(config))
    }

    pub fn save(&self, config: &RepoConfig) -> Result<()> {
        self.validate(config)?;
        
        let config_path = paths::get_repo_config_path(&self.repo_root);
        
        // Ensure .thoughts directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }
        
        let json = serde_json::to_string_pretty(config)
            .context("Failed to serialize configuration")?;
        
        AtomicFile::new(&config_path, OverwriteBehavior::AllowOverwrite)
            .write(|f| f.write_all(json.as_bytes()))
            .with_context(|| format!("Failed to write config to {:?}", config_path))?;
        
        Ok(())
    }

    pub fn ensure_default(&self) -> Result<RepoConfig> {
        if let Some(config) = self.load()? {
            return Ok(config);
        }
        
        let default_config = RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs::default(),
            requires: vec![],
            rules: vec![],
        };
        
        self.save(&default_config)?;
        Ok(default_config)
    }

    fn validate(&self, config: &RepoConfig) -> Result<()> {
        // Validate version
        if config.version != "1.0" {
            anyhow::bail!("Unsupported configuration version: {}", config.version);
        }

        // Validate mount directories don't conflict
        if config.mount_dirs.repository == "personal" {
            anyhow::bail!("Repository mount directory cannot be named 'personal'");
        }

        if config.mount_dirs.repository == config.mount_dirs.personal {
            anyhow::bail!("Repository and personal mount directories must be different");
        }

        // Validate mount paths are unique
        let mut seen_paths = std::collections::HashSet::new();
        for mount in &config.requires {
            if !seen_paths.insert(&mount.mount_path) {
                anyhow::bail!("Duplicate mount path: {}", mount.mount_path);
            }
        }

        // Validate required mounts have valid remotes
        for mount in &config.requires {
            self.validate_remote(&mount.remote)?;
        }

        // Validate rules have valid patterns
        for rule in &config.rules {
            glob::Pattern::new(&rule.pattern)
                .with_context(|| format!("Invalid pattern: {}", rule.pattern))?;
        }

        Ok(())
    }

    fn validate_remote(&self, remote: &str) -> Result<()> {
        if remote.starts_with("./") {
            // Local mount - relative path is OK
            return Ok(());
        }
        
        if !remote.starts_with("git@") && 
           !remote.starts_with("https://") && 
           !remote.starts_with("ssh://") {
            anyhow::bail!("Invalid remote URL: {}. Must be a git URL or relative path starting with ./", remote);
        }
        
        Ok(())
    }

    pub fn add_mount(&mut self, mount: RequiredMount) -> Result<()> {
        let mut config = self.load()?.unwrap_or_else(|| RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs::default(),
            requires: vec![],
            rules: vec![],
        });

        // Check for duplicate mount paths
        if config.requires.iter().any(|m| m.mount_path == mount.mount_path) {
            anyhow::bail!("Mount path '{}' already exists", mount.mount_path);
        }

        config.requires.push(mount);
        self.save(&config)?;
        Ok(())
    }

    pub fn remove_mount(&mut self, mount_path: &str) -> Result<bool> {
        let mut config = self.load()?
            .ok_or_else(|| anyhow::anyhow!("No repository configuration found"))?;

        let initial_len = config.requires.len();
        config.requires.retain(|m| m.mount_path != mount_path);
        
        if config.requires.len() == initial_len {
            return Ok(false);
        }

        self.save(&config)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load_config() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RepoConfigManager::new(temp_dir.path().to_path_buf());

        let config = RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs::default(),
            requires: vec![
                RequiredMount {
                    remote: "git@github.com:test/repo.git".to_string(),
                    mount_path: "test".to_string(),
                    subpath: None,
                    description: "Test repository".to_string(),
                    optional: false,
                    override_rules: None,
                    sync: crate::config::SyncStrategy::Auto,
                },
            ],
            rules: vec![],
        };

        // Save
        manager.save(&config).unwrap();

        // Load
        let loaded = manager.load().unwrap().unwrap();
        
        assert_eq!(loaded.version, config.version);
        assert_eq!(loaded.requires.len(), config.requires.len());
        assert_eq!(loaded.requires[0].remote, config.requires[0].remote);
    }

    #[test]
    fn test_validation_invalid_version() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RepoConfigManager::new(temp_dir.path().to_path_buf());

        let config = RepoConfig {
            version: "2.0".to_string(), // Invalid version
            mount_dirs: MountDirs::default(),
            requires: vec![],
            rules: vec![],
        };

        assert!(manager.save(&config).is_err());
    }

    #[test]
    fn test_validation_conflicting_mount_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RepoConfigManager::new(temp_dir.path().to_path_buf());

        let config = RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs {
                repository: "personal".to_string(), // Invalid: can't be named "personal"
                personal: "personal".to_string(),
            },
            requires: vec![],
            rules: vec![],
        };

        assert!(manager.save(&config).is_err());
    }

    #[test]
    fn test_validation_duplicate_mount_paths() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RepoConfigManager::new(temp_dir.path().to_path_buf());

        let config = RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs::default(),
            requires: vec![
                RequiredMount {
                    remote: "git@github.com:test/repo1.git".to_string(),
                    mount_path: "test".to_string(),
                    subpath: None,
                    description: "Test 1".to_string(),
                    optional: false,
                    override_rules: None,
                    sync: crate::config::SyncStrategy::None,
                },
                RequiredMount {
                    remote: "git@github.com:test/repo2.git".to_string(),
                    mount_path: "test".to_string(), // Duplicate
                    subpath: None,
                    description: "Test 2".to_string(),
                    optional: false,
                    override_rules: None,
                    sync: crate::config::SyncStrategy::None,
                },
            ],
            rules: vec![],
        };

        assert!(manager.save(&config).is_err());
    }

    #[test]
    fn test_validation_invalid_remote() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RepoConfigManager::new(temp_dir.path().to_path_buf());

        let config = RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs::default(),
            requires: vec![
                RequiredMount {
                    remote: "invalid-url".to_string(), // Invalid URL
                    mount_path: "test".to_string(),
                    subpath: None,
                    description: "Test".to_string(),
                    optional: false,
                    override_rules: None,
                    sync: crate::config::SyncStrategy::None,
                },
            ],
            rules: vec![],
        };

        assert!(manager.save(&config).is_err());
    }

    #[test]
    fn test_validation_local_mount() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RepoConfigManager::new(temp_dir.path().to_path_buf());

        let config = RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs::default(),
            requires: vec![
                RequiredMount {
                    remote: "./local/path".to_string(), // Valid local mount
                    mount_path: "local".to_string(),
                    subpath: None,
                    description: "Local mount".to_string(),
                    optional: false,
                    override_rules: None,
                    sync: crate::config::SyncStrategy::None,
                },
            ],
            rules: vec![],
        };

        assert!(manager.save(&config).is_ok());
    }

    #[test]
    fn test_add_and_remove_mount() {
        let temp_dir = TempDir::new().unwrap();
        let mut manager = RepoConfigManager::new(temp_dir.path().to_path_buf());

        // Add mount
        let mount = RequiredMount {
            remote: "git@github.com:test/repo.git".to_string(),
            mount_path: "test".to_string(),
            subpath: None,
            description: "Test repository".to_string(),
            optional: false,
            override_rules: None,
            sync: crate::config::SyncStrategy::Auto,
        };

        manager.add_mount(mount.clone()).unwrap();

        // Verify it was added
        let config = manager.load().unwrap().unwrap();
        assert_eq!(config.requires.len(), 1);
        assert_eq!(config.requires[0].mount_path, "test");

        // Remove mount
        assert!(manager.remove_mount("test").unwrap());

        // Verify it was removed
        let config = manager.load().unwrap().unwrap();
        assert_eq!(config.requires.len(), 0);

        // Try to remove non-existent mount
        assert!(!manager.remove_mount("test").unwrap());
    }
}