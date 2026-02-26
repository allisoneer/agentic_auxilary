//! Security validation for recipe arguments.
//!
//! Validates argument values against forbidden patterns to prevent injection attacks.

use serde_json::Value;
use std::collections::HashMap;

/// Shell metacharacters that enable command chaining or redirection.
const FORBIDDEN_SHELL_CHARS: &[char] = &[';', '&', '|', '>', '<'];

/// Newline characters that could enable shell escape vectors.
const FORBIDDEN_NEWLINE_CHARS: &[char] = &['\n', '\r'];

/// Pattern types for forbidden content detection using `str::contains()`.
#[derive(Clone, Copy)]
enum ForbiddenPattern {
    /// Match any character in the given set (e.g., shell metacharacters).
    AnyChar(&'static [char]),
    /// Match an exact substring (e.g., "$(" or "..").
    Substring(&'static str),
    /// Match a single character (e.g., backtick or null byte).
    Char(char),
}

impl ForbiddenPattern {
    /// Check if the pattern matches anywhere in the given string.
    fn is_match(self, s: &str) -> bool {
        match self {
            Self::AnyChar(chars) => s.contains(chars),
            Self::Substring(substr) => s.contains(substr),
            Self::Char(ch) => s.contains(ch),
        }
    }
}

/// Validator for recipe argument security.
pub struct SecurityValidator {
    forbidden: Vec<ForbiddenPattern>,
    max_len: usize,
}

impl Default for SecurityValidator {
    fn default() -> Self {
        Self {
            forbidden: vec![
                ForbiddenPattern::AnyChar(FORBIDDEN_SHELL_CHARS), // r"[;&|><]"
                ForbiddenPattern::Substring("$("),                // r"\$\("
                ForbiddenPattern::Char('`'),                      // r"`"
                ForbiddenPattern::Substring("${"),                // r"\$\{"
                ForbiddenPattern::Substring(".."),                // r"\.\."
                ForbiddenPattern::AnyChar(FORBIDDEN_NEWLINE_CHARS), // r"[\n\r]"
                ForbiddenPattern::Char('\0'),                     // r"\x00"
            ],
            max_len: 1024,
        }
    }
}

impl SecurityValidator {
    /// Validate all arguments against security rules.
    pub fn validate(&self, args: &HashMap<String, Value>) -> Result<(), String> {
        for (name, val) in args {
            self.validate_value(name, val)?;
        }
        Ok(())
    }

    fn validate_value(&self, name: &str, val: &Value) -> Result<(), String> {
        if let Value::Array(items) = val {
            for (i, item) in items.iter().enumerate() {
                self.validate_value(&format!("{name}[{i}]"), item)?;
            }
        } else {
            let s = value_to_string(val)?;
            if s.len() > self.max_len {
                return Err(format!(
                    "Argument '{}' exceeds max length {}",
                    name, self.max_len
                ));
            }
            if s.starts_with('/') || s.starts_with('~') {
                return Err(format!(
                    "Argument '{name}' looks like absolute path; use repo-relative"
                ));
            }
            for pat in &self.forbidden {
                if pat.is_match(&s) {
                    return Err(format!("Argument '{name}' contains forbidden pattern"));
                }
            }
        }
        Ok(())
    }
}

/// Convert a JSON value to a string for validation/execution.
pub fn value_to_string(v: &Value) -> Result<String, String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Array(a) => Ok(a
            .iter()
            .map(value_to_string)
            .collect::<Result<Vec<_>, _>>()?
            .join(" ")),
        Value::Null => Ok(String::new()),
        Value::Object(_) => Err("Object arguments are not supported".into()),
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn allows_safe_values() {
        let v = SecurityValidator::default();
        let mut args = HashMap::new();
        args.insert("name".into(), json!("hello"));
        args.insert("count".into(), json!(42));
        args.insert("enabled".into(), json!(true));
        assert!(v.validate(&args).is_ok());
    }

    #[test]
    fn blocks_shell_operators() {
        let v = SecurityValidator::default();
        let mut args = HashMap::new();

        args.insert("cmd".into(), json!("foo; bar"));
        assert!(v.validate(&args).is_err());

        args.clear();
        args.insert("cmd".into(), json!("foo && bar"));
        assert!(v.validate(&args).is_err());

        args.clear();
        args.insert("cmd".into(), json!("foo | bar"));
        assert!(v.validate(&args).is_err());

        args.clear();
        args.insert("cmd".into(), json!("foo > out"));
        assert!(v.validate(&args).is_err());
    }

    #[test]
    fn blocks_command_substitution() {
        let v = SecurityValidator::default();
        let mut args = HashMap::new();

        args.insert("cmd".into(), json!("$(whoami)"));
        assert!(v.validate(&args).is_err());

        args.clear();
        args.insert("cmd".into(), json!("`whoami`"));
        assert!(v.validate(&args).is_err());

        args.clear();
        args.insert("cmd".into(), json!("${HOME}"));
        assert!(v.validate(&args).is_err());
    }

    #[test]
    fn blocks_path_traversal() {
        let v = SecurityValidator::default();
        let mut args = HashMap::new();

        args.insert("path".into(), json!("../secret"));
        assert!(v.validate(&args).is_err());
    }

    #[test]
    fn blocks_newlines() {
        let v = SecurityValidator::default();
        let mut args = HashMap::new();

        // Block \n
        args.insert("cmd".into(), json!("foo\nbar"));
        assert!(v.validate(&args).is_err());

        // Block \r
        args.clear();
        args.insert("cmd".into(), json!("foo\rbar"));
        assert!(v.validate(&args).is_err());

        // Block \r\n
        args.clear();
        args.insert("cmd".into(), json!("foo\r\nbar"));
        assert!(v.validate(&args).is_err());
    }

    #[test]
    fn blocks_null_bytes() {
        let v = SecurityValidator::default();
        let mut args = HashMap::new();

        args.insert("cmd".into(), json!("foo\x00bar"));
        assert!(v.validate(&args).is_err());
    }

    #[test]
    fn blocks_absolute_paths() {
        let v = SecurityValidator::default();
        let mut args = HashMap::new();

        args.insert("path".into(), json!("/etc/passwd"));
        assert!(v.validate(&args).is_err());

        args.clear();
        args.insert("path".into(), json!("~/secret"));
        assert!(v.validate(&args).is_err());
    }

    #[test]
    fn blocks_long_values() {
        let v = SecurityValidator::default();
        let mut args = HashMap::new();
        args.insert("data".into(), json!("x".repeat(2000)));
        assert!(v.validate(&args).is_err());
    }

    #[test]
    fn validates_array_items() {
        let v = SecurityValidator::default();
        let mut args = HashMap::new();

        // Safe array
        args.insert("items".into(), json!(["foo", "bar", "baz"]));
        assert!(v.validate(&args).is_ok());

        // Array with forbidden pattern
        args.clear();
        args.insert("items".into(), json!(["foo", "$(bad)", "baz"]));
        assert!(v.validate(&args).is_err());
    }

    #[test]
    fn value_to_string_conversions() {
        assert_eq!(value_to_string(&json!("hello")).unwrap(), "hello");
        assert_eq!(value_to_string(&json!(42)).unwrap(), "42");
        assert_eq!(value_to_string(&json!(true)).unwrap(), "true");
        assert_eq!(value_to_string(&json!(["a", "b", "c"])).unwrap(), "a b c");
        assert_eq!(value_to_string(&json!(null)).unwrap(), "");
        assert!(value_to_string(&json!({"key": "value"})).is_err());
    }

    // Helper to create args map for edge-case tests
    fn args_map<const N: usize>(pairs: [(&str, &str); N]) -> HashMap<String, Value> {
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), json!(v)))
            .collect()
    }

    // Edge-case tests for contains()-based pattern matching

    #[test]
    fn allows_dollar_alone() {
        let v = SecurityValidator::default();
        assert!(v.validate(&args_map([("x", "$")])).is_ok());
    }

    #[test]
    fn blocks_dollar_paren() {
        let v = SecurityValidator::default();
        assert!(v.validate(&args_map([("x", "$(")])).is_err());
    }

    #[test]
    fn allows_single_dot() {
        let v = SecurityValidator::default();
        assert!(v.validate(&args_map([("x", ".")])).is_ok());
    }

    #[test]
    fn blocks_double_dot() {
        let v = SecurityValidator::default();
        assert!(v.validate(&args_map([("x", "..")])).is_err());
    }

    #[test]
    fn allows_dollar_space_paren() {
        // "$ (" with a space should be allowed (not "$(")
        let v = SecurityValidator::default();
        assert!(v.validate(&args_map([("x", "$ (")])).is_ok());
    }

    #[test]
    fn allows_escaped_newline_literal() {
        // The literal string "\\n" (two characters: backslash, n) should be allowed
        let v = SecurityValidator::default();
        assert!(v.validate(&args_map([("x", "\\n")])).is_ok());
    }
}
