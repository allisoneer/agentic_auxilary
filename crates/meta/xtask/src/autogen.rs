//! Autogen block replacement utilities.
//!
//! Provides utilities for replacing content within BEGIN/END marker blocks.

use anyhow::{Context, Result};
use regex::Regex;

/// Replace content within a named autogen block.
///
/// Blocks are delimited by:
/// - `<!-- BEGIN:xtask:autogen <key> -->`
/// - `<!-- END:xtask:autogen -->`
///
/// If the block doesn't exist in the input, it will be appended at the end.
///
/// Returns (updated_content, changed) where changed indicates if content was modified.
pub fn replace_named_block(input: &str, key: &str, body: &str) -> Result<(String, bool)> {
    let pattern = format!(
        r"(?s)<!--\s*BEGIN:xtask:autogen\s+{}\s*-->.*?<!--\s*END:xtask:autogen\s*-->",
        regex::escape(key)
    );
    let re = Regex::new(&pattern).context("Failed to compile autogen regex")?;

    let replacement = format!(
        "<!-- BEGIN:xtask:autogen {} -->\n{}\n<!-- END:xtask:autogen -->",
        key, body
    );

    if let Some(m) = re.find(input) {
        let original_block = m.as_str();
        let changed = original_block != replacement;

        let mut out = String::with_capacity(input.len() + body.len());
        out.push_str(&input[..m.start()]);
        out.push_str(&replacement);
        out.push_str(&input[m.end()..]);

        Ok((out, changed))
    } else {
        // Block not found - append at end
        let mut out = String::with_capacity(input.len() + replacement.len() + 2);
        out.push_str(input);
        if !input.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&replacement);
        out.push('\n');
        Ok((out, true))
    }
}

/// Replace content within a named autogen block using TOML-style comments.
///
/// Blocks are delimited by:
/// - `# BEGIN:xtask:autogen <key>`
/// - `# END:xtask:autogen`
///
/// Returns (updated_content, changed) where changed indicates if content was modified.
pub fn replace_named_block_toml(input: &str, key: &str, body: &str) -> Result<(String, bool)> {
    let pattern = format!(
        r"(?s)#\s*BEGIN:xtask:autogen\s+{}\s*\n.*?#\s*END:xtask:autogen",
        regex::escape(key)
    );
    let re = Regex::new(&pattern).context("Failed to compile autogen regex for TOML")?;

    let replacement = format!(
        "# BEGIN:xtask:autogen {}\n{}\n# END:xtask:autogen",
        key, body
    );

    if let Some(m) = re.find(input) {
        let original_block = m.as_str();
        let changed = original_block != replacement;

        let mut out = String::with_capacity(input.len() + body.len());
        out.push_str(&input[..m.start()]);
        out.push_str(&replacement);
        out.push_str(&input[m.end()..]);

        Ok((out, changed))
    } else {
        // Block not found - append at end
        let mut out = String::with_capacity(input.len() + replacement.len() + 2);
        out.push_str(input);
        if !input.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&replacement);
        out.push('\n');
        Ok((out, true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_existing_block() {
        let input = r#"Some header
<!-- BEGIN:xtask:autogen test-key -->
old content
<!-- END:xtask:autogen -->
Some footer"#;

        let (output, changed) = replace_named_block(input, "test-key", "new content").unwrap();
        assert!(changed);
        assert!(output.contains("new content"));
        assert!(!output.contains("old content"));
        assert!(output.contains("Some header"));
        assert!(output.contains("Some footer"));
    }

    #[test]
    fn test_append_missing_block() {
        let input = "Some content\n";
        let (output, changed) = replace_named_block(input, "new-key", "new body").unwrap();
        assert!(changed);
        assert!(output.contains("<!-- BEGIN:xtask:autogen new-key -->"));
        assert!(output.contains("new body"));
    }

    #[test]
    fn test_idempotent() {
        let input = r#"<!-- BEGIN:xtask:autogen key -->
same content
<!-- END:xtask:autogen -->"#;

        let (output, changed) = replace_named_block(input, "key", "same content").unwrap();
        assert!(!changed);
        assert_eq!(output, input);
    }
}
