use crate::config::{
    ContextMount, Mount, MountDirs, MountDirsV2, ReferenceEntry, ReferenceMount, RepoConfig,
    RepoConfigV2, RequiredMount, SyncStrategy, ThoughtsMount,
};
use crate::mount::MountSpace;
use crate::utils::paths;
use anyhow::{Context, Result};
use atomicwrites::{AtomicFile, OverwriteBehavior};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct DesiredState {
    pub mount_dirs: MountDirsV2,
    pub thoughts_mount: Option<ThoughtsMount>,
    pub context_mounts: Vec<ContextMount>,
    pub references: Vec<ReferenceMount>,
    pub was_v1: bool, // for messaging
}

impl DesiredState {
    /// Find a mount by its MountSpace identifier
    pub fn find_mount(&self, space: &MountSpace) -> Option<Mount> {
        match space {
            MountSpace::Thoughts => self.thoughts_mount.as_ref().map(|tm| Mount::Git {
                url: tm.remote.clone(),
                sync: tm.sync,
                subpath: tm.subpath.clone(),
            }),
            MountSpace::Context(mount_path) => self
                .context_mounts
                .iter()
                .find(|cm| &cm.mount_path == mount_path)
                .map(|cm| Mount::Git {
                    url: cm.remote.clone(),
                    sync: cm.sync,
                    subpath: cm.subpath.clone(),
                }),
            MountSpace::Reference { org: _, repo: _ } => {
                // References need URL lookup - for now return None
                // This will be addressed when references commands are implemented
                None
            }
        }
    }

    /// Get target path for a mount space
    pub fn get_mount_target(&self, space: &MountSpace, repo_root: &Path) -> PathBuf {
        repo_root
            .join(".thoughts-data")
            .join(space.relative_path(&self.mount_dirs))
    }
}

pub struct RepoConfigManager {
    repo_root: PathBuf,
}

impl RepoConfigManager {
    pub fn new(repo_root: PathBuf) -> Self {
        // Ensure absolute path at construction (defense-in-depth)
        let abs = if repo_root.is_absolute() {
            repo_root
        } else {
            std::fs::canonicalize(&repo_root).unwrap_or_else(|_| {
                std::env::current_dir()
                    .expect("Failed to determine current directory for path normalization")
                    .join(&repo_root)
            })
        };
        Self { repo_root: abs }
    }

    /// Load v1 configuration. Prefer using `load_desired_state()` or `ensure_v2_default()` for new code.
    pub fn load(&self) -> Result<Option<RepoConfig>> {
        let config_path = paths::get_repo_config_path(&self.repo_root);
        if !config_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config from {config_path:?}"))?;
        let config: RepoConfig = serde_json::from_str(&content)
            .with_context(|| "Failed to parse repository configuration")?;

        self.validate(&config)?;
        Ok(Some(config))
    }

    /// Save v1 configuration. Prefer using `save_v2_validated()` for new code.
    pub fn save(&self, config: &RepoConfig) -> Result<()> {
        self.validate(config)?;

        let config_path = paths::get_repo_config_path(&self.repo_root);

        // Ensure .thoughts directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {parent:?}"))?;
        }

        let json =
            serde_json::to_string_pretty(config).context("Failed to serialize configuration")?;

        AtomicFile::new(&config_path, OverwriteBehavior::AllowOverwrite)
            .write(|f| f.write_all(json.as_bytes()))
            .with_context(|| format!("Failed to write config to {config_path:?}"))?;

        Ok(())
    }

    /// Ensure v1 default configuration. Prefer using `ensure_v2_default()` for new code.
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

    pub fn load_desired_state(&self) -> Result<Option<DesiredState>> {
        let config_path = paths::get_repo_config_path(&self.repo_root);
        if !config_path.exists() {
            return Ok(None);
        }

        let raw = std::fs::read_to_string(&config_path)?;
        // Peek version
        let v: serde_json::Value = serde_json::from_str(&raw)?;
        let version = v.get("version").and_then(|x| x.as_str()).unwrap_or("1.0");

        if version == "2.0" {
            let v2: RepoConfigV2 = serde_json::from_str(&raw)?;
            // Normalize ReferenceEntry to ReferenceMount
            let refs = v2
                .references
                .into_iter()
                .map(|e| match e {
                    ReferenceEntry::Simple(url) => ReferenceMount {
                        remote: url,
                        description: None,
                    },
                    ReferenceEntry::WithMetadata(rm) => rm,
                })
                .collect();
            return Ok(Some(DesiredState {
                mount_dirs: v2.mount_dirs,
                thoughts_mount: v2.thoughts_mount,
                context_mounts: v2.context_mounts,
                references: refs,
                was_v1: false,
            }));
        }

        // Fallback: v1 mapping (legacy RepoConfig)
        let v1: RepoConfig = serde_json::from_str(&raw)?;
        let ds = self.map_v1_to_desired_state(&v1);
        Ok(Some(ds))
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

        if !remote.starts_with("git@")
            && !remote.starts_with("https://")
            && !remote.starts_with("ssh://")
        {
            anyhow::bail!(
                "Invalid remote URL: {}. Must be a git URL or relative path starting with ./",
                remote
            );
        }

        Ok(())
    }

    #[allow(dead_code)]
    // TODO(2): Refactor mount add/remove to use these manager methods
    pub fn add_mount(&mut self, mount: RequiredMount) -> Result<()> {
        let mut config = self.load()?.unwrap_or_else(|| RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs::default(),
            requires: vec![],
            rules: vec![],
        });

        // Check for duplicate mount paths
        if config
            .requires
            .iter()
            .any(|m| m.mount_path == mount.mount_path)
        {
            anyhow::bail!("Mount path '{}' already exists", mount.mount_path);
        }

        config.requires.push(mount);
        self.save(&config)?;
        Ok(())
    }

    #[allow(dead_code)]
    // TODO(2): Refactor mount remove to use this method
    pub fn remove_mount(&mut self, mount_path: &str) -> Result<bool> {
        let mut config = self
            .load()?
            .ok_or_else(|| anyhow::anyhow!("No repository configuration found"))?;

        let initial_len = config.requires.len();
        config.requires.retain(|m| m.mount_path != mount_path);

        if config.requires.len() == initial_len {
            return Ok(false);
        }

        self.save(&config)?;
        Ok(true)
    }

    /// Load v2 config or error if it doesn't exist
    fn map_v1_to_desired_state(&self, v1: &RepoConfig) -> DesiredState {
        // Map to v2-like DesiredState
        let defaults = MountDirsV2::default();
        let mut context_mounts = vec![];
        let mut references = vec![];

        for req in &v1.requires {
            let is_ref =
                req.mount_path.starts_with("references/") || req.sync == SyncStrategy::None;
            if is_ref {
                references.push(ReferenceMount {
                    remote: req.remote.clone(),
                    description: Some(req.description.clone()),
                });
            } else {
                context_mounts.push(ContextMount {
                    remote: req.remote.clone(),
                    subpath: req.subpath.clone(),
                    mount_path: req.mount_path.clone(),
                    // guardrail; never none for context
                    sync: if req.sync == SyncStrategy::None {
                        SyncStrategy::Auto
                    } else {
                        req.sync
                    },
                });
            }
        }

        DesiredState {
            mount_dirs: MountDirsV2 {
                thoughts: defaults.thoughts,
                context: v1.mount_dirs.repository.clone(), // keep existing "context" name
                references: defaults.references,
            },
            thoughts_mount: None, // requires explicit config in v2
            context_mounts,
            references,
            was_v1: true,
        }
    }

    /// Load v2 config or error if it doesn't exist
    pub fn load_v2_or_bail(&self) -> Result<RepoConfigV2> {
        let config_path = paths::get_repo_config_path(&self.repo_root);
        if !config_path.exists() {
            anyhow::bail!("No repository configuration found. Run 'thoughts init' first.");
        }

        let raw = std::fs::read_to_string(&config_path)?;
        let v: serde_json::Value = serde_json::from_str(&raw)?;
        let version = v.get("version").and_then(|x| x.as_str()).unwrap_or("1.0");

        if version == "2.0" {
            let v2: RepoConfigV2 = serde_json::from_str(&raw)?;
            Ok(v2)
        } else {
            anyhow::bail!(
                "Repository is using v1 configuration. Please migrate to v2 configuration format."
            );
        }
    }

    /// Save v2 configuration
    pub fn save_v2(&self, config: &RepoConfigV2) -> Result<()> {
        let config_path = paths::get_repo_config_path(&self.repo_root);

        // Ensure .thoughts directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {parent:?}"))?;
        }

        let json =
            serde_json::to_string_pretty(config).context("Failed to serialize configuration")?;

        AtomicFile::new(&config_path, OverwriteBehavior::AllowOverwrite)
            .write(|f| f.write_all(json.as_bytes()))
            .with_context(|| format!("Failed to write config to {config_path:?}"))?;

        Ok(())
    }

    /// Ensure v2 config exists, create default if not
    pub fn ensure_v2_default(&self) -> Result<RepoConfigV2> {
        let config_path = paths::get_repo_config_path(&self.repo_root);
        if config_path.exists() {
            // Try to load existing config
            let raw = std::fs::read_to_string(&config_path)?;
            let v: serde_json::Value = serde_json::from_str(&raw)?;
            let version = v.get("version").and_then(|x| x.as_str()).unwrap_or("1.0");

            if version == "2.0" {
                return serde_json::from_str(&raw).context("Failed to parse v2 configuration");
            }

            // v1 path: parse full v1 once, then map without re-reading
            let v1: RepoConfig = serde_json::from_str(&raw)?;
            let ds = self.map_v1_to_desired_state(&v1);

            // Backup only if meaningful content present
            let needs_backup =
                !ds.context_mounts.is_empty() || !ds.references.is_empty() || !v1.rules.is_empty();
            if needs_backup {
                use chrono::Local;
                let ts = Local::now().format("%Y%m%d-%H%M%S");
                let backup_path = config_path
                    .parent()
                    .unwrap()
                    .join(format!("config.v1.bak-{}.json", ts));
                AtomicFile::new(&backup_path, OverwriteBehavior::AllowOverwrite)
                    .write(|f| f.write_all(raw.as_bytes()))
                    .with_context(|| format!("Failed to write backup to {:?}", backup_path))?;
            }

            // Build v2 config from DesiredState
            let v2_config = RepoConfigV2 {
                version: "2.0".to_string(),
                mount_dirs: ds.mount_dirs,
                thoughts_mount: ds.thoughts_mount,
                context_mounts: ds.context_mounts,
                references: ds
                    .references
                    .into_iter()
                    .map(|rm| {
                        if rm.description.is_some() {
                            ReferenceEntry::WithMetadata(rm)
                        } else {
                            ReferenceEntry::Simple(rm.remote)
                        }
                    })
                    .collect(),
            };

            // Save with hard validation
            self.save_v2_validated(&v2_config)?;
            return Ok(v2_config);
        }

        // Create default v2 config
        let default_config = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![],
        };

        self.save_v2(&default_config)?;
        Ok(default_config)
    }

    /// Soft validation for v2 configuration returning warnings only
    pub fn validate_v2_soft(&self, cfg: &RepoConfigV2) -> Vec<String> {
        let mut warnings = Vec::new();
        for r in &cfg.references {
            let url = match r {
                ReferenceEntry::Simple(s) => s.as_str(),
                ReferenceEntry::WithMetadata(rm) => rm.remote.as_str(),
            };
            if let Err(e) = crate::config::validation::validate_reference_url(url) {
                warnings.push(format!("Invalid reference '{}': {}", url, e));
            }
        }
        warnings
    }

    /// Peek the on-disk config version without fully parsing
    pub fn peek_config_version(&self) -> Result<Option<String>> {
        let config_path = paths::get_repo_config_path(&self.repo_root);
        if !config_path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&config_path)?;
        let v: serde_json::Value = serde_json::from_str(&raw)?;
        Ok(v.get("version")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string()))
    }

    /// Hard validator for v2 config. Returns warnings (non-fatal).
    pub fn validate_v2_hard(&self, cfg: &RepoConfigV2) -> Result<Vec<String>> {
        if cfg.version != "2.0" {
            anyhow::bail!("Unsupported configuration version: {}", cfg.version);
        }

        // mount_dirs: non-empty and distinct
        let m = &cfg.mount_dirs;
        for (name, val) in [
            ("thoughts", &m.thoughts),
            ("context", &m.context),
            ("references", &m.references),
        ] {
            if val.trim().is_empty() {
                anyhow::bail!("Mount directory '{}' cannot be empty", name);
            }
            if val == ".thoughts-data" {
                anyhow::bail!(
                    "Mount directory '{}' cannot be named '.thoughts-data'",
                    name
                );
            }
            if val == "." || val == ".." {
                anyhow::bail!("Mount directory '{}' cannot be '.' or '..'", name);
            }
            if val.contains('/') || val.contains('\\') {
                anyhow::bail!(
                    "Mount directory '{}' must be a single path segment (got {})",
                    name,
                    val
                );
            }
        }
        if m.thoughts == m.context || m.thoughts == m.references || m.context == m.references {
            anyhow::bail!("Mount directories must be distinct (thoughts/context/references)");
        }

        // thoughts_mount remote validation
        if let Some(tm) = &cfg.thoughts_mount {
            self.validate_remote(&tm.remote)?;
        }

        // context_mounts: unique mount_path, valid remotes; warn on sync:None
        let mut warnings = Vec::new();
        let mut seen_mount_paths = std::collections::HashSet::new();
        for cm in &cfg.context_mounts {
            // uniqueness
            if !seen_mount_paths.insert(&cm.mount_path) {
                anyhow::bail!("Duplicate context mount path: {}", cm.mount_path);
            }

            // mount_path validation
            let mp = cm.mount_path.trim();
            if mp.is_empty() {
                anyhow::bail!("Context mount path cannot be empty");
            }
            if mp == "." || mp == ".." {
                anyhow::bail!("Context mount path cannot be '.' or '..'");
            }
            if mp.contains('/') || mp.contains('\\') {
                anyhow::bail!(
                    "Context mount path must be a single path segment (got {})",
                    cm.mount_path
                );
            }
            let m = &cfg.mount_dirs;
            if mp == m.thoughts || mp == m.context || mp == m.references {
                anyhow::bail!(
                    "Context mount path '{}' cannot conflict with configured mount_dirs names ('{}', '{}', '{}')",
                    cm.mount_path,
                    m.thoughts,
                    m.context,
                    m.references
                );
            }

            // remote validity
            self.validate_remote(&cm.remote)?;
            if matches!(cm.sync, SyncStrategy::None) {
                warnings.push(format!(
                    "Context mount '{}' has sync:None; allowed but discouraged. Consider SyncStrategy::Auto.",
                    cm.mount_path
                ));
            }
        }

        // references: validate and ensure uniqueness by canonical key
        use crate::config::validation::{canonical_reference_key, validate_reference_url};
        let mut seen_refs = std::collections::HashSet::new();
        for r in &cfg.references {
            let url = match r {
                ReferenceEntry::Simple(s) => s.as_str(),
                ReferenceEntry::WithMetadata(rm) => rm.remote.as_str(),
            };
            validate_reference_url(url).with_context(|| format!("Invalid reference '{}'", url))?;
            let key = canonical_reference_key(url)?;
            if !seen_refs.insert(key) {
                anyhow::bail!("Duplicate reference detected: {}", url);
            }
        }

        Ok(warnings)
    }

    /// Save v2 configuration with hard validation. Returns warnings (non-fatal).
    pub fn save_v2_validated(&self, config: &RepoConfigV2) -> Result<Vec<String>> {
        let warnings = self.validate_v2_hard(config)?;
        self.save_v2(config)?;
        Ok(warnings)
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
            requires: vec![RequiredMount {
                remote: "git@github.com:test/repo.git".to_string(),
                mount_path: "test".to_string(),
                subpath: None,
                description: "Test repository".to_string(),
                optional: false,
                override_rules: None,
                sync: crate::config::SyncStrategy::Auto,
            }],
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
            requires: vec![RequiredMount {
                remote: "invalid-url".to_string(), // Invalid URL
                mount_path: "test".to_string(),
                subpath: None,
                description: "Test".to_string(),
                optional: false,
                override_rules: None,
                sync: crate::config::SyncStrategy::None,
            }],
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
            requires: vec![RequiredMount {
                remote: "./local/path".to_string(), // Valid local mount
                mount_path: "local".to_string(),
                subpath: None,
                description: "Local mount".to_string(),
                optional: false,
                override_rules: None,
                sync: crate::config::SyncStrategy::None,
            }],
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

    #[test]
    fn test_v1_to_desired_state_mapping() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RepoConfigManager::new(temp_dir.path().to_path_buf());

        // Create a v1 config with both context mounts and references
        let v1_config = RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs {
                repository: "context".to_string(),
                personal: "personal".to_string(),
            },
            requires: vec![
                RequiredMount {
                    remote: "git@github.com:user/context-repo.git".to_string(),
                    mount_path: "context-mount".to_string(),
                    subpath: Some("subdir".to_string()),
                    description: "Context mount".to_string(),
                    optional: false,
                    override_rules: None,
                    sync: crate::config::SyncStrategy::Auto,
                },
                RequiredMount {
                    remote: "git@github.com:org/ref-repo.git".to_string(),
                    mount_path: "references/ref-mount".to_string(),
                    subpath: None,
                    description: "Reference mount".to_string(),
                    optional: true,
                    override_rules: None,
                    sync: crate::config::SyncStrategy::None,
                },
            ],
            rules: vec![],
        };

        // Save the v1 config
        manager.save(&v1_config).unwrap();

        // Load as DesiredState
        let desired_state = manager.load_desired_state().unwrap().unwrap();

        // Verify the mapping
        assert!(desired_state.was_v1);
        assert_eq!(desired_state.mount_dirs.context, "context");
        assert_eq!(desired_state.mount_dirs.thoughts, "thoughts");
        assert_eq!(desired_state.mount_dirs.references, "references");

        // Check context mounts
        assert_eq!(desired_state.context_mounts.len(), 1);
        assert_eq!(
            desired_state.context_mounts[0].remote,
            "git@github.com:user/context-repo.git"
        );
        assert_eq!(desired_state.context_mounts[0].mount_path, "context-mount");
        assert_eq!(
            desired_state.context_mounts[0].subpath,
            Some("subdir".to_string())
        );

        // Check references
        assert_eq!(desired_state.references.len(), 1);
        assert_eq!(
            desired_state.references[0].remote,
            "git@github.com:org/ref-repo.git"
        );
        assert_eq!(
            desired_state.references[0].description.as_deref(),
            Some("Reference mount")
        );

        // Verify no thoughts mount (requires explicit config in v2)
        assert!(desired_state.thoughts_mount.is_none());
    }

    #[test]
    fn test_v2_config_loading() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RepoConfigManager::new(temp_dir.path().to_path_buf());

        // Create a v2 config
        let v2_config = crate::config::RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: crate::config::MountDirsV2::default(),
            thoughts_mount: Some(crate::config::ThoughtsMount {
                remote: "git@github.com:user/thoughts.git".to_string(),
                subpath: None,
                sync: crate::config::SyncStrategy::Auto,
            }),
            context_mounts: vec![crate::config::ContextMount {
                remote: "git@github.com:user/context.git".to_string(),
                subpath: Some("docs".to_string()),
                mount_path: "docs".to_string(),
                sync: crate::config::SyncStrategy::Auto,
            }],
            references: vec![
                ReferenceEntry::Simple("git@github.com:org/ref1.git".to_string()),
                ReferenceEntry::Simple("https://github.com/org/ref2.git".to_string()),
            ],
        };

        // Save the v2 config directly using JSON
        let config_path = paths::get_repo_config_path(temp_dir.path());
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        let json = serde_json::to_string_pretty(&v2_config).unwrap();
        std::fs::write(&config_path, json).unwrap();

        // Load as DesiredState
        let desired_state = manager.load_desired_state().unwrap().unwrap();

        // Verify the loading
        assert!(!desired_state.was_v1);
        assert!(desired_state.thoughts_mount.is_some());
        assert_eq!(
            desired_state.thoughts_mount.as_ref().unwrap().remote,
            "git@github.com:user/thoughts.git"
        );
        assert_eq!(desired_state.context_mounts.len(), 1);
        assert_eq!(desired_state.references.len(), 2);
    }

    #[test]
    fn test_v2_references_normalize_to_reference_mount() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RepoConfigManager::new(temp_dir.path().to_path_buf());

        let json = r#"{
            "version": "2.0",
            "mount_dirs": {},
            "context_mounts": [],
            "references": [
                "git@github.com:org/ref1.git",
                {"remote": "https://github.com/org/ref2.git", "description": "Ref 2"}
            ]
        }"#;

        let config_path = paths::get_repo_config_path(temp_dir.path());
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        std::fs::write(&config_path, json).unwrap();

        let ds = manager.load_desired_state().unwrap().unwrap();
        assert_eq!(ds.references.len(), 2);
        assert_eq!(ds.references[0].remote, "git@github.com:org/ref1.git");
        assert_eq!(ds.references[0].description, None);
        assert_eq!(ds.references[1].remote, "https://github.com/org/ref2.git");
        assert_eq!(ds.references[1].description.as_deref(), Some("Ref 2"));
    }

    #[test]
    fn test_v1_migration_preserves_reference_descriptions() {
        let temp_dir = TempDir::new().unwrap();
        let manager = RepoConfigManager::new(temp_dir.path().to_path_buf());

        // Create v1 config with reference having description
        let v1_config = RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs::default(),
            requires: vec![RequiredMount {
                remote: "git@github.com:org/ref-repo.git".to_string(),
                mount_path: "references/ref-mount".to_string(),
                subpath: None,
                description: "Important reference repository".to_string(),
                optional: true,
                override_rules: None,
                sync: crate::config::SyncStrategy::None,
            }],
            rules: vec![],
        };

        // Save the v1 config
        manager.save(&v1_config).unwrap();

        // Load via load_desired_state()
        let ds = manager.load_desired_state().unwrap().unwrap();

        // Verify description is preserved in DesiredState.references
        assert!(ds.was_v1);
        assert_eq!(ds.references.len(), 1);
        assert_eq!(ds.references[0].remote, "git@github.com:org/ref-repo.git");
        assert_eq!(
            ds.references[0].description.as_deref(),
            Some("Important reference repository")
        );
    }

    #[test]
    fn test_validate_v2_soft_handles_both_variants() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());

        let cfg = RepoConfigV2 {
            version: "2.0".into(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![
                ReferenceEntry::Simple("https://github.com/org/repo".into()),
                ReferenceEntry::WithMetadata(ReferenceMount {
                    remote: "git@github.com:org/repo.git:docs".into(), // invalid: subpath
                    description: None,
                }),
            ],
        };

        let warnings = mgr.validate_v2_soft(&cfg);
        assert_eq!(warnings.len(), 1, "Expected one invalid reference warning");
        assert!(warnings[0].contains("git@github.com:org/repo.git:docs"));
    }

    #[test]
    fn test_peek_config_version_returns_none_when_no_config() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        assert_eq!(mgr.peek_config_version().unwrap(), None);
    }

    #[test]
    fn test_peek_config_version_returns_v1() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let v1_config = RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs::default(),
            requires: vec![],
            rules: vec![],
        };
        mgr.save(&v1_config).unwrap();
        assert_eq!(mgr.peek_config_version().unwrap(), Some("1.0".to_string()));
    }

    #[test]
    fn test_peek_config_version_returns_v2() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let v2_config = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![],
        };
        mgr.save_v2(&v2_config).unwrap();
        assert_eq!(mgr.peek_config_version().unwrap(), Some("2.0".to_string()));
    }

    #[test]
    fn test_validate_v2_hard_rejects_invalid_version() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "3.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unsupported configuration version: 3.0")
        );
    }

    #[test]
    fn test_validate_v2_hard_rejects_empty_mount_dirs() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2 {
                thoughts: "".to_string(),
                context: "context".to_string(),
                references: "references".to_string(),
            },
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_v2_hard_rejects_reserved_mount_dir_name() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2 {
                thoughts: ".thoughts-data".to_string(),
                context: "context".to_string(),
                references: "references".to_string(),
            },
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".thoughts-data"));
    }

    #[test]
    fn test_validate_v2_hard_rejects_dot_mount_dirs() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2 {
                thoughts: ".".to_string(),
                context: "context".to_string(),
                references: "references".to_string(),
            },
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot be '.' or '..'")
        );
    }

    #[test]
    fn test_validate_v2_hard_rejects_multi_segment_mount_dirs() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2 {
                thoughts: "sub/path".to_string(),
                context: "context".to_string(),
                references: "references".to_string(),
            },
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("must be a single path segment")
        );
    }

    #[test]
    fn test_validate_v2_hard_rejects_duplicate_mount_dirs() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2 {
                thoughts: "same".to_string(),
                context: "same".to_string(),
                references: "references".to_string(),
            },
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be distinct"));
    }

    #[test]
    fn test_validate_v2_hard_rejects_invalid_thoughts_mount_remote() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: Some(ThoughtsMount {
                remote: "invalid-url".to_string(),
                subpath: None,
                sync: SyncStrategy::Auto,
            }),
            context_mounts: vec![],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid remote URL")
        );
    }

    #[test]
    fn test_validate_v2_hard_rejects_duplicate_context_mount_path() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![
                ContextMount {
                    remote: "git@github.com:org/repo1.git".to_string(),
                    subpath: None,
                    mount_path: "same".to_string(),
                    sync: SyncStrategy::Auto,
                },
                ContextMount {
                    remote: "git@github.com:org/repo2.git".to_string(),
                    subpath: None,
                    mount_path: "same".to_string(),
                    sync: SyncStrategy::Auto,
                },
            ],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Duplicate context mount path")
        );
    }

    #[test]
    fn test_validate_v2_hard_rejects_invalid_context_remote() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![ContextMount {
                remote: "invalid-url".to_string(),
                subpath: None,
                mount_path: "mount1".to_string(),
                sync: SyncStrategy::Auto,
            }],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid remote URL")
        );
    }

    #[test]
    fn test_validate_v2_hard_warns_on_sync_none() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![ContextMount {
                remote: "git@github.com:org/repo.git".to_string(),
                subpath: None,
                mount_path: "mount1".to_string(),
                sync: SyncStrategy::None,
            }],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("sync:None"));
        assert!(warnings[0].contains("discouraged"));
    }

    #[test]
    fn test_validate_v2_hard_rejects_invalid_reference_url() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![ReferenceEntry::Simple(
                "git@github.com:org/repo.git:subpath".to_string(),
            )],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("subpath"));
    }

    #[test]
    fn test_validate_v2_hard_rejects_duplicate_references() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![
                ReferenceEntry::Simple("git@github.com:Org/Repo.git".to_string()),
                ReferenceEntry::Simple("https://github.com/org/repo".to_string()),
            ],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Duplicate reference")
        );
    }

    #[test]
    fn test_validate_v2_hard_accepts_valid_config() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: Some(ThoughtsMount {
                remote: "git@github.com:user/thoughts.git".to_string(),
                subpath: None,
                sync: SyncStrategy::Auto,
            }),
            context_mounts: vec![ContextMount {
                remote: "git@github.com:org/context.git".to_string(),
                subpath: Some("docs".to_string()),
                mount_path: "docs".to_string(),
                sync: SyncStrategy::Auto,
            }],
            references: vec![
                ReferenceEntry::Simple("git@github.com:org/repo1.git".to_string()),
                ReferenceEntry::WithMetadata(ReferenceMount {
                    remote: "https://github.com/org/repo2".to_string(),
                    description: Some("Reference 2".to_string()),
                }),
            ],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_save_v2_validated_fails_before_write_on_invalid() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2 {
                thoughts: "same".to_string(),
                context: "same".to_string(),
                references: "references".to_string(),
            },
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![],
        };

        let result = mgr.save_v2_validated(&cfg);
        assert!(result.is_err());

        // Verify no file was written
        let config_path = paths::get_repo_config_path(temp_dir.path());
        assert!(!config_path.exists());
    }

    #[test]
    fn test_save_v2_validated_returns_warnings_on_valid() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![ContextMount {
                remote: "git@github.com:org/repo.git".to_string(),
                subpath: None,
                mount_path: "mount1".to_string(),
                sync: SyncStrategy::None,
            }],
            references: vec![],
        };

        let result = mgr.save_v2_validated(&cfg);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("sync:None"));

        // Verify file was written
        let config_path = paths::get_repo_config_path(temp_dir.path());
        assert!(config_path.exists());
    }

    #[test]
    fn test_ensure_v2_default_migrates_v1_without_panic() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());

        // Create v1 config with one context mount and one reference
        let v1_config = RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs {
                repository: "mycontext".to_string(),
                personal: "personal".to_string(),
            },
            requires: vec![
                RequiredMount {
                    remote: "git@github.com:org/context-repo.git".to_string(),
                    mount_path: "docs".to_string(),
                    subpath: Some("content".to_string()),
                    description: "Documentation".to_string(),
                    optional: false,
                    override_rules: None,
                    sync: SyncStrategy::Auto,
                },
                RequiredMount {
                    remote: "git@github.com:org/ref-repo.git".to_string(),
                    mount_path: "references/ref".to_string(),
                    subpath: None,
                    description: "Reference repo".to_string(),
                    optional: true,
                    override_rules: None,
                    sync: SyncStrategy::None,
                },
            ],
            rules: vec![],
        };

        // Save v1 config
        mgr.save(&v1_config).unwrap();

        // Call ensure_v2_default() - should migrate without panic
        let v2_config = mgr.ensure_v2_default().unwrap();

        // Verify migration succeeded
        assert_eq!(v2_config.version, "2.0");
        assert_eq!(v2_config.mount_dirs.context, "mycontext");
        assert_eq!(v2_config.context_mounts.len(), 1);
        assert_eq!(v2_config.context_mounts[0].mount_path, "docs");
        assert_eq!(
            v2_config.context_mounts[0].subpath,
            Some("content".to_string())
        );
        assert_eq!(v2_config.references.len(), 1);

        // Verify config file was updated to v2
        let ds = mgr.load_desired_state().unwrap().unwrap();
        assert!(!ds.was_v1); // Now it's v2 on disk
        assert_eq!(ds.context_mounts.len(), 1);
        assert_eq!(ds.references.len(), 1);
    }

    #[test]
    fn test_validate_v2_hard_rejects_empty_context_mount_path() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![ContextMount {
                remote: "git@github.com:org/repo.git".to_string(),
                subpath: None,
                mount_path: "  ".to_string(), // whitespace-only
                sync: SyncStrategy::Auto,
            }],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validate_v2_hard_rejects_dot_context_mount_path() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![ContextMount {
                remote: "git@github.com:org/repo.git".to_string(),
                subpath: None,
                mount_path: ".".to_string(),
                sync: SyncStrategy::Auto,
            }],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot be '.' or '..'")
        );
    }

    #[test]
    fn test_validate_v2_hard_rejects_dotdot_context_mount_path() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![ContextMount {
                remote: "git@github.com:org/repo.git".to_string(),
                subpath: None,
                mount_path: "..".to_string(),
                sync: SyncStrategy::Auto,
            }],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("cannot be '.' or '..'")
        );
    }

    #[test]
    fn test_validate_v2_hard_rejects_slash_in_context_mount_path() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![ContextMount {
                remote: "git@github.com:org/repo.git".to_string(),
                subpath: None,
                mount_path: "sub/path".to_string(),
                sync: SyncStrategy::Auto,
            }],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("single path segment")
        );
    }

    #[test]
    fn test_validate_v2_hard_rejects_backslash_in_context_mount_path() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![ContextMount {
                remote: "git@github.com:org/repo.git".to_string(),
                subpath: None,
                mount_path: "sub\\path".to_string(),
                sync: SyncStrategy::Auto,
            }],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("single path segment")
        );
    }

    #[test]
    fn test_validate_v2_hard_accepts_valid_context_mount_path() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let mgr = RepoConfigManager::new(temp_dir.path().to_path_buf());
        let cfg = RepoConfigV2 {
            version: "2.0".to_string(),
            mount_dirs: MountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![ContextMount {
                remote: "git@github.com:org/repo.git".to_string(),
                subpath: None,
                mount_path: "docs".to_string(),
                sync: SyncStrategy::Auto,
            }],
            references: vec![],
        };
        let result = mgr.validate_v2_hard(&cfg);
        assert!(result.is_ok());
        let warnings = result.unwrap();
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn test_new_makes_absolute_when_given_relative_repo_root() {
        let temp_dir = TempDir::new().unwrap();
        let cwd_before = std::env::current_dir().unwrap();

        // Change cwd to temp_dir so a relative path exists
        std::env::set_current_dir(temp_dir.path()).unwrap();

        // Create a subdir to use as repo root
        std::fs::create_dir_all("repo").unwrap();

        let mgr = RepoConfigManager::new(PathBuf::from("repo"));

        // repo_root field is private but we can verify via behavior
        // The test passes if construction succeeds (no panic) and
        // subsequent operations work correctly
        assert!(mgr.peek_config_version().is_ok());

        // Restore cwd
        std::env::set_current_dir(cwd_before).unwrap();
    }
}
