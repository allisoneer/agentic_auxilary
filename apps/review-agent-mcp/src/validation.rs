//! JSON extraction and semantic validation for reviewer outputs.

use crate::types::{Confidence, ReviewLens, ReviewReport};
use agentic_tools_core::ToolError;

/// Extract JSON from model output, handling raw JSON, fenced blocks, and preamble/trailing text.
///
/// Tries multiple extraction strategies in order:
/// 1. Whole string as valid JSON
/// 2. Fenced code blocks (```json or ```)
/// 3. First `{` to last `}` fallback
pub fn extract_json_best_effort(text: &str) -> Result<String, ToolError> {
    let t = text.trim();

    // 1) Try whole string as JSON
    if serde_json::from_str::<serde_json::Value>(t).is_ok() {
        return Ok(t.to_string());
    }

    // 2) Try extracting from fenced code blocks
    if t.contains("```") {
        for chunk in t.split("```").skip(1).step_by(2) {
            // Skip language identifier if present (e.g., "json\n{...")
            let chunk = chunk.trim_start_matches(|c: char| c.is_alphabetic() || c == '\n');
            if let (Some(a), Some(b)) = (chunk.find('{'), chunk.rfind('}')) {
                let candidate = &chunk[a..=b];
                if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                    return Ok(candidate.to_string());
                }
            }
        }
    }

    // 3) Fallback: find first { to last }
    if let (Some(a), Some(b)) = (t.find('{'), t.rfind('}')) {
        let candidate = &t[a..=b];
        if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
            return Ok(candidate.to_string());
        }
    }

    Err(ToolError::Internal(
        "Failed to extract valid JSON from reviewer output".into(),
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_raw_json() {
        let s = r#"{"lens":"security","verdict":"approved","findings":[],"notes":[]}"#;
        let j = extract_json_best_effort(s).unwrap();
        assert!(j.starts_with('{'));
        assert!(j.ends_with('}'));
    }

    #[test]
    fn extracts_fenced_json() {
        let s = "Here is the review:\n```json\n{\"lens\":\"security\",\"verdict\":\"approved\",\"findings\":[],\"notes\":[]}\n```\nDone.";
        let j = extract_json_best_effort(s).unwrap();
        assert!(j.contains("\"lens\":\"security\""));
    }

    #[test]
    fn extracts_json_with_preamble() {
        let s = "I found the following issues:\n{\"lens\":\"correctness\",\"verdict\":\"needs_changes\",\"findings\":[],\"notes\":[]}";
        let j = extract_json_best_effort(s).unwrap();
        assert!(j.starts_with('{'));
    }

    #[test]
    fn rejects_invalid_json() {
        let s = "This is not JSON at all";
        let result = extract_json_best_effort(s);
        assert!(result.is_err());
    }
}
