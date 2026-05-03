//! Reviewer runner infrastructure for spawning Claude sessions.

use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use claudecode::client::Client;
use claudecode::config::MCPConfig;
use claudecode::config::MCPServer;
use claudecode::config::SessionConfig;
use claudecode::types::Model;
use claudecode::types::OutputFormat;
use claudecode::types::PermissionMode;
use claudecode::types::Result as ClaudeResult;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

use crate::types::ReviewLens;

/// Reviewer sub-agent builtin tools (Claude Code native).
pub const REVIEWER_BUILTIN_TOOLS: [&str; 3] = ["Read", "Grep", "Glob"];

/// Reviewer capability profile for a lens-specific reviewer session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewerCapabilityProfile {
    Narrow,
    Completeness,
}

/// Reviewer sub-agent MCP tool allowlist for the narrow profile.
pub const REVIEWER_MCP_ALLOWLIST_NARROW: [&str; 1] = ["cli_ls"];

/// Reviewer sub-agent MCP tool allowlist for the completeness profile.
pub const REVIEWER_MCP_ALLOWLIST_COMPLETENESS: [&str; 2] = ["cli_ls", "ask_agent"];

/// Reviewer sub-agent MCP tool names for the narrow profile.
pub const REVIEWER_MCP_TOOL_NAMES_NARROW: [&str; 1] = ["mcp__agentic-mcp__cli_ls"];

/// Reviewer sub-agent MCP tool names for the completeness profile.
pub const REVIEWER_MCP_TOOL_NAMES_COMPLETENESS: [&str; 2] =
    ["mcp__agentic-mcp__cli_ls", "mcp__agentic-mcp__ask_agent"];

/// Maximum concurrent Claude reviewer sessions.
pub const MAX_CONCURRENT_SESSIONS: usize = 3;

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
        profile: ReviewerCapabilityProfile,
        system_prompt: String,
        user_prompt: String,
        ctx: ToolContext,
    ) -> BoxFuture<'static, Result<String, ToolError>>;
}

/// Select the reviewer capability profile for a lens.
pub fn capability_profile_for_lens(lens: ReviewLens) -> ReviewerCapabilityProfile {
    match lens {
        ReviewLens::Completeness => ReviewerCapabilityProfile::Completeness,
        ReviewLens::Security
        | ReviewLens::Correctness
        | ReviewLens::Maintainability
        | ReviewLens::Testing
        | ReviewLens::Simplification => ReviewerCapabilityProfile::Narrow,
    }
}

async fn wait_for_review_result<F, C, CFn>(
    ctx: &ToolContext,
    wait_fut: F,
    cancel_fn: CFn,
) -> Result<ClaudeResult, ToolError>
where
    F: Future<Output = claudecode::Result<ClaudeResult>>,
    C: Future<Output = claudecode::Result<()>>,
    CFn: FnOnce() -> C,
{
    tokio::select! {
        () = ctx.cancelled() => {
            cancel_fn()
                .await
                .map_err(|e| ToolError::Internal(format!("Failed to cancel Claude session: {e}")))?;
            Err(ToolError::cancelled(None))
        }
        result = wait_fut => {
            result.map_err(|e| ToolError::Internal(format!("Failed to run Claude session: {e}")))
        }
    }
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
    fn all_tools(profile: ReviewerCapabilityProfile) -> Vec<String> {
        REVIEWER_BUILTIN_TOOLS
            .iter()
            .chain(Self::mcp_tool_names(profile).iter())
            .map(|s| (*s).to_string())
            .collect()
    }

    /// Select allowed MCP short names for a reviewer profile.
    fn mcp_allowlist(profile: ReviewerCapabilityProfile) -> &'static [&'static str] {
        match profile {
            ReviewerCapabilityProfile::Narrow => &REVIEWER_MCP_ALLOWLIST_NARROW,
            ReviewerCapabilityProfile::Completeness => &REVIEWER_MCP_ALLOWLIST_COMPLETENESS,
        }
    }

    /// Select allowed MCP fully qualified tool names for a reviewer profile.
    fn mcp_tool_names(profile: ReviewerCapabilityProfile) -> &'static [&'static str] {
        match profile {
            ReviewerCapabilityProfile::Narrow => &REVIEWER_MCP_TOOL_NAMES_NARROW,
            ReviewerCapabilityProfile::Completeness => &REVIEWER_MCP_TOOL_NAMES_COMPLETENESS,
        }
    }

    /// Build MCP config for reviewer subagents.
    fn mcp_config(profile: ReviewerCapabilityProfile) -> MCPConfig {
        let mut servers: HashMap<String, MCPServer> = HashMap::new();
        let args = vec![
            "--allow".to_string(),
            Self::mcp_allowlist(profile).join(","),
        ];
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
        profile: ReviewerCapabilityProfile,
        system_prompt: String,
        user_prompt: String,
        ctx: ToolContext,
    ) -> BoxFuture<'static, Result<String, ToolError>> {
        let semaphore = Arc::clone(&self.semaphore);
        let builtin_tools = Self::builtin_tools();
        let all_tools = Self::all_tools(profile);
        let mcp_config = Self::mcp_config(profile);

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

                let session = client.launch(cfg).await.map_err(|e| {
                    ToolError::Internal(format!("Failed to start Claude session: {e}"))
                })?;

                wait_for_review_result(&ctx, session.wait(), || session.cancel()).await
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
        _profile: ReviewerCapabilityProfile,
        _system_prompt: String,
        _user_prompt: String,
        _ctx: ToolContext,
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
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;

    #[test]
    fn reviewer_tool_constants_cover_both_profiles() {
        assert_eq!(REVIEWER_BUILTIN_TOOLS, ["Read", "Grep", "Glob"]);
        assert_eq!(REVIEWER_MCP_ALLOWLIST_NARROW, ["cli_ls"]);
        assert_eq!(REVIEWER_MCP_TOOL_NAMES_NARROW, ["mcp__agentic-mcp__cli_ls"]);
        assert_eq!(REVIEWER_MCP_ALLOWLIST_COMPLETENESS, ["cli_ls", "ask_agent"]);
        assert_eq!(
            REVIEWER_MCP_TOOL_NAMES_COMPLETENESS,
            ["mcp__agentic-mcp__cli_ls", "mcp__agentic-mcp__ask_agent"]
        );
    }

    #[test]
    fn max_concurrent_sessions_is_3() {
        assert_eq!(MAX_CONCURRENT_SESSIONS, 3);
    }

    #[test]
    fn capability_profile_for_lens_maps_completeness_only() {
        assert_eq!(
            capability_profile_for_lens(ReviewLens::Completeness),
            ReviewerCapabilityProfile::Completeness
        );

        for lens in [
            ReviewLens::Security,
            ReviewLens::Correctness,
            ReviewLens::Maintainability,
            ReviewLens::Testing,
            ReviewLens::Simplification,
        ] {
            assert_eq!(
                capability_profile_for_lens(lens),
                ReviewerCapabilityProfile::Narrow
            );
        }
    }

    #[test]
    fn completeness_profile_includes_ask_agent_only() {
        assert!(
            ClaudeCliRunner::all_tools(ReviewerCapabilityProfile::Completeness)
                .contains(&"mcp__agentic-mcp__ask_agent".to_string())
        );
        assert!(
            !ClaudeCliRunner::all_tools(ReviewerCapabilityProfile::Narrow)
                .contains(&"mcp__agentic-mcp__ask_agent".to_string())
        );
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
        let result = runner
            .run_text(
                ReviewerCapabilityProfile::Narrow,
                "system".into(),
                "user".into(),
                ToolContext::default(),
            )
            .await;
        assert_eq!(result.unwrap(), "test response");
    }

    #[tokio::test]
    async fn mock_runner_exhaustion() {
        let runner = MockRunner::new(vec![]);
        let result = runner
            .run_text(
                ReviewerCapabilityProfile::Narrow,
                "system".into(),
                "user".into(),
                ToolContext::default(),
            )
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn wait_for_review_result_runs_cancel_branch() {
        let ctx = ToolContext::default();
        let cancel = ctx.cancellation_token();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_flag = Arc::clone(&cancelled);

        let task = tokio::spawn(async move {
            wait_for_review_result(
                &ctx,
                std::future::pending::<claudecode::Result<ClaudeResult>>(),
                move || async move {
                    cancelled_flag.store(true, Ordering::SeqCst);
                    Ok(())
                },
            )
            .await
        });

        tokio::task::yield_now().await;
        cancel.cancel();

        let result = match task.await {
            Ok(result) => result,
            Err(err) => panic!("task join failed: {err}"),
        };
        assert!(matches!(result, Err(ToolError::Cancelled { .. })));
        assert!(cancelled.load(Ordering::SeqCst));
    }
}
