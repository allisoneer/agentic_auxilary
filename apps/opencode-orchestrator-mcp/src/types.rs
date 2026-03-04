//! Tool input/output types with JSON schema and `TextFormat` implementations.

use agentic_tools_core::fmt::{TextFormat, TextOptions};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::Write;

// ============================================================================
// orchestrator_run
// ============================================================================

/// Input for the `orchestrator_run` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct OrchestratorRunInput {
    /// Existing session ID to resume. Omit to create a new session.
    #[serde(default)]
    pub session_id: Option<String>,

    /// `OpenCode` command name (e.g., `research`, `implement_plan`).
    /// If provided, the message becomes the `$ARGUMENTS` for template expansion.
    /// If omitted, sends a raw prompt without command template.
    #[serde(default)]
    pub command: Option<String>,

    /// The message/prompt to send. Required for new sessions or when sending new work.
    /// For resume-only calls (checking status), can be omitted.
    #[serde(default)]
    pub message: Option<String>,
}

/// Completion status for `orchestrator_run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Session completed successfully
    Completed,
    /// Session requires permission approval before continuing
    PermissionRequired,
}

/// Output from the `orchestrator_run` tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrchestratorRunOutput {
    /// Session ID for future resume calls
    pub session_id: String,

    /// Completion status
    pub status: RunStatus,

    /// Final assistant response text (when `status=completed`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,

    /// Partial response accumulated before permission request (when `status=permission_required`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_response: Option<String>,

    /// Permission request ID for responding (when `status=permission_required`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_request_id: Option<String>,

    /// Permission type (e.g., "file.write", "bash.execute")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_type: Option<String>,

    /// Permission patterns being requested
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permission_patterns: Vec<String>,

    /// Any warnings (e.g., summarization triggered)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl TextFormat for OrchestratorRunOutput {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        let mut out = String::new();

        // Status line with session ID
        let status_icon = match self.status {
            RunStatus::Completed => "\u{2713}",          // checkmark
            RunStatus::PermissionRequired => "\u{23f8}", // pause
        };
        let status_str = match self.status {
            RunStatus::Completed => "completed",
            RunStatus::PermissionRequired => "permission_required",
        };
        let _ = writeln!(
            out,
            "{status_icon} session {status_str} {}",
            self.session_id
        );

        // Warnings
        for warning in &self.warnings {
            let _ = writeln!(out, "  warning: {warning}");
        }

        // Permission info
        if self.status == RunStatus::PermissionRequired {
            out.push_str("\n--- Permission Request ---\n");
            if let Some(ptype) = &self.permission_type {
                let _ = writeln!(out, "Type: {ptype}");
            }
            if !self.permission_patterns.is_empty() {
                let _ = writeln!(out, "Patterns: {}", self.permission_patterns.join(", "));
            }
            if let Some(req_id) = &self.permission_request_id {
                let _ = writeln!(out, "Request ID: {req_id}");
            }
            out.push_str("\nTo respond: orchestrator_respond_permission(session_id, reply)\n");
            out.push_str("  reply options: once | always | reject\n");
        }

        // Response content
        if let Some(response) = &self.response {
            out.push_str("\n--- Response ---\n");
            out.push_str(response);
        } else if let Some(partial) = &self.partial_response
            && !partial.trim().is_empty()
        {
            out.push_str("\n--- Partial Response ---\n");
            out.push_str(partial);
        }

        out
    }
}

// ============================================================================
// orchestrator_list_sessions
// ============================================================================

/// Input for the `orchestrator_list_sessions` tool.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ListSessionsInput {
    /// Maximum number of sessions to return (default: 20)
    #[serde(default)]
    pub limit: Option<usize>,
}

/// Summary of a single session.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SessionSummary {
    /// Session ID
    pub id: String,
    /// Session title
    pub title: String,
    /// Unix timestamp of last update
    pub updated: Option<i64>,
}

/// Output from the `orchestrator_list_sessions` tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListSessionsOutput {
    /// List of sessions
    pub sessions: Vec<SessionSummary>,
}

impl TextFormat for ListSessionsOutput {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        let mut out = format!("Sessions ({}):\n", self.sessions.len());

        for s in &self.sessions {
            let age = s
                .updated
                .map_or_else(|| "unknown".to_string(), format_time_ago);
            let _ = writeln!(out, "  {} - {} ({age})", s.id, s.title);
        }

        if self.sessions.is_empty() {
            out.push_str("  (no sessions found)\n");
        }

        out
    }
}

// ============================================================================
// orchestrator_list_commands
// ============================================================================

/// Input for the `orchestrator_list_commands` tool.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ListCommandsInput {}

/// Information about an available command.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CommandInfo {
    /// Command name
    pub name: String,
    /// Command description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Output from the `orchestrator_list_commands` tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListCommandsOutput {
    /// List of available commands
    pub commands: Vec<CommandInfo>,
}

impl TextFormat for ListCommandsOutput {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        let mut out = String::from("Available commands:\n");

        for cmd in &self.commands {
            let _ = write!(out, "  {}", cmd.name);
            if let Some(desc) = &cmd.description {
                // First line only for brevity
                if let Some(first_line) = desc.lines().next() {
                    let _ = write!(out, " - {first_line}");
                }
            }
            out.push('\n');
        }

        if self.commands.is_empty() {
            out.push_str("  (no commands available)\n");
        }

        out.push_str("\nUse orchestrator_run(command=<name>, message=<args>) to execute\n");
        out
    }
}

// ============================================================================
// orchestrator_respond_permission
// ============================================================================

/// Input for the `orchestrator_respond_permission` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct RespondPermissionInput {
    /// Session ID with pending permission
    pub session_id: String,

    /// How to respond: "once", "always", or "reject"
    pub reply: PermissionReply,

    /// Optional message to include with reply
    #[serde(default)]
    pub message: Option<String>,
}

/// How to respond to a permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PermissionReply {
    /// Allow this single request
    Once,
    /// Allow always for matching patterns
    Always,
    /// Deny the request
    Reject,
}

/// Response from permission reply - same as run output since we continue monitoring.
pub type RespondPermissionOutput = OrchestratorRunOutput;

// ============================================================================
// Helpers
// ============================================================================

fn format_time_ago(unix_ts: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0);

    let diff = now - unix_ts;
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_output_text_format_completed() {
        let out = OrchestratorRunOutput {
            session_id: "sess-123".into(),
            status: RunStatus::Completed,
            response: Some("Task completed successfully.".into()),
            partial_response: None,
            permission_request_id: None,
            permission_type: None,
            permission_patterns: vec![],
            warnings: vec![],
        };

        let text = out.fmt_text(&TextOptions::default());
        assert!(text.contains("completed"));
        assert!(text.contains("sess-123"));
        assert!(text.contains("Task completed successfully"));
    }

    #[test]
    fn run_output_text_format_permission_required() {
        let out = OrchestratorRunOutput {
            session_id: "sess-456".into(),
            status: RunStatus::PermissionRequired,
            response: None,
            partial_response: Some("Starting task...".into()),
            permission_request_id: Some("perm-789".into()),
            permission_type: Some("file.write".into()),
            permission_patterns: vec!["src/**/*.rs".into()],
            warnings: vec![],
        };

        let text = out.fmt_text(&TextOptions::default());
        assert!(text.contains("permission_required"));
        assert!(text.contains("sess-456"));
        assert!(text.contains("file.write"));
        assert!(text.contains("src/**/*.rs"));
        assert!(text.contains("perm-789"));
        assert!(text.contains("orchestrator_respond_permission"));
    }

    #[test]
    fn list_sessions_text_format() {
        let out = ListSessionsOutput {
            sessions: vec![SessionSummary {
                id: "sess-1".into(),
                title: "Research caching".into(),
                updated: Some(0),
            }],
        };

        let text = out.fmt_text(&TextOptions::default());
        assert!(text.contains("Sessions (1)"));
        assert!(text.contains("sess-1"));
        assert!(text.contains("Research caching"));
    }

    #[test]
    fn list_commands_text_format() {
        let out = ListCommandsOutput {
            commands: vec![CommandInfo {
                name: "research".into(),
                description: Some("Run research workflow".into()),
            }],
        };

        let text = out.fmt_text(&TextOptions::default());
        assert!(text.contains("research"));
        assert!(text.contains("Run research workflow"));
        assert!(text.contains("orchestrator_run"));
    }
}
