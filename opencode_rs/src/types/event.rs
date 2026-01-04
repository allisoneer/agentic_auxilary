//! SSE event types for opencode_rs.
//!
//! Contains 40 event variants matching OpenCode's server.ts.

use serde::{Deserialize, Serialize};

/// Wrapper for events from /global/event which include directory context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalEventEnvelope {
    /// Directory context for the event.
    pub directory: String,
    /// The actual event payload.
    pub payload: Event,
}

/// SSE Event from OpenCode server (40 variants).
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        /// Event properties.
        properties: SessionEventProps,
    },

    /// Session updated.
    #[serde(rename = "session.updated")]
    SessionUpdated {
        /// Event properties.
        properties: SessionEventProps,
    },

    /// Session deleted.
    #[serde(rename = "session.deleted")]
    SessionDeleted {
        /// Event properties.
        properties: SessionEventProps,
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
        /// Event properties.
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
        /// Event properties.
        properties: SessionEventProps,
    },

    // ==================== Messages (4) ====================
    /// Message updated.
    #[serde(rename = "message.updated")]
    MessageUpdated {
        /// Event properties.
        properties: MessageEventProps,
    },

    /// Message removed.
    #[serde(rename = "message.removed")]
    MessageRemoved {
        /// Event properties.
        properties: MessageEventProps,
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
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// Permission asked.
    #[serde(rename = "permission.asked")]
    PermissionAsked {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
    },

    /// Permission replied next.
    #[serde(rename = "permission.replied-next")]
    PermissionRepliedNext {
        /// Event properties.
        #[serde(default)]
        properties: serde_json::Value,
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

/// Properties for session events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionEventProps {
    /// Session ID.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Additional properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Properties for session error events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionErrorProps {
    /// Session ID.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Error message.
    #[serde(default)]
    pub error: Option<String>,
    /// Additional properties.
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Properties for message events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageEventProps {
    /// Session ID.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Message ID.
    #[serde(default)]
    pub message_id: Option<String>,
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

impl Event {
    /// Extract session_id if present in this event.
    pub fn session_id(&self) -> Option<&str> {
        match self {
            Event::SessionCreated { properties } => properties.session_id.as_deref(),
            Event::SessionUpdated { properties } => properties.session_id.as_deref(),
            Event::SessionDeleted { properties } => properties.session_id.as_deref(),
            Event::SessionIdle { properties } => properties.session_id.as_deref(),
            Event::SessionError { properties } => properties.session_id.as_deref(),
            Event::MessageUpdated { properties } => properties.session_id.as_deref(),
            Event::MessageRemoved { properties } => properties.session_id.as_deref(),
            Event::MessagePartUpdated { properties } => properties.session_id.as_deref(),
            _ => None,
        }
    }

    /// Check if this is a heartbeat event.
    pub fn is_heartbeat(&self) -> bool {
        matches!(self, Event::ServerHeartbeat { .. })
    }

    /// Check if this is a connection event.
    pub fn is_connected(&self) -> bool {
        matches!(self, Event::ServerConnected { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_deserialize_session_created() {
        let json = r#"{"type":"session.created","properties":{"sessionId":"abc123"}}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::SessionCreated { .. }));
        assert_eq!(event.session_id(), Some("abc123"));
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
        let json = r#"{"type":"permission.asked","properties":{"requestId":"r1"}}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::PermissionAsked { .. }));
    }

    #[test]
    fn test_event_deserialize_todo_updated() {
        let json = r#"{"type":"todo.updated","properties":{}}"#;
        let event: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(event, Event::TodoUpdated { .. }));
    }
}
