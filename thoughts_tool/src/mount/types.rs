use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::SystemTime;

use crate::platform::common::MAX_MOUNT_RETRIES;

/// Information about an active mount
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountInfo {
    /// Target mount point
    pub target: PathBuf,

    /// Source directories being merged
    pub sources: Vec<PathBuf>,

    /// Mount status
    pub status: MountStatus,

    /// Filesystem type (e.g., "fuse.mergerfs")
    pub fs_type: String,

    /// Mount options used
    pub options: Vec<String>,

    /// When the mount was created
    pub mounted_at: Option<SystemTime>,

    /// Process ID of the mount process (if applicable)
    pub pid: Option<u32>,

    /// Additional platform-specific metadata
    pub metadata: MountMetadata,
}

/// Mount status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MountStatus {
    /// Successfully mounted and accessible
    Mounted,

    /// Not currently mounted
    Unmounted,

    /// Mount exists but may have issues
    Degraded(String),

    /// Mount failed with error
    Error(String),

    /// Status cannot be determined
    Unknown,
}

/// Platform-specific mount metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MountMetadata {
    Linux {
        mount_id: Option<u32>,
        parent_id: Option<u32>,
        major_minor: Option<String>,
    },
    MacOS {
        volume_name: Option<String>,
        volume_uuid: Option<String>,
        disk_identifier: Option<String>,
    },
    Unknown,
}

/// Options for mount operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountOptions {
    /// Read-only mount
    pub read_only: bool,

    /// Allow other users to access the mount
    pub allow_other: bool,

    /// Custom volume name (macOS)
    pub volume_name: Option<String>,

    /// Additional platform-specific options
    pub extra_options: Vec<String>,

    /// Timeout for mount operation
    pub timeout: Option<std::time::Duration>,

    /// Number of retries on failure
    pub retries: u32,
}

impl Default for MountOptions {
    fn default() -> Self {
        Self {
            read_only: false,
            allow_other: false,
            volume_name: None,
            extra_options: Vec::new(),
            timeout: None,
            retries: MAX_MOUNT_RETRIES,
        }
    }
}

/// Mount state cache for persistence (macOS FUSE-T)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountStateCache {
    pub version: String,
    pub mounts: HashMap<PathBuf, CachedMountInfo>,
}

/// Cached information about a mount
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMountInfo {
    pub target: PathBuf,
    pub sources: Vec<PathBuf>,
    pub mount_options: MountOptions,
    pub created_at: SystemTime,
    pub mount_command: String,
    pub pid: Option<u32>,
}

use anyhow::Result;
use std::fmt;

/// Represents the different types of mount spaces in thoughts_tool.
///
/// The three-space architecture consists of:
/// - `Thoughts`: Single workspace for active development thoughts
/// - `Context`: Multiple mounts for team-shared documentation
/// - `Reference`: Read-only external repository references organized by org/repo
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MountSpace {
    /// The primary thoughts workspace mount
    Thoughts,

    /// A context mount with its mount path
    Context(String),

    /// A reference mount organized by organization and repository
    Reference {
        /// Organization or user name
        org: String,
        /// Repository name
        repo: String,
    },
}

impl MountSpace {
    /// Parse a mount identifier string into a MountSpace
    pub fn parse(input: &str) -> Result<Self> {
        if input == "thoughts" {
            Ok(MountSpace::Thoughts)
        } else if input.starts_with("references/") {
            let parts: Vec<&str> = input.splitn(3, '/').collect();
            if parts.len() == 3 && parts[0] == "references" {
                Ok(MountSpace::Reference {
                    org: parts[1].to_string(),
                    repo: parts[2].to_string(),
                })
            } else {
                anyhow::bail!("Invalid reference format: {}", input)
            }
        } else {
            // Assume it's a context mount
            Ok(MountSpace::Context(input.to_string()))
        }
    }

    /// Get the string identifier for this mount space
    pub fn as_str(&self) -> String {
        match self {
            MountSpace::Thoughts => "thoughts".to_string(),
            MountSpace::Context(path) => path.clone(),
            MountSpace::Reference { org, repo } => format!("references/{}/{}", org, repo),
        }
    }

    /// Get the relative path under .thoughts-data for this mount
    pub fn relative_path(&self, mount_dirs: &crate::config::MountDirsV2) -> String {
        match self {
            MountSpace::Thoughts => mount_dirs.thoughts.clone(),
            MountSpace::Context(path) => format!("{}/{}", mount_dirs.context, path),
            MountSpace::Reference { org, repo } => {
                format!("{}/{}/{}", mount_dirs.references, org, repo)
            }
        }
    }

    /// Check if this mount space should be read-only
    pub fn is_read_only(&self) -> bool {
        matches!(self, MountSpace::Reference { .. })
    }
}

impl fmt::Display for MountSpace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_options_default() {
        let options = MountOptions::default();

        assert!(!options.read_only);
        assert!(!options.allow_other);
        assert_eq!(options.retries, MAX_MOUNT_RETRIES);
        assert_eq!(options.volume_name, None);
    }

    #[test]
    fn test_mount_status_serialization() {
        let status = MountStatus::Mounted;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: MountStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_mount_space_parse() {
        // Test thoughts mount
        let thoughts = MountSpace::parse("thoughts").unwrap();
        assert_eq!(thoughts, MountSpace::Thoughts);

        // Test context mount
        let context = MountSpace::parse("api-docs").unwrap();
        assert_eq!(context, MountSpace::Context("api-docs".to_string()));

        // Test reference mount
        let reference = MountSpace::parse("references/github/example").unwrap();
        assert_eq!(
            reference,
            MountSpace::Reference {
                org: "github".to_string(),
                repo: "example".to_string(),
            }
        );

        // Test invalid reference format
        assert!(MountSpace::parse("references/invalid").is_err());
    }

    #[test]
    fn test_mount_space_as_str() {
        assert_eq!(MountSpace::Thoughts.as_str(), "thoughts");
        assert_eq!(MountSpace::Context("docs".to_string()).as_str(), "docs");
        assert_eq!(
            MountSpace::Reference {
                org: "org".to_string(),
                repo: "repo".to_string(),
            }
            .as_str(),
            "references/org/repo"
        );
    }

    #[test]
    fn test_mount_space_round_trip() {
        let cases = vec![
            ("thoughts", MountSpace::Thoughts),
            ("api-docs", MountSpace::Context("api-docs".to_string())),
            (
                "references/github/example",
                MountSpace::Reference {
                    org: "github".to_string(),
                    repo: "example".to_string(),
                },
            ),
        ];

        for (input, expected) in cases {
            let parsed = MountSpace::parse(input).unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(parsed.as_str(), input);
        }
    }

    #[test]
    fn test_mount_space_is_read_only() {
        assert!(!MountSpace::Thoughts.is_read_only());
        assert!(!MountSpace::Context("test".to_string()).is_read_only());
        assert!(
            MountSpace::Reference {
                org: "test".to_string(),
                repo: "repo".to_string(),
            }
            .is_read_only()
        );
    }
}
