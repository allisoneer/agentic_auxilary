//! Makefile synchronization for autotools markers.
//!
//! This module discovers tool projects by scanning top-level subdirectories
//! for Makefile presence and generates Makefile sections via marker-based updates.

use anyhow::{bail, Context, Result};
use regex::Regex;
use std::fs;
use std::path::Path;

/// Represents a discovered tool project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    /// Directory name of the tool.
    pub dir: String,
}

/// Discovers tool projects by scanning for top-level directories containing a Makefile.
///
/// Excludes known non-tool directories like `xtask`, `context`, `target`, and hidden directories.
/// Results are sorted alphabetically for deterministic output.
pub fn discover_tools(root: &Path) -> Result<Vec<Tool>> {
    const IGNORE_DIRS: &[&str] = &["xtask", "context", "target", ".git", ".github", ".thoughts"];

    let mut tools = Vec::new();
    for entry in fs::read_dir(root).context("Failed to read root directory")? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if !metadata.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();

        // Skip hidden directories and known non-tool directories
        if name.starts_with('.') || IGNORE_DIRS.contains(&name.as_str()) {
            continue;
        }

        // Check if directory contains a Makefile
        if entry.path().join("Makefile").is_file() {
            tools.push(Tool { dir: name });
        }
    }

    tools.sort_by(|a, b| a.dir.cmp(&b.dir));
    Ok(tools)
}

/// Renders the TOOLS marker block content.
fn render_tools_block(tools: &[Tool]) -> String {
    let names: Vec<&str> = tools.iter().map(|t| t.dir.as_str()).collect();
    format!(
        "# BEGIN:autotools TOOLS\nTOOLS := {}\n# END:autotools TOOLS",
        names.join(" ")
    )
}

/// Replaces a named autotools marker block in the input string.
///
/// Returns the new string and whether any changes were made.
fn replace_block(input: &str, name: &str, replacement: &str) -> Result<(String, bool)> {
    // Match: optional leading whitespace, # BEGIN:autotools NAME, content, # END:autotools NAME
    let pattern = format!(
        r"(?m)^[ \t]*#\s*BEGIN:autotools\s+{name}[ \t]*\n(?s:.*?)^[ \t]*#\s*END:autotools\s+{name}[ \t]*\n?"
    );
    let re = Regex::new(&pattern).context("Failed to compile autotools regex")?;

    if let Some(m) = re.find(input) {
        let mut out = String::with_capacity(input.len());
        out.push_str(&input[..m.start()]);

        // Ensure replacement ends with newline
        let rep = if replacement.ends_with('\n') {
            replacement.to_string()
        } else {
            format!("{replacement}\n")
        };
        out.push_str(&rep);
        out.push_str(&input[m.end()..]);

        // Compare matched region with replacement (accounting for trailing newline)
        let original = &input[m.start()..m.end()];
        let changed = original != rep;

        Ok((out, changed))
    } else {
        bail!(
            "autotools block '{}' not found; add markers to Makefile:\n# BEGIN:autotools {}\n# END:autotools {}",
            name,
            name,
            name
        )
    }
}

/// Synchronizes the Makefile at the given path.
///
/// - `dry_run`: Print output to stdout instead of writing.
/// - `check`: Fail if the Makefile is out of sync.
pub fn sync(path: &Path, dry_run: bool, check: bool) -> Result<()> {
    let root = path
        .parent()
        .map(|p| {
            if p.as_os_str().is_empty() {
                Path::new(".")
            } else {
                p
            }
        })
        .unwrap_or(Path::new("."));

    let tools = discover_tools(root)?;
    let input =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

    let tools_block = render_tools_block(&tools);
    let (output, changed) = replace_block(&input, "TOOLS", &tools_block)?;

    if changed {
        if check {
            bail!("[autotools] Makefile is out of sync; run `cargo run -p xtask -- makefile-sync`");
        }
        if dry_run {
            println!("{output}");
        } else {
            fs::write(path, &output)
                .with_context(|| format!("Failed to write {}", path.display()))?;
            eprintln!("[autotools] Updated {}", path.display());
        }
    } else {
        eprintln!("[autotools] No changes needed for {}", path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use tempfile::TempDir;

    #[test]
    fn test_discover_tools_sorts_and_filters() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create tool directories with Makefiles
        for name in &["thoughts_tool", "claudecode_rs", "anthropic_async"] {
            let dir = root.join(name);
            fs::create_dir(&dir).unwrap();
            File::create(dir.join("Makefile")).unwrap();
        }

        // Create directories that should be ignored
        fs::create_dir(root.join("xtask")).unwrap();
        File::create(root.join("xtask").join("Makefile")).unwrap();

        fs::create_dir(root.join("context")).unwrap();

        fs::create_dir(root.join(".hidden")).unwrap();
        File::create(root.join(".hidden").join("Makefile")).unwrap();

        // Create directory without Makefile (should be ignored)
        fs::create_dir(root.join("docs")).unwrap();

        let tools = discover_tools(root).unwrap();

        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].dir, "anthropic_async");
        assert_eq!(tools[1].dir, "claudecode_rs");
        assert_eq!(tools[2].dir, "thoughts_tool");
    }

    #[test]
    fn test_replace_tools_block() {
        let input = r#"# Some header
SHELL := /bin/bash

# Tools to build (autogenerated)
# BEGIN:autotools TOOLS
TOOLS := old_tool
# END:autotools TOOLS

# More content
"#;

        let replacement =
            "# BEGIN:autotools TOOLS\nTOOLS := alpha beta gamma\n# END:autotools TOOLS";
        let (output, changed) = replace_block(input, "TOOLS", replacement).unwrap();

        assert!(changed);
        assert!(output.contains("TOOLS := alpha beta gamma"));
        assert!(!output.contains("old_tool"));
        assert!(output.contains("# Some header"));
        assert!(output.contains("# More content"));
    }

    #[test]
    fn test_replace_tools_block_no_change() {
        let input = r#"# BEGIN:autotools TOOLS
TOOLS := alpha beta
# END:autotools TOOLS
"#;

        let replacement = "# BEGIN:autotools TOOLS\nTOOLS := alpha beta\n# END:autotools TOOLS";
        let (output, changed) = replace_block(input, "TOOLS", replacement).unwrap();

        assert!(!changed);
        assert_eq!(output, input);
    }

    #[test]
    fn test_replace_tools_block_missing_marker() {
        let input = "# No markers here\nTOOLS := something\n";
        let result = replace_block(input, "TOOLS", "replacement");

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_render_tools_block() {
        let tools = vec![
            Tool {
                dir: "alpha".to_string(),
            },
            Tool {
                dir: "beta".to_string(),
            },
        ];

        let block = render_tools_block(&tools);

        assert_eq!(
            block,
            "# BEGIN:autotools TOOLS\nTOOLS := alpha beta\n# END:autotools TOOLS"
        );
    }
}
