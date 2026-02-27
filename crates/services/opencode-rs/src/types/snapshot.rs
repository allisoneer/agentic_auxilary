//! Snapshot types for `opencode_rs`.
//!
//! Types for file snapshots and diffs.

use serde::{Deserialize, Serialize};

/// Request to track files for snapshotting.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotTrackRequest {
    /// Files to track.
    pub files: Vec<String>,

    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Response from track operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotTrackResponse {
    /// Number of files tracked.
    #[serde(default)]
    pub tracked: u32,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Request for snapshot patch operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPatchRequest {
    /// Session ID for the snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Message ID for the snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,

    /// Additional fields.
    #[serde(default, flatten)]
    pub extra: serde_json::Value,
}

/// A file patch/diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePatch {
    /// File path.
    pub path: String,

    /// Patch content (unified diff format).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<String>,

    /// Whether the file was added.
    #[serde(default)]
    pub added: bool,

    /// Whether the file was deleted.
    #[serde(default)]
    pub deleted: bool,

    /// Whether the file was modified.
    #[serde(default)]
    pub modified: bool,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Response containing patches.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPatchResponse {
    /// List of file patches.
    #[serde(default)]
    pub patches: Vec<FilePatch>,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Request for snapshot diff operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiffRequest {
    /// Session ID for the snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Message ID for the snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,

    /// Additional fields.
    #[serde(default, flatten)]
    pub extra: serde_json::Value,
}

/// A file diff entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    /// File path.
    pub path: String,

    /// Original content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original: Option<String>,

    /// Modified content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,

    /// Diff status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Response containing diffs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiffResponse {
    /// List of file diffs.
    #[serde(default)]
    pub diffs: Vec<FileDiff>,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Request for snapshot restore operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRestoreRequest {
    /// Session ID for the snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Message ID for the snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,

    /// Specific files to restore (empty = all).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,

    /// Additional fields.
    #[serde(default, flatten)]
    pub extra: serde_json::Value,
}

/// Response from restore operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRestoreResponse {
    /// Number of files restored.
    #[serde(default)]
    pub restored: u32,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Request for snapshot revert operation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRevertRequest {
    /// Session ID for the snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Message ID for the snapshot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,

    /// Additional fields.
    #[serde(default, flatten)]
    pub extra: serde_json::Value,
}

/// Response from revert operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRevertResponse {
    /// Whether the revert was successful.
    #[serde(default)]
    pub success: bool,

    /// Number of files reverted.
    #[serde(default)]
    pub reverted: u32,

    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_track_request() {
        let req = SnapshotTrackRequest {
            files: vec!["src/main.rs".to_string(), "Cargo.toml".to_string()],
            extra: serde_json::Value::Null,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("src/main.rs"));
    }

    #[test]
    fn test_file_patch() {
        let json = r#"{
            "path": "src/lib.rs",
            "patch": "@@ -1,3 +1,4 @@\n+use serde;",
            "modified": true
        }"#;
        let patch: FilePatch = serde_json::from_str(json).unwrap();
        assert_eq!(patch.path, "src/lib.rs");
        assert!(patch.modified);
        assert!(!patch.added);
        assert!(!patch.deleted);
    }

    #[test]
    fn test_file_diff() {
        let json = r##"{
            "path": "README.md",
            "original": "# Old Title",
            "modified": "# New Title",
            "status": "modified"
        }"##;
        let diff: FileDiff = serde_json::from_str(json).unwrap();
        assert_eq!(diff.path, "README.md");
        assert_eq!(diff.original, Some("# Old Title".to_string()));
        assert_eq!(diff.modified, Some("# New Title".to_string()));
        assert_eq!(diff.status, Some("modified".to_string()));
    }

    #[test]
    fn test_snapshot_restore_response() {
        let json = r#"{"restored": 5}"#;
        let resp: SnapshotRestoreResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.restored, 5);
    }

    #[test]
    fn test_snapshot_revert_response() {
        let json = r#"{"success": true, "reverted": 3}"#;
        let resp: SnapshotRevertResponse = serde_json::from_str(json).unwrap();
        assert!(resp.success);
        assert_eq!(resp.reverted, 3);
    }
}
