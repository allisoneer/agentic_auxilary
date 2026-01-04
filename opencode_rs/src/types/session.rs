//! Session types for opencode_rs.

use crate::types::permission::Ruleset;
use serde::{Deserialize, Serialize};

/// A session in OpenCode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Unique session identifier.
    pub id: String,
    /// Project identifier (may not be present in all responses).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Working directory for the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    /// Parent session ID (for forked sessions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Session summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<SessionSummary>,
    /// Share information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<ShareInfo>,
    /// Session title.
    #[serde(default)]
    pub title: String,
    /// Session version.
    #[serde(default)]
    pub version: String,
    /// Timestamps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<SessionTime>,
    /// Pending permission ruleset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<Ruleset>,
    /// Revert information.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revert: Option<RevertInfo>,
    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Session summary with file changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    /// Lines added.
    pub additions: u64,
    /// Lines deleted.
    pub deletions: u64,
    /// Number of files changed.
    pub files: u64,
    /// File diffs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diffs: Option<Vec<FileDiffLite>>,
}

/// Lightweight file diff information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiffLite {
    /// File path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Content before changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Content after changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    /// Lines added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additions: Option<u64>,
    /// Lines deleted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deletions: Option<u64>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Share information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareInfo {
    /// Share secret (for editing).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// Share URL.
    pub url: String,
}

/// Session timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTime {
    /// Creation timestamp.
    pub created: i64,
    /// Last update timestamp.
    pub updated: i64,
    /// Compaction timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compacting: Option<i64>,
    /// Archive timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived: Option<i64>,
}

/// Revert information.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertInfo {
    /// Message ID to revert to.
    pub message_id: String,
    /// Part ID to revert to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_id: Option<String>,
    /// Snapshot ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<String>,
    /// Diff content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
}

/// Request to create a new session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    /// Parent session ID to fork from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Optional title for the session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Initial permission ruleset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<Ruleset>,
}

/// Request to update a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionRequest {
    /// New title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Request to summarize a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizeRequest {
    /// Provider ID.
    pub provider_id: String,
    /// Model ID.
    pub model_id: String,
    /// Whether this is automatic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto: Option<bool>,
}

/// Request to revert a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RevertRequest {
    /// Message ID to revert to.
    pub message_id: String,
    /// Part ID to revert to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_id: Option<String>,
}

/// Session status response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    /// Active session ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_session_id: Option<String>,
    /// Whether any session is busy.
    #[serde(default)]
    pub busy: bool,
}

/// Session diff response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDiff {
    /// Diff content.
    #[serde(default)]
    pub diff: String,
    /// Files changed.
    #[serde(default)]
    pub files: Vec<String>,
    /// Additional fields.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Session todo item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    /// Todo ID.
    pub id: String,
    /// Todo content.
    pub content: String,
    /// Whether completed.
    #[serde(default)]
    pub completed: bool,
    /// Priority.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_deserialize() {
        let json = r#"{
            "id": "s1",
            "projectId": "p1",
            "directory": "/path/to/project",
            "title": "Test Session",
            "version": "1.0",
            "time": {"created": 1234567890, "updated": 1234567890}
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.id, "s1");
        assert_eq!(session.title, "Test Session");
    }

    #[test]
    fn test_session_minimal() {
        // Session with only required field (id)
        let json = r#"{"id": "s1"}"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.id, "s1");
        assert!(session.project_id.is_none());
    }

    #[test]
    fn test_session_with_optional_fields() {
        let json = r#"{
            "id": "s1",
            "projectId": "p1",
            "directory": "/path",
            "title": "Test",
            "version": "1.0",
            "time": {"created": 1234567890, "updated": 1234567890},
            "parentId": "s0",
            "share": {"url": "https://example.com/share/s1"}
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.parent_id, Some("s0".to_string()));
        assert!(session.share.is_some());
    }
}
