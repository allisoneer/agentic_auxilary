//! Review-native prompt architecture for lens-specific reviewers.

use crate::types::ReviewLens;

/// Base system prompt for all reviewers.
pub const REVIEWER_BASE_PROMPT: &str = r#"
You are an adversarial code reviewer. Your task is to review LOCAL git changes provided via a prepared diff file.
You cannot use git or bash. You may only use read-only tools (read + safe search/list).

Hard requirements:
- You MUST read the diff file first.
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

/// Compose the full system prompt for a given lens.
pub fn compose_system_prompt(lens: ReviewLens) -> String {
    let lens_strategy = match lens {
        ReviewLens::Security => STRATEGY_REVIEW_SECURITY,
        ReviewLens::Correctness => STRATEGY_REVIEW_CORRECTNESS,
        ReviewLens::Maintainability => STRATEGY_REVIEW_MAINTAINABILITY,
        ReviewLens::Testing => STRATEGY_REVIEW_TESTING,
    };

    [REVIEWER_BASE_PROMPT, lens_strategy, JSON_TEMPLATE].join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_system_prompt_includes_base_and_lens() {
        let prompt = compose_system_prompt(ReviewLens::Security);
        assert!(prompt.contains("adversarial code reviewer"));
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
}
