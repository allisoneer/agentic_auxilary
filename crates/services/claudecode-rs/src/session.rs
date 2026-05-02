use crate::config::SessionConfig;
use crate::error::ClaudeError;
use crate::error::Result;
use crate::process::KillHandle;
use crate::process::ProcessHandle;
use crate::stream::JsonStreamParser;
use crate::stream::SingleJsonParser;
use crate::stream::TextParser;
use crate::types::Event;
use crate::types::OutputFormat;
use crate::types::Result as ClaudeResult;
use chrono::Utc;
use futures::StreamExt;
use nix::sys::signal::Signal;
use std::sync::Arc;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::io::AsyncBufReadExt;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::debug;
use tracing::info;
use tracing::warn;
use uuid::Uuid;

const TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);

pub struct Session {
    id: String,
    config: SessionConfig,
    start_time: chrono::DateTime<Utc>,
    kill: KillHandle,

    // Event channel for streaming
    events_tx: Option<mpsc::UnboundedSender<Event>>,
    events: Option<mpsc::UnboundedReceiver<Event>>,

    // Background tasks
    worker_task: std::sync::Mutex<Option<JoinHandle<()>>>,
    stderr_task: std::sync::Mutex<Option<JoinHandle<()>>>,

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

        let kill = process.kill_handle()?;
        let result = Arc::new(RwLock::new(None));
        let error = Arc::new(RwLock::new(None));

        let mut session = Self {
            id,
            config: config.clone(),
            start_time: Utc::now(),
            kill,
            events_tx,
            events,
            worker_task: std::sync::Mutex::new(None),
            stderr_task: std::sync::Mutex::new(None),
            result: result.clone(),
            error: error.clone(),
            _mcp_temp_file: None,
        };

        // Start background tasks based on output format
        session.start_tasks(process).await?;

        Ok(session)
    }

    async fn start_tasks(&mut self, mut process: ProcessHandle) -> Result<()> {
        let result = self.result.clone();
        let error = self.error.clone();

        match self.config.output_format {
            OutputFormat::StreamingJson => {
                let stderr = process
                    .take_stderr()
                    .ok_or_else(|| ClaudeError::SessionError {
                        message: "No stderr reader".to_string(),
                    })?;
                let events_tx = self
                    .events_tx
                    .take()
                    .expect("events_tx must exist for StreamingJson output format");
                let result_clone = result.clone();
                let error_clone = error.clone();
                let stderr_task = tokio::spawn(async move {
                    Self::capture_stderr(stderr, error_clone).await;
                });
                Self::store_task(&self.stderr_task, stderr_task)?;

                let worker_task = tokio::spawn(async move {
                    if let Err(e) =
                        Self::handle_streaming_json(process, events_tx, result_clone, error.clone())
                            .await
                    {
                        error.write().await.replace(e);
                    }
                });
                Self::store_task(&self.worker_task, worker_task)?;
            }
            OutputFormat::Json => {
                let worker_task = tokio::spawn(async move {
                    match Self::handle_json(process, error.clone()).await {
                        Ok(r) => {
                            result.write().await.replace(r);
                        }
                        Err(e) => {
                            error.write().await.replace(e);
                        }
                    }
                });
                Self::store_task(&self.worker_task, worker_task)?;
            }
            OutputFormat::Text => {
                let worker_task = tokio::spawn(async move {
                    match Self::handle_text(process, error.clone()).await {
                        Ok(r) => {
                            result.write().await.replace(r);
                        }
                        Err(e) => {
                            error.write().await.replace(e);
                        }
                    }
                });
                Self::store_task(&self.worker_task, worker_task)?;
            }
        }

        Ok(())
    }

    fn store_task(
        slot: &std::sync::Mutex<Option<JoinHandle<()>>>,
        task: JoinHandle<()>,
    ) -> Result<()> {
        let mut guard = slot.lock().map_err(|_| ClaudeError::SessionError {
            message: "Session task mutex poisoned".to_string(),
        })?;
        guard.replace(task);
        Ok(())
    }

    fn take_task(
        slot: &std::sync::Mutex<Option<JoinHandle<()>>>,
    ) -> Result<Option<JoinHandle<()>>> {
        let mut guard = slot.lock().map_err(|_| ClaudeError::SessionError {
            message: "Session task mutex poisoned".to_string(),
        })?;
        Ok(guard.take())
    }

    async fn await_task(task: Option<JoinHandle<()>>, label: &str) -> Result<()> {
        if let Some(task) = task {
            task.await.map_err(|err| ClaudeError::SessionError {
                message: format!("{label} task failed: {err}"),
            })?;
        }
        Ok(())
    }

    async fn shutdown_task(mut task: Option<JoinHandle<()>>, label: &str) -> Result<()> {
        if let Some(mut handle) = task.take()
            && tokio::time::timeout(TASK_SHUTDOWN_TIMEOUT, &mut handle)
                .await
                .is_err()
        {
            warn!(task = label, "aborting stalled session task");
            handle.abort();
            let _ = tokio::time::timeout(TASK_SHUTDOWN_TIMEOUT, handle).await;
        }
        Ok(())
    }

    async fn capture_stderr(
        stderr: tokio::io::BufReader<tokio::process::ChildStderr>,
        error: Arc<RwLock<Option<ClaudeError>>>,
    ) {
        let mut stderr_content = String::new();
        let mut lines = stderr.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            stderr_content.push_str(&line);
            stderr_content.push('\n');
        }
        if !stderr_content.trim().is_empty() {
            error.write().await.replace(ClaudeError::ProcessFailed {
                code: -1,
                stderr: stderr_content,
            });
        }
    }

    async fn handle_streaming_json(
        mut process: ProcessHandle,
        events_tx: mpsc::UnboundedSender<Event>,
        result_arc: Arc<RwLock<Option<ClaudeResult>>>,
        error: Arc<RwLock<Option<ClaudeError>>>,
    ) -> Result<()> {
        let stdout = process
            .take_stdout()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "No stdout reader".to_string(),
            })?;

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
        mut process: ProcessHandle,
        _error: Arc<RwLock<Option<ClaudeError>>>,
    ) -> Result<ClaudeResult> {
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
        mut process: ProcessHandle,
        _error: Arc<RwLock<Option<ClaudeError>>>,
    ) -> Result<ClaudeResult> {
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
    pub async fn wait(&self) -> Result<ClaudeResult> {
        let worker_task = Self::take_task(&self.worker_task)?;
        let stderr_task = Self::take_task(&self.stderr_task)?;

        Self::await_task(worker_task, "worker").await?;
        Self::await_task(stderr_task, "stderr").await?;

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

    pub async fn cancel(&self) -> Result<()> {
        info!(session_id = %self.id, "cancelling Claude session");
        self.kill.graceful_terminate().await?;

        let worker_task = Self::take_task(&self.worker_task)?;
        let stderr_task = Self::take_task(&self.stderr_task)?;
        Self::shutdown_task(worker_task, "worker").await?;
        Self::shutdown_task(stderr_task, "stderr").await?;

        Ok(())
    }

    /// Kill the Claude process
    pub async fn kill(&mut self) -> Result<()> {
        info!(session_id = %self.id, "force-killing Claude session");
        self.cancel().await
    }

    /// Send interrupt signal to the Claude process
    ///
    /// On Unix systems, this sends SIGINT which allows graceful shutdown.
    pub async fn interrupt(&mut self) -> Result<()> {
        self.kill
            .signal(Signal::SIGINT)
            .map_err(|err| ClaudeError::SessionError {
                message: format!("Failed to send interrupt signal: {err}"),
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
        self.worker_task
            .lock()
            .ok()
            .and_then(|task| task.as_ref().map(|task| !task.is_finished()))
            .unwrap_or(false)
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
        let _ = self.kill.kill_now();

        if let Ok(mut worker_task) = self.worker_task.lock()
            && let Some(task) = worker_task.take()
        {
            task.abort();
        }

        if let Ok(mut stderr_task) = self.stderr_task.lock()
            && let Some(task) = stderr_task.take()
        {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SessionConfig;
    use crate::error::ClaudeError;
    use crate::types::OutputFormat;
    use std::path::Path;

    async fn test_kill_handle() -> KillHandle {
        let process = ProcessHandle::spawn(
            Path::new("/bin/sh"),
            vec!["-c".to_string(), "exit 0".to_string()],
            None,
            None,
        )
        .await
        .unwrap();

        process.kill_handle().unwrap()
    }

    #[tokio::test]
    async fn wait_returns_processfailed_preserving_stderr() {
        let cfg = SessionConfig::builder("test".to_string())
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let kill = test_kill_handle().await;

        let session = Session {
            id: "test".into(),
            config: cfg,
            start_time: Utc::now(),
            kill,
            events_tx: None,
            events: None,
            worker_task: std::sync::Mutex::new(None),
            stderr_task: std::sync::Mutex::new(None),
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

        let kill = test_kill_handle().await;

        let session = Session {
            id: "test".into(),
            config: cfg,
            start_time: Utc::now(),
            kill,
            events_tx: None,
            events: None,
            worker_task: std::sync::Mutex::new(None),
            stderr_task: std::sync::Mutex::new(None),
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
        let kill = test_kill_handle().await;

        let session = Session {
            id: "test".into(),
            config: cfg,
            start_time: Utc::now(),
            kill,
            events_tx: None,
            events: None,
            worker_task: std::sync::Mutex::new(None),
            stderr_task: std::sync::Mutex::new(None),
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

        let kill = test_kill_handle().await;

        let session = Session {
            id: "test".into(),
            config: cfg,
            start_time: Utc::now(),
            kill,
            events_tx: None,
            events: None,
            worker_task: std::sync::Mutex::new(None),
            stderr_task: std::sync::Mutex::new(None),
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
