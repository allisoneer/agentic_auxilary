//! SSE event types for `opencode_rs`.
//!
//! Contains 40 event variants matching `OpenCode`'s server.ts.

use crate::types::error::APIError;
use crate::types::permission::PermissionReply;
use crate::types::permission::PermissionRequest;
use crate::types::session::Session;
use serde::Deserialize;
use serde::Serialize;

/// Wrapper for events from /global/event which include directory context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalEventEnvelope {
    /// Directory context for the event.
    pub directory: String,
    /// The actual event payload.
    pub payload: Event,
}

/// SSE Event from `OpenCode` server (40 variants).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type")]
pub enum Event {
    // ==================== Server/Instance (4) ====================
    /// Server connection established.
    #[serde(rename = "server.connected")]
    ServerConnected {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// Server heartbeat (sent periodically).
    #[serde(rename = "server.heartbeat")]
    ServerHeartbeat {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// Server instance disposed.
    #[serde(rename = "server.instance.disposed")]
    ServerInstanceDisposed {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// Global disposed.
    #[serde(rename = "global.disposed")]
    GlobalDisposed {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    // ==================== Session (8) ====================
    /// Session created.
    #[serde(rename = "session.created")]
    SessionCreated {
        /// Event properties with full session info.
        properties: SessionInfoProps,
    },

    /// Session updated.
    #[serde(rename = "session.updated")]
    SessionUpdated {
        /// Event properties with full session info.
        properties: SessionInfoProps,
    },

    /// Session deleted.
    #[serde(rename = "session.deleted")]
    SessionDeleted {
        /// Event properties with full session info.
        properties: SessionInfoProps,
    },

    /// Session diff.
    #[serde(rename = "session.diff")]
    SessionDiff {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// Session error.
    #[serde(rename = "session.error")]
    SessionError {
        /// Event properties with typed error.
        properties: SessionErrorProps,
    },

    /// Session compacted.
    #[serde(rename = "session.compacted")]
    SessionCompacted {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// Session status changed.
    #[serde(rename = "session.status")]
    SessionStatus {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// Session became idle.
    #[serde(rename = "session.idle")]
    SessionIdle {
        /// Event properties with session ID.
        properties: SessionIdleProps,
    },

    // ==================== Messages (4) ====================
    /// Message updated.
    #[serde(rename = "message.updated")]
    MessageUpdated {
        /// Event properties with full message info.
        properties: MessageUpdatedProps,
    },

    /// Message removed.
    #[serde(rename = "message.removed")]
    MessageRemoved {
        /// Event properties with session and message IDs.
        properties: MessageRemovedProps,
    },

    /// Message part updated (streaming).
    #[serde(rename = "message.part.updated")]
    MessagePartUpdated {
        /// Event properties (boxed to reduce enum size).
        properties: Box<MessagePartEventProps>,
    },

    /// Message part removed.
    #[serde(rename = "message.part.removed")]
    MessagePartRemoved {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    // ==================== PTY (4) ====================
    /// PTY created.
    #[serde(rename = "pty.created")]
    PtyCreated {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// PTY updated.
    #[serde(rename = "pty.updated")]
    PtyUpdated {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// PTY exited.
    #[serde(rename = "pty.exited")]
    PtyExited {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// PTY deleted.
    #[serde(rename = "pty.deleted")]
    PtyDeleted {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    // ==================== Permissions (4) ====================
    /// Permission updated.
    #[serde(rename = "permission.updated")]
    PermissionUpdated {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// Permission replied.
    #[serde(rename = "permission.replied")]
    PermissionReplied {
        /// Event properties with reply info.
        properties: PermissionRepliedProps,
    },

    /// Permission asked.
    #[serde(rename = "permission.asked")]
    PermissionAsked {
        /// Event properties with permission request.
        properties: PermissionAskedProps,
    },

    /// Permission replied next.
    #[serde(rename = "permission.replied-next")]
    PermissionRepliedNext {
        /// Event properties with reply info.
        properties: PermissionRepliedProps,
    },

    // ==================== Project/Files (4) ====================
    /// Project updated.
    #[serde(rename = "project.updated")]
    ProjectUpdated {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// File edited.
    #[serde(rename = "file.edited")]
    FileEdited {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// File watcher updated.
    #[serde(rename = "file.watcher.updated")]
    FileWatcherUpdated {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// VCS branch updated.
    #[serde(rename = "vcs.branch.updated")]
    VcsBranchUpdated {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    // ==================== LSP/Tools (4) ====================
    /// LSP updated.
    #[serde(rename = "lsp.updated")]
    LspUpdated {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// LSP client diagnostics.
    #[serde(rename = "lsp.client.diagnostics")]
    LspClientDiagnostics {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// Command executed.
    #[serde(rename = "command.executed")]
    CommandExecuted {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// MCP tools changed.
    #[serde(rename = "mcp.tools.changed")]
    McpToolsChanged {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    // ==================== Installation (3) ====================
    /// Installation updated.
    #[serde(rename = "installation.updated")]
    InstallationUpdated {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// Installation update available.
    #[serde(rename = "installation.update-available")]
    InstallationUpdateAvailable {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// IDE installed.
    #[serde(rename = "ide.installed")]
    IdeInstalled {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    // ==================== TUI (4) ====================
    /// TUI prompt append.
    #[serde(rename = "tui.prompt.append")]
    TuiPromptAppend {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// TUI command execute.
    #[serde(rename = "tui.command.execute")]
    TuiCommandExecute {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// TUI toast show.
    #[serde(rename = "tui.toast.show")]
    TuiToastShow {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// TUI session select.
    #[serde(rename = "tui.session.select")]
    TuiSessionSelect {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    // ==================== Question (3) ====================
    /// Question asked by the server.
    #[serde(rename = "question.asked")]
    QuestionAsked {
        /// Event properties with question request.
        properties: QuestionAskedProps,
    },

    /// Question replied by the user.
    #[serde(rename = "question.replied")]
    QuestionReplied {
        /// Event properties with reply info.
        properties: QuestionRepliedProps,
    },

    /// Question rejected by the user.
    #[serde(rename = "question.rejected")]
    QuestionRejected {
        /// Event properties with rejection info.
        properties: QuestionRejectedProps,
    },

    // ==================== Todo (1) ====================
    /// Todo updated.
    #[serde(rename = "todo.updated")]
    TodoUpdated {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// Fallback for unknown event types.
    #[serde(other)]
    Unknown,
}

// ==================== Session Event Properties ====================

/// Properties for session events (created/updated/deleted).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoProps {
    /// Full session info.
    pub info: Session,
    /// Additional properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Properties for session.idle events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionIdleProps {
    /// Session ID.
    #[serde(alias = "sessionID")]
    pub session_id: String,
    /// Additional properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Error union that can be `APIError` or unknown value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AssistantError {
    /// Known API error.
    Api(APIError),
    /// Unknown error format (forward compatibility).
    Unknown(serde_json::Value),
}

/// Properties for session error events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionErrorProps {
    /// Session ID.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Typed error.
    #[serde(default)]
    pub error: Option<AssistantError>,
    /// Additional properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ==================== Message Event Properties ====================

/// Properties for message.updated events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageUpdatedProps {
    /// Full message info.
    pub info: crate::types::message::MessageInfo,
    /// Additional properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Properties for message.removed events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRemovedProps {
    /// Session ID.
    pub session_id: String,
    /// Message ID.
    pub message_id: String,
    /// Additional properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Properties for message part update events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePartEventProps {
    /// Session ID.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Message ID.
    #[serde(default)]
    pub message_id: Option<String>,
    /// Part index.
    #[serde(default)]
    pub index: Option<usize>,
    /// Updated part content.
    #[serde(default)]
    pub part: Option<crate::types::message::Part>,
    /// Streaming delta (incremental text).
    #[serde(default)]
    pub delta: Option<String>,
    /// Additional properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ==================== Permission Event Properties ====================

/// Properties for permission.asked events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAskedProps {
    /// The permission request (flattened).
    #[serde(flatten)]
    pub request: PermissionRequest,
}

/// Properties for permission.replied events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRepliedProps {
    /// Session ID.
    pub session_id: String,
    /// Request ID that was replied to.
    pub request_id: String,
    /// The reply given.
    pub reply: PermissionReply,
    /// Additional properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// ==================== Question Event Properties ====================

/// Properties for question.asked events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionAskedProps {
    /// The question request.
    #[serde(flatten)]
    pub request: crate::types::question::QuestionRequest,
}

/// Properties for question.replied events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRepliedProps {
    /// Session ID.
    pub session_id: String,
    /// Request ID that was replied to.
    pub request_id: String,
    /// The answers given.
    pub answers: Vec<Vec<String>>,
    /// Additional properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Properties for question.rejected events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionRejectedProps {
    /// Session ID.
    pub session_id: String,
    /// Request ID that was rejected.
    pub request_id: String,
    /// Optional reason for rejection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Additional properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

impl Event {
    /// Extract `session_id` if present in this event.
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Self::SessionCreated { properties }
            | Self::SessionUpdated { properties }
            | Self::SessionDeleted { properties } => Some(&properties.info.id),
            Self::SessionIdle { properties } => Some(&properties.session_id),
            Self::SessionError { properties } => properties.session_id.as_deref(),
            Self::MessageUpdated { properties } => properties.info.session_id.as_deref(),
            Self::MessageRemoved { properties } => Some(&properties.session_id),
            Self::MessagePartUpdated { properties } => properties.session_id.as_deref(),
            Self::PermissionAsked { properties } => Some(&properties.request.session_id),
            Self::PermissionReplied { properties } | Self::PermissionRepliedNext { properties } => {
                Some(&properties.session_id)
            }
            Self::QuestionAsked { properties } => Some(&properties.request.session_id),
            Self::QuestionReplied { properties } => Some(&properties.session_id),
            Self::QuestionRejected { properties } => Some(&properties.session_id),
            _ => None,
        }
    }

    /// Check if this is a heartbeat event.
    pub fn is_heartbeat(&self) -> bool {
        matches!(self, Self::ServerHeartbeat { .. })
    }

    /// Check if this is a connection event.
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::ServerConnected { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_deserialize_session_created() {
        let json = r#"{
            "type": "session.created",
            "properties": {
                "info": {
                    "id": "sess-123",
                    "slug": "sess-123",
                    "title": "Test Session",
                    "version": "1.0"
                }
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::SessionCreated { .. }));
        assert_eq!(event.session_id(), Some("sess-123"));
    }

    #[test]
    fn test_event_deserialize_heartbeat() {
        let json = r#"{"type":"server.heartbeat","properties":{}}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::ServerHeartbeat { .. }));
        assert!(event.is_heartbeat());
    }

    #[test]
    fn test_event_deserialize_unknown() {
        let json = r#"{"type":"some.future.event","properties":{}}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::Unknown));
    }

    #[test]
    fn test_message_part_with_delta() {
        let json = r#"{"type":"message.part.updated","properties":{"sessionId":"s1","messageId":"m1","delta":"Hello"}}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        if let Event::MessagePartUpdated { properties } = &event {
            assert_eq!(properties.delta, Some("Hello".to_string()));
        } else {
            panic!("Expected MessagePartUpdated");
        }
    }

    #[test]
    fn test_event_deserialize_pty_created() {
        let json = r#"{"type":"pty.created","properties":{"id":"pty1"}}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::PtyCreated { .. }));
    }

    #[test]
    fn test_event_deserialize_permission_asked() {
        let json = r#"{
            "type": "permission.asked",
            "properties": {
                "id": "req-123",
                "sessionID": "sess-456",
                "permission": "file.write",
                "patterns": ["**/*.rs"]
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::PermissionAsked { .. }));
        assert_eq!(event.session_id(), Some("sess-456"));
    }

    #[test]
    fn test_event_deserialize_permission_replied() {
        let json = r#"{
            "type": "permission.replied",
            "properties": {
                "sessionId": "sess-456",
                "requestId": "req-123",
                "reply": "always"
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::PermissionReplied { .. }));
        assert_eq!(event.session_id(), Some("sess-456"));
    }

    #[test]
    fn test_event_deserialize_message_updated() {
        let json = r#"{
            "type": "message.updated",
            "properties": {
                "info": {
                    "id": "msg-123",
                    "sessionId": "sess-456",
                    "role": "assistant",
                    "time": {"created": 1234567890}
                }
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::MessageUpdated { .. }));
        assert_eq!(event.session_id(), Some("sess-456"));
    }

    #[test]
    fn test_event_deserialize_message_removed() {
        let json = r#"{
            "type": "message.removed",
            "properties": {
                "sessionId": "sess-456",
                "messageId": "msg-123"
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::MessageRemoved { .. }));
        assert_eq!(event.session_id(), Some("sess-456"));
    }

    #[test]
    fn test_event_deserialize_session_error() {
        let json = r#"{
            "type": "session.error",
            "properties": {
                "sessionId": "sess-456",
                "error": {"message": "Something went wrong", "isRetryable": false}
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        if let Event::SessionError { properties } = &event {
            assert!(properties.error.is_some());
            if let Some(AssistantError::Api(err)) = &properties.error {
                assert_eq!(err.message, "Something went wrong");
            } else {
                panic!("Expected APIError");
            }
        } else {
            panic!("Expected SessionError");
        }
    }

    #[test]
    fn test_event_deserialize_todo_updated() {
        let json = r#"{"type":"todo.updated","properties":{}}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::TodoUpdated { .. }));
    }

    #[test]
    fn test_event_deserialize_question_asked() {
        let json = r#"{
            "type": "question.asked",
            "properties": {
                "id": "req-123",
                "sessionId": "sess-456",
                "questions": [{"question": "Continue?"}]
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::QuestionAsked { .. }));
        assert_eq!(event.session_id(), Some("sess-456"));
    }

    #[test]
    fn test_event_deserialize_question_replied() {
        let json = r#"{
            "type": "question.replied",
            "properties": {
                "sessionId": "sess-456",
                "requestId": "req-123",
                "answers": [["Yes"]]
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        if let Event::QuestionReplied { properties } = &event {
            assert_eq!(properties.session_id, "sess-456");
            assert_eq!(properties.request_id, "req-123");
            assert_eq!(properties.answers, vec![vec!["Yes"]]);
        } else {
            panic!("Expected QuestionReplied");
        }
    }

    #[test]
    fn test_event_deserialize_question_rejected() {
        let json = r#"{
            "type": "question.rejected",
            "properties": {
                "sessionId": "sess-456",
                "requestId": "req-123",
                "reason": "User cancelled"
            }
        }"#;
        let event: Event = serde_json::from_str(json).unwrap();
        if let Event::QuestionRejected { properties } = &event {
            assert_eq!(properties.session_id, "sess-456");
            assert_eq!(properties.request_id, "req-123");
            assert_eq!(properties.reason, Some("User cancelled".to_string()));
        } else {
            panic!("Expected QuestionRejected");
        }
    }

    // TODO(3): Add tests for GlobalEventEnvelope deserialization and round-trip serialization
}
