//! Tool input/output types with JSON schema and `TextFormat` implementations.

use agentic_tools_core::fmt::TextFormat;
use agentic_tools_core::fmt::TextOptions;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Write;

// ============================================================================
// run
// ============================================================================

/// Input for the `run` tool.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
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

    /// When true, do not early-exit simply because the session is currently idle.
    /// Used by permission replies to wait for post-permission activity.
    #[serde(default)]
    pub wait_for_activity: Option<bool>,
}

/// Completion status for `run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Session completed successfully
    Completed,
    /// Session requires permission approval before continuing
    PermissionRequired,
    /// Session requires answers to one or more questions before continuing
    QuestionRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QuestionOptionView {
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct QuestionInfoView {
    pub question: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub header: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<QuestionOptionView>,
    #[serde(default)]
    pub multiple: bool,
    #[serde(default)]
    pub custom: bool,
}

/// Output from the `run` tool.
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

    /// Question request ID for responding (when `status=question_required`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_request_id: Option<String>,

    /// Pending question details (when `status=question_required`)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub questions: Vec<QuestionInfoView>,

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
            RunStatus::QuestionRequired => "?",
        };
        let status_str = match self.status {
            RunStatus::Completed => "completed",
            RunStatus::PermissionRequired => "permission_required",
            RunStatus::QuestionRequired => "question_required",
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
            // NOTE: Keep the `orchestrator_*` prefix in user-facing instructions because OpenCode
            // displays tool names as `<server>_<tool>` (server name is "orchestrator").
            out.push_str("\nTo respond: orchestrator_respond_permission(session_id, reply)\n");
            out.push_str("  reply options: once | always | reject\n");
            out.push_str("  tip: include permission_request_id=<Request ID> when provided\n");
        }

        if self.status == RunStatus::QuestionRequired {
            out.push_str("\n--- Question Request ---\n");
            if let Some(req_id) = &self.question_request_id {
                let _ = writeln!(out, "Request ID: {req_id}");
            }
            for (index, question) in self.questions.iter().enumerate() {
                if !question.header.is_empty() {
                    let _ = writeln!(out, "Header: {}", question.header);
                }
                let _ = writeln!(out, "Question {}: {}", index + 1, question.question);
                for option in &question.options {
                    if option.description.is_empty() {
                        let _ = writeln!(out, "  - {}", option.label);
                    } else {
                        let _ = writeln!(out, "  - {}: {}", option.label, option.description);
                    }
                }
                let _ = writeln!(out, "  multiple: {}", question.multiple);
                let _ = writeln!(out, "  custom: {}", question.custom);
            }
            out.push_str(
                "\nTo respond: orchestrator_respond_question(session_id, action, answers)\n",
            );
            out.push_str("  action options: reply | reject\n");
            out.push_str("  tip: include question_request_id=<Request ID> when provided\n");
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
// list_sessions
// ============================================================================

/// Input for the `list_sessions` tool.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<i64>,
    /// Unix timestamp of creation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<i64>,
    /// Session working directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    /// Current session status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<SessionStatusSummary>,
    /// File change statistics
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_stats: Option<ChangeStats>,
    /// True when the current orchestrator created this session.
    pub launched_by_you: bool,
}

/// Session status summary for list/detail views.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum SessionStatusSummary {
    Idle,
    Busy,
    Retry {
        attempt: u64,
        message: String,
        /// Unix timestamp (ms) of next retry attempt.
        next: u64,
    },
}

/// File change statistics from a session summary.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChangeStats {
    pub additions: u64,
    pub deletions: u64,
    pub files: u64,
}

/// Input for the `get_session_state` tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetSessionStateInput {
    /// The session ID to inspect.
    pub session_id: String,
}

/// Detailed session state output.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetSessionStateOutput {
    pub session_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    pub status: SessionStatusSummary,
    /// True when the current orchestrator created this session.
    pub launched_by_you: bool,
    /// Number of pending messages awaiting an assistant reply.
    pub pending_message_count: usize,
    /// Unix timestamp of the latest activity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<i64>,
    /// Recent tool calls, most recent first.
    pub recent_tool_calls: Vec<ToolCallSummary>,
}

/// Summary of a recent tool call in a session.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolCallSummary {
    pub call_id: String,
    pub tool_name: String,
    pub state: ToolStateSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<i64>,
}

/// Simplified tool state for session diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum ToolStateSummary {
    Pending,
    Running,
    Completed,
    Error { message: String },
}

/// Output from the `list_sessions` tool.
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
            let status = match &s.status {
                Some(SessionStatusSummary::Idle) => "idle",
                Some(SessionStatusSummary::Busy) => "busy",
                Some(SessionStatusSummary::Retry { .. }) => "retry",
                None => "unknown",
            };
            let launched = if s.launched_by_you {
                " [launched by you]"
            } else {
                ""
            };
            let _ = writeln!(out, "  {} - {} ({age}, {status}){launched}", s.id, s.title);
        }

        if self.sessions.is_empty() {
            out.push_str("  (no sessions found)\n");
        }

        out
    }
}

impl TextFormat for GetSessionStateOutput {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        let mut out = format!("Session: {} ({})\n", self.session_id, self.title);

        if let Some(dir) = &self.directory {
            let _ = writeln!(out, "  Directory: {dir}");
        }

        let status_str = match &self.status {
            SessionStatusSummary::Idle => "Idle".to_string(),
            SessionStatusSummary::Busy => "Busy".to_string(),
            SessionStatusSummary::Retry {
                attempt,
                message,
                next,
            } => format!("Retry (attempt {attempt}, next at {next}, reason: {message})"),
        };
        let _ = writeln!(out, "  Status: {status_str}");
        let _ = writeln!(out, "  Launched by you: {}", self.launched_by_you);
        let _ = writeln!(out, "  Pending messages: {}", self.pending_message_count);

        if let Some(last) = self.last_activity {
            let _ = writeln!(out, "  Last activity: {}", format_time_ago(last));
        }

        if !self.recent_tool_calls.is_empty() {
            out.push_str("  Recent tool calls:\n");
            for tool_call in &self.recent_tool_calls {
                let state_str = match &tool_call.state {
                    ToolStateSummary::Pending => "pending".to_string(),
                    ToolStateSummary::Running => "running".to_string(),
                    ToolStateSummary::Completed => "completed".to_string(),
                    ToolStateSummary::Error { message } => format!("error: {message}"),
                };
                let _ = writeln!(
                    out,
                    "    {} ({}) - {}",
                    tool_call.call_id, tool_call.tool_name, state_str
                );
            }
        }

        out
    }
}

// ============================================================================
// list_commands
// ============================================================================

/// Input for the `list_commands` tool.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
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

/// Output from the `list_commands` tool.
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

        // NOTE: Keep prefixed name for OpenCode UX (client-visible tool name).
        out.push_str("\nUse orchestrator_run(command=<name>, message=<args>) to execute\n");
        out
    }
}

// ============================================================================
// respond_permission
// ============================================================================

/// Input for the `respond_permission` tool.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RespondPermissionInput {
    /// Session ID with pending permission
    pub session_id: String,

    /// Permission request ID to respond to (returned by `run` when `status=permission_required`).
    /// Recommended when present to avoid replying to the wrong request.
    #[serde(default)]
    pub permission_request_id: Option<String>,

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuestionAction {
    Reply,
    Reject,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct RespondQuestionInput {
    pub session_id: String,
    #[serde(default)]
    pub question_request_id: Option<String>,
    pub action: QuestionAction,
    #[serde(default)]
    pub answers: Vec<Vec<String>>,
}

pub type RespondQuestionOutput = OrchestratorRunOutput;

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
            question_request_id: None,
            questions: vec![],
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
            question_request_id: None,
            questions: vec![],
            warnings: vec![],
        };

        let text = out.fmt_text(&TextOptions::default());
        assert!(text.contains("permission_required"));
        assert!(text.contains("sess-456"));
        assert!(text.contains("file.write"));
        assert!(text.contains("src/**/*.rs"));
        assert!(text.contains("perm-789"));
        assert!(text.contains("orchestrator_respond_permission"));
        assert!(text.contains("permission_request_id"));
    }

    #[test]
    fn run_output_text_format_question_required() {
        let out = OrchestratorRunOutput {
            session_id: "sess-789".into(),
            status: RunStatus::QuestionRequired,
            response: None,
            partial_response: Some("Need confirmation".into()),
            permission_request_id: None,
            permission_type: None,
            permission_patterns: vec![],
            question_request_id: Some("question-123".into()),
            questions: vec![QuestionInfoView {
                question: "Continue with deploy?".into(),
                header: "Deployment".into(),
                options: vec![QuestionOptionView {
                    label: "yes".into(),
                    description: "Proceed".into(),
                }],
                multiple: false,
                custom: false,
            }],
            warnings: vec![],
        };

        let text = out.fmt_text(&TextOptions::default());
        assert!(text.contains("question_required"));
        assert!(text.contains("question-123"));
        assert!(text.contains("Continue with deploy?"));
        assert!(text.contains("orchestrator_respond_question"));
    }

    #[test]
    fn list_sessions_text_format() {
        let out = ListSessionsOutput {
            sessions: vec![SessionSummary {
                id: "sess-1".into(),
                title: "Research caching".into(),
                updated: Some(0),
                created: Some(0),
                directory: Some("/tmp/project".into()),
                status: Some(SessionStatusSummary::Idle),
                change_stats: Some(ChangeStats {
                    additions: 1,
                    deletions: 0,
                    files: 1,
                }),
                launched_by_you: true,
            }],
        };

        let text = out.fmt_text(&TextOptions::default());
        assert!(text.contains("Sessions (1)"));
        assert!(text.contains("sess-1"));
        assert!(text.contains("Research caching"));
        assert!(text.contains("idle"));
        assert!(text.contains("launched by you"));
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
