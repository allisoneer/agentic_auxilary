//! Reviewer runner infrastructure for spawning Claude sessions.

use agentic_tools_core::ToolError;
use claudecode::client::Client;
use claudecode::config::{MCPConfig, MCPServer, SessionConfig};
use claudecode::types::{Model, OutputFormat, PermissionMode, Result as ClaudeResult};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

/// Reviewer sub-agent builtin tools (Claude Code native).
pub const REVIEWER_BUILTIN_TOOLS: [&str; 3] = ["Read", "Grep", "Glob"];

/// Reviewer sub-agent MCP tool allowlist (short names for config).
pub const REVIEWER_MCP_ALLOWLIST: [&str; 1] = ["cli_ls"];

/// Reviewer sub-agent MCP tool names (fully qualified for session config).
pub const REVIEWER_MCP_TOOL_NAMES: [&str; 1] = ["mcp__agentic-mcp__cli_ls"];

/// Maximum concurrent Claude reviewer sessions.
pub const MAX_CONCURRENT_SESSIONS: usize = 2;

/// Wall-clock timeout for a single reviewer session (30 minutes).
pub const SESSION_TIMEOUT: Duration = Duration::from_secs(1800);

/// Fixed retry delays for session attempts: 3 total attempts with [0ms, 500ms, 1000ms].
pub const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_millis(0),
    Duration::from_millis(500),
    Duration::from_millis(1000),
];

/// Trait for reviewer runners (enables mocking in tests).
pub trait ReviewerRunner: Send + Sync {
    /// Run a reviewer session and return the raw text output.
    fn run_text(
        &self,
        system_prompt: String,
        user_prompt: String,
    ) -> BoxFuture<'static, Result<String, ToolError>>;
}

/// Production implementation using Claude CLI.
#[derive(Clone)]
pub struct ClaudeCliRunner {
    semaphore: Arc<Semaphore>,
}

impl ClaudeCliRunner {
    /// Create a new Claude CLI runner with the shared semaphore.
    pub fn new() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_SESSIONS)),
        }
    }

    /// Build the list of builtin tool names for reviewer sessions.
    fn builtin_tools() -> Vec<String> {
        REVIEWER_BUILTIN_TOOLS
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    /// Build the complete list of all tool names (builtin + MCP) for reviewer sessions.
    fn all_tools() -> Vec<String> {
        REVIEWER_BUILTIN_TOOLS
            .iter()
            .chain(REVIEWER_MCP_TOOL_NAMES.iter())
            .map(|s| (*s).to_string())
            .collect()
    }

    /// Build MCP config for reviewer subagents.
    fn mcp_config() -> MCPConfig {
        let mut servers: HashMap<String, MCPServer> = HashMap::new();
        let args = vec!["--allow".to_string(), REVIEWER_MCP_ALLOWLIST.join(",")];
        servers.insert(
            "agentic-mcp".to_string(),
            MCPServer::stdio("agentic-mcp", args),
        );
        MCPConfig {
            mcp_servers: servers,
        }
    }
}

impl Default for ClaudeCliRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl ReviewerRunner for ClaudeCliRunner {
    fn run_text(
        &self,
        system_prompt: String,
        user_prompt: String,
    ) -> BoxFuture<'static, Result<String, ToolError>> {
        let semaphore = Arc::clone(&self.semaphore);
        let builtin_tools = Self::builtin_tools();
        let all_tools = Self::all_tools();
        let mcp_config = Self::mcp_config();

        Box::pin(async move {
            // Acquire semaphore permit
            let _permit = semaphore
                .acquire()
                .await
                .map_err(|_| ToolError::Internal("Semaphore closed".into()))?;

            // Build session config
            let cfg = SessionConfig::builder(user_prompt)
                .model(Model::Opus)
                .output_format(OutputFormat::Text)
                .permission_mode(PermissionMode::DontAsk)
                .system_prompt(system_prompt)
                .tools(builtin_tools)
                .allowed_tools(all_tools)
                .mcp_config(mcp_config)
                .strict_mcp_config(true)
                .build()
                .map_err(|e| ToolError::Internal(format!("Failed to build session config: {e}")))?;

            // Run with timeout
            let result = tokio::time::timeout(SESSION_TIMEOUT, async {
                let client = Client::new()
                    .await
                    .map_err(|e| ToolError::Internal(format!("Claude CLI not runnable: {e}")))?;

                client
                    .launch_and_wait(cfg)
                    .await
                    .map_err(|e| ToolError::Internal(format!("Failed to run Claude session: {e}")))
            })
            .await
            .map_err(|_| {
                ToolError::Internal(format!("Timed out after {}s", SESSION_TIMEOUT.as_secs()))
            })??;

            claude_result_to_text(result)
        })
    }
}

/// Extract text from a `ClaudeResult`.
fn claude_result_to_text(result: ClaudeResult) -> Result<String, ToolError> {
    if result.is_error {
        return Err(ToolError::Internal(
            result
                .error
                .unwrap_or_else(|| "Reviewer session error".into()),
        ));
    }

    result
        .result
        .filter(|s| !s.trim().is_empty())
        .or_else(|| result.content.filter(|s| !s.trim().is_empty()))
        .ok_or_else(|| ToolError::Internal("Reviewer produced no text output".into()))
}

/// Mock runner for testing.
#[cfg(test)]
pub struct MockRunner {
    responses: std::sync::Mutex<Vec<Result<String, ToolError>>>,
}

#[cfg(test)]
impl MockRunner {
    pub fn new(responses: Vec<Result<String, ToolError>>) -> Self {
        Self {
            responses: std::sync::Mutex::new(responses),
        }
    }
}

#[cfg(test)]
impl ReviewerRunner for MockRunner {
    fn run_text(
        &self,
        _system_prompt: String,
        _user_prompt: String,
    ) -> BoxFuture<'static, Result<String, ToolError>> {
        let response = {
            let mut responses = self.responses.lock().expect("lock poisoned");
            if responses.is_empty() {
                Err(ToolError::Internal("No mock responses left".into()))
            } else {
                responses.remove(0)
            }
        };
        Box::pin(async move { response })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewer_tool_constants() {
        assert_eq!(REVIEWER_BUILTIN_TOOLS, ["Read", "Grep", "Glob"]);
        assert_eq!(REVIEWER_MCP_ALLOWLIST, ["cli_ls"]);
        assert_eq!(REVIEWER_MCP_TOOL_NAMES, ["mcp__agentic-mcp__cli_ls"]);
    }

    #[test]
    fn max_concurrent_sessions_is_2() {
        assert_eq!(MAX_CONCURRENT_SESSIONS, 2);
    }

    #[test]
    fn session_timeout_is_30_minutes() {
        assert_eq!(SESSION_TIMEOUT, Duration::from_secs(1800));
    }

    #[test]
    fn retry_delays_are_correct() {
        assert_eq!(
            RETRY_DELAYS,
            [
                Duration::from_millis(0),
                Duration::from_millis(500),
                Duration::from_millis(1000)
            ]
        );
    }

    #[tokio::test]
    async fn mock_runner_returns_responses() {
        let runner = MockRunner::new(vec![Ok("test response".into())]);
        let result = runner.run_text("system".into(), "user".into()).await;
        assert_eq!(result.unwrap(), "test response");
    }

    #[tokio::test]
    async fn mock_runner_exhaustion() {
        let runner = MockRunner::new(vec![]);
        let result = runner.run_text("system".into(), "user".into()).await;
        assert!(result.is_err());
    }
}
