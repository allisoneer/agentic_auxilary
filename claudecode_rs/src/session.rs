use crate::config::SessionConfig;
use crate::error::{ClaudeError, Result};
use crate::process::ProcessHandle;
use crate::stream::{JsonStreamParser, SingleJsonParser, TextParser};
use crate::types::{Event, OutputFormat, Result as ClaudeResult};
use chrono::Utc;
use futures::StreamExt;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tokio::io::AsyncBufReadExt;
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::task::JoinHandle;
use tracing::{debug, warn};
use uuid::Uuid;

pub struct Session {
    id: String,
    config: SessionConfig,
    start_time: chrono::DateTime<Utc>,

    // Process handle
    process: Arc<Mutex<Option<ProcessHandle>>>,

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

        let process = Arc::new(Mutex::new(Some(process)));
        let result = Arc::new(RwLock::new(None));
        let error = Arc::new(RwLock::new(None));

        let mut session = Self {
            id,
            config: config.clone(),
            start_time: Utc::now(),
            process: process.clone(),
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
        let mut process_guard = process.lock().await;
        let mut process = process_guard
            .take()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "Process already taken".to_string(),
            })?;

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
        let mut process_guard = process.lock().await;
        let mut process = process_guard
            .take()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "Process already taken".to_string(),
            })?;

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
        let mut process_guard = process.lock().await;
        let mut process = process_guard
            .take()
            .ok_or_else(|| ClaudeError::SessionError {
                message: "Process already taken".to_string(),
            })?;

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
        // Wait for all tasks to complete
        for task in self.tasks.drain(..) {
            let _ = task.await;
        }

        // Check for errors first
        if let Some(error) = self.error.read().await.as_ref() {
            return Err(ClaudeError::SessionError {
                message: error.to_string(),
            });
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

    /// Kill the Claude process
    pub async fn kill(&mut self) -> Result<()> {
        if let Some(mut process) = self.process.lock().await.take() {
            process.kill().await?;
        }
        Ok(())
    }

    /// Send interrupt signal to the Claude process
    ///
    /// On Unix systems, this sends SIGINT which allows graceful shutdown.
    pub async fn interrupt(&mut self) -> Result<()> {
        if let Some(process) = self.process.lock().await.as_mut()
            && let Some(pid) = process.id()
        {
            // Send SIGINT for graceful shutdown
            unsafe {
                let result = libc::kill(pid as i32, libc::SIGINT);
                if result == 0 {
                    return Ok(());
                } else {
                    return Err(ClaudeError::SessionError {
                        message: format!(
                            "Failed to send interrupt signal: {}",
                            std::io::Error::last_os_error()
                        ),
                    });
                }
            }
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
        if let Some(ref _process) = *self.process.lock().await {
            // Process is still held, might be running
            true
        } else {
            false
        }
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
        // Ensure all tasks are aborted on drop
        for task in &self.tasks {
            task.abort();
        }
    }
}
