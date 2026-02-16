//! Migration from legacy V2 `.thoughts/config.json` to `agentic.json`.
//!
//! This module handles one-time, idempotent migration of V2 thoughts repo configs
//! to the new unified `agentic.json` format. V1 configs are not supported and
//! will produce an error.

use crate::types::{
    ContextMount, ReferenceEntry, ReferenceMount, SyncStrategy, ThoughtsConfig, ThoughtsMount,
    ThoughtsMountDirs,
};
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

/// Internal struct for reading legacy V2 `.thoughts/config.json`.
///
/// This mirrors the old `RepoConfigV2` structure from thoughts-core but is
/// defined here to avoid circular dependencies.
#[derive(Debug, Deserialize)]
pub struct LegacyRepoConfigV2 {
    pub version: String,
    #[serde(default)]
    pub mount_dirs: LegacyMountDirsV2,
    #[serde(default)]
    pub thoughts_mount: Option<LegacyThoughtsMount>,
    #[serde(default)]
    pub context_mounts: Vec<LegacyContextMount>,
    #[serde(default)]
    pub references: Vec<LegacyReferenceEntry>,
}

#[derive(Debug, Default, Deserialize)]
pub struct LegacyMountDirsV2 {
    #[serde(default = "default_thoughts_dir")]
    pub thoughts: String,
    #[serde(default = "default_context_dir")]
    pub context: String,
    #[serde(default = "default_references_dir")]
    pub references: String,
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

#[derive(Debug, Deserialize)]
pub struct LegacyThoughtsMount {
    pub remote: String,
    #[serde(default)]
    pub subpath: Option<String>,
    #[serde(default)]
    pub sync: LegacySyncStrategy,
}

#[derive(Debug, Deserialize)]
pub struct LegacyContextMount {
    pub remote: String,
    #[serde(default)]
    pub subpath: Option<String>,
    pub mount_path: String,
    #[serde(default)]
    pub sync: LegacySyncStrategy,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum LegacyReferenceEntry {
    Simple(String),
    WithMetadata(LegacyReferenceMount),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LegacyReferenceMount {
    pub remote: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacySyncStrategy {
    #[default]
    None,
    Auto,
}

impl From<LegacySyncStrategy> for SyncStrategy {
    fn from(s: LegacySyncStrategy) -> Self {
        match s {
            LegacySyncStrategy::None => SyncStrategy::None,
            LegacySyncStrategy::Auto => SyncStrategy::Auto,
        }
    }
}

/// Path to legacy V2 config file.
pub fn legacy_thoughts_v2_path(local_dir: &Path) -> std::path::PathBuf {
    local_dir.join(".thoughts").join("config.json")
}

/// Check if migration is needed and possible.
///
/// Returns `Some(legacy_path)` if migration should be attempted, `None` otherwise.
pub fn should_migrate(local_dir: &Path, agentic_path: &Path) -> Option<std::path::PathBuf> {
    // If agentic.json already exists, no migration needed
    if agentic_path.exists() {
        return None;
    }

    let legacy = legacy_thoughts_v2_path(local_dir);
    if legacy.exists() { Some(legacy) } else { None }
}

/// Read and parse the legacy V2 config, returning an error if it's V1.
pub fn read_legacy_v2(path: &Path) -> Result<LegacyRepoConfigV2> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read legacy config: {}", path.display()))?;

    let v: Value = serde_json::from_str(&raw)
        .with_context(|| format!("Invalid JSON in legacy config: {}", path.display()))?;

    // Check version
    let version = v.get("version").and_then(|x| x.as_str()).unwrap_or("1.0");

    if version != "2.0" {
        anyhow::bail!(
            "Unsupported legacy config version: {} (only v2 is supported for migration)",
            version
        );
    }

    serde_json::from_value(v)
        .with_context(|| format!("Failed to parse legacy V2 config: {}", path.display()))
}

/// Map legacy V2 config to the new `agentic.json` format.
///
/// Returns a JSON Value that can be written to `agentic.json`.
pub fn map_v2_to_agentic_value(v2: LegacyRepoConfigV2) -> Result<Value> {
    let thoughts_config = map_v2_to_thoughts_config(v2);

    Ok(json!({
        "thoughts": thoughts_config
    }))
}

/// Map legacy V2 config to ThoughtsConfig struct.
pub fn map_v2_to_thoughts_config(v2: LegacyRepoConfigV2) -> ThoughtsConfig {
    ThoughtsConfig {
        mount_dirs: ThoughtsMountDirs {
            thoughts: v2.mount_dirs.thoughts,
            context: v2.mount_dirs.context,
            references: v2.mount_dirs.references,
        },
        thoughts_mount: v2.thoughts_mount.map(|tm| ThoughtsMount {
            remote: tm.remote,
            subpath: tm.subpath,
            sync: tm.sync.into(),
        }),
        context_mounts: v2
            .context_mounts
            .into_iter()
            .map(|cm| ContextMount {
                remote: cm.remote,
                subpath: cm.subpath,
                mount_path: cm.mount_path,
                sync: cm.sync.into(),
            })
            .collect(),
        references: v2
            .references
            .into_iter()
            .map(|r| match r {
                LegacyReferenceEntry::Simple(url) => ReferenceEntry::Simple(url),
                LegacyReferenceEntry::WithMetadata(rm) => {
                    ReferenceEntry::WithMetadata(ReferenceMount {
                        remote: rm.remote,
                        description: rm.description,
                    })
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_v2_config(dir: &Path, content: &str) {
        let thoughts_dir = dir.join(".thoughts");
        std::fs::create_dir_all(&thoughts_dir).unwrap();
        std::fs::write(thoughts_dir.join("config.json"), content).unwrap();
    }

    #[test]
    fn test_should_migrate_no_files() {
        let temp = TempDir::new().unwrap();
        let agentic_path = temp.path().join("agentic.json");
        assert!(should_migrate(temp.path(), &agentic_path).is_none());
    }

    #[test]
    fn test_should_migrate_agentic_exists() {
        let temp = TempDir::new().unwrap();
        std::fs::write(temp.path().join("agentic.json"), "{}").unwrap();
        create_v2_config(temp.path(), r#"{"version": "2.0"}"#);

        let agentic_path = temp.path().join("agentic.json");
        assert!(should_migrate(temp.path(), &agentic_path).is_none());
    }

    #[test]
    fn test_should_migrate_legacy_exists() {
        let temp = TempDir::new().unwrap();
        create_v2_config(temp.path(), r#"{"version": "2.0"}"#);

        let agentic_path = temp.path().join("agentic.json");
        assert!(should_migrate(temp.path(), &agentic_path).is_some());
    }

    #[test]
    fn test_read_legacy_v2_rejects_v1() {
        let temp = TempDir::new().unwrap();
        create_v2_config(temp.path(), r#"{"version": "1.0"}"#);

        let legacy_path = legacy_thoughts_v2_path(temp.path());
        let result = read_legacy_v2(&legacy_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("only v2"));
    }

    #[test]
    fn test_read_legacy_v2_success() {
        let temp = TempDir::new().unwrap();
        let config = r#"{
            "version": "2.0",
            "mount_dirs": {
                "thoughts": "my-thoughts",
                "context": "ctx",
                "references": "refs"
            },
            "thoughts_mount": {
                "remote": "git@github.com:user/thoughts.git",
                "sync": "auto"
            },
            "context_mounts": [
                {
                    "remote": "https://github.com/team/docs.git",
                    "mount_path": "team-docs",
                    "sync": "auto"
                }
            ],
            "references": [
                "https://github.com/rust-lang/rust.git",
                {"remote": "https://github.com/tokio-rs/tokio.git", "description": "Async runtime"}
            ]
        }"#;
        create_v2_config(temp.path(), config);

        let legacy_path = legacy_thoughts_v2_path(temp.path());
        let v2 = read_legacy_v2(&legacy_path).unwrap();

        assert_eq!(v2.version, "2.0");
        assert_eq!(v2.mount_dirs.thoughts, "my-thoughts");
        assert!(v2.thoughts_mount.is_some());
        assert_eq!(v2.context_mounts.len(), 1);
        assert_eq!(v2.references.len(), 2);
    }

    #[test]
    fn test_map_v2_to_agentic_value() {
        let v2 = LegacyRepoConfigV2 {
            version: "2.0".into(),
            mount_dirs: LegacyMountDirsV2 {
                thoughts: "thoughts".into(),
                context: "context".into(),
                references: "references".into(),
            },
            thoughts_mount: Some(LegacyThoughtsMount {
                remote: "git@github.com:user/thoughts.git".into(),
                subpath: None,
                sync: LegacySyncStrategy::Auto,
            }),
            context_mounts: vec![LegacyContextMount {
                remote: "https://github.com/team/docs.git".into(),
                subpath: None,
                mount_path: "team-docs".into(),
                sync: LegacySyncStrategy::Auto,
            }],
            references: vec![LegacyReferenceEntry::Simple(
                "https://github.com/rust-lang/rust.git".into(),
            )],
        };

        let value = map_v2_to_agentic_value(v2).unwrap();

        // Verify the structure
        assert!(value.get("thoughts").is_some());
        let thoughts = value.get("thoughts").unwrap();
        assert!(thoughts.get("mount_dirs").is_some());
        assert!(thoughts.get("thoughts_mount").is_some());
        assert!(thoughts.get("context_mounts").is_some());
        assert!(thoughts.get("references").is_some());
    }

    #[test]
    fn test_map_v2_preserves_references_with_metadata() {
        let v2 = LegacyRepoConfigV2 {
            version: "2.0".into(),
            mount_dirs: LegacyMountDirsV2::default(),
            thoughts_mount: None,
            context_mounts: vec![],
            references: vec![
                LegacyReferenceEntry::Simple("https://github.com/org/simple.git".into()),
                LegacyReferenceEntry::WithMetadata(LegacyReferenceMount {
                    remote: "https://github.com/org/with-desc.git".into(),
                    description: Some("Has description".into()),
                }),
            ],
        };

        let config = map_v2_to_thoughts_config(v2);

        assert_eq!(config.references.len(), 2);
        assert!(matches!(&config.references[0], ReferenceEntry::Simple(_)));
        match &config.references[1] {
            ReferenceEntry::WithMetadata(rm) => {
                assert_eq!(rm.description.as_deref(), Some("Has description"));
            }
            _ => panic!("Expected WithMetadata"),
        }
    }
}
