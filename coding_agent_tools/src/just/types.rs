//! Types for just search and execute tools.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use universal_tool_core::mcp::McpFormatter;

/// Parameters for the search tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct SearchParams {
    /// Search query (substring match on name/docs)
    pub query: Option<String>,
    /// Directory filter (repo-relative or absolute)
    pub dir: Option<String>,
}

/// Parameters for the execute tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteParams {
    /// Recipe name (e.g., "check", "test", "build")
    pub recipe: String,
    /// Directory containing the justfile (from search results)
    pub dir: Option<String>,
    /// Arguments keyed by parameter name; star params accept arrays
    pub args: Option<HashMap<String, serde_json::Value>>,
}

/// A single search result item.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchItem {
    /// Recipe name
    pub recipe: String,
    /// Directory containing the justfile
    pub dir: String,
    /// Documentation comment (first line)
    pub doc: Option<String>,
    /// Parameter names (with ? for optional, * for variadic)
    pub params: Vec<String>,
}

/// Output from the search tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SearchOutput {
    /// Items in the current page
    pub items: Vec<SearchItem>,
    /// Whether more results are available
    pub has_more: bool,
}

impl McpFormatter for SearchOutput {
    fn mcp_format_text(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(out, "just recipes:");
        for it in &self.items {
            let params = if it.params.is_empty() {
                String::new()
            } else {
                format!("({})", it.params.join(", "))
            };
            let _ = writeln!(out, "  {} {}  [dir: {}]", it.recipe, params, it.dir);
            if let Some(doc) = &it.doc
                && let Some(line) = doc.lines().next()
                && !line.trim().is_empty()
            {
                let _ = writeln!(out, "    {}", line.trim());
            }
        }
        if self.has_more {
            let _ = writeln!(
                out,
                "(more results — call again with same params for next page)"
            );
        }
        out.trim_end().to_string()
    }
}

/// Output from the execute tool.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteOutput {
    /// Directory where recipe was executed
    pub dir: String,
    /// Recipe that was executed
    pub recipe: String,
    /// Whether execution succeeded (exit code 0)
    pub success: bool,
    /// Exit code (if available)
    pub exit_code: Option<i32>,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
}

impl McpFormatter for ExecuteOutput {
    fn mcp_format_text(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "just {} in {} => {} (exit: {})",
            self.recipe,
            self.dir,
            if self.success { "SUCCESS" } else { "FAILURE" },
            self.exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "?".into())
        );
        if !self.stdout.is_empty() {
            let _ = writeln!(out, "\nstdout:\n{}", self.stdout.trim_end());
        }
        if !self.stderr.is_empty() {
            let _ = writeln!(out, "\nstderr:\n{}", self.stderr.trim_end());
        }
        out.trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_output_format_empty() {
        let output = SearchOutput {
            items: vec![],
            has_more: false,
        };
        let text = output.mcp_format_text();
        assert!(text.contains("just recipes:"));
        assert!(!text.contains("more results"));
    }

    #[test]
    fn search_output_format_with_items() {
        let output = SearchOutput {
            items: vec![
                SearchItem {
                    recipe: "build".into(),
                    dir: "/repo/crate1".into(),
                    doc: Some("Build the project".into()),
                    params: vec!["target".into()],
                },
                SearchItem {
                    recipe: "test".into(),
                    dir: "/repo/crate2".into(),
                    doc: None,
                    params: vec![],
                },
            ],
            has_more: true,
        };
        let text = output.mcp_format_text();
        assert!(text.contains("build (target)"));
        assert!(text.contains("[dir: /repo/crate1]"));
        assert!(text.contains("Build the project"));
        assert!(text.contains("test   [dir: /repo/crate2]"));
        assert!(text.contains("more results"));
    }

    #[test]
    fn execute_output_format_success() {
        let output = ExecuteOutput {
            dir: "/repo".into(),
            recipe: "check".into(),
            success: true,
            exit_code: Some(0),
            stdout: "All checks passed\n".into(),
            stderr: String::new(),
        };
        let text = output.mcp_format_text();
        assert!(text.contains("SUCCESS"));
        assert!(text.contains("exit: 0"));
        assert!(text.contains("stdout:"));
        assert!(text.contains("All checks passed"));
        assert!(!text.contains("stderr:"));
    }

    #[test]
    fn execute_output_format_failure() {
        let output = ExecuteOutput {
            dir: "/repo".into(),
            recipe: "build".into(),
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "error: compilation failed\n".into(),
        };
        let text = output.mcp_format_text();
        assert!(text.contains("FAILURE"));
        assert!(text.contains("exit: 1"));
        assert!(!text.contains("stdout:"));
        assert!(text.contains("stderr:"));
        assert!(text.contains("compilation failed"));
    }
}
