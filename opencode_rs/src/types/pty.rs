//! PTY (pseudo-terminal) types for opencode_rs.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A PTY session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pty {
    /// PTY identifier.
    pub id: String,
    /// PTY title or command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Shell command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Environment variables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    /// PTY size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<PtySize>,
    /// Whether the PTY has exited.
    #[serde(default)]
    pub exited: bool,
    /// Exit code if exited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

/// PTY terminal size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PtySize {
    /// Number of columns.
    pub cols: u16,
    /// Number of rows.
    pub rows: u16,
}

/// Request to create a PTY.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePtyRequest {
    /// Shell command to run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Environment variables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    /// Initial terminal size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<PtySize>,
}

/// Request to update a PTY.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePtyRequest {
    /// New terminal size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<PtySize>,
    /// New title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}
