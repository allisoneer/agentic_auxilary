//! Tool implementations for review-agent-mcp.

use agentic_tools_core::{Tool, ToolContext, ToolError, ToolRegistry};
use claudecode::client::Client;
use claudecode::config::{MCPConfig, MCPServer, SessionConfig};
use claudecode::types::{Model, OutputFormat, PermissionMode};
use futures::future::BoxFuture;
use std::collections::HashMap;

use crate::prompts::compose_system_prompt;
use crate::types::{ReviewReport, ReviewVerdict, SpawnInput, SpawnOutput};
use crate::validation::parse_and_validate_report;

/// Diff line count threshold for large diff warning.
const LARGE_DIFF_THRESHOLD: usize = 1500;

/// Count lines in a string.
fn count_lines(s: &str) -> usize {
    s.lines().count()
}

/// Build MCP config for reviewer subagents with strict read-only allowlist.
///
/// Only permits `cli_ls`, `cli_grep`, `cli_glob` from agentic-mcp.
/// The reviewer MUST NOT have access to git, bash, write, edit, or `just_execute`.
fn build_reviewer_mcp_config() -> MCPConfig {
    let mut servers: HashMap<String, MCPServer> = HashMap::new();

    // Read-only MCP tools for the reviewer
    let allowlist = ["cli_ls", "cli_grep", "cli_glob"];
    let args = vec!["--allow".to_string(), allowlist.join(",")];

    servers.insert(
        "agentic-mcp".to_string(),
        MCPServer::stdio("agentic-mcp", args),
    );

    MCPConfig {
        mcp_servers: servers,
    }
}

/// Run a single reviewer session and return the raw text output.
async fn run_reviewer_session(
    system_prompt: &str,
    user_prompt: &str,
    builtin_tools: Vec<String>,
    all_tools: Vec<String>,
    mcp_config: MCPConfig,
) -> Result<String, ToolError> {
    let cfg = SessionConfig::builder(user_prompt.to_string())
        .model(Model::Opus)
        .output_format(OutputFormat::Text)
        .permission_mode(PermissionMode::DontAsk)
        .system_prompt(system_prompt.to_string())
        .tools(builtin_tools)
        .allowed_tools(all_tools)
        .mcp_config(mcp_config)
        .strict_mcp_config(true)
        .build()
        .map_err(|e| ToolError::Internal(format!("Failed to build session config: {e}")))?;

    let client = Client::new()
        .await
        .map_err(|e| ToolError::Internal(format!("Claude CLI not runnable: {e}")))?;

    let result = client
        .launch_and_wait(cfg)
        .await
        .map_err(|e| ToolError::Internal(format!("Failed to run Claude session: {e}")))?;

    if result.is_error {
        return Err(ToolError::Internal(
            result
                .error
                .unwrap_or_else(|| "Reviewer session error".into()),
        ));
    }

    // Prefer result.result, then result.content; reject empty/whitespace
    let text = result
        .result
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .or_else(|| {
            result
                .content
                .as_ref()
                .filter(|s| !s.trim().is_empty())
                .cloned()
        })
        .ok_or_else(|| ToolError::Internal("Reviewer produced no text output".into()))?;

    Ok(text)
}

/// Tool for spawning a lens-specific Opus code reviewer.
#[derive(Clone, Default)]
pub struct SpawnTool;

impl SpawnTool {
    async fn spawn_impl(&self, input: SpawnInput) -> Result<SpawnOutput, ToolError> {
        let diff_path = input
            .diff_path
            .clone()
            .unwrap_or_else(|| "./review.diff".into());

        // Read diff file
        let diff = std::fs::read_to_string(&diff_path).map_err(|e| {
            ToolError::InvalidInput(format!("Failed to read diff at {diff_path}: {e}"))
        })?;

        // Handle empty diff
        if diff.trim().is_empty() {
            return Ok(SpawnOutput {
                report: ReviewReport {
                    lens: input.lens,
                    verdict: ReviewVerdict::Approved,
                    findings: vec![],
                    notes: vec!["No changes to review (diff empty)".into()],
                },
                large_diff_warning: None,
            });
        }

        // Check for large diff
        let line_count = count_lines(&diff);
        let large_diff_warning = (line_count > LARGE_DIFF_THRESHOLD).then(|| {
            format!(
                "Diff is large ({line_count} lines > {LARGE_DIFF_THRESHOLD}); review may be incomplete."
            )
        });

        // Compose prompts
        let system_prompt = compose_system_prompt(input.lens);
        let focus = input.focus.clone().unwrap_or_default();
        let user_prompt = format!(
            "Review the changes in {diff_path}.\n\
             Focus guidance: {focus}\n\
             Requirements: read the diff first, then inspect referenced files as needed. \
             Output ONLY valid JSON matching the template."
        );

        // Read-only tool boundary
        let builtin_tools: Vec<String> = vec!["Read".into()];
        let mcp_tools: [String; 3] = [
            "mcp__agentic-mcp__cli_ls".into(),
            "mcp__agentic-mcp__cli_grep".into(),
            "mcp__agentic-mcp__cli_glob".into(),
        ];
        let all_tools: Vec<String> = builtin_tools
            .iter()
            .cloned()
            .chain(mcp_tools.iter().cloned())
            .collect();

        let mcp_config = build_reviewer_mcp_config();

        // Attempt #1
        let raw1 = run_reviewer_session(
            &system_prompt,
            &user_prompt,
            builtin_tools.clone(),
            all_tools.clone(),
            mcp_config.clone(),
        )
        .await?;

        match parse_and_validate_report(&raw1, input.lens) {
            Ok(report) => Ok(SpawnOutput {
                report,
                large_diff_warning,
            }),
            Err(err1) => {
                // Retry #2 with repair prompt
                tracing::warn!("First reviewer attempt failed validation: {err1}, retrying...");

                let repair_prompt = format!(
                    "Your previous response was invalid.\n\
                     Error: {err1}\n\
                     Previous response:\n{raw1}\n\n\
                     Return ONLY a single valid JSON object matching the required template. \
                     Do not use markdown fences. Do not add new findings; only repair formatting/fields."
                );

                let raw2 = run_reviewer_session(
                    &system_prompt,
                    &repair_prompt,
                    builtin_tools,
                    all_tools,
                    mcp_config,
                )
                .await?;

                let report = parse_and_validate_report(&raw2, input.lens)?;
                Ok(SpawnOutput {
                    report,
                    large_diff_warning,
                })
            }
        }
    }
}

impl Tool for SpawnTool {
    type Input = SpawnInput;
    type Output = SpawnOutput;

    const NAME: &'static str = "spawn";
    const DESCRIPTION: &'static str =
        "Spawn a lens-specific Opus code reviewer over a prepared diff file (./review.diff).";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let this = self.clone();
        Box::pin(async move { this.spawn_impl(input).await })
    }
}

/// Build the tool registry with all review-agent tools.
pub fn build_registry() -> ToolRegistry {
    ToolRegistry::builder()
        .register::<SpawnTool, ()>(SpawnTool)
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_tools_core::Tool;

    #[test]
    fn tool_name_is_spawn() {
        assert_eq!(<SpawnTool as Tool>::NAME, "spawn");
    }

    #[test]
    fn count_lines_works() {
        assert_eq!(count_lines("a\nb\nc"), 3);
        assert_eq!(count_lines(""), 0);
        assert_eq!(count_lines("single line"), 1);
    }

    #[test]
    fn large_diff_threshold_is_1500() {
        assert_eq!(LARGE_DIFF_THRESHOLD, 1500);
    }
}
