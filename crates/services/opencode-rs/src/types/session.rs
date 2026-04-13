//! Session types for `opencode_rs`.

use crate::types::permission::Ruleset;
use serde::Deserialize;
use serde::Serialize;

/// A session in `OpenCode`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Unique session identifier.
    pub id: String,
    /// URL-safe session slug (upstream-required).
    pub slug: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none", rename = "parentID")]
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

/// Legacy-compatible session status summary.
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

/// Rich per-session status information returned by modern `/session/status` responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SessionStatusInfo {
    /// Session is idle.
    Idle,
    /// Session is busy processing work.
    Busy,
    /// Session is retrying work.
    Retry {
        /// Retry attempt number.
        attempt: u64,
        /// Retry message/reason.
        message: String,
        /// Next retry timestamp.
        next: u64,
    },
}

/// A file diff entry from the session diff endpoint.
///
/// The server returns an array of these objects representing changes to each file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionFileDiff {
    /// File path.
    pub file: String,
    /// Content before changes.
    pub before: String,
    /// Content after changes.
    pub after: String,
    /// Number of lines added.
    pub additions: u64,
    /// Number of lines deleted.
    pub deletions: u64,
    /// Diff status: "added", "deleted", or "modified".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<SessionDiffStatus>,
    /// Additional fields from server.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Status of a file in a session diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionDiffStatus {
    /// File was added.
    Added,
    /// File was deleted.
    Deleted,
    /// File was modified.
    Modified,
}

/// Session diff response - a list of file diffs.
pub type SessionDiff = Vec<SessionFileDiff>;

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
    use std::collections::HashMap;

    #[test]
    fn test_session_deserialize() {
        let json = r#"{
            "id": "s1",
            "slug": "s1",
            "projectId": "p1",
            "directory": "/path/to/project",
            "title": "Test Session",
            "version": "1.0",
            "time": {"created": 1234567890, "updated": 1234567890}
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.id, "s1");
        assert_eq!(session.slug, "s1");
        assert_eq!(session.title, "Test Session");
    }

    #[test]
    fn test_session_minimal_upstream() {
        // Session with only required fields (id + slug)
        let json = r#"{"id": "s1", "slug": "s1"}"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.id, "s1");
        assert_eq!(session.slug, "s1");
        assert!(session.project_id.is_none());
    }

    #[test]
    fn test_session_missing_slug_fails() {
        // Session without slug should fail deserialization (slug is upstream-required)
        let json = r#"{"id": "s1"}"#;
        assert!(serde_json::from_str::<Session>(json).is_err());
    }

    #[test]
    fn test_session_with_optional_fields() {
        let json = r#"{
            "id": "s1",
            "slug": "s1",
            "projectId": "p1",
            "directory": "/path",
            "title": "Test",
            "version": "1.0",
            "time": {"created": 1234567890, "updated": 1234567890},
            "parentId": "s0",
            "share": {"url": "https://example.com/share/s1"}
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.slug, "s1");
        assert_eq!(session.parent_id, Some("s0".to_string()));
        assert!(session.share.is_some());
    }

    #[test]
    fn parse_legacy_status_shape_is_rejected() {
        let json = r#"{"busy": true, "activeSessionId": "s1"}"#;
        let resp: Result<HashMap<String, SessionStatusInfo>, _> = serde_json::from_str(json);
        assert!(resp.is_err());
    }

    #[test]
    fn parse_map_status() {
        let json = r#"{"s1": {"type": "busy"}, "s2": {"type": "retry", "attempt": 2, "message": "rate limited", "next": 12345}}"#;
        let resp: HashMap<String, SessionStatusInfo> = serde_json::from_str(json).unwrap();

        assert!(matches!(resp.get("s1"), Some(SessionStatusInfo::Busy)));
        assert!(matches!(
            resp.get("s2"),
            Some(SessionStatusInfo::Retry { attempt: 2, .. })
        ));
        assert!(!resp.contains_key("s3"));
    }

    #[test]
    fn parse_empty_map_status() {
        let json = r"{}";
        let resp: HashMap<String, SessionStatusInfo> = serde_json::from_str(json).unwrap();

        assert!(resp.is_empty());
    }

    #[test]
    fn parse_session_file_diff() {
        let json = r#"{
            "file": "src/main.rs",
            "before": "fn main() {}",
            "after": "fn main() { println!(\"hello\"); }",
            "additions": 1,
            "deletions": 0,
            "status": "modified"
        }"#;
        let diff: SessionFileDiff = serde_json::from_str(json).unwrap();
        assert_eq!(diff.file, "src/main.rs");
        assert_eq!(diff.additions, 1);
        assert_eq!(diff.deletions, 0);
        assert_eq!(diff.status, Some(SessionDiffStatus::Modified));
    }

    #[test]
    fn parse_session_diff_array() {
        let json = r#"[
            {"file": "a.rs", "before": "", "after": "new", "additions": 1, "deletions": 0, "status": "added"},
            {"file": "b.rs", "before": "old", "after": "", "additions": 0, "deletions": 1, "status": "deleted"}
        ]"#;
        let diff: SessionDiff = serde_json::from_str(json).unwrap();
        assert_eq!(diff.len(), 2);
        assert_eq!(diff[0].status, Some(SessionDiffStatus::Added));
        assert_eq!(diff[1].status, Some(SessionDiffStatus::Deleted));
    }

    // ==================== CreateSessionRequest Tests ====================

    #[test]
    fn test_create_session_request_parent_id_serializes_as_uppercase() {
        let req = CreateSessionRequest {
            parent_id: Some("ses-123".to_string()),
            title: None,
            permission: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""parentID""#));
        assert!(!json.contains(r#""parentId""#));
    }

    #[test]
    fn test_create_session_request_without_parent_id() {
        let req = CreateSessionRequest {
            parent_id: None,
            title: Some("Test Session".to_string()),
            permission: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        // parentID should not appear when None
        assert!(!json.contains("parentID"));
        assert!(json.contains(r#""title":"Test Session""#));
    }
}
