//! Tool implementations for review-agent-mcp.

use agentic_tools_core::{Tool, ToolContext, ToolError, ToolRegistry};
use claudecode::client::Client;
use claudecode::config::{MCPConfig, MCPServer, SessionConfig};
use claudecode::types::{Model, OutputFormat, PermissionMode};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::prompts::compose_system_prompt;
use crate::types::{ReviewReport, ReviewVerdict, SpawnInput, SpawnOutput};
use crate::validation::parse_and_validate_report;

/// Diff line count threshold for large diff warning.
const LARGE_DIFF_THRESHOLD: usize = 1500;

/// Reviewer sub-agent builtin tools (Claude Code native).
/// Aligned with analyzer pattern: Read + Grep + Glob for better source citations.
const REVIEWER_BUILTIN_TOOLS: [&str; 3] = ["Read", "Grep", "Glob"];

/// Reviewer sub-agent MCP tool allowlist (short names for config).
/// Only `cli_ls` is needed; Grep/Glob are now builtin.
const REVIEWER_MCP_ALLOWLIST: [&str; 1] = ["cli_ls"];

/// Reviewer sub-agent MCP tool names (fully qualified for session config).
const REVIEWER_MCP_TOOL_NAMES: [&str; 1] = ["mcp__agentic-mcp__cli_ls"];

/// Build the list of builtin tool names for reviewer sessions.
fn reviewer_builtin_tools() -> Vec<String> {
    REVIEWER_BUILTIN_TOOLS
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

/// Build the complete list of all tool names (builtin + MCP) for reviewer sessions.
fn reviewer_all_tools() -> Vec<String> {
    REVIEWER_BUILTIN_TOOLS
        .iter()
        .chain(REVIEWER_MCP_TOOL_NAMES.iter())
        .map(|s| (*s).to_string())
        .collect()
}

/// Count lines in a string.
fn count_lines(s: &str) -> usize {
    s.lines().count()
}

/// Validate that `diff_path` resolves to a `review.diff` file within the repo root.
///
/// Security hardening: prevents path traversal and symlink escape attacks.
fn validate_diff_path(repo_root: &Path, diff_path: &str) -> Result<PathBuf, ToolError> {
    let repo_root = repo_root
        .canonicalize()
        .map_err(|e| ToolError::Internal(format!("Failed to canonicalize repo root: {e}")))?;

    let candidate = repo_root.join(diff_path);
    let candidate = candidate.canonicalize().map_err(|e| {
        ToolError::InvalidInput(format!(
            "Invalid diff_path (cannot canonicalize): {diff_path} ({e})"
        ))
    })?;

    if !candidate.starts_with(&repo_root) {
        return Err(ToolError::InvalidInput(format!(
            "Invalid diff_path (resolves outside repo root): {diff_path}"
        )));
    }

    if candidate.file_name() != Some(OsStr::new("review.diff")) {
        return Err(ToolError::InvalidInput(format!(
            "Invalid diff_path (must point to review.diff): {diff_path}"
        )));
    }

    Ok(candidate)
}

/// Build MCP config for reviewer subagents with strict read-only allowlist.
///
/// Only permits `cli_ls` from agentic-mcp (Grep/Glob are now builtin tools).
/// The reviewer MUST NOT have access to git, bash, write, edit, or `just_execute`.
fn build_reviewer_mcp_config() -> MCPConfig {
    let mut servers: HashMap<String, MCPServer> = HashMap::new();

    // Read-only MCP tools for the reviewer (using canonical constants)
    let args = vec!["--allow".to_string(), REVIEWER_MCP_ALLOWLIST.join(",")];

    servers.insert(
        "agentic-mcp".to_string(),
        MCPServer::stdio("agentic-mcp", args),
    );

    MCPConfig {
        mcp_servers: servers,
    }
}

// =============================================================================
// Grounding Validation Infrastructure
// =============================================================================

/// Represents a grounding issue for a finding with an invalid `file:line` reference.
#[derive(Debug, Clone)]
struct GroundingIssue {
    /// Index into the findings array.
    finding_index: usize,
    /// File path from the finding.
    file: String,
    /// Line number from the finding.
    line: u32,
    /// Actual line count of the file, if it exists.
    file_line_count: Option<usize>,
    /// Human-readable reason for the issue.
    reason: String,
}

/// Count lines in a file using byte-based newline counting.
///
/// This is more robust than UTF-8 line iteration and handles binary files.
fn count_file_lines_bytes(path: &Path) -> Result<usize, ToolError> {
    let bytes = std::fs::read(path).map_err(|e| {
        ToolError::Internal(format!(
            "Failed to read file for line count: {} ({e})",
            path.display()
        ))
    })?;
    if bytes.is_empty() {
        return Ok(0);
    }
    // Count newlines (naive approach is fine for our use case; files are typically small)
    #[expect(clippy::naive_bytecount, reason = "Not performance-critical")]
    let nl = bytes.iter().filter(|&&b| b == b'\n').count();
    // If file ends without newline, count the final partial line
    Ok(nl + usize::from(bytes.last() != Some(&b'\n')))
}

/// Resolve a finding's file field to a repo-contained absolute path.
///
/// Returns `(display_file, Some(abs_path))` if the file exists and is within `repo_root`,
/// or `(display_file, None)` if the file doesn't exist (e.g., deleted file).
///
/// Security: Rejects absolute paths, `..` components, and paths resolving outside `repo_root`.
fn resolve_finding_file_path(
    repo_root: &Path,
    file_field: &str,
) -> Result<(String, Option<PathBuf>), ToolError> {
    let repo_root = repo_root
        .canonicalize()
        .map_err(|e| ToolError::Internal(format!("Failed to canonicalize repo root: {e}")))?;

    let raw = file_field.trim().trim_matches('`');
    if raw.is_empty() {
        return Ok((raw.to_string(), None));
    }

    // Helper to check if a relative path exists and is contained within repo_root
    let try_paths = |p: &str| -> Result<Option<PathBuf>, ToolError> {
        let rel = Path::new(p);
        if rel.is_absolute()
            || rel
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(ToolError::Internal(format!(
                "Invalid finding file path (must be repo-relative): {p}"
            )));
        }

        let candidate = repo_root.join(rel);
        if !candidate.exists() {
            return Ok(None);
        }
        let canon = candidate.canonicalize().map_err(|e| {
            ToolError::Internal(format!(
                "Failed to canonicalize finding path: {} ({e})",
                candidate.display()
            ))
        })?;
        if !canon.starts_with(&repo_root) {
            return Err(ToolError::Internal(format!(
                "Finding path resolves outside repo root: {p}"
            )));
        }
        Ok(Some(canon))
    };

    // 1) as-is (after stripping ./ only)
    let normalized = raw.strip_prefix("./").unwrap_or(raw);
    if let Some(abs) = try_paths(normalized)? {
        return Ok((normalized.to_string(), Some(abs)));
    }

    // 2) if it starts with a/ or b/, try stripping that prefix (diff notation)
    if let Some(stripped) = normalized
        .strip_prefix("a/")
        .or_else(|| normalized.strip_prefix("b/"))
        && let Some(abs) = try_paths(stripped)?
    {
        return Ok((stripped.to_string(), Some(abs)));
    }

    // File doesn't exist (possibly deleted)
    Ok((normalized.to_string(), None))
}

/// Collect grounding issues for all findings in a report.
///
/// Validates that each finding's `file:line` reference is plausible:
/// - `line=0` is always valid (sentinel for unknown/file-level)
/// - If `line>0`, the file must exist and `line <= file_line_count`
/// - Missing/deleted files with `line>0` are flagged
fn collect_grounding_issues(
    repo_root: &Path,
    report: &ReviewReport,
) -> Result<Vec<GroundingIssue>, ToolError> {
    let mut issues = Vec::new();
    let mut cache: std::collections::HashMap<PathBuf, usize> = std::collections::HashMap::new();

    for (idx, f) in report.findings.iter().enumerate() {
        // line=0 is always valid (sentinel)
        if f.line == 0 {
            continue;
        }

        let (display_file, abs_opt) = resolve_finding_file_path(repo_root, &f.file)?;
        match abs_opt {
            None => {
                // File doesn't exist, but line>0 was claimed
                issues.push(GroundingIssue {
                    finding_index: idx,
                    file: display_file,
                    line: f.line,
                    file_line_count: None,
                    reason: "File does not exist; if deleted/renamed you must use line=0".into(),
                });
            }
            Some(abs) => {
                let line_count = if let Some(cached) = cache.get(&abs) {
                    *cached
                } else {
                    let c = count_file_lines_bytes(&abs)?;
                    cache.insert(abs.clone(), c);
                    c
                };

                if (f.line as usize) > line_count || line_count == 0 {
                    issues.push(GroundingIssue {
                        finding_index: idx,
                        file: display_file,
                        line: f.line,
                        file_line_count: Some(line_count),
                        reason: "Line exceeds file length".into(),
                    });
                }
            }
        }
    }

    Ok(issues)
}

/// Format grounding issues into a human-readable prompt section.
///
/// This is included in the repair prompt to give the reviewer specific feedback
/// about which `file:line` references are invalid and why.
fn format_grounding_issues_for_prompt(issues: &[GroundingIssue]) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    for i in issues {
        match i.file_line_count {
            Some(n) => {
                let _ = writeln!(
                    out,
                    "- {}:{} (file has {n} lines) — {}",
                    i.file, i.line, i.reason
                );
            }
            None => {
                let _ = writeln!(out, "- {}:{} (file missing) — {}", i.file, i.line, i.reason);
            }
        }
    }
    out
}

/// Apply graceful fallback by setting invalid lines to 0.
///
/// Returns the number of findings that were modified.
fn apply_grounding_fallback(report: &mut ReviewReport, issues: &[GroundingIssue]) -> usize {
    let mut changed = 0;
    for i in issues {
        if let Some(f) = report.findings.get_mut(i.finding_index)
            && f.line != 0
        {
            f.line = 0;
            changed += 1;
        }
    }
    changed
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
        let diff_path_input = input
            .diff_path
            .clone()
            .unwrap_or_else(|| "./review.diff".into());

        // Validate diff_path: must resolve to review.diff within repo root
        let repo_root = std::env::current_dir().map_err(|e| {
            ToolError::Internal(format!("Failed to determine repo root (current_dir): {e}"))
        })?;

        let diff_path = validate_diff_path(&repo_root, &diff_path_input)?;

        // Read diff file (path is now validated)
        let diff = std::fs::read_to_string(&diff_path).map_err(|e| {
            ToolError::InvalidInput(format!(
                "Failed to read diff at {}: {e}",
                diff_path.display()
            ))
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
            "Review the changes in {}.\n\
             Focus guidance: {focus}\n\
             Line numbers MUST be SOURCE-FILE line numbers (not ./review.diff line numbers); use 0 if unknown.\n\
             Requirements: read the diff first, then inspect referenced files as needed. \
             Output ONLY valid JSON matching the template.",
            diff_path.display()
        );

        // Read-only tool boundary (using canonical constants)
        let builtin_tools = reviewer_builtin_tools();
        let all_tools = reviewer_all_tools();

        let mcp_config = build_reviewer_mcp_config();

        // Attempt #1: Run reviewer session
        let raw1 = run_reviewer_session(
            &system_prompt,
            &user_prompt,
            builtin_tools.clone(),
            all_tools.clone(),
            mcp_config.clone(),
        )
        .await?;

        // Parse attempt #1
        let report1 = match parse_and_validate_report(&raw1, input.lens) {
            Ok(report) => report,
            Err(err1) => {
                // Schema/semantic validation failed - retry with repair prompt
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
                    builtin_tools.clone(),
                    all_tools.clone(),
                    mcp_config.clone(),
                )
                .await?;

                parse_and_validate_report(&raw2, input.lens)?
            }
        };

        // Grounding validation: check file:line plausibility
        let issues1 = collect_grounding_issues(&repo_root, &report1)?;
        if issues1.is_empty() {
            return Ok(SpawnOutput {
                report: report1,
                large_diff_warning,
            });
        }

        // Grounding issues found - retry with grounding-specific repair prompt
        tracing::warn!(
            "First reviewer attempt has {} grounding issue(s), retrying with grounding repair prompt...",
            issues1.len()
        );

        let grounding_details = format_grounding_issues_for_prompt(&issues1);
        let grounding_repair_prompt = format!(
            "Your previous response was invalid.\n\
             Problem: Some findings have impossible/unverifiable SOURCE-FILE line numbers.\n\
             Invalid file:line pairs:\n{grounding_details}\n\
             Instructions:\n\
             - The `line` field must be a SOURCE-FILE line number (1-based), NOT a ./review.diff line number.\n\
             - Use Grep on the source file to find the snippet and get the real line number.\n\
             - If you cannot verify the exact source line or the file is missing/deleted: set \"line\": 0.\n\
             - Do not add new findings; only repair file/line fields and formatting.\n\n\
             Previous response:\n{}\n\n\
             Return ONLY a single valid JSON object matching the required template.",
            serde_json::to_string(&report1).unwrap_or_else(|_| "<serialization error>".into())
        );

        let raw2 = run_reviewer_session(
            &system_prompt,
            &grounding_repair_prompt,
            builtin_tools,
            all_tools,
            mcp_config,
        )
        .await?;

        let mut report2 = parse_and_validate_report(&raw2, input.lens)?;

        // Check grounding again after retry
        let issues2 = collect_grounding_issues(&repo_root, &report2)?;
        if issues2.is_empty() {
            return Ok(SpawnOutput {
                report: report2,
                large_diff_warning,
            });
        }

        // Graceful degradation: set invalid lines to 0 and add warning note
        let changed = apply_grounding_fallback(&mut report2, &issues2);
        tracing::warn!(
            "Grounding validation failed after retry; sanitized {changed} finding(s) to line=0 (lens={:?})",
            input.lens
        );
        report2.notes.push(format!(
            "Warning: {changed} finding(s) had invalid/unverifiable source-file line numbers and were set to line=0."
        ));

        Ok(SpawnOutput {
            report: report2,
            large_diff_warning,
        })
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
    use tempfile::tempdir;

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

    // --- diff_path validation tests ---

    #[test]
    fn validate_diff_path_accepts_in_repo_review_diff() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("review.diff"), "diff --git a/a b/a\n").unwrap();

        let validated = validate_diff_path(&repo, "./review.diff").unwrap();
        assert!(validated.starts_with(repo.canonicalize().unwrap()));
        assert_eq!(validated.file_name(), Some(OsStr::new("review.diff")));
    }

    #[test]
    fn validate_diff_path_rejects_outside_repo_via_traversal() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("review.diff"), "ok").unwrap();
        std::fs::write(dir.path().join("review.diff"), "outside").unwrap();

        let err = validate_diff_path(&repo, "../review.diff").unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("outside repo root"));
    }

    #[test]
    fn validate_diff_path_rejects_wrong_filename() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("not_review.diff"), "nope").unwrap();

        let err = validate_diff_path(&repo, "./not_review.diff").unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("must point to review.diff"));
    }

    #[cfg(unix)]
    #[test]
    fn validate_diff_path_rejects_symlink_to_outside_repo() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let outside = dir.path().join("outside");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&outside).unwrap();

        let outside_target = outside.join("review.diff");
        std::fs::write(&outside_target, "outside").unwrap();

        let link = repo.join("review.diff");
        symlink(&outside_target, &link).unwrap();

        let err = validate_diff_path(&repo, "./review.diff").unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("outside repo root"));
    }

    // --- Reviewer tool invariant tests ---

    /// Security boundary test: enforces exact equality of reviewer tool allowlists.
    /// If you change this allowlist, update this test AND perform a least-privilege review.
    #[test]
    fn reviewer_tool_invariant_is_exact() {
        // Assert exact contents of the canonical constants
        // Aligned with analyzer pattern: Read + Grep + Glob builtin
        assert_eq!(REVIEWER_BUILTIN_TOOLS, ["Read", "Grep", "Glob"]);
        // MCP reduced to cli_ls only (Grep/Glob are now builtin)
        assert_eq!(REVIEWER_MCP_ALLOWLIST, ["cli_ls"]);
        assert_eq!(REVIEWER_MCP_TOOL_NAMES, ["mcp__agentic-mcp__cli_ls"]);

        // Assert helper functions produce expected output
        assert_eq!(
            reviewer_builtin_tools(),
            vec!["Read".to_string(), "Grep".to_string(), "Glob".to_string()]
        );
        assert_eq!(
            reviewer_all_tools(),
            vec![
                "Read".to_string(),
                "Grep".to_string(),
                "Glob".to_string(),
                "mcp__agentic-mcp__cli_ls".to_string(),
            ]
        );
    }

    // --- Grounding validation tests ---

    #[test]
    fn grounding_validation_flags_line_past_eof() {
        use crate::types::{Confidence, ReviewFinding, ReviewLens, Severity};

        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        // 3-line file
        std::fs::write(repo.join("src.rs"), "a\nb\nc\n").unwrap();

        let report = ReviewReport {
            lens: ReviewLens::Security,
            verdict: ReviewVerdict::NeedsChanges,
            findings: vec![ReviewFinding {
                file: "src.rs".into(),
                line: 10, // impossible: file only has 3 lines
                category: ReviewLens::Security,
                severity: Severity::High,
                confidence: Confidence::High,
                title: "test finding".into(),
                evidence: "x".into(),
                suggested_fix: "y".into(),
                caveat: None,
            }],
            notes: vec![],
        };

        let issues = collect_grounding_issues(&repo, &report).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file_line_count, Some(3));
        assert_eq!(issues[0].line, 10);
    }

    #[test]
    fn grounding_validation_allows_line_zero() {
        use crate::types::{Confidence, ReviewFinding, ReviewLens, Severity};

        let dir = tempdir().unwrap();
        let repo = dir.path();

        let report = ReviewReport {
            lens: ReviewLens::Security,
            verdict: ReviewVerdict::Approved,
            findings: vec![ReviewFinding {
                file: "missing.rs".into(),
                line: 0, // sentinel for unknown
                category: ReviewLens::Security,
                severity: Severity::Low,
                confidence: Confidence::Medium,
                title: "note".into(),
                evidence: "x".into(),
                suggested_fix: "y".into(),
                caveat: Some("file-level".into()),
            }],
            notes: vec![],
        };

        let issues = collect_grounding_issues(repo, &report).unwrap();
        assert!(issues.is_empty()); // line=0 should never be flagged
    }

    #[test]
    fn grounding_validation_flags_missing_file_with_nonzero_line() {
        use crate::types::{Confidence, ReviewFinding, ReviewLens, Severity};

        let dir = tempdir().unwrap();
        let repo = dir.path();

        let report = ReviewReport {
            lens: ReviewLens::Security,
            verdict: ReviewVerdict::NeedsChanges,
            findings: vec![ReviewFinding {
                file: "deleted.rs".into(),
                line: 42, // claimed line in non-existent file
                category: ReviewLens::Security,
                severity: Severity::High,
                confidence: Confidence::High,
                title: "issue".into(),
                evidence: "x".into(),
                suggested_fix: "y".into(),
                caveat: None,
            }],
            notes: vec![],
        };

        let issues = collect_grounding_issues(repo, &report).unwrap();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].file_line_count.is_none()); // file missing
    }

    #[test]
    fn grounding_validation_valid_file_and_line_passes() {
        use crate::types::{Confidence, ReviewFinding, ReviewLens, Severity};

        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        // 10-line file
        std::fs::write(repo.join("valid.rs"), "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n").unwrap();

        let report = ReviewReport {
            lens: ReviewLens::Correctness,
            verdict: ReviewVerdict::NeedsChanges,
            findings: vec![ReviewFinding {
                file: "valid.rs".into(),
                line: 5, // valid: file has 10 lines
                category: ReviewLens::Correctness,
                severity: Severity::Medium,
                confidence: Confidence::High,
                title: "valid finding".into(),
                evidence: "x".into(),
                suggested_fix: "y".into(),
                caveat: None,
            }],
            notes: vec![],
        };

        let issues = collect_grounding_issues(&repo, &report).unwrap();
        assert!(issues.is_empty()); // no issues for valid file:line
    }

    #[test]
    fn grounding_validation_handles_diff_prefix_a_b() {
        use crate::types::{Confidence, ReviewFinding, ReviewLens, Severity};

        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        // File without the a/ or b/ prefix
        std::fs::write(repo.join("src.rs"), "line1\nline2\n").unwrap();

        let report = ReviewReport {
            lens: ReviewLens::Security,
            verdict: ReviewVerdict::NeedsChanges,
            findings: vec![ReviewFinding {
                file: "a/src.rs".into(), // diff notation with a/ prefix
                line: 1,
                category: ReviewLens::Security,
                severity: Severity::Medium,
                confidence: Confidence::High,
                title: "test".into(),
                evidence: "x".into(),
                suggested_fix: "y".into(),
                caveat: None,
            }],
            notes: vec![],
        };

        // Should resolve a/src.rs -> src.rs and find it
        let issues = collect_grounding_issues(&repo, &report).unwrap();
        assert!(issues.is_empty()); // valid after prefix stripping
    }

    #[test]
    fn count_file_lines_bytes_works() {
        let dir = tempdir().unwrap();

        // Empty file
        std::fs::write(dir.path().join("empty.txt"), "").unwrap();
        assert_eq!(
            count_file_lines_bytes(&dir.path().join("empty.txt")).unwrap(),
            0
        );

        // File with newline at end
        std::fs::write(dir.path().join("with_nl.txt"), "a\nb\nc\n").unwrap();
        assert_eq!(
            count_file_lines_bytes(&dir.path().join("with_nl.txt")).unwrap(),
            3
        );

        // File without newline at end
        std::fs::write(dir.path().join("no_nl.txt"), "a\nb\nc").unwrap();
        assert_eq!(
            count_file_lines_bytes(&dir.path().join("no_nl.txt")).unwrap(),
            3
        );

        // Single line with newline
        std::fs::write(dir.path().join("one_nl.txt"), "single\n").unwrap();
        assert_eq!(
            count_file_lines_bytes(&dir.path().join("one_nl.txt")).unwrap(),
            1
        );

        // Single line without newline
        std::fs::write(dir.path().join("one_no_nl.txt"), "single").unwrap();
        assert_eq!(
            count_file_lines_bytes(&dir.path().join("one_no_nl.txt")).unwrap(),
            1
        );
    }

    // --- Fallback and repair prompt tests ---

    #[test]
    fn fallback_sets_invalid_lines_to_zero() {
        use crate::types::{Confidence, ReviewFinding, ReviewLens, Severity};

        let mut report = ReviewReport {
            lens: ReviewLens::Security,
            verdict: ReviewVerdict::NeedsChanges,
            findings: vec![ReviewFinding {
                file: "test.rs".into(),
                line: 999,
                category: ReviewLens::Security,
                severity: Severity::High,
                confidence: Confidence::High,
                title: "issue".into(),
                evidence: "x".into(),
                suggested_fix: "y".into(),
                caveat: None,
            }],
            notes: vec![],
        };

        let issues = vec![GroundingIssue {
            finding_index: 0,
            file: "test.rs".into(),
            line: 999,
            file_line_count: Some(100),
            reason: "Line exceeds file length".into(),
        }];

        let changed = apply_grounding_fallback(&mut report, &issues);
        assert_eq!(changed, 1);
        assert_eq!(report.findings[0].line, 0);
    }

    #[test]
    fn repair_prompt_includes_file_info_and_line_counts() {
        let issues = vec![GroundingIssue {
            finding_index: 0,
            file: "tools.rs".into(),
            line: 895,
            file_line_count: Some(408),
            reason: "Line exceeds file length".into(),
        }];

        let prompt_section = format_grounding_issues_for_prompt(&issues);
        assert!(
            prompt_section.contains("tools.rs:895"),
            "Should include file:line"
        );
        assert!(
            prompt_section.contains("408 lines"),
            "Should include file length"
        );
    }

    #[test]
    fn repair_prompt_handles_missing_files() {
        let issues = vec![GroundingIssue {
            finding_index: 0,
            file: "deleted.rs".into(),
            line: 42,
            file_line_count: None,
            reason: "File does not exist".into(),
        }];

        let prompt_section = format_grounding_issues_for_prompt(&issues);
        assert!(
            prompt_section.contains("deleted.rs:42"),
            "Should include file:line"
        );
        assert!(
            prompt_section.contains("file missing"),
            "Should indicate file is missing"
        );
    }

    #[test]
    fn fallback_skips_already_zero_lines() {
        use crate::types::{Confidence, ReviewFinding, ReviewLens, Severity};

        let mut report = ReviewReport {
            lens: ReviewLens::Security,
            verdict: ReviewVerdict::Approved,
            findings: vec![ReviewFinding {
                file: "test.rs".into(),
                line: 0, // already zero
                category: ReviewLens::Security,
                severity: Severity::Low,
                confidence: Confidence::Medium,
                title: "note".into(),
                evidence: "x".into(),
                suggested_fix: "y".into(),
                caveat: Some("file-level".into()),
            }],
            notes: vec![],
        };

        // This shouldn't happen in practice (line=0 passes validation),
        // but test the safety check
        let issues = vec![GroundingIssue {
            finding_index: 0,
            file: "test.rs".into(),
            line: 0,
            file_line_count: None,
            reason: "shouldn't happen".into(),
        }];

        let changed = apply_grounding_fallback(&mut report, &issues);
        assert_eq!(changed, 0); // No change when already 0
    }

    // --- Regression tests ---

    /// Regression test mirroring the exact failure mode reported:
    /// "tools.rs Line 895" when file only has 408 lines.
    /// This test verifies detection + fallback behavior.
    #[test]
    fn regression_impossible_line_number_is_sanitized() {
        use crate::types::{Confidence, ReviewFinding, ReviewLens, Severity};

        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        // 408-line file (mirrors the reported "895 when file has 408" failure mode)
        let mut s = String::new();
        for _ in 0..408 {
            s.push_str("x\n");
        }
        std::fs::write(repo.join("tools.rs"), s).unwrap();

        let mut report = ReviewReport {
            lens: ReviewLens::Testing,
            verdict: ReviewVerdict::NeedsChanges,
            findings: vec![ReviewFinding {
                file: "tools.rs".into(),
                line: 895, // impossible: file only has 408 lines
                category: ReviewLens::Testing,
                severity: Severity::High,
                confidence: Confidence::High,
                title: "spawn_impl untested".into(),
                evidence: "No tests for this function".into(),
                suggested_fix: "Add tests".into(),
                caveat: None,
            }],
            notes: vec![],
        };

        // Detect the issue
        let issues = collect_grounding_issues(&repo, &report).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].file_line_count, Some(408));
        assert_eq!(issues[0].line, 895);

        // Apply fallback
        let changed = apply_grounding_fallback(&mut report, &issues);
        assert_eq!(changed, 1);
        assert_eq!(report.findings[0].line, 0);

        // Verify no remaining issues after fallback
        let issues_after = collect_grounding_issues(&repo, &report).unwrap();
        assert!(
            issues_after.is_empty(),
            "No grounding issues should remain after fallback"
        );
    }
}
