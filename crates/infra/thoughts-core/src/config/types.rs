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
    #[cfg(test)] // Only used in tests
    pub fn is_git(&self) -> bool {
        matches!(self, Mount::Git { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum SyncStrategy {
    #[default]
    None,
    Auto,
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

// Note: V1 config types (RepoConfig, MountDirs, RequiredMount, PersonalConfig,
// MountPattern, PersonalMount, Rule, FileMetadata) have been removed.
// See CLAUDE.md for V2 config API guidance.

/// Maps git URLs to local filesystem paths
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RepoMapping {
    pub version: String,
    pub mappings: HashMap<String, RepoLocation>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

    // Note: V1 config tests (test_repo_config_serialization, test_personal_config_serialization,
    // test_mount_dirs_defaults, test_file_metadata_serialization) have been removed.

    #[test]
    fn test_reference_entry_deserialize_simple() {
        let json = r#""git@github.com:org/repo.git""#;
        let entry: ReferenceEntry = serde_json::from_str(json).unwrap();
        assert!(matches!(entry, ReferenceEntry::Simple(_)));
        if let ReferenceEntry::Simple(url) = entry {
            assert_eq!(url, "git@github.com:org/repo.git");
        }
    }

    #[test]
    fn test_reference_entry_deserialize_with_metadata() {
        let json = r#"{"remote": "https://github.com/org/repo.git", "description": "Test repo"}"#;
        let entry: ReferenceEntry = serde_json::from_str(json).unwrap();
        match entry {
            ReferenceEntry::WithMetadata(rm) => {
                assert_eq!(rm.remote, "https://github.com/org/repo.git");
                assert_eq!(rm.description.as_deref(), Some("Test repo"));
            }
            _ => panic!("Expected WithMetadata"),
        }
    }

    #[test]
    fn test_reference_entry_deserialize_with_metadata_no_description() {
        let json = r#"{"remote": "https://github.com/org/repo.git"}"#;
        let entry: ReferenceEntry = serde_json::from_str(json).unwrap();
        match entry {
            ReferenceEntry::WithMetadata(rm) => {
                assert_eq!(rm.remote, "https://github.com/org/repo.git");
                assert_eq!(rm.description, None);
            }
            _ => panic!("Expected WithMetadata"),
        }
    }

    #[test]
    fn test_reference_entry_mixed_array() {
        let json = r#"[
            "git@github.com:org/ref1.git",
            {"remote": "https://github.com/org/ref2.git", "description": "Ref 2"}
        ]"#;
        let entries: Vec<ReferenceEntry> = serde_json::from_str(json).unwrap();
        assert_eq!(entries.len(), 2);

        // First should be Simple
        assert!(matches!(entries[0], ReferenceEntry::Simple(_)));

        // Second should be WithMetadata
        match &entries[1] {
            ReferenceEntry::WithMetadata(rm) => {
                assert_eq!(rm.remote, "https://github.com/org/ref2.git");
                assert_eq!(rm.description.as_deref(), Some("Ref 2"));
            }
            _ => panic!("Expected WithMetadata"),
        }
    }

    #[test]
    fn test_repo_config_v2_with_reference_entries() {
        let json = r#"{
            "version": "2.0",
            "mount_dirs": {},
            "context_mounts": [],
            "references": [
                "git@github.com:org/ref1.git",
                {"remote": "https://github.com/org/ref2.git", "description": "Reference 2"}
            ]
        }"#;

        let config: RepoConfigV2 = serde_json::from_str(json).unwrap();
        assert_eq!(config.version, "2.0");
        assert_eq!(config.references.len(), 2);

        // Verify first reference (simple)
        assert!(matches!(config.references[0], ReferenceEntry::Simple(_)));

        // Verify second reference (with metadata)
        match &config.references[1] {
            ReferenceEntry::WithMetadata(rm) => {
                assert_eq!(rm.description.as_deref(), Some("Reference 2"));
            }
            _ => panic!("Expected WithMetadata"),
        }
    }

    #[test]
    fn test_cross_variant_duplicate_detection() {
        use crate::config::validation::canonical_reference_key;
        use std::collections::HashSet;

        let entries = vec![
            ReferenceEntry::Simple("git@github.com:User/Repo.git".to_string()),
            ReferenceEntry::WithMetadata(ReferenceMount {
                remote: "https://github.com/user/repo".to_string(),
                description: Some("Same repo".into()),
            }),
        ];

        let mut keys = HashSet::new();
        for e in &entries {
            let url = match e {
                ReferenceEntry::Simple(s) => s.as_str(),
                ReferenceEntry::WithMetadata(rm) => rm.remote.as_str(),
            };
            let key = canonical_reference_key(url).unwrap();
            keys.insert(key);
        }

        // Both variants should normalize to the same canonical key
        assert_eq!(keys.len(), 1);
    }
}

// New v2 config types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountDirsV2 {
    #[serde(default = "default_thoughts_dir")]
    pub thoughts: String,
    #[serde(default = "default_context_dir")]
    pub context: String,
    #[serde(default = "default_references_dir")]
    pub references: String,
}

impl Default for MountDirsV2 {
    fn default() -> Self {
        Self {
            thoughts: default_thoughts_dir(),
            context: default_context_dir(),
            references: default_references_dir(),
        }
    }
}

fn default_thoughts_dir() -> String {
    "thoughts".into()
}
fn default_context_dir() -> String {
    "context".into()
}
fn default_references_dir() -> String {
    "references".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThoughtsMount {
    pub remote: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    #[serde(default = "default_git_sync")]
    pub sync: SyncStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMount {
    pub remote: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subpath: Option<String>,
    pub mount_path: String,
    #[serde(default = "default_git_sync")]
    pub sync: SyncStrategy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceMount {
    pub remote: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ReferenceEntry {
    Simple(String),
    WithMetadata(ReferenceMount),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfigV2 {
    pub version: String, // "2.0"
    #[serde(default)]
    pub mount_dirs: MountDirsV2,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thoughts_mount: Option<ThoughtsMount>,
    #[serde(default)]
    pub context_mounts: Vec<ContextMount>,
    #[serde(default)]
    pub references: Vec<ReferenceEntry>,
}
