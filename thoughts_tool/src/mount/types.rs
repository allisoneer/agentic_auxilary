use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

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
#[derive(Debug, Clone, Default)]
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

impl MountOptions {
    pub fn new() -> Self {
        Self {
            read_only: false,
            allow_other: false,
            volume_name: None,
            extra_options: Vec::new(),
            timeout: Some(crate::platform::common::MOUNT_TIMEOUT),
            retries: crate::platform::common::MAX_MOUNT_RETRIES,
        }
    }

    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn allow_other(mut self) -> Self {
        self.allow_other = true;
        self
    }

    pub fn with_volume_name(mut self, name: String) -> Self {
        self.volume_name = Some(name);
        self
    }

    pub fn with_extra_options(mut self, options: Vec<String>) -> Self {
        self.extra_options = options;
        self
    }
}

/// Result of a mount operation attempt
#[derive(Debug)]
pub struct MountAttempt {
    pub success: bool,
    pub command: String,
    pub stdout: String,
    pub stderr: String,
    pub duration: std::time::Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_options_builder() {
        let options = MountOptions::new()
            .read_only()
            .allow_other()
            .with_volume_name("TestVolume".to_string());

        assert!(options.read_only);
        assert!(options.allow_other);
        assert_eq!(options.volume_name, Some("TestVolume".to_string()));
    }

    #[test]
    fn test_mount_status_serialization() {
        let status = MountStatus::Mounted;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: MountStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(status, deserialized);
    }
}
