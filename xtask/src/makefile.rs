//! Makefile synchronization for autotools markers.
//!
//! This module discovers tool projects by scanning top-level subdirectories
//! for Makefile presence and generates Makefile sections via marker-based updates.

use anyhow::{bail, Context, Result};
use cargo_metadata::MetadataCommand;
use regex::Regex;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Represents a discovered tool project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    /// Directory name of the tool.
    pub dir: String,
}

/// Configuration loaded from `[workspace.metadata.autotools]` in Cargo.toml.
#[derive(Debug, Default, Deserialize)]
pub struct AutoToolsConfig {
    /// Custom alias overrides (tool_dir -> alias).
    #[serde(default)]
    pub alias: BTreeMap<String, String>,
    /// Directories to ignore during discovery.
    #[serde(default)]
    pub ignore: Vec<String>,
}

/// Returns the default alias mappings for known tools.
fn default_aliases() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("thoughts_tool".into(), "thoughts".into()),
        ("claudecode_rs".into(), "claude".into()),
        ("universal_tool".into(), "universal".into()),
        ("pr_comments".into(), "pr".into()),
        ("gpt5_reasoner".into(), "gpt5".into()),
        ("anthropic_async".into(), "anthropic".into()),
        ("coding_agent_tools".into(), "coding".into()),
    ])
}

/// Loads autotools configuration from workspace metadata.
///
/// Returns default config if `[workspace.metadata.autotools]` is not present.
fn load_autotools_config() -> Result<AutoToolsConfig> {
    let meta = MetadataCommand::new()
        .no_deps()
        .exec()
        .context("Failed to run `cargo metadata`")?;

    if let Some(v) = meta.workspace_metadata.get("autotools") {
        serde_json::from_value(v.clone()).context("Failed to parse [workspace.metadata.autotools]")
    } else {
        Ok(AutoToolsConfig::default())
    }
}

/// Merges default aliases with config overrides.
fn get_aliases(config: &AutoToolsConfig) -> BTreeMap<String, String> {
    let mut aliases = default_aliases();
    // Config overrides take precedence
    for (k, v) in &config.alias {
        aliases.insert(k.clone(), v.clone());
    }
    aliases
}

/// Discovers tool projects by scanning for top-level directories containing a Makefile.
///
/// Excludes known non-tool directories like `xtask`, `context`, `target`, and hidden directories.
/// Additional directories can be ignored via config.
/// Results are sorted alphabetically for deterministic output.
pub fn discover_tools(root: &Path, config: &AutoToolsConfig) -> Result<Vec<Tool>> {
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

        // Skip directories in config ignore list
        if config.ignore.iter().any(|ig| ig == &name) {
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

/// Renders the ALIAS_CASE marker block content.
///
/// Generates a single-line shell case statement that maps tool directories to their aliases.
/// Uses `:=` assignment instead of `define` to avoid multi-line expansion issues in recipes.
fn render_alias_case_block(tools: &[Tool], aliases: &BTreeMap<String, String>) -> String {
    let mut cases = Vec::new();
    for t in tools {
        let alias = aliases
            .get(&t.dir)
            .cloned()
            .unwrap_or_else(|| t.dir.clone());
        cases.push(format!("{}) alias=\"{}\" ;;", t.dir, alias));
    }
    // Add catch-all case
    cases.push("*) alias=\"\" ;;".to_string());

    format!(
        "# BEGIN:autotools ALIAS_CASE\n\
         ALIAS_CASE := case $$tool in {} esac\n\
         # END:autotools ALIAS_CASE",
        cases.join(" ")
    )
}

/// Renders the TARGETS marker block content.
///
/// Generates individual alias targets for each tool (e.g., coding-check, coding-test, etc.).
fn render_targets_block(tools: &[Tool], aliases: &BTreeMap<String, String>) -> String {
    let mut out = String::from("# BEGIN:autotools TARGETS\n");

    for t in tools {
        let alias = aliases
            .get(&t.dir)
            .cloned()
            .unwrap_or_else(|| t.dir.clone());

        out.push_str(&format!(
            "# Individual tool targets - {}\n\
             {}-check:\n\
             \t@$(MAKE) -C {} check\n\n\
             {}-test:\n\
             \t@$(MAKE) -C {} test\n\n\
             {}-build:\n\
             \t@$(MAKE) -C {} build\n\n\
             {}-all:\n\
             \t@$(MAKE) -C {} all\n\n",
            t.dir, alias, t.dir, alias, t.dir, alias, t.dir, alias, t.dir
        ));
    }

    out.push_str("# END:autotools TARGETS");
    out
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

/// Tries to replace a marker block, returning Ok(None) if the marker doesn't exist.
fn try_replace_block(input: &str, name: &str, replacement: &str) -> Result<Option<(String, bool)>> {
    match replace_block(input, name, replacement) {
        Ok(result) => Ok(Some(result)),
        Err(e) if e.to_string().contains("not found") => Ok(None),
        Err(e) => Err(e),
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

    // Load config from workspace metadata
    let config = load_autotools_config()?;
    let aliases = get_aliases(&config);

    let tools = discover_tools(root, &config)?;
    let input =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;

    // Track total changes
    let mut output = input.clone();
    let mut any_changed = false;

    // Replace TOOLS block (required)
    let tools_block = render_tools_block(&tools);
    let (new_output, changed) = replace_block(&output, "TOOLS", &tools_block)?;
    output = new_output;
    any_changed |= changed;

    // Replace ALIAS_CASE block (optional)
    let alias_case_block = render_alias_case_block(&tools, &aliases);
    if let Some((new_output, changed)) =
        try_replace_block(&output, "ALIAS_CASE", &alias_case_block)?
    {
        output = new_output;
        any_changed |= changed;
    }

    // Replace TARGETS block (optional)
    let targets_block = render_targets_block(&tools, &aliases);
    if let Some((new_output, changed)) = try_replace_block(&output, "TARGETS", &targets_block)? {
        output = new_output;
        any_changed |= changed;
    }

    if any_changed {
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

    fn empty_config() -> AutoToolsConfig {
        AutoToolsConfig::default()
    }

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

        let tools = discover_tools(root, &empty_config()).unwrap();

        assert_eq!(tools.len(), 3);
        assert_eq!(tools[0].dir, "anthropic_async");
        assert_eq!(tools[1].dir, "claudecode_rs");
        assert_eq!(tools[2].dir, "thoughts_tool");
    }

    #[test]
    fn test_discover_tools_respects_config_ignore() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Create tool directories with Makefiles
        for name in &["alpha", "beta", "gamma"] {
            let dir = root.join(name);
            fs::create_dir(&dir).unwrap();
            File::create(dir.join("Makefile")).unwrap();
        }

        // Config ignores "beta"
        let config = AutoToolsConfig {
            ignore: vec!["beta".to_string()],
            ..Default::default()
        };

        let tools = discover_tools(root, &config).unwrap();

        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].dir, "alpha");
        assert_eq!(tools[1].dir, "gamma");
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

    #[test]
    fn test_render_alias_case_block() {
        let tools = vec![
            Tool {
                dir: "thoughts_tool".to_string(),
            },
            Tool {
                dir: "unknown_tool".to_string(),
            },
        ];
        let aliases = BTreeMap::from([("thoughts_tool".to_string(), "thoughts".to_string())]);

        let block = render_alias_case_block(&tools, &aliases);

        assert!(block.starts_with("# BEGIN:autotools ALIAS_CASE"));
        assert!(block.ends_with("# END:autotools ALIAS_CASE"));
        assert!(block.contains("ALIAS_CASE :="));
        assert!(block.contains("thoughts_tool) alias=\"thoughts\" ;;"));
        // Unknown tool should use its own name as alias
        assert!(block.contains("unknown_tool) alias=\"unknown_tool\" ;;"));
        assert!(block.contains("*) alias=\"\" ;;"));
        // Should be single-line (no define/endef)
        assert!(!block.contains("define"));
        assert!(!block.contains("endef"));
    }

    #[test]
    fn test_render_targets_block() {
        let tools = vec![Tool {
            dir: "thoughts_tool".to_string(),
        }];
        let aliases = BTreeMap::from([("thoughts_tool".to_string(), "thoughts".to_string())]);

        let block = render_targets_block(&tools, &aliases);

        assert!(block.starts_with("# BEGIN:autotools TARGETS"));
        assert!(block.ends_with("# END:autotools TARGETS"));
        assert!(block.contains("thoughts-check:"));
        assert!(block.contains("thoughts-test:"));
        assert!(block.contains("thoughts-build:"));
        assert!(block.contains("thoughts-all:"));
        assert!(block.contains("@$(MAKE) -C thoughts_tool check"));
    }

    #[test]
    fn test_default_aliases() {
        let aliases = default_aliases();

        assert_eq!(aliases.get("thoughts_tool"), Some(&"thoughts".to_string()));
        assert_eq!(aliases.get("claudecode_rs"), Some(&"claude".to_string()));
        assert_eq!(
            aliases.get("coding_agent_tools"),
            Some(&"coding".to_string())
        );
    }

    #[test]
    fn test_get_aliases_merges_config() {
        let config = AutoToolsConfig {
            alias: BTreeMap::from([
                ("thoughts_tool".to_string(), "custom_thoughts".to_string()),
                ("new_tool".to_string(), "new".to_string()),
            ]),
            ..Default::default()
        };

        let aliases = get_aliases(&config);

        // Config override takes precedence
        assert_eq!(
            aliases.get("thoughts_tool"),
            Some(&"custom_thoughts".to_string())
        );
        // Default alias preserved
        assert_eq!(aliases.get("claudecode_rs"), Some(&"claude".to_string()));
        // New alias from config
        assert_eq!(aliases.get("new_tool"), Some(&"new".to_string()));
    }

    #[test]
    fn test_try_replace_block_missing_returns_none() {
        let input = "# No markers here\n";
        let result = try_replace_block(input, "MISSING", "replacement").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_try_replace_block_exists_returns_some() {
        let input = "# BEGIN:autotools TEST\nold\n# END:autotools TEST\n";
        let result = try_replace_block(
            input,
            "TEST",
            "# BEGIN:autotools TEST\nnew\n# END:autotools TEST",
        )
        .unwrap();
        assert!(result.is_some());
        let (output, changed) = result.unwrap();
        assert!(changed);
        assert!(output.contains("new"));
    }
}
