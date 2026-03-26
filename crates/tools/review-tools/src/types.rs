//! Type definitions for review tools.

use agentic_tools_core::fmt::TextFormat;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

/// Default page size in lines (~800 lines per page).
pub const DEFAULT_PAGE_SIZE_LINES: u32 = 800;

/// Mode for diff generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDiffMode {
    /// Diff working tree + staged changes against merge-base.
    #[default]
    Default,
    /// Diff only staged changes (like `git diff --staged`).
    Staged,
}

// =============================================================================
// Diff Snapshot Input/Output
// =============================================================================

/// Input for the `review_diff_snapshot` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReviewDiffSnapshotInput {
    /// Diff mode: "default" (working tree + staged) or "staged" (staged only).
    #[serde(default)]
    pub mode: ReviewDiffMode,

    /// Optional pathspecs to limit the diff scope.
    #[serde(default)]
    pub paths: Vec<String>,

    /// Optional page size in lines (default: ~800).
    #[serde(default)]
    pub page_size_lines: Option<u32>,
}

/// Statistics about the diff.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiffStats {
    /// Number of files changed.
    pub files_changed: u32,
    /// Total lines inserted.
    pub insertions: u32,
    /// Total lines deleted.
    pub deletions: u32,
}

/// Index entry for a single page in the diff.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiffPageIndex {
    /// Page number (1-based).
    pub page: u32,
    /// Files included in this page.
    pub files: Vec<String>,
    /// Number of lines in this page.
    pub line_count: u32,
    /// Warning if this page is oversized (single large file/hunk).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oversized_warning: Option<String>,
}

/// Pagination metadata for the diff snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DiffPaging {
    /// Page size in lines.
    pub page_size_lines: u32,
    /// Total number of pages.
    pub total_pages: u32,
    /// Total lines in the diff.
    pub total_lines: u32,
    /// Index of all pages.
    #[serde(default)]
    pub page_index: Vec<DiffPageIndex>,
}

/// Output of the `review_diff_snapshot` tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewDiffSnapshotOutput {
    /// Opaque handle to reference this snapshot.
    pub diff_handle: String,
    /// Whether there are any changes in the diff.
    pub has_changes: bool,
    /// Slugified branch name (for artifact naming).
    pub branch_slug: String,
    /// Name of the base ref used for diff (e.g., "origin/main").
    pub base_ref_name: String,
    /// Diff statistics.
    pub stats: DiffStats,
    /// Pagination metadata.
    pub paging: DiffPaging,
    /// List of changed files.
    #[serde(default)]
    pub changed_files: Vec<String>,
}

impl TextFormat for ReviewDiffSnapshotOutput {}

// =============================================================================
// Diff Page Input/Output
// =============================================================================

/// Input for the `review_diff_page` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReviewDiffPageInput {
    /// Handle from `review_diff_snapshot`.
    pub diff_handle: String,
    /// Page number to retrieve (1-based).
    pub page: u32,
}

/// Output of the `review_diff_page` tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewDiffPageOutput {
    /// Page number (1-based).
    pub page: u32,
    /// Total number of pages.
    pub total_pages: u32,
    /// Diff content for this page.
    pub content: String,
    /// Files included in this page.
    #[serde(default)]
    pub files_in_page: Vec<String>,
    /// Warning if this page is oversized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oversized_warning: Option<String>,
}

impl TextFormat for ReviewDiffPageOutput {}

// =============================================================================
// Internal Snapshot Representation
// =============================================================================

/// A single page of diff content (internal representation).
#[derive(Debug, Clone)]
pub struct DiffPage {
    /// Page number (1-based).
    pub page: u32,
    /// Diff content for this page.
    pub content: String,
    /// Files included in this page.
    pub files_in_page: Vec<String>,
    /// Warning if this page is oversized.
    pub oversized_warning: Option<String>,
}

/// Cached review snapshot (internal representation).
#[derive(Debug)]
pub struct ReviewSnapshot {
    /// Repository root path.
    pub repo_root: PathBuf,
    /// Slugified branch name.
    pub branch_slug: String,
    /// Name of the base ref used.
    pub base_ref_name: String,
    /// Paginated diff content.
    pub pages: Vec<DiffPage>,
    /// Diff statistics.
    pub stats: DiffStats,
    /// Total lines in the diff.
    pub total_lines: u32,
    /// Page size used.
    pub page_size_lines: u32,
    /// List of changed files.
    pub changed_files: Vec<String>,
}

// =============================================================================
// Review Report Types
// =============================================================================

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

// =============================================================================
// Review Run Input/Output
// =============================================================================

/// Input for the `review_run` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ReviewRunInput {
    /// Handle from `review_diff_snapshot`.
    pub diff_handle: String,
    /// Which review lens to use.
    pub lens: ReviewLens,
    /// Optional focus guidance for the reviewer.
    #[serde(default)]
    pub focus: Option<String>,
    /// Page to start reviewing from (1-based, defaults to 1).
    #[serde(default)]
    pub page_start: Option<u32>,
    /// Maximum pages to review (defaults to all remaining).
    #[serde(default)]
    pub max_pages: Option<u32>,
}

/// Pagination info in the review run output.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewRunPaging {
    /// First page reviewed (1-based).
    pub page_start: u32,
    /// Number of pages reviewed.
    pub pages_reviewed: u32,
    /// Total pages in the snapshot.
    pub total_pages: u32,
}

/// Output from the `review_run` tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewRunOutput {
    /// The validated review report.
    pub report: ReviewReport,
    /// Warning if the diff was large.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_diff_warning: Option<String>,
    /// Pagination info.
    pub paging: ReviewRunPaging,
}

impl TextFormat for ReviewRunOutput {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_mode_is_default() {
        let mode = ReviewDiffMode::default();
        assert_eq!(mode, ReviewDiffMode::Default);
    }

    #[test]
    fn default_page_size_is_800() {
        assert_eq!(DEFAULT_PAGE_SIZE_LINES, 800);
    }
}
