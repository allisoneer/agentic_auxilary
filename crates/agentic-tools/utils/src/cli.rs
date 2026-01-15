//! CLI and environment parsing helpers.
//!
//! This module provides utilities for parsing environment variables
//! and CLI inputs in consistent ways.

use std::collections::BTreeSet;

/// Parse a comma/whitespace separated string into a lowercase-trimmed set.
///
/// # Example
///
/// ```
/// use agentic_tools_utils::cli::parse_comma_set;
///
/// let set = parse_comma_set("foo, BAR, baz");
/// assert!(set.contains("foo"));
/// assert!(set.contains("bar"));
/// assert!(set.contains("baz"));
/// assert_eq!(set.len(), 3);
/// ```
pub fn parse_comma_set(input: &str) -> BTreeSet<String> {
    input
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_lowercase())
        .collect()
}

/// Read a boolean from an environment variable.
///
/// Accepted truthy values: `1`, `true`, `yes`, `on`
/// Accepted falsy values: `0`, `false`, `no`, `off`
/// Any other value or unset variable returns the default.
///
/// # Example
///
/// ```
/// use agentic_tools_utils::cli::bool_from_env;
///
/// // Returns default when var is not set
/// let value = bool_from_env("NONEXISTENT_VAR_12345", true);
/// assert!(value);
/// ```
pub fn bool_from_env(var: &str, default: bool) -> bool {
    match std::env::var(var) {
        Ok(v) => match v.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            _ => default,
        },
        Err(_) => default,
    }
}

/// Read a usize from an environment variable.
///
/// Returns the default if the variable is not set or cannot be parsed.
///
/// # Example
///
/// ```
/// use agentic_tools_utils::cli::usize_from_env;
///
/// // Returns default when var is not set
/// let value = usize_from_env("NONEXISTENT_VAR_12345", 100);
/// assert_eq!(value, 100);
/// ```
pub fn usize_from_env(var: &str, default: usize) -> usize {
    std::env::var(var)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(default)
}

/// Return `Option<BTreeSet>` if the environment variable is present and non-empty.
///
/// # Example
///
/// ```
/// use agentic_tools_utils::cli::set_from_env;
///
/// // Returns None when var is not set
/// let value = set_from_env("NONEXISTENT_VAR_12345");
/// assert!(value.is_none());
/// ```
pub fn set_from_env(var: &str) -> Option<BTreeSet<String>> {
    std::env::var(var)
        .ok()
        .map(|s| parse_comma_set(&s))
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_comma_set_basic() {
        let set = parse_comma_set("foo,bar,baz");
        assert_eq!(set.len(), 3);
        assert!(set.contains("foo"));
        assert!(set.contains("bar"));
        assert!(set.contains("baz"));
    }

    #[test]
    fn parse_comma_set_with_spaces() {
        let set = parse_comma_set("foo, bar , baz");
        assert_eq!(set.len(), 3);
        assert!(set.contains("foo"));
        assert!(set.contains("bar"));
        assert!(set.contains("baz"));
    }

    #[test]
    fn parse_comma_set_whitespace_separated() {
        let set = parse_comma_set("foo bar baz");
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn parse_comma_set_mixed_separators() {
        let set = parse_comma_set("foo, bar baz");
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn parse_comma_set_lowercases() {
        let set = parse_comma_set("FOO, Bar, BAZ");
        assert!(set.contains("foo"));
        assert!(set.contains("bar"));
        assert!(set.contains("baz"));
        assert!(!set.contains("FOO"));
    }

    #[test]
    fn parse_comma_set_empty() {
        let set = parse_comma_set("");
        assert!(set.is_empty());
    }

    #[test]
    fn parse_comma_set_only_separators() {
        let set = parse_comma_set(", , , ");
        assert!(set.is_empty());
    }

    #[test]
    fn parse_comma_set_duplicates_deduplicated() {
        let set = parse_comma_set("foo, FOO, Foo");
        assert_eq!(set.len(), 1);
        assert!(set.contains("foo"));
    }

    #[test]
    fn bool_from_env_returns_default_when_unset() {
        // Use a var name that definitely doesn't exist
        let result = bool_from_env("__AGENTIC_TEST_NONEXISTENT_VAR__", true);
        assert!(result);

        let result = bool_from_env("__AGENTIC_TEST_NONEXISTENT_VAR__", false);
        assert!(!result);
    }

    #[test]
    fn usize_from_env_returns_default_when_unset() {
        let result = usize_from_env("__AGENTIC_TEST_NONEXISTENT_VAR__", 42);
        assert_eq!(result, 42);
    }

    #[test]
    fn set_from_env_returns_none_when_unset() {
        let result = set_from_env("__AGENTIC_TEST_NONEXISTENT_VAR__");
        assert!(result.is_none());
    }
}
