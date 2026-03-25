//! LLM output extraction utilities.

use thiserror::Error;

/// Error type for LLM output extraction.
#[derive(Debug, Error)]
#[error("Failed to extract valid JSON from model output")]
pub struct JsonExtractionError;

/// Best-effort JSON extraction from model output.
///
/// Tries multiple extraction strategies in order:
/// 1. Whole string as valid JSON
/// 2. Fenced code blocks (```json or ```)
/// 3. First `{` to last `}` fallback
///
/// # Errors
///
/// Returns an error if no valid JSON can be extracted from the input.
pub fn extract_json_best_effort(text: &str) -> Result<String, JsonExtractionError> {
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

    Err(JsonExtractionError)
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

    #[test]
    fn extracts_fenced_json_without_language_tag() {
        let s = "Preamble\n```\n{\"lens\":\"security\",\"verdict\":\"approved\",\"findings\":[],\"notes\":[]}\n```\n";
        let j = extract_json_best_effort(s).unwrap();
        assert!(j.contains("\"lens\":\"security\""));
    }

    #[test]
    fn extracts_json_from_second_fence_when_first_is_not_json() {
        let s = "```text\nnot json\n```\n```json\n{\"lens\":\"security\",\"verdict\":\"approved\",\"findings\":[],\"notes\":[]}\n```\n";
        let j = extract_json_best_effort(s).unwrap();
        assert!(j.contains("\"verdict\":\"approved\""));
    }

    #[test]
    fn extracts_json_outside_fences_when_fences_contain_no_json() {
        let s = "```text\nhello\n```\nTrailing:\n{\"lens\":\"security\",\"verdict\":\"approved\",\"findings\":[],\"notes\":[]}\n";
        let j = extract_json_best_effort(s).unwrap();
        assert!(j.contains("\"lens\":\"security\""));
    }
}
