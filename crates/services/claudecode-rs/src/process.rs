use crate::error::{ClaudeError, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::BufReader;
use tokio::process::{Child, Command};
use which::which;

pub struct ProcessHandle {
    child: Child,
    stdout_reader: Option<BufReader<tokio::process::ChildStdout>>,
    stderr_reader: Option<BufReader<tokio::process::ChildStderr>>,
}

impl ProcessHandle {
    /// Spawn a new Claude process with optional environment overlay
    ///
    /// # Arguments
    /// * `claude_path` - Path to the claude executable
    /// * `args` - Command line arguments
    /// * `working_dir` - Optional working directory
    /// * `env_overlay` - Optional environment variables to add/override
    pub async fn spawn(
        claude_path: &Path,
        args: Vec<String>,
        working_dir: Option<&Path>,
        env_overlay: Option<&HashMap<String, String>>,
    ) -> Result<Self> {
        let mut cmd = Command::new(claude_path);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // Set working directory if specified
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        // Apply environment overlay: inherit current env and add/override with overlay
        if let Some(env_map) = env_overlay {
            // Inherit all current environment variables
            cmd.envs(std::env::vars());
            // Override/add from the overlay
            for (k, v) in env_map {
                cmd.env(k, v);
            }
        }

        let mut child = cmd.spawn().map_err(|e| ClaudeError::SpawnError {
            command: claude_path.display().to_string(),
            args,
            source: e,
        })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "Failed to capture stdout".to_string(),
            })?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "Failed to capture stderr".to_string(),
            })?;

        Ok(ProcessHandle {
            child,
            stdout_reader: Some(BufReader::new(stdout)),
            stderr_reader: Some(BufReader::new(stderr)),
        })
    }

    pub async fn wait(mut self) -> Result<std::process::ExitStatus> {
        Ok(self.child.wait().await?)
    }

    pub async fn kill(&mut self) -> Result<()> {
        self.child.kill().await?;
        Ok(())
    }

    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        Ok(self.child.try_wait()?)
    }

    pub(crate) fn take_stdout(&mut self) -> Option<BufReader<tokio::process::ChildStdout>> {
        self.stdout_reader.take()
    }

    pub(crate) fn take_stderr(&mut self) -> Option<BufReader<tokio::process::ChildStderr>> {
        self.stderr_reader.take()
    }
}

/// Find the Claude executable.
///
/// First checks the CLAUDE_PATH environment variable. If set, uses that path
/// (with tilde expansion). Otherwise, searches PATH for 'claude'.
pub async fn find_claude_in_path() -> Result<PathBuf> {
    // Check CLAUDE_PATH environment variable first
    if let Ok(path_str) = std::env::var("CLAUDE_PATH") {
        let path = expand_tilde(&path_str);
        if path.exists() {
            return Ok(path);
        } else {
            return Err(ClaudeError::ClaudeNotFoundAtPath { path });
        }
    }

    // Fall back to searching PATH
    tokio::task::spawn_blocking(|| which("claude").map_err(|_| ClaudeError::ClaudeNotFound))
        .await
        .map_err(|_| ClaudeError::SessionError {
            message: "Failed to spawn blocking task".to_string(),
        })?
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(stripped);
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_expand_tilde() {
        // Test tilde expansion
        let expanded = expand_tilde("~/test");
        assert!(!expanded.to_string_lossy().starts_with("~"));

        // Test path without tilde
        let path = "/absolute/path";
        let expanded = expand_tilde(path);
        assert_eq!(expanded.to_string_lossy(), path);

        // Test relative path
        let path = "relative/path";
        let expanded = expand_tilde(path);
        assert_eq!(expanded.to_string_lossy(), path);
    }

    #[test]
    fn test_expand_tilde_with_home() {
        // Test that tilde expands to home directory
        if let Some(home) = dirs::home_dir() {
            let expanded = expand_tilde("~/some/path");
            assert_eq!(expanded, home.join("some/path"));
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_find_claude_uses_claude_path_env() {
        // Create a temp file to use as fake claude binary
        let temp_dir = std::env::temp_dir();
        let fake_claude = temp_dir.join("fake_claude_for_test");
        std::fs::write(&fake_claude, "#!/bin/sh\necho test").unwrap();

        // Set CLAUDE_PATH (unsafe because env vars are process-global)
        // SAFETY: This is a test that runs in isolation
        unsafe {
            std::env::set_var("CLAUDE_PATH", fake_claude.to_str().unwrap());
        }

        let result = find_claude_in_path().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), fake_claude);

        // Cleanup
        // SAFETY: This is a test that runs in isolation
        unsafe {
            std::env::remove_var("CLAUDE_PATH");
        }
        std::fs::remove_file(&fake_claude).ok();
    }

    #[tokio::test]
    #[serial]
    async fn test_find_claude_claude_path_not_exists() {
        // Set CLAUDE_PATH to non-existent path
        // SAFETY: This is a test that runs in isolation
        unsafe {
            std::env::set_var("CLAUDE_PATH", "/nonexistent/path/to/claude");
        }

        let result = find_claude_in_path().await;
        assert!(result.is_err());

        match result.unwrap_err() {
            ClaudeError::ClaudeNotFoundAtPath { path } => {
                assert_eq!(path, PathBuf::from("/nonexistent/path/to/claude"));
            }
            _ => panic!("Expected ClaudeNotFoundAtPath error"),
        }

        // Cleanup
        // SAFETY: This is a test that runs in isolation
        unsafe {
            std::env::remove_var("CLAUDE_PATH");
        }
    }
}
