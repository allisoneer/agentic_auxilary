use crate::error::{ClaudeError, Result};
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
    pub async fn spawn(
        claude_path: &Path,
        args: Vec<String>,
        working_dir: Option<&Path>,
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

pub async fn find_claude_in_path() -> Result<PathBuf> {
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
}
