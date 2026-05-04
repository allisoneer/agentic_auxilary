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
    /// Project-relative path for the session when provided by the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
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
    pub diffs: Option<Vec<SnapshotFileDiff>>,
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
    /// Optional workspace ID for the session.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "workspaceID"
    )]
    pub workspace_id: Option<String>,
}

/// Request to update a session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionRequest {
    /// New title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Scope options for `GET /session` filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionListScope {
    /// List sessions across the current project.
    Project,
}

/// Typed query parameters for `GET /session`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListParams {
    /// Optional scope override for whole-project listing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<SessionListScope>,
    /// Optional project-relative path filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Whether only root sessions should be returned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roots: Option<bool>,
    /// Only include sessions updated at or after this timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    /// Optional case-insensitive title search term.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    /// Optional maximum number of sessions to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

impl SessionListParams {
    /// Construct params for whole-project listing.
    pub fn project() -> Self {
        Self {
            scope: Some(SessionListScope::Project),
            ..Self::default()
        }
    }

    /// Encode params into query pairs for `GET /session`.
    pub(crate) fn to_query_pairs(&self) -> Vec<(&'static str, String)> {
        let mut query = Vec::new();

        if let Some(scope) = self.scope {
            query.push((
                "scope",
                match scope {
                    SessionListScope::Project => "project".to_string(),
                },
            ));
        }

        if let Some(path) = &self.path {
            query.push(("path", path.clone()));
        }

        if let Some(roots) = self.roots {
            query.push(("roots", roots.to_string()));
        }

        if let Some(start) = self.start {
            query.push(("start", start.to_string()));
        }

        if let Some(search) = &self.search {
            query.push(("search", search.clone()));
        }

        if let Some(limit) = self.limit {
            query.push(("limit", limit.to_string()));
        }

        query
    }
}

/// Request body for `POST /session/{sessionID}/init`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInitRequest {
    /// Model identifier.
    #[serde(rename = "modelID")]
    pub model_id: String,
    /// Provider identifier.
    #[serde(rename = "providerID")]
    pub provider_id: String,
    /// Message identifier.
    #[serde(rename = "messageID")]
    pub message_id: String,
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
pub struct SnapshotFileDiff {
    /// File path.
    pub file: String,
    /// Unified patch content.
    pub patch: String,
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
pub type SessionDiff = Vec<SnapshotFileDiff>;

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
            "path": "src/lib.rs",
            "title": "Test Session",
            "version": "1.0",
            "time": {"created": 1234567890, "updated": 1234567890}
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.id, "s1");
        assert_eq!(session.slug, "s1");
        assert_eq!(session.title, "Test Session");
        assert_eq!(session.path.as_deref(), Some("src/lib.rs"));
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
            "path": "src/main.rs",
            "title": "Test",
            "version": "1.0",
            "time": {"created": 1234567890, "updated": 1234567890},
            "parentId": "s0",
            "share": {"url": "https://example.com/share/s1"}
        }"#;
        let session: Session = serde_json::from_str(json).unwrap();
        assert_eq!(session.slug, "s1");
        assert_eq!(session.path.as_deref(), Some("src/main.rs"));
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
            "patch": "@@ -1 +1 @@\n-fn main() {}\n+fn main() { println!(\"hello\"); }",
            "additions": 1,
            "deletions": 0,
            "status": "modified"
        }"#;
        let diff: SnapshotFileDiff = serde_json::from_str(json).unwrap();
        assert_eq!(diff.file, "src/main.rs");
        assert_eq!(diff.additions, 1);
        assert_eq!(diff.deletions, 0);
        assert_eq!(diff.status, Some(SessionDiffStatus::Modified));
    }

    #[test]
    fn parse_session_summary_with_patch_diffs() {
        let json = r#"{
            "additions": 1,
            "deletions": 0,
            "files": 1,
            "diffs": [
                {
                    "file": "src/main.rs",
                    "patch": "@@ -0,0 +1 @@\n+fn main() {}",
                    "additions": 1,
                    "deletions": 0,
                    "status": "added"
                }
            ]
        }"#;

        let summary: SessionSummary = serde_json::from_str(json).unwrap();
        let diffs = summary.diffs.expect("summary diffs should parse");
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].file, "src/main.rs");
        assert!(diffs[0].patch.contains("fn main"));
        assert_eq!(diffs[0].status, Some(SessionDiffStatus::Added));
    }

    #[test]
    fn parse_session_diff_array() {
        let json = r#"[
            {"file": "a.rs", "patch": "@@ -0,0 +1 @@\n+new", "additions": 1, "deletions": 0, "status": "added"},
            {"file": "b.rs", "patch": "@@ -1 +0,0 @@\n-old", "additions": 0, "deletions": 1, "status": "deleted"}
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
            workspace_id: None,
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
            workspace_id: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        // parentID should not appear when None
        assert!(!json.contains("parentID"));
        assert!(json.contains(r#""title":"Test Session""#));
    }

    #[test]
    fn test_session_init_request_uses_uppercase_ids() {
        let req = SessionInitRequest {
            model_id: "claude-sonnet-4".to_string(),
            provider_id: "anthropic".to_string(),
            message_id: "msg-123".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""modelID""#));
        assert!(json.contains(r#""providerID""#));
        assert!(json.contains(r#""messageID""#));
    }

    #[test]
    fn test_session_list_params_project_helper() {
        assert_eq!(
            SessionListParams::project(),
            SessionListParams {
                scope: Some(SessionListScope::Project),
                ..SessionListParams::default()
            }
        );
    }

    #[test]
    fn test_session_list_params_to_query_pairs() {
        let params = SessionListParams {
            scope: Some(SessionListScope::Project),
            path: Some("src/lib.rs".to_string()),
            roots: Some(true),
            start: Some(1_234_567_890),
            search: Some("hello".to_string()),
            limit: Some(25),
        };

        assert_eq!(
            params.to_query_pairs(),
            vec![
                ("scope", "project".to_string()),
                ("path", "src/lib.rs".to_string()),
                ("roots", "true".to_string()),
                ("start", "1234567890".to_string()),
                ("search", "hello".to_string()),
                ("limit", "25".to_string()),
            ]
        );
    }
}
