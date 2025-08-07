use crate::config::{PersonalConfig, MountPattern, PersonalMount, Rule, MountDirs};
use crate::utils::paths;
use anyhow::{Result, Context};
use std::fs;
use std::collections::HashMap;
use std::io::Write;
use atomicwrites::{AtomicFile, OverwriteBehavior};

pub struct PersonalConfigManager;

impl PersonalConfigManager {
    pub fn load() -> Result<Option<PersonalConfig>> {
        let config_path = paths::get_personal_config_path()?;
        if !config_path.exists() {
            return Ok(None);
        }
        
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read personal config from {:?}", config_path))?;
        let config: PersonalConfig = serde_json::from_str(&content)
            .context("Failed to parse personal configuration")?;
        
        Ok(Some(config))
    }
    
    pub fn save(config: &PersonalConfig) -> Result<()> {
        let config_path = paths::get_personal_config_path()?;
        
        // Ensure directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {:?}", parent))?;
        }
        
        let json = serde_json::to_string_pretty(config)
            .context("Failed to serialize personal configuration")?;
        
        AtomicFile::new(&config_path, OverwriteBehavior::AllowOverwrite)
            .write(|f| f.write_all(json.as_bytes()))
            .with_context(|| format!("Failed to write personal config to {:?}", config_path))?;
        
        Ok(())
    }
    
    pub fn ensure_default() -> Result<PersonalConfig> {
        if let Some(config) = Self::load()? {
            return Ok(config);
        }
        
        let default_config = PersonalConfig {
            patterns: vec![],
            repository_mounts: HashMap::new(),
            rules: vec![],
            default_mount_dirs: MountDirs::default(),
        };
        
        Self::save(&default_config)?;
        Ok(default_config)
    }
    
    pub fn add_pattern(pattern: MountPattern) -> Result<()> {
        let mut config = Self::load()?.unwrap_or_else(|| PersonalConfig {
            patterns: vec![],
            repository_mounts: HashMap::new(),
            rules: vec![],
            default_mount_dirs: MountDirs::default(),
        });
        
        config.patterns.push(pattern);
        Self::save(&config)?;
        Ok(())
    }
    
    pub fn add_rule(rule: Rule) -> Result<()> {
        let mut config = Self::load()?.unwrap_or_else(|| PersonalConfig {
            patterns: vec![],
            repository_mounts: HashMap::new(),
            rules: vec![],
            default_mount_dirs: MountDirs::default(),
        });
        
        config.rules.push(rule);
        Self::save(&config)?;
        Ok(())
    }
    
    pub fn add_repository_mount(repo_url: &str, mount: PersonalMount) -> Result<()> {
        let mut config = Self::load()?.unwrap_or_else(|| PersonalConfig {
            patterns: vec![],
            repository_mounts: HashMap::new(),
            rules: vec![],
            default_mount_dirs: MountDirs::default(),
        });
        
        config.repository_mounts
            .entry(repo_url.to_string())
            .or_insert_with(Vec::new)
            .push(mount);
        
        Self::save(&config)?;
        Ok(())
    }
    
    pub fn remove_repository_mount(repo_url: &str, mount_path: &str) -> Result<bool> {
        let mut config = Self::load()?.unwrap_or_else(|| PersonalConfig {
            patterns: vec![],
            repository_mounts: HashMap::new(),
            rules: vec![],
            default_mount_dirs: MountDirs::default(),
        });
        
        let should_remove_key = if let Some(mounts) = config.repository_mounts.get_mut(repo_url) {
            let initial_len = mounts.len();
            mounts.retain(|m| m.mount_path != mount_path);
            
            let changed = mounts.len() < initial_len;
            let is_empty = mounts.is_empty();
            
            if changed {
                if is_empty {
                    true // Mark for removal outside this block
                } else {
                    Self::save(&config)?;
                    return Ok(true);
                }
            } else {
                false
            }
        } else {
            false
        };
        
        if should_remove_key {
            config.repository_mounts.remove(repo_url);
            Self::save(&config)?;
            return Ok(true);
        }
        
        Ok(false)
    }
    
    pub fn get_repository_mounts(repo_url: &str) -> Result<Vec<PersonalMount>> {
        if let Some(config) = Self::load()? {
            if let Some(mounts) = config.repository_mounts.get(repo_url) {
                return Ok(mounts.clone());
            }
        }
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::env;
    use serial_test::serial;

    fn with_temp_home<F>(test: F)
    where
        F: FnOnce(),
    {
        let temp_dir = TempDir::new().unwrap();
        let original_home = env::var("HOME").ok();
        
        unsafe {
            env::set_var("HOME", temp_dir.path());
        }
        test();
        
        unsafe {
            if let Some(home) = original_home {
                env::set_var("HOME", home);
            } else {
                env::remove_var("HOME");
            }
        }
    }

    #[test]
    #[serial]
    fn test_save_and_load_personal_config() {
        with_temp_home(|| {
            // Clear any existing config first
            let config_path = paths::get_personal_config_path().unwrap();
            if config_path.exists() {
                fs::remove_file(&config_path).ok();
            }
            let config = PersonalConfig {
                patterns: vec![
                    MountPattern {
                        match_remote: "git@github.com:test/*".to_string(),
                        personal_mounts: vec![
                            PersonalMount {
                                remote: "git@github.com:me/notes.git".to_string(),
                                mount_path: "notes".to_string(),
                                subpath: None,
                                description: "My notes".to_string(),
                            },
                        ],
                        description: "Test pattern".to_string(),
                    },
                ],
                repository_mounts: HashMap::new(),
                rules: vec![],
                default_mount_dirs: MountDirs::default(),
            };

            // Save
            PersonalConfigManager::save(&config).unwrap();

            // Load
            let loaded = PersonalConfigManager::load().unwrap().unwrap();
            
            assert_eq!(loaded.patterns.len(), config.patterns.len());
            assert_eq!(loaded.patterns[0].match_remote, config.patterns[0].match_remote);
        });
    }

    #[test]
    #[serial]
    fn test_add_pattern() {
        with_temp_home(|| {
            let pattern = MountPattern {
                match_remote: "git@github.com:company/*".to_string(),
                personal_mounts: vec![],
                description: "Company repos".to_string(),
            };

            PersonalConfigManager::add_pattern(pattern.clone()).unwrap();

            let config = PersonalConfigManager::load().unwrap().unwrap();
            assert_eq!(config.patterns.len(), 1);
            assert_eq!(config.patterns[0].match_remote, pattern.match_remote);
        });
    }

    #[test]
    #[serial]
    fn test_repository_mounts() {
        with_temp_home(|| {
            let repo_url = "git@github.com:test/project.git";
            let mount = PersonalMount {
                remote: "git@github.com:me/personal.git".to_string(),
                mount_path: "personal".to_string(),
                subpath: None,
                description: "Personal files".to_string(),
            };

            // Add mount
            PersonalConfigManager::add_repository_mount(repo_url, mount.clone()).unwrap();

            // Get mounts
            let mounts = PersonalConfigManager::get_repository_mounts(repo_url).unwrap();
            assert_eq!(mounts.len(), 1);
            assert_eq!(mounts[0].mount_path, mount.mount_path);

            // Remove mount
            assert!(PersonalConfigManager::remove_repository_mount(repo_url, &mount.mount_path).unwrap());

            // Verify removed
            let mounts = PersonalConfigManager::get_repository_mounts(repo_url).unwrap();
            assert_eq!(mounts.len(), 0);

            // Try to remove non-existent
            assert!(!PersonalConfigManager::remove_repository_mount(repo_url, "nonexistent").unwrap());
        });
    }

    #[test]
    #[serial]
    fn test_ensure_default() {
        with_temp_home(|| {
            // Ensure default creates a new config
            let config = PersonalConfigManager::ensure_default().unwrap();
            assert_eq!(config.patterns.len(), 0);
            assert_eq!(config.repository_mounts.len(), 0);
            assert_eq!(config.rules.len(), 0);

            // Add some data
            PersonalConfigManager::add_pattern(MountPattern {
                match_remote: "test/*".to_string(),
                personal_mounts: vec![],
                description: "Test".to_string(),
            }).unwrap();

            // Ensure default returns existing config
            let config = PersonalConfigManager::ensure_default().unwrap();
            assert_eq!(config.patterns.len(), 1);
        });
    }
}