use crate::error::ClaudeError;
use crate::error::Result;
use nix::errno::Errno;
use nix::sys::signal;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::Command;
use tokio::sync::Mutex;
use which::which;

pub(crate) const KILL_GRACE: Duration = Duration::from_millis(250);

pub struct ProcessHandle {
    child: Arc<Mutex<Child>>,
    stdout_reader: Option<BufReader<tokio::process::ChildStdout>>,
    stderr_reader: Option<BufReader<tokio::process::ChildStderr>>,
}

#[derive(Clone)]
pub struct KillHandle {
    pid: i32,
}

impl std::fmt::Debug for KillHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KillHandle")
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
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

        #[cfg(unix)]
        {
            // TODO(2): add Windows-equivalent process-tree handling.
            cmd.process_group(0);
        }

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
            child: Arc::new(Mutex::new(child)),
            stdout_reader: Some(BufReader::new(stdout)),
            stderr_reader: Some(BufReader::new(stderr)),
        })
    }

    pub async fn wait(self) -> Result<std::process::ExitStatus> {
        let mut child = self.child.lock().await;
        Ok(child.wait().await?)
    }

    pub async fn kill(&mut self) -> Result<()> {
        let mut child = self.child.lock().await;
        child.kill().await?;
        Ok(())
    }

    pub fn id(&self) -> Option<u32> {
        self.child.try_lock().ok().and_then(|child| child.id())
    }

    pub fn try_wait(&mut self) -> Result<Option<std::process::ExitStatus>> {
        let mut child = self
            .child
            .try_lock()
            .map_err(|_| ClaudeError::SessionError {
                message: "Process wait already in progress".to_string(),
            })?;
        Ok(child.try_wait()?)
    }

    pub fn kill_handle(&self) -> Result<KillHandle> {
        let pid = self.id().ok_or_else(|| ClaudeError::SessionError {
            message: "Process not found or already terminated".to_string(),
        })?;

        Ok(KillHandle { pid: pid as i32 })
    }

    pub(crate) fn take_stdout(&mut self) -> Option<BufReader<tokio::process::ChildStdout>> {
        self.stdout_reader.take()
    }

    pub(crate) fn take_stderr(&mut self) -> Option<BufReader<tokio::process::ChildStderr>> {
        self.stderr_reader.take()
    }
}

impl KillHandle {
    pub fn signal(&self, sig: Signal) -> nix::Result<()> {
        signal_process_group(self.pid, sig)
    }

    pub async fn graceful_terminate(&self) -> Result<()> {
        tracing::info!(pid = self.pid, "terminating Claude process group");
        self.signal(Signal::SIGTERM).map_err(nix_to_claude_error)?;

        tokio::time::sleep(KILL_GRACE).await;

        self.kill_now().map_err(nix_to_claude_error)?;

        Ok(())
    }

    pub fn kill_now(&self) -> nix::Result<()> {
        tracing::info!(pid = self.pid, "force killing Claude process group");
        self.signal(Signal::SIGKILL)
    }
}

fn nix_to_claude_error(err: Errno) -> ClaudeError {
    ClaudeError::SessionError {
        message: format!("Process-group signal failed: {err}"),
    }
}

fn signal_process_group(pid: i32, sig: Signal) -> nix::Result<()> {
    match signal::killpg(Pid::from_raw(pid), sig) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(err) => Err(err),
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
