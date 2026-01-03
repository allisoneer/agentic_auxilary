//! Session types for opencode_rs.

use serde::{Deserialize, Serialize};

/// A session in OpenCode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Unique session identifier.
    pub id: String,
    /// Project identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    /// Working directory for the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    /// Session title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Parent session ID (for forked sessions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

/// Request to create a new session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionRequest {
    /// Optional title for the session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Parent session ID to fork from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}
