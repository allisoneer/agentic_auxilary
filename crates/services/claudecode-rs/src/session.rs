use crate::config::SessionConfig;
use crate::error::ClaudeError;
use crate::error::Result;
use crate::process::ProcessControl;
use crate::process::ProcessHandle;
use crate::stream::JsonStreamParser;
use crate::stream::SingleJsonParser;
use crate::stream::TextParser;
use crate::types::Event;
use crate::types::OutputFormat;
use crate::types::Result as ClaudeResult;
use chrono::Utc;
use futures::StreamExt;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::io::AsyncBufReadExt;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::debug;
use tracing::warn;
use uuid::Uuid;

pub struct Session {
    id: String,
    config: SessionConfig,
    start_time: chrono::DateTime<Utc>,

    // Process handle
    process: Arc<Mutex<Option<ProcessHandle>>>,
    process_control: ProcessControl,

    // Event channel for streaming
    events_tx: Option<mpsc::UnboundedSender<Event>>,
    events: Option<mpsc::UnboundedReceiver<Event>>,

    // Background tasks
    tasks: Vec<JoinHandle<()>>,

    // Result storage
    result: Arc<RwLock<Option<ClaudeResult>>>,
    error: Arc<RwLock<Option<ClaudeError>>>,

    // Temp file for MCP config (must be kept alive)
    _mcp_temp_file: Option<NamedTempFile>,
}

impl Session {
    pub async fn new(config: SessionConfig, process: ProcessHandle) -> Result<Self> {
        // Determine session ID from explicit_session_id, resume_session_id, or generate new
        let id = if let Some(ref id) = config.explicit_session_id {
            id.clone()
        } else if let Some(ref id) = config.resume_session_id {
            id.clone()
        } else {
            Uuid::new_v4().to_string()
        };

        let (events_tx, events) = match config.output_format {
            OutputFormat::StreamingJson => {
                let (tx, rx) = mpsc::unbounded_channel();
                (Some(tx), Some(rx))
            }
            _ => (None, None),
        };

        let process_control = process.control();
        let process = Arc::new(Mutex::new(Some(process)));
        let result = Arc::new(RwLock::new(None));
        let error = Arc::new(RwLock::new(None));

        let mut session = Self {
            id,
            config: config.clone(),
            start_time: Utc::now(),
            process: process.clone(),
            process_control,
            events_tx,
            events,
            tasks: Vec::new(),
            result: result.clone(),
            error: error.clone(),
            _mcp_temp_file: None,
        };

        // Start background tasks based on output format
        session.start_tasks().await?;

        Ok(session)
    }

    async fn start_tasks(&mut self) -> Result<()> {
        let process = self.process.clone();
        let result = self.result.clone();
        let error = self.error.clone();

        match self.config.output_format {
            OutputFormat::StreamingJson => {
                let events_tx = self
                    .events_tx
                    .take()
                    .expect("events_tx must exist for StreamingJson output format");
                let result_clone = result.clone();
                let task = tokio::spawn(async move {
                    if let Err(e) =
                        Self::handle_streaming_json(process, events_tx, result_clone, error.clone())
                            .await
                    {
                        error.write().await.replace(e);
                    }
                });
                self.tasks.push(task);
            }
            OutputFormat::Json => {
                let task = tokio::spawn(async move {
                    match Self::handle_json(process, error.clone()).await {
                        Ok(r) => {
                            result.write().await.replace(r);
                        }
                        Err(e) => {
                            error.write().await.replace(e);
                        }
                    }
                });
                self.tasks.push(task);
            }
            OutputFormat::Text => {
                let task = tokio::spawn(async move {
                    match Self::handle_text(process, error.clone()).await {
                        Ok(r) => {
                            result.write().await.replace(r);
                        }
                        Err(e) => {
                            error.write().await.replace(e);
                        }
                    }
                });
                self.tasks.push(task);
            }
        }

        Ok(())
    }

    async fn handle_streaming_json(
        process: Arc<Mutex<Option<ProcessHandle>>>,
        events_tx: mpsc::UnboundedSender<Event>,
        result_arc: Arc<RwLock<Option<ClaudeResult>>>,
        error: Arc<RwLock<Option<ClaudeError>>>,
    ) -> Result<()> {
        let mut process = {
            let mut process_guard = process.lock().await;
            process_guard
                .take()
                .ok_or_else(|| ClaudeError::SessionError {
                    message: "Process already taken".to_string(),
                })?
        };

        let stdout = process
            .take_stdout()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "No stdout reader".to_string(),
            })?;

        let stderr = process
            .take_stderr()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "No stderr reader".to_string(),
            })?;

        // Handle stderr in background
        let error_clone = error.clone();
        tokio::spawn(async move {
            let mut stderr_content = String::new();
            let mut lines = stderr.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                stderr_content.push_str(&line);
                stderr_content.push('\n');
            }
            if !stderr_content.trim().is_empty() {
                error_clone
                    .write()
                    .await
                    .replace(ClaudeError::ProcessFailed {
                        code: -1,
                        stderr: stderr_content,
                    });
            }
        });

        // Parse streaming JSON from stdout
        let parser = JsonStreamParser::new(stdout);
        let stream = parser.into_event_stream();
        tokio::pin!(stream);

        while let Some(result) = stream.next().await {
            match result {
                Ok(event) => {
                    // Check if this is a result event and store it
                    if let Event::Result(ref result_event) = event {
                        let claude_result = ClaudeResult {
                            result_type: Some("result".to_string()),
                            subtype: None,
                            session_id: Some(result_event.session_id.clone()),
                            result: result_event.result.clone(),
                            content: result_event.result.clone(), // For compatibility
                            is_error: result_event.is_error,
                            error: result_event.error.clone(),
                            total_cost_usd: result_event.total_cost_usd,
                            duration_ms: result_event.duration_ms,
                            duration_api_ms: result_event.duration_api_ms,
                            num_turns: result_event.num_turns,
                            exit_code: None,
                            usage: result_event.usage.clone(),
                        };
                        result_arc.write().await.replace(claude_result);
                    }

                    // Send event
                    if events_tx.send(event).is_err() {
                        debug!("Event receiver dropped, stopping stream");
                        break;
                    }
                }
                Err(e) => {
                    warn!("Failed to parse JSON event: {}", e);
                    // Continue on parse errors
                }
            }
        }

        // Explicitly drop the sender to signal end of stream
        drop(events_tx);

        // Wait for process to complete
        let status = process.wait().await?;
        if !status.success() {
            let code = status.code().unwrap_or(-1);
            if error.read().await.is_none() {
                error.write().await.replace(ClaudeError::ProcessFailed {
                    code,
                    stderr: "Process exited with non-zero status".to_string(),
                });
            }
        }

        Ok(())
    }

    async fn handle_json(
        process: Arc<Mutex<Option<ProcessHandle>>>,
        _error: Arc<RwLock<Option<ClaudeError>>>,
    ) -> Result<ClaudeResult> {
        let mut process = {
            let mut process_guard = process.lock().await;
            process_guard
                .take()
                .ok_or_else(|| ClaudeError::SessionError {
                    message: "Process already taken".to_string(),
                })?
        };

        let stdout = process
            .take_stdout()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "No stdout reader".to_string(),
            })?;

        let stderr = process
            .take_stderr()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "No stderr reader".to_string(),
            })?;

        let parser = SingleJsonParser::new(stdout, stderr);
        let result = parser.parse().await?;

        // Wait for process
        let status = process.wait().await?;
        if !status.success() && !result.is_error {
            return Err(ClaudeError::ProcessFailed {
                code: status.code().unwrap_or(-1),
                stderr: result.error.unwrap_or_default(),
            });
        }

        Ok(result)
    }

    async fn handle_text(
        process: Arc<Mutex<Option<ProcessHandle>>>,
        _error: Arc<RwLock<Option<ClaudeError>>>,
    ) -> Result<ClaudeResult> {
        let mut process = {
            let mut process_guard = process.lock().await;
            process_guard
                .take()
                .ok_or_else(|| ClaudeError::SessionError {
                    message: "Process already taken".to_string(),
                })?
        };

        let stdout = process
            .take_stdout()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "No stdout reader".to_string(),
            })?;

        let stderr = process
            .take_stderr()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "No stderr reader".to_string(),
            })?;

        let parser = TextParser::new(stdout, stderr);
        let result = parser.parse().await?;

        // Wait for process
        let status = process.wait().await?;
        if !status.success() && !result.is_error {
            return Err(ClaudeError::ProcessFailed {
                code: status.code().unwrap_or(-1),
                stderr: result.error.unwrap_or_default(),
            });
        }

        Ok(result)
    }

    /// Wait for the session to complete and return the result
    pub async fn wait(mut self) -> Result<ClaudeResult> {
        self.wait_for_tasks().await;

        // Check for errors first - preserve original error variant (e.g., ProcessFailed{stderr})
        if let Some(error) = self.error.write().await.take() {
            return Err(error);
        }

        // Return result
        self.result
            .read()
            .await
            .clone()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "No result available".to_string(),
            })
    }

    async fn wait_for_tasks(&mut self) {
        while !self.tasks.is_empty() {
            let _ = (&mut self.tasks[0]).await;
            self.tasks.remove(0);
        }
    }

    /// Kill the Claude process
    pub async fn kill(&mut self) -> Result<()> {
        if let Some(mut process) = self.process.lock().await.take() {
            process.kill().await?;
        } else {
            self.process_control.terminate_with_grace().await?;
        }
        self.wait_for_tasks().await;
        Ok(())
    }

    /// Send interrupt signal to the Claude process
    ///
    /// On Unix systems, this sends SIGINT which allows graceful shutdown.
    pub async fn interrupt(&mut self) -> Result<()> {
        if self.process_control.id().is_some() {
            return self
                .process_control
                .interrupt()
                .map_err(|error| ClaudeError::SessionError {
                    message: format!("Failed to send interrupt signal: {error}"),
                });
        }
        Err(ClaudeError::SessionError {
            message: "Process not found or already terminated".to_string(),
        })
    }

    /// Get the session ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the start time
    pub fn start_time(&self) -> chrono::DateTime<Utc> {
        self.start_time
    }

    /// Check if session is still running
    pub async fn is_running(&self) -> bool {
        !self.process_control.is_exited()
    }

    /// Take the event stream receiver
    pub fn take_event_stream(&mut self) -> Option<mpsc::UnboundedReceiver<Event>> {
        self.events.take()
    }

    /// Set the MCP temp file to keep it alive for the session duration
    pub fn set_mcp_temp_file(&mut self, temp_file: NamedTempFile) {
        self._mcp_temp_file = Some(temp_file);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Err(error) = self.process_control.start_kill() {
            warn!("failed to signal Claude process group on session drop: {error}");
        }

        // Ensure all tasks are aborted on drop
        for task in &self.tasks {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::Client;
    use crate::config::SessionConfig;
    use crate::error::ClaudeError;
    use crate::types::OutputFormat;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::path::PathBuf;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::time::Instant;
    use tokio::time::sleep;

    const FAKE_CLAUDE_PID_FILE: &str = "FAKE_CLAUDE_PID_FILE";
    const FAKE_CLAUDE_PGID_FILE: &str = "FAKE_CLAUDE_PGID_FILE";
    const FAKE_CLAUDE_CHILD_PID_FILE: &str = "FAKE_CLAUDE_CHILD_PID_FILE";
    const FAKE_CLAUDE_CHILD_PGID_FILE: &str = "FAKE_CLAUDE_CHILD_PGID_FILE";
    const FAKE_CLAUDE_READY_FILE: &str = "FAKE_CLAUDE_READY_FILE";

    struct FakeClaude {
        _temp_dir: TempDir,
        script_path: PathBuf,
        pid_file: PathBuf,
        pgid_file: PathBuf,
        child_pid_file: PathBuf,
        child_pgid_file: PathBuf,
        ready_file: PathBuf,
    }

    struct FakeClaudeTree {
        pid: i32,
        pgid: i32,
        child_pid: i32,
        child_pgid: i32,
    }

    impl FakeClaude {
        fn new() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            let script_path = temp_dir.path().join("fake-claude.sh");
            let pid_file = temp_dir.path().join("claude.pid");
            let pgid_file = temp_dir.path().join("claude.pgid");
            let child_pid_file = temp_dir.path().join("child.pid");
            let child_pgid_file = temp_dir.path().join("child.pgid");
            let ready_file = temp_dir.path().join("ready");

            std::fs::write(
                &script_path,
                r#"#!/bin/sh
set -eu

trim_pgid() {
    ps -o pgid= -p "$1" | tr -d ' '
}

printf '%s\n' "$$" > "$FAKE_CLAUDE_PID_FILE"
printf '%s\n' "$(trim_pgid "$$")" > "$FAKE_CLAUDE_PGID_FILE"

sh -c 'trap "" TERM INT; sleep 300' &
child="$!"
printf '%s\n' "$child" > "$FAKE_CLAUDE_CHILD_PID_FILE"
printf '%s\n' "$(trim_pgid "$child")" > "$FAKE_CLAUDE_CHILD_PGID_FILE"
printf 'ready\n' > "$FAKE_CLAUDE_READY_FILE"

sleep 300
"#,
            )
            .unwrap();

            let mut permissions = std::fs::metadata(&script_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&script_path, permissions).unwrap();

            Self {
                _temp_dir: temp_dir,
                script_path,
                pid_file,
                pgid_file,
                child_pid_file,
                child_pgid_file,
                ready_file,
            }
        }

        fn config(&self) -> SessionConfig {
            SessionConfig::builder("test query".to_string())
                .output_format(OutputFormat::Text)
                .env_var(FAKE_CLAUDE_PID_FILE, self.pid_file.to_string_lossy())
                .env_var(FAKE_CLAUDE_PGID_FILE, self.pgid_file.to_string_lossy())
                .env_var(
                    FAKE_CLAUDE_CHILD_PID_FILE,
                    self.child_pid_file.to_string_lossy(),
                )
                .env_var(
                    FAKE_CLAUDE_CHILD_PGID_FILE,
                    self.child_pgid_file.to_string_lossy(),
                )
                .env_var(FAKE_CLAUDE_READY_FILE, self.ready_file.to_string_lossy())
                .build()
                .unwrap()
        }

        async fn wait_for_tree(&self) -> FakeClaudeTree {
            wait_for_file(&self.ready_file).await;
            FakeClaudeTree {
                pid: read_i32_file(&self.pid_file).await,
                pgid: read_i32_file(&self.pgid_file).await,
                child_pid: read_i32_file(&self.child_pid_file).await,
                child_pgid: read_i32_file(&self.child_pgid_file).await,
            }
        }
    }

    async fn wait_for_file(path: &Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if tokio::fs::try_exists(path).await.unwrap_or(false) {
                return;
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for {}", path.display());
    }

    async fn read_i32_file(path: &Path) -> i32 {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(content) = tokio::fs::read_to_string(path).await
                && let Ok(value) = content.trim().parse()
            {
                return value;
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out reading integer from {}", path.display());
    }

    async fn wait_for_process_exit(pid: i32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if !process_exists(pid) {
                return true;
            }
            sleep(Duration::from_millis(10)).await;
        }
        false
    }

    async fn wait_for_process_handle_taken(session: &Session) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if session.process.lock().await.is_none() {
                return;
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out waiting for output worker to take process handle");
    }

    fn process_exists(pid: i32) -> bool {
        let result = unsafe { libc::kill(pid, 0) };
        if result == 0 {
            return true;
        }

        std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }

    fn current_process_group_id() -> i32 {
        unsafe { libc::getpgrp() }
    }

    fn cleanup_process_group(pgid: i32) {
        if pgid > 1 {
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
            }
        }
    }

    #[tokio::test]
    async fn aborting_launch_and_wait_terminates_fake_claude_tree() {
        let fake = FakeClaude::new();
        let client = Client::with_path(&fake.script_path).await.unwrap();
        let config = fake.config();

        let handle = tokio::spawn(async move { client.launch_and_wait(config).await });
        let tree = fake.wait_for_tree().await;

        assert_ne!(tree.pgid, current_process_group_id());
        assert_eq!(tree.child_pgid, tree.pgid);

        handle.abort();
        let _ = handle.await;

        let parent_exited = wait_for_process_exit(tree.pid).await;
        let child_exited = wait_for_process_exit(tree.child_pid).await;
        cleanup_process_group(tree.pgid);

        assert!(parent_exited, "fake Claude process was not terminated");
        assert!(child_exited, "fake Claude child process was not terminated");
    }

    #[tokio::test]
    async fn kill_terminates_fake_claude_tree_after_worker_takes_process() {
        let fake = FakeClaude::new();
        let client = Client::with_path(&fake.script_path).await.unwrap();
        let config = fake.config();

        let mut session = client.launch(config).await.unwrap();
        let tree = fake.wait_for_tree().await;
        wait_for_process_handle_taken(&session).await;

        assert_ne!(tree.pgid, current_process_group_id());
        assert_eq!(tree.child_pgid, tree.pgid);

        let kill_result = session.kill().await;
        let parent_exited = wait_for_process_exit(tree.pid).await;
        let child_exited = wait_for_process_exit(tree.child_pid).await;
        cleanup_process_group(tree.pgid);

        kill_result.unwrap();
        assert!(!session.is_running().await);
        assert!(parent_exited, "fake Claude process was not terminated");
        assert!(child_exited, "fake Claude child process was not terminated");
    }

    #[tokio::test]
    async fn wait_returns_processfailed_preserving_stderr() {
        let cfg = SessionConfig::builder("test".to_string())
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let session = Session {
            id: "test".into(),
            config: cfg,
            start_time: Utc::now(),
            process: Arc::new(Mutex::new(None)),
            process_control: ProcessControl::empty(),
            events_tx: None,
            events: None,
            tasks: vec![],
            result: Arc::new(RwLock::new(None)),
            error: Arc::new(RwLock::new(Some(ClaudeError::ProcessFailed {
                code: 1,
                stderr: "stderr details".into(),
            }))),
            _mcp_temp_file: None,
        };

        let err = session.wait().await.unwrap_err();
        match err {
            ClaudeError::ProcessFailed { code, stderr } => {
                assert_eq!(code, 1);
                assert!(stderr.contains("stderr details"));
            }
            other => panic!("expected ProcessFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_returns_sessionerror_preserving_message() {
        let cfg = SessionConfig::builder("test".to_string())
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let session = Session {
            id: "test".into(),
            config: cfg,
            start_time: Utc::now(),
            process: Arc::new(Mutex::new(None)),
            process_control: ProcessControl::empty(),
            events_tx: None,
            events: None,
            tasks: vec![],
            result: Arc::new(RwLock::new(None)),
            error: Arc::new(RwLock::new(Some(ClaudeError::SessionError {
                message: "custom session error".into(),
            }))),
            _mcp_temp_file: None,
        };

        let err = session.wait().await.unwrap_err();
        match err {
            ClaudeError::SessionError { message } => assert_eq!(message, "custom session error"),
            other => panic!("expected SessionError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_returns_ioerror_preserving_source() {
        let cfg = SessionConfig::builder("test".to_string())
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let io = std::io::Error::other("disk full");

        let session = Session {
            id: "test".into(),
            config: cfg,
            start_time: Utc::now(),
            process: Arc::new(Mutex::new(None)),
            process_control: ProcessControl::empty(),
            events_tx: None,
            events: None,
            tasks: vec![],
            result: Arc::new(RwLock::new(None)),
            error: Arc::new(RwLock::new(Some(io.into()))),
            _mcp_temp_file: None,
        };

        let err = session.wait().await.unwrap_err();
        match err {
            ClaudeError::IoError { source } => {
                assert_eq!(source.kind(), std::io::ErrorKind::Other);
                assert!(source.to_string().contains("disk full"));
            }
            other => panic!("expected IoError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_returns_no_result_available_when_result_and_error_missing() {
        let cfg = SessionConfig::builder("test".to_string())
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let session = Session {
            id: "test".into(),
            config: cfg,
            start_time: Utc::now(),
            process: Arc::new(Mutex::new(None)),
            process_control: ProcessControl::empty(),
            events_tx: None,
            events: None,
            tasks: vec![],
            result: Arc::new(RwLock::new(None)),
            error: Arc::new(RwLock::new(None)),
            _mcp_temp_file: None,
        };

        let err = session.wait().await.unwrap_err();
        match err {
            ClaudeError::SessionError { message } => assert_eq!(message, "No result available"),
            other => panic!("expected SessionError, got {other:?}"),
        }
    }
}
