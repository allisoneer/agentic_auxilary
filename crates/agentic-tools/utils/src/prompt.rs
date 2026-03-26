//! Prompt hardening utilities.

/// Truncate a string for safe embedding into LLM prompts.
///
/// Returns `(truncated_string, was_truncated)`. Truncation is UTF-8 safe (char-boundary slicing)
/// and appends `...[truncated]` when the input exceeds `max_chars`.
pub fn truncate_for_prompt(s: &str, max_chars: usize) -> (String, bool) {
    const SUFFIX: &str = "...[truncated]";

    match s.char_indices().nth(max_chars) {
        None => (s.to_string(), false),
        Some((byte_idx, _)) => {
            let mut out = String::with_capacity(byte_idx + SUFFIX.len());
            out.push_str(&s[..byte_idx]);
            out.push_str(SUFFIX);
            (out, true)
        }
    }
}

/// Wrap untrusted text in explicit XML-like tags to make the security boundary obvious to the model.
pub fn wrap_untrusted(tag: &str, body: &str) -> String {
    format!("<{tag}>\n{body}\n</{tag}>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_does_not_modify_short_string() {
        let s = "hello";
        let (out, truncated) = truncate_for_prompt(s, 10);
        assert_eq!(out, "hello");
        assert!(!truncated);
    }

    #[test]
    fn truncate_adds_suffix_to_long_string() {
        let s = "hello world this is a long string";
        let (out, truncated) = truncate_for_prompt(s, 10);
        assert!(truncated);
        assert!(out.ends_with("...[truncated]"));
        // First 10 chars + suffix
        assert_eq!(&out[..10], "hello worl");
    }

    #[test]
    fn truncate_handles_exact_length() {
        let s = "12345";
        let (out, truncated) = truncate_for_prompt(s, 5);
        assert_eq!(out, "12345");
        assert!(!truncated);
    }

    #[test]
    fn truncate_handles_unicode() {
        // 3 chars: emoji, emoji, "a"
        let s = "🎉🎊a";
        let (out, truncated) = truncate_for_prompt(s, 2);
        assert!(truncated);
        // Should keep first 2 chars (🎉🎊)
        assert!(out.starts_with("🎉🎊"));
        assert!(out.ends_with("...[truncated]"));
    }

    #[test]
    fn wrap_untrusted_produces_xml_tags() {
        let result = wrap_untrusted("untrusted_input", "some data");
        assert_eq!(result, "<untrusted_input>\nsome data\n</untrusted_input>");
    }

    #[test]
    fn wrap_untrusted_preserves_body() {
        let body = "line1\nline2\nline3";
        let result = wrap_untrusted("data", body);
        assert!(result.contains("line1\nline2\nline3"));
    }
}
