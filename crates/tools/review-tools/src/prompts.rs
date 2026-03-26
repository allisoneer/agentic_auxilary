//! Review-native prompt architecture for lens-specific reviewers.
//!
//! Updated for fileless diff embedding (diff content is embedded in the prompt,
//! not read from a file).

use crate::types::ReviewLens;

/// Base system prompt for all reviewers (fileless version).
pub const REVIEWER_BASE_PROMPT: &str = r#"
You are an adversarial code reviewer. Your task is to review LOCAL git changes provided inline in this prompt.
The diff content will be provided within <untrusted_diff> tags. You cannot use git or bash.
You may only use read-only tools (Read, Grep, Glob) for source file inspection.

Hard requirements:
- Review the diff content provided in <untrusted_diff> tags.
- The `line` field MUST be a SOURCE-FILE line number (1-based) in the file named by `file`.
  - DO NOT use line numbers from the inline diff.
  - Use Grep on the source file to locate a unique snippet from the diff; use the Grep result line number.
  - If the file is deleted/non-existent OR you cannot verify the exact source line: set "line": 0 (do not guess).
- Output MUST be valid JSON matching the provided template.
- No markdown, no code fences, no commentary outside JSON.
- If unsure: confidence="medium" and include a caveat explaining uncertainty.
- category MUST match the lens you were assigned.
"#;

/// Security lens focus areas.
pub const STRATEGY_REVIEW_SECURITY: &str = r"
Security lens focus:
- Authentication and authorization boundaries
- Input validation, injection surfaces (SQL, command, path traversal)
- Secrets exposure, logging of sensitive data
- Unsafe deserialization, crypto misuse
- SSRF, privilege escalation, multi-tenant safety
- PII leaks, dependency vulnerabilities
";

/// Correctness lens focus areas.
pub const STRATEGY_REVIEW_CORRECTNESS: &str = r"
Correctness lens focus:
- Logic bugs, off-by-one errors, wrong defaults
- Error handling gaps, panic paths
- Concurrency issues, race conditions
- Resource leaks, timeout/retry handling
- API contract violations, invariant breakage
- State machine changes, nullability issues
";

/// Maintainability lens focus areas.
pub const STRATEGY_REVIEW_MAINTAINABILITY: &str = r#"
Maintainability lens focus:
- Complexity, unclear naming, missing documentation
- Code duplication, architectural drift
- Layering violations, coupling issues
- Performance footguns when material
- Configuration sprawl, backwards compatibility
- "Quick hacks" that should be flagged
"#;

/// Testing lens focus areas.
pub const STRATEGY_REVIEW_TESTING: &str = r"
Testing lens focus:
- Missing tests for new behavior or edge cases
- Regression risk without coverage
- Brittle or flaky test patterns
- Logging, metrics, tracing gaps
- Failure-mode visibility, debuggability
- Determinism issues in tests
";

/// JSON output template embedded in prompts.
pub const JSON_TEMPLATE: &str = r#"
Return ONLY this JSON structure (no markdown fences):
{
  "lens": "security|correctness|maintainability|testing",
  "verdict": "approved|needs_changes",
  "findings": [
    {
      "file": "path/to/file",
      "line": 0,
      "category": "security|correctness|maintainability|testing",
      "severity": "critical|high|medium|low",
      "confidence": "high|medium",
      "title": "short title",
      "evidence": "quote relevant diff snippet + why it indicates the issue",
      "suggested_fix": "concrete next step",
      "caveat": "required when confidence=medium, null otherwise"
    }
  ],
  "notes": ["optional notes"]
}
"#;

/// Durable reviewer philosophy shared across all lenses.
pub const REVIEWER_PHILOSOPHY: &str = r#"
Review philosophy:
- Be adversarial but accurate; avoid speculation.
- Every finding must be grounded in the diff (quote a snippet or describe the hunk).
- Prefer actionable, minimal fixes with concrete next steps.
- If uncertain, set confidence="medium" and include a caveat.
- Respect tool boundaries: no git, no bash, no write/edit.
"#;

/// Compose the full system prompt for a given lens.
pub fn compose_system_prompt(lens: ReviewLens) -> String {
    let lens_strategy = match lens {
        ReviewLens::Security => STRATEGY_REVIEW_SECURITY,
        ReviewLens::Correctness => STRATEGY_REVIEW_CORRECTNESS,
        ReviewLens::Maintainability => STRATEGY_REVIEW_MAINTAINABILITY,
        ReviewLens::Testing => STRATEGY_REVIEW_TESTING,
    };

    [
        REVIEWER_BASE_PROMPT,
        REVIEWER_PHILOSOPHY,
        lens_strategy,
        JSON_TEMPLATE,
    ]
    .join("\n\n")
}

/// Compose the user prompt with embedded diff content.
pub fn compose_user_prompt(lens: ReviewLens, diff_content: &str, focus: Option<&str>) -> String {
    let focus_text = focus.unwrap_or("(no specific focus guidance)");
    let lens_name = match lens {
        ReviewLens::Security => "security",
        ReviewLens::Correctness => "correctness",
        ReviewLens::Maintainability => "maintainability",
        ReviewLens::Testing => "testing",
    };

    format!(
        "Review these changes with the {lens_name} lens.\n\
         Focus guidance: {focus_text}\n\
         Line numbers MUST be SOURCE-FILE line numbers; use 0 if unknown.\n\
         Requirements: analyze the diff below, then inspect referenced source files as needed.\n\
         Output ONLY valid JSON matching the template.\n\n\
         <untrusted_diff>\n{diff_content}\n</untrusted_diff>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_system_prompt_includes_base_philosophy_and_lens() {
        let prompt = compose_system_prompt(ReviewLens::Security);
        assert!(prompt.contains("adversarial code reviewer"));
        assert!(prompt.contains("Review philosophy"));
        assert!(prompt.contains("Security lens focus"));
        assert!(prompt.contains("JSON structure"));
    }

    #[test]
    fn compose_system_prompt_includes_json_template() {
        let prompt = compose_system_prompt(ReviewLens::Correctness);
        assert!(prompt.contains("\"lens\":"));
        assert!(prompt.contains("\"verdict\":"));
        assert!(prompt.contains("\"findings\":"));
    }

    #[test]
    fn all_lenses_produce_prompts() {
        for lens in [
            ReviewLens::Security,
            ReviewLens::Correctness,
            ReviewLens::Maintainability,
            ReviewLens::Testing,
        ] {
            let prompt = compose_system_prompt(lens);
            assert!(!prompt.is_empty());
            assert!(prompt.contains("lens focus"));
        }
    }

    #[test]
    fn reviewer_base_prompt_requires_source_file_line_numbers() {
        assert!(
            REVIEWER_BASE_PROMPT.contains("SOURCE-FILE line number"),
            "REVIEWER_BASE_PROMPT must require SOURCE-FILE line numbers"
        );
        assert!(
            REVIEWER_BASE_PROMPT.contains("DO NOT use line numbers from the inline diff"),
            "REVIEWER_BASE_PROMPT must explicitly forbid inline diff line numbers"
        );
        assert!(
            REVIEWER_BASE_PROMPT.contains(r#"set "line": 0"#),
            "REVIEWER_BASE_PROMPT must instruct using line=0 when unverifiable"
        );
    }

    #[test]
    fn compose_user_prompt_includes_diff() {
        let prompt = compose_user_prompt(
            ReviewLens::Security,
            "diff --git a/test.rs b/test.rs",
            Some("focus on auth"),
        );
        assert!(prompt.contains("<untrusted_diff>"));
        assert!(prompt.contains("diff --git"));
        assert!(prompt.contains("</untrusted_diff>"));
        assert!(prompt.contains("security lens"));
        assert!(prompt.contains("focus on auth"));
    }

    #[test]
    fn compose_user_prompt_handles_no_focus() {
        let prompt = compose_user_prompt(ReviewLens::Testing, "diff content", None);
        assert!(prompt.contains("no specific focus guidance"));
    }
}
