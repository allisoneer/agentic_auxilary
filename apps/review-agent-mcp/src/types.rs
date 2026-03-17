//! Type definitions for review-agent-mcp.

use agentic_tools_core::fmt::TextFormat;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Lens for code review focus area.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewLens {
    Security,
    Correctness,
    Maintainability,
    Testing,
}

/// Overall verdict from the review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Approved,
    NeedsChanges,
}

/// Severity level for a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
}

/// Confidence level for a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
}

/// A single review finding.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewFinding {
    /// File path where the issue was found.
    pub file: String,
    /// Line number (best-effort; 0 if unknown).
    pub line: u32,
    /// Category (should match the review lens).
    pub category: ReviewLens,
    /// Severity level.
    pub severity: Severity,
    /// Confidence level.
    pub confidence: Confidence,
    /// Short title describing the issue.
    pub title: String,
    /// Evidence from the diff supporting the finding.
    pub evidence: String,
    /// Suggested fix or next step.
    pub suggested_fix: String,
    /// Caveat explaining uncertainty (required when confidence=medium).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caveat: Option<String>,
}

/// Complete review report from a single lens reviewer.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewReport {
    /// Which lens produced this report.
    pub lens: ReviewLens,
    /// Overall verdict.
    pub verdict: ReviewVerdict,
    /// List of findings (may be empty if approved).
    #[serde(default)]
    pub findings: Vec<ReviewFinding>,
    /// Optional notes from the reviewer.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Input for the spawn tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct SpawnInput {
    /// Which review lens to use.
    pub lens: ReviewLens,
    /// Path to the diff file (defaults to `./review.diff`).
    #[serde(default)]
    pub diff_path: Option<String>,
    /// Optional focus guidance for the reviewer.
    #[serde(default)]
    pub focus: Option<String>,
}

/// Output from the spawn tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SpawnOutput {
    /// The validated review report.
    pub report: ReviewReport,
    /// Warning if the diff was large (>1500 lines).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_diff_warning: Option<String>,
}

// Use default TextFormat implementation (pretty JSON).
impl TextFormat for SpawnOutput {}
