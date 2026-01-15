//! CLI capability detection module.
//!
//! Provides optional functionality to probe the Claude CLI for supported flags
//! and capabilities. This is useful for validating SDK compatibility with the
//! installed CLI version.

use crate::error::{ClaudeError, Result};
use std::collections::HashSet;
use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Represents the capabilities detected from the CLI.
#[derive(Debug, Clone, Default)]
pub struct CliCapabilities {
    /// Set of flags detected from `claude --help` output.
    pub flags: HashSet<String>,
}

impl CliCapabilities {
    /// Check if a specific flag is supported by the CLI.
    ///
    /// # Example
    /// ```ignore
    /// let caps = client.probe_cli().await?;
    /// if caps.supports("--permission-mode") {
    ///     // Use permission mode
    /// }
    /// ```
    pub fn supports(&self, flag: &str) -> bool {
        self.flags.contains(flag)
    }

    /// Check if all the given flags are supported.
    pub fn supports_all(&self, flags: &[&str]) -> bool {
        flags.iter().all(|f| self.supports(f))
    }

    /// Check if any of the given flags are supported.
    pub fn supports_any(&self, flags: &[&str]) -> bool {
        flags.iter().any(|f| self.supports(f))
    }
}

/// Probe the Claude CLI for supported flags by parsing `--help` output.
///
/// This function runs `claude --help` and extracts all flags (tokens starting
/// with `--`) from the output.
///
/// # Arguments
/// * `claude_path` - Path to the claude executable
///
/// # Returns
/// * `Ok(CliCapabilities)` - The detected capabilities
/// * `Err(ClaudeError::ProbeError)` - If the probe failed
///
/// # Example
/// ```ignore
/// use claudecode::probe::probe_cli;
/// use std::path::Path;
///
/// let caps = probe_cli(Path::new("/usr/local/bin/claude")).await?;
/// println!("Supports {} flags", caps.flags.len());
/// ```
pub async fn probe_cli(claude_path: &Path) -> Result<CliCapabilities> {
    let mut cmd = Command::new(claude_path);
    cmd.arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| ClaudeError::SpawnError {
        command: claude_path.display().to_string(),
        args: vec!["--help".into()],
        source: e,
    })?;

    let mut stdout_content = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        stdout.read_to_string(&mut stdout_content).await.ok();
    }

    let status = child.wait().await.map_err(|e| ClaudeError::ProbeError {
        message: format!("Failed to wait for --help: {}", e),
    })?;

    if !status.success() {
        return Err(ClaudeError::ProbeError {
            message: "claude --help exited with non-zero status".into(),
        });
    }

    let flags = parse_flags_from_help(&stdout_content);
    Ok(CliCapabilities { flags })
}

/// Parse flags from help output text.
fn parse_flags_from_help(help_text: &str) -> HashSet<String> {
    let mut flags = HashSet::new();

    for line in help_text.lines() {
        for token in line.split_whitespace() {
            if token.starts_with("--") {
                // Clean up the flag (remove trailing punctuation)
                let cleaned = token
                    .trim_end_matches([',', ';', ')', ']'])
                    .trim_start_matches('[');

                // Handle flags with = in them (e.g., --model=<model>)
                let flag = cleaned.split('=').next().unwrap_or(cleaned);

                if !flag.is_empty() && flag.starts_with("--") {
                    flags.insert(flag.to_string());
                }
            }
        }
    }

    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_flags_from_help() {
        let help_text = r#"
Usage: claude [options] [query]

Options:
  --help                   Show help
  --version                Show version
  --model <model>          Model to use
  --output-format <format> Output format (text, json, stream-json)
  --permission-mode <mode> Permission mode
  --dangerously-skip-permissions  Skip permission checks
  --allow-dangerously-skip-permissions  Allow skipping permissions
  --mcp-config <path>      MCP configuration file
  --add-dir <path>         Add directory to context
  --resume <id>            Resume session
  --continue               Continue last session
  --session-id <uuid>      Use specific session ID
  --fork-session           Fork existing session
  --tools <tools>          Tools to enable
  --allowedTools <tools>   Allowed tools
  --disallowedTools <tools> Disallowed tools
  --json-schema <schema>   JSON schema for output
  --include-partial-messages  Include partial messages
  --replay-user-messages   Replay user messages
  --settings <json>        Settings JSON
  --setting-sources <sources>  Setting sources
  --plugin-dir <path>      Plugin directory
  --ide                    IDE mode
  --agents <json>          Agents configuration
  --debug [filter]         Debug mode
  --verbose                Verbose output
        "#;

        let flags = parse_flags_from_help(help_text);

        assert!(flags.contains("--help"));
        assert!(flags.contains("--version"));
        assert!(flags.contains("--model"));
        assert!(flags.contains("--permission-mode"));
        assert!(flags.contains("--dangerously-skip-permissions"));
        assert!(flags.contains("--allow-dangerously-skip-permissions"));
        assert!(flags.contains("--mcp-config"));
        assert!(flags.contains("--add-dir"));
        assert!(flags.contains("--resume"));
        assert!(flags.contains("--continue"));
        assert!(flags.contains("--session-id"));
        assert!(flags.contains("--fork-session"));
        assert!(flags.contains("--tools"));
        assert!(flags.contains("--allowedTools"));
        assert!(flags.contains("--disallowedTools"));
        assert!(flags.contains("--json-schema"));
        assert!(flags.contains("--include-partial-messages"));
        assert!(flags.contains("--replay-user-messages"));
        assert!(flags.contains("--settings"));
        assert!(flags.contains("--plugin-dir"));
        assert!(flags.contains("--ide"));
        assert!(flags.contains("--agents"));
        assert!(flags.contains("--debug"));
        assert!(flags.contains("--verbose"));
    }

    #[test]
    fn test_cli_capabilities_supports() {
        let mut caps = CliCapabilities::default();
        caps.flags.insert("--help".to_string());
        caps.flags.insert("--model".to_string());
        caps.flags.insert("--verbose".to_string());

        assert!(caps.supports("--help"));
        assert!(caps.supports("--model"));
        assert!(!caps.supports("--nonexistent"));

        assert!(caps.supports_all(&["--help", "--model"]));
        assert!(!caps.supports_all(&["--help", "--nonexistent"]));

        assert!(caps.supports_any(&["--nonexistent", "--model"]));
        assert!(!caps.supports_any(&["--nonexistent", "--also-nonexistent"]));
    }
}
