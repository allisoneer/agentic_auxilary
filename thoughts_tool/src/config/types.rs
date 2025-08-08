use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: String,
    pub mounts: HashMap<String, Mount>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: "2.0".to_string(), // New version for URL-based configs
            mounts: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Mount {
    Directory {
        path: PathBuf,
        #[serde(default)]
        sync: SyncStrategy,
    },
    Git {
        url: String, // ONLY the URL - no paths!
        #[serde(default = "default_git_sync")]
        sync: SyncStrategy,
        #[serde(skip_serializing_if = "Option::is_none")]
        subpath: Option<String>, // For mounts like "url:docs/api"
    },
}

fn default_git_sync() -> SyncStrategy {
    SyncStrategy::Auto
}

// Helper methods for compatibility with existing code
impl Mount {
    pub fn mount_type(&self) -> MountType {
        match self {
            Mount::Directory { .. } => MountType::Directory,
            Mount::Git { .. } => MountType::Git,
        }
    }

    pub fn sync_strategy(&self) -> SyncStrategy {
        match self {
            Mount::Directory { sync, .. } => *sync,
            Mount::Git { sync, .. } => *sync,
        }
    }

    pub fn is_git(&self) -> bool {
        matches!(self, Mount::Git { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountType {
    Directory,
    Git,
}

impl std::str::FromStr for MountType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "directory" | "dir" => Ok(MountType::Directory),
            "git" => Ok(MountType::Git),
            _ => Err(anyhow::anyhow!(
                "Invalid mount type: {}. Must be 'directory' or 'git'",
                s
            )),
        }
    }
}

impl std::fmt::Display for MountType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MountType::Directory => write!(f, "directory"),
            MountType::Git => write!(f, "git"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStrategy {
    None,
    Auto,
}

impl Default for SyncStrategy {
    fn default() -> Self {
        SyncStrategy::None
    }
}

impl std::str::FromStr for SyncStrategy {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "none" => Ok(SyncStrategy::None),
            "auto" => Ok(SyncStrategy::Auto),
            _ => Err(anyhow::anyhow!(
                "Invalid sync strategy: {}. Must be 'none' or 'auto'",
                s
            )),
        }
    }
}

impl std::fmt::Display for SyncStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncStrategy::None => write!(f, "none"),
            SyncStrategy::Auto => write!(f, "auto"),
        }
    }
}

// Add after line 101 (after existing types)
// use std::collections::HashMap; - already imported at the top

// New configuration structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub version: String,
    #[serde(default)]
    pub mount_dirs: MountDirs,
    #[serde(default)]
    pub requires: Vec<RequiredMount>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountDirs {
    #[serde(default = "default_repository_dir")]
    pub repository: String,
    #[serde(default = "default_personal_dir")]
    pub personal: String,
}

impl Default for MountDirs {
    fn default() -> Self {
        Self {
            repository: default_repository_dir(),
            personal: default_personal_dir(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredMount {
    pub remote: String,
    pub mount_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    pub description: String,
    #[serde(default)]
    pub optional: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_rules: Option<bool>,
    #[serde(default = "default_sync_strategy")]
    pub sync: SyncStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalConfig {
    #[serde(default)]
    pub patterns: Vec<MountPattern>,
    #[serde(default)]
    pub repository_mounts: HashMap<String, Vec<PersonalMount>>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub default_mount_dirs: MountDirs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountPattern {
    pub match_remote: String,
    pub personal_mounts: Vec<PersonalMount>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalMount {
    pub remote: String,
    pub mount_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub pattern: String,
    pub metadata: HashMap<String, serde_json::Value>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    #[serde(flatten)]
    pub auto_metadata: HashMap<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub manual_metadata: HashMap<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<chrono::DateTime<chrono::Utc>>,
}

fn default_repository_dir() -> String {
    "context".to_string()
}

fn default_personal_dir() -> String {
    "personal".to_string()
}

fn default_sync_strategy() -> SyncStrategy {
    SyncStrategy::None
}

/// Maps git URLs to local filesystem paths
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoMapping {
    pub version: String,
    pub mappings: HashMap<String, RepoLocation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoLocation {
    pub path: PathBuf,
    pub auto_managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sync: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for RepoMapping {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            mappings: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_repo_config_serialization() {
        let config = RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs::default(),
            requires: vec![RequiredMount {
                remote: "git@github.com:example/repo.git".to_string(),
                mount_path: "repo".to_string(),
                subpath: None,
                description: "Example repository".to_string(),
                optional: false,
                override_rules: Some(true),
                sync: SyncStrategy::Auto,
            }],
            rules: vec![Rule {
                pattern: "*.md".to_string(),
                metadata: {
                    let mut m = HashMap::new();
                    m.insert(
                        "type".to_string(),
                        serde_json::Value::String("documentation".to_string()),
                    );
                    m
                },
                description: "Markdown files".to_string(),
            }],
        };

        // Serialize
        let json = serde_json::to_string_pretty(&config).unwrap();

        // Deserialize
        let deserialized: RepoConfig = serde_json::from_str(&json).unwrap();

        // Verify
        assert_eq!(deserialized.version, config.version);
        assert_eq!(
            deserialized.mount_dirs.repository,
            config.mount_dirs.repository
        );
        assert_eq!(deserialized.mount_dirs.personal, config.mount_dirs.personal);
        assert_eq!(deserialized.requires.len(), config.requires.len());
        assert_eq!(deserialized.rules.len(), config.rules.len());
    }

    #[test]
    fn test_personal_config_serialization() {
        let config = PersonalConfig {
            patterns: vec![MountPattern {
                match_remote: "git@github.com:mycompany/*".to_string(),
                personal_mounts: vec![PersonalMount {
                    remote: "git@github.com:me/notes.git".to_string(),
                    mount_path: "notes".to_string(),
                    subpath: None,
                    description: "My notes".to_string(),
                }],
                description: "Company projects".to_string(),
            }],
            repository_mounts: {
                let mut m = HashMap::new();
                m.insert(
                    "git@github.com:example/project.git".to_string(),
                    vec![PersonalMount {
                        remote: "git@github.com:me/personal.git".to_string(),
                        mount_path: "personal".to_string(),
                        subpath: None,
                        description: "Personal files".to_string(),
                    }],
                );
                m
            },
            rules: vec![],
            default_mount_dirs: MountDirs::default(),
        };

        // Serialize
        let json = serde_json::to_string_pretty(&config).unwrap();

        // Deserialize
        let deserialized: PersonalConfig = serde_json::from_str(&json).unwrap();

        // Verify
        assert_eq!(deserialized.patterns.len(), config.patterns.len());
        assert_eq!(
            deserialized.repository_mounts.len(),
            config.repository_mounts.len()
        );
        assert_eq!(deserialized.rules.len(), config.rules.len());
    }

    #[test]
    fn test_mount_dirs_defaults() {
        let dirs = MountDirs::default();
        assert_eq!(dirs.repository, "context");
        assert_eq!(dirs.personal, "personal");
    }

    #[test]
    fn test_file_metadata_serialization() {
        let mut metadata = FileMetadata {
            auto_metadata: {
                let mut m = HashMap::new();
                m.insert(
                    "type".to_string(),
                    serde_json::Value::String("research".to_string()),
                );
                m.insert(
                    "tags".to_string(),
                    serde_json::Value::Array(vec![
                        serde_json::Value::String("important".to_string()),
                        serde_json::Value::String("review".to_string()),
                    ]),
                );
                m
            },
            manual_metadata: HashMap::new(),
            last_updated: Some(chrono::Utc::now()),
        };

        // Serialize
        let json = serde_json::to_string_pretty(&metadata).unwrap();

        // Deserialize
        let deserialized: FileMetadata = serde_json::from_str(&json).unwrap();

        // Verify
        assert_eq!(
            deserialized.auto_metadata.len(),
            metadata.auto_metadata.len()
        );
        assert!(deserialized.last_updated.is_some());
    }
}
