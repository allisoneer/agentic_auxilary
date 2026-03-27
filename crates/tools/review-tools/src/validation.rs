//! JSON extraction, semantic validation, and grounding validation for reviewer outputs.

use agentic_tools_core::ToolError;
use agentic_tools_utils::llm_output::extract_json_best_effort as extract_json_impl;
use std::path::Path;

use crate::types::Confidence;
use crate::types::ReviewLens;
use crate::types::ReviewReport;

/// Extract JSON from model output, wrapping the utility function.
fn extract_json_best_effort(text: &str) -> Result<String, ToolError> {
    extract_json_impl(text)
        .map_err(|e| ToolError::Internal(format!("Failed to extract valid JSON: {e}")))
}

/// Parse JSON and validate semantic constraints (lens match, caveat requirement).
///
/// # Errors
///
/// Returns an error if:
/// - JSON extraction fails
/// - JSON doesn't match the `ReviewReport` schema
/// - Report lens doesn't match expected lens
/// - Finding category doesn't match expected lens
/// - Finding has confidence=medium without a non-empty caveat
pub fn parse_and_validate_report(
    text: &str,
    expected_lens: ReviewLens,
) -> Result<ReviewReport, ToolError> {
    let json = extract_json_best_effort(text)?;
    let report: ReviewReport = serde_json::from_str(&json)
        .map_err(|e| ToolError::Internal(format!("JSON parsed but did not match schema: {e}")))?;

    // Validate lens matches expected
    if report.lens != expected_lens {
        return Err(ToolError::Internal(format!(
            "Lens mismatch: expected {:?}, got {:?}",
            expected_lens, report.lens
        )));
    }

    // Validate semantic constraints for each finding
    for f in &report.findings {
        // Validate caveat requirement for medium confidence
        if f.confidence == Confidence::Medium && f.caveat.as_deref().unwrap_or("").trim().is_empty()
        {
            return Err(ToolError::Internal(format!(
                "Invalid finding ({}:{}): confidence=medium requires non-empty caveat",
                f.file, f.line
            )));
        }

        // Validate category matches lens
        if f.category != expected_lens {
            return Err(ToolError::Internal(format!(
                "Invalid finding ({}:{}): category {:?} does not match lens {:?}",
                f.file, f.line, f.category, expected_lens
            )));
        }
    }

    Ok(report)
}

// =============================================================================
// Grounding Validation Infrastructure
// =============================================================================

/// Represents a grounding issue for a finding with an invalid `file:line` reference.
#[derive(Debug, Clone)]
pub struct GroundingIssue {
    /// Index into the findings array.
    pub finding_index: usize,
    /// File path from the finding.
    pub file: String,
    /// Line number from the finding.
    pub line: u32,
    /// Actual line count of the file, if it exists.
    pub file_line_count: Option<usize>,
    /// Human-readable reason for the issue.
    pub reason: String,
}

/// Count lines in a file using byte-based newline counting.
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
    #[expect(clippy::naive_bytecount, reason = "Not performance-critical")]
    let nl = bytes.iter().filter(|&&b| b == b'\n').count();
    Ok(nl + usize::from(bytes.last() != Some(&b'\n')))
}

/// Resolve a finding's file field to a repo-contained absolute path.
fn resolve_finding_file_path(
    repo_root: &Path,
    file_field: &str,
) -> Result<(String, Option<std::path::PathBuf>), ToolError> {
    let repo_root = repo_root
        .canonicalize()
        .map_err(|e| ToolError::Internal(format!("Failed to canonicalize repo root: {e}")))?;

    let raw = file_field.trim().trim_matches('`');
    if raw.is_empty() {
        return Ok((raw.to_string(), None));
    }

    let try_paths = |p: &str| -> Result<Option<std::path::PathBuf>, ToolError> {
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

    let normalized = raw.strip_prefix("./").unwrap_or(raw);
    if let Some(abs) = try_paths(normalized)? {
        return Ok((normalized.to_string(), Some(abs)));
    }

    if let Some(stripped) = normalized
        .strip_prefix("a/")
        .or_else(|| normalized.strip_prefix("b/"))
        && let Some(abs) = try_paths(stripped)?
    {
        return Ok((stripped.to_string(), Some(abs)));
    }

    Ok((normalized.to_string(), None))
}

/// Collect grounding issues for all findings in a report.
pub fn collect_grounding_issues(
    repo_root: &Path,
    report: &ReviewReport,
) -> Result<Vec<GroundingIssue>, ToolError> {
    let mut issues = Vec::new();
    let mut cache: std::collections::HashMap<std::path::PathBuf, usize> =
        std::collections::HashMap::new();

    for (idx, f) in report.findings.iter().enumerate() {
        if f.line == 0 {
            continue;
        }

        let (display_file, abs_opt) = resolve_finding_file_path(repo_root, &f.file)?;
        match abs_opt {
            None => {
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
pub fn format_grounding_issues_for_prompt(issues: &[GroundingIssue]) -> String {
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
pub fn apply_grounding_fallback(report: &mut ReviewReport, issues: &[GroundingIssue]) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ReviewFinding;
    use crate::types::ReviewVerdict;
    use crate::types::Severity;
    use tempfile::tempdir;

    #[test]
    fn parse_and_validate_accepts_valid_report() {
        let json = r#"{"lens":"security","verdict":"approved","findings":[],"notes":[]}"#;
        let report = parse_and_validate_report(json, ReviewLens::Security).unwrap();
        assert_eq!(report.lens, ReviewLens::Security);
        assert_eq!(report.verdict, ReviewVerdict::Approved);
    }

    #[test]
    fn parse_and_validate_rejects_lens_mismatch() {
        let json = r#"{"lens":"correctness","verdict":"approved","findings":[],"notes":[]}"#;
        let result = parse_and_validate_report(json, ReviewLens::Security);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Lens mismatch"));
    }

    #[test]
    fn grounding_validation_flags_line_past_eof() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("src.rs"), "a\nb\nc\n").unwrap();

        let report = ReviewReport {
            lens: ReviewLens::Security,
            verdict: ReviewVerdict::NeedsChanges,
            findings: vec![ReviewFinding {
                file: "src.rs".into(),
                line: 10,
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
        let dir = tempdir().unwrap();
        let repo = dir.path();

        let report = ReviewReport {
            lens: ReviewLens::Security,
            verdict: ReviewVerdict::Approved,
            findings: vec![ReviewFinding {
                file: "missing.rs".into(),
                line: 0,
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
        assert!(issues.is_empty());
    }

    #[test]
    fn fallback_sets_invalid_lines_to_zero() {
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
}
