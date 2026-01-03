//! SSE event types for opencode_rs.

use serde::{Deserialize, Serialize};

/// Wrapper for events from /global/event which include directory context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalEventEnvelope {
    /// Directory context for the event.
    pub directory: String,
    /// The actual event payload.
    pub payload: Event,
}

/// SSE Event from OpenCode server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    // Server lifecycle
    /// Server connection established.
    #[serde(rename = "server.connected")]
    ServerConnected {
        /// Event properties.
        properties: serde_json::Value,
    },

    /// Server heartbeat (sent every 30s).
    #[serde(rename = "server.heartbeat")]
    ServerHeartbeat {
        /// Event properties.
        properties: serde_json::Value,
    },

    // Session lifecycle
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

    /// Session became idle (processing complete).
    #[serde(rename = "session.idle")]
    SessionIdle {
        /// Event properties.
        properties: SessionEventProps,
    },

    /// Session encountered an error.
    #[serde(rename = "session.error")]
    SessionError {
        /// Event properties.
        properties: SessionErrorProps,
    },

    // Message events
    /// Message updated.
    #[serde(rename = "message.updated")]
    MessageUpdated {
        /// Event properties.
        properties: MessageEventProps,
    },

    /// Message part updated (streaming).
    #[serde(rename = "message.part.updated")]
    MessagePartUpdated {
        /// Event properties (boxed to reduce enum size).
        properties: Box<MessagePartEventProps>,
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
            Event::MessagePartUpdated { properties } => properties.session_id.as_deref(),
            _ => None,
        }
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
}
