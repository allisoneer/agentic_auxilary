//! Security validation for recipe arguments.
//!
//! Validates argument values against forbidden patterns to prevent injection attacks.

use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

/// Validator for recipe argument security.
pub struct SecurityValidator {
    forbidden: Vec<Regex>,
    max_len: usize,
}

impl Default for SecurityValidator {
    fn default() -> Self {
        Self {
            forbidden: vec![
                Regex::new(r"[;&|><]").unwrap(),
                Regex::new(r"\$\(").unwrap(),
                Regex::new(r"`").unwrap(),
                Regex::new(r"\$\{").unwrap(),
                Regex::new(r"\.\.").unwrap(),
                Regex::new(r"[\n\r]").unwrap(), // Block newlines (shell escape vector)
                Regex::new(r"\x00").unwrap(),   // Block null bytes (string truncation)
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
        match val {
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    self.validate_value(&format!("{}[{}]", name, i), item)?;
                }
            }
            _ => {
                let s = value_to_string(val)?;
                if s.len() > self.max_len {
                    return Err(format!(
                        "Argument '{}' exceeds max length {}",
                        name, self.max_len
                    ));
                }
                if s.starts_with('/') || s.starts_with('~') {
                    return Err(format!(
                        "Argument '{}' looks like absolute path; use repo-relative",
                        name
                    ));
                }
                for re in &self.forbidden {
                    if re.is_match(&s) {
                        return Err(format!("Argument '{}' contains forbidden pattern", name));
                    }
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
}
