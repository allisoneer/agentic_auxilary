use crate::config::SessionConfig;
use crate::error::ClaudeError;
use crate::error::Result;
use crate::process::KillHandle;
use crate::process::ProcessHandle;
use crate::stream::DIAGNOSTIC_BYTE_LIMIT;
use crate::stream::JsonStreamParser;
use crate::stream::SingleJsonParser;
use crate::stream::TextParser;
use crate::stream::normalize_error_event;
use crate::stream::normalize_result_event;
use crate::stream::read_bounded_tail;
use crate::types::Event;
use crate::types::InvocationMetadata;
use crate::types::OutputFormat;
use crate::types::RawEvent;
use crate::types::Result as ClaudeResult;
use crate::types::SessionFailure;
use crate::types::SessionOutcome;
use aho_corasick::AhoCorasick;
use aho_corasick::AhoCorasickBuilder;
use aho_corasick::MatchKind;
use chrono::Utc;
use futures::StreamExt;
use nix::sys::signal::Signal;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::info;
use tracing::warn;
use uuid::Uuid;

const TASK_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(1);
const DIAGNOSTIC_LIMIT: usize = DIAGNOSTIC_BYTE_LIMIT;
const TRANSCRIPT_BYTE_LIMIT: usize = 2 * 1024 * 1024;
const TRANSCRIPT_EVENT_LIMIT: usize = 2_048;
const EVENT_CHANNEL_LIMIT: usize = 2_048;

#[derive(Clone, Default)]
struct SecretRedactor {
    matcher: Option<AhoCorasick>,
}

impl SecretRedactor {
    fn from_config(config: &SessionConfig) -> Result<Self> {
        fn sensitive(key: &str) -> bool {
            let key = key.to_ascii_lowercase();
            [
                "key",
                "token",
                "secret",
                "password",
                "authorization",
                "credential",
            ]
            .iter()
            .any(|needle| key.contains(needle))
        }

        let mut values = Vec::new();
        let mut collect = |entries: &std::collections::HashMap<String, String>| {
            values.extend(
                entries
                    .iter()
                    .filter(|(key, value)| sensitive(key) && !value.is_empty())
                    .map(|(_, value)| value.clone()),
            );
        };
        if let Some(env) = &config.env {
            collect(env);
        }
        if let Some(mcp) = &config.mcp_config {
            for server in mcp.mcp_servers.values() {
                match server {
                    crate::config::MCPServer::Stdio { env: Some(env), .. } => collect(env),
                    crate::config::MCPServer::Http {
                        headers: Some(headers),
                        ..
                    } => collect(headers),
                    _ => {}
                }
            }
        }
        for (key, value) in std::env::vars() {
            if sensitive(&key) && !value.is_empty() {
                values.push(value);
            }
        }
        Self::from_values(values)
    }

    fn from_values(mut values: Vec<String>) -> Result<Self> {
        values.sort_by(|left, right| right.len().cmp(&left.len()).then_with(|| left.cmp(right)));
        values.dedup();
        if values.is_empty() {
            return Ok(Self::default());
        }
        let matcher = AhoCorasickBuilder::new()
            .match_kind(MatchKind::LeftmostLongest)
            .build(values)
            .map_err(|error| ClaudeError::SessionError {
                message: format!("Failed to initialize secret redaction: {error}"),
            })?;
        Ok(Self {
            matcher: Some(matcher),
        })
    }

    fn text(&self, value: &str) -> String {
        const REPLACEMENT: &str = "<redacted>";
        let Some(matcher) = &self.matcher else {
            return value.to_string();
        };
        let mut redacted = String::with_capacity(value.len());
        let mut copied_through = 0;
        for matched in matcher.find_iter(value) {
            redacted.push_str(&value[copied_through..matched.start()]);
            redacted.push_str(REPLACEMENT);
            copied_through = matched.end();
        }
        redacted.push_str(&value[copied_through..]);
        redacted
    }

    fn json(&self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(text) => *text = self.text(text),
            serde_json::Value::Array(values) => {
                for value in values {
                    self.json(value);
                }
            }
            serde_json::Value::Object(values) => {
                for value in values.values_mut() {
                    self.json(value);
                }
            }
            _ => {}
        }
    }

    fn outcome(&self, outcome: &mut SessionOutcome) {
        self.result(&mut outcome.result);
        bound_transcript(&mut outcome.transcript);
        for event in &mut outcome.transcript {
            self.json(&mut event.raw);
            if let Ok(redacted) = RawEvent::from_value(event.raw.clone()) {
                *event = redacted;
            }
        }
        bound_transcript(&mut outcome.transcript);
        outcome.raw_stdout = bounded_tail(self.text(&outcome.raw_stdout));
        outcome.stderr = bounded_tail(self.text(&outcome.stderr));
    }

    fn result(&self, result: &mut ClaudeResult) {
        result.result = result.result.as_deref().map(|value| self.text(value));
        result.content = result.content.as_deref().map(|value| self.text(value));
        result.error = result.error.as_deref().map(|value| self.text(value));
    }

    fn failure(&self, failure: &mut SessionFailure) {
        failure.message = self.text(&failure.message);
        bound_transcript(&mut failure.transcript);
        for event in &mut failure.transcript {
            self.json(&mut event.raw);
            if let Ok(redacted) = RawEvent::from_value(event.raw.clone()) {
                *event = redacted;
            }
        }
        bound_transcript(&mut failure.transcript);
        failure.raw_stdout = bounded_tail(self.text(&failure.raw_stdout));
        failure.stderr = bounded_tail(self.text(&failure.stderr));
    }
}

fn bounded_tail(mut value: String) -> String {
    if value.len() <= DIAGNOSTIC_LIMIT {
        return value;
    }
    let mut start = value.len() - DIAGNOSTIC_LIMIT;
    while !value.is_char_boundary(start) {
        start += 1;
    }
    value.drain(..start);
    value
}

async fn append_bounded_json(target: &RwLock<String>, value: &serde_json::Value) {
    if let Ok(line) = serde_json::to_string(value) {
        let mut target = target.write().await;
        target.push_str(&line);
        target.push('\n');
        if target.len() > DIAGNOSTIC_LIMIT {
            *target = bounded_tail(std::mem::take(&mut *target));
        }
    }
}

async fn append_bounded_text(target: &RwLock<String>, value: &str) {
    let mut target = target.write().await;
    target.push_str(value);
    if !value.ends_with('\n') {
        target.push('\n');
    }
    if target.len() > DIAGNOSTIC_LIMIT {
        *target = bounded_tail(std::mem::take(&mut *target));
    }
}

async fn append_bounded_event(target: &RwLock<Vec<RawEvent>>, event: RawEvent) {
    let mut target = target.write().await;
    target.push(event);
    bound_transcript(&mut target);
}

fn bound_transcript(target: &mut Vec<RawEvent>) {
    let count_bounded_start = target.len().saturating_sub(TRANSCRIPT_EVENT_LIMIT);
    let mut retained_start = target.len();
    let mut retained_bytes = 0_usize;
    for index in (count_bounded_start..target.len()).rev() {
        let event_bytes = target[index].raw.to_string().len();
        let Some(total_bytes) = retained_bytes.checked_add(event_bytes) else {
            break;
        };
        if total_bytes > TRANSCRIPT_BYTE_LIMIT {
            break;
        }
        retained_bytes = total_bytes;
        retained_start = index;
    }
    if retained_start > 0 {
        target.drain(..retained_start);
    }
}

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
    process_group_owned: Arc<AtomicBool>,

    // Result storage
    result: Arc<RwLock<Option<ClaudeResult>>>,
    error: Arc<RwLock<Option<ClaudeError>>>,
    transcript: Arc<RwLock<Vec<RawEvent>>>,
    raw_stdout: Arc<RwLock<String>>,
    stderr: Arc<RwLock<String>>,
    exit_code: Arc<RwLock<Option<i32>>>,
    invocation: InvocationMetadata,
    secret_redactor: SecretRedactor,

    // Temp file for MCP config (must be kept alive)
    _mcp_temp_file: Option<NamedTempFile>,
}

impl Session {
    pub async fn new(config: SessionConfig, process: ProcessHandle) -> Result<Self> {
        Self::new_with_invocation(config, process, InvocationMetadata::default()).await
    }

    pub async fn new_with_invocation(
        config: SessionConfig,
        process: ProcessHandle,
        invocation: InvocationMetadata,
    ) -> Result<Self> {
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
        let transcript = Arc::new(RwLock::new(Vec::new()));
        let raw_stdout = Arc::new(RwLock::new(String::new()));
        let stderr = Arc::new(RwLock::new(String::new()));
        let exit_code = Arc::new(RwLock::new(None));
        let process_group_owned = Arc::new(AtomicBool::new(true));

        let secret_redactor = SecretRedactor::from_config(&config)?;
        let mut session = Self {
            id,
            config: config.clone(),
            start_time: Utc::now(),
            kill,
            events_tx,
            events,
            worker_task: std::sync::Mutex::new(None),
            stderr_task: std::sync::Mutex::new(None),
            process_group_owned,
            result: Arc::clone(&result),
            error: Arc::clone(&error),
            transcript,
            raw_stdout,
            stderr,
            exit_code,
            invocation,
            secret_redactor,
            _mcp_temp_file: None,
        };

        // Start background tasks based on output format
        session.start_tasks(process).await?;

        Ok(session)
    }

    #[expect(
        clippy::unused_async,
        reason = "async preserved so Session::new can await a fallible task-setup step"
    )]
    async fn start_tasks(&mut self, mut process: ProcessHandle) -> Result<()> {
        let result = Arc::clone(&self.result);
        let error = Arc::clone(&self.error);
        let process_group_owned = Arc::clone(&self.process_group_owned);
        let transcript = Arc::clone(&self.transcript);
        let raw_stdout = Arc::clone(&self.raw_stdout);
        let stderr_diagnostics = Arc::clone(&self.stderr);
        let exit_code = Arc::clone(&self.exit_code);
        let secret_redactor = self.secret_redactor.clone();

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
                    .ok_or_else(|| ClaudeError::SessionError {
                        message: "events_tx must exist for StreamingJson output format".to_string(),
                    })?;
                let result_clone = result;
                let stderr_redactor = secret_redactor.clone();
                let stderr_task = tokio::spawn(async move {
                    Self::capture_stderr(stderr, stderr_diagnostics, stderr_redactor).await;
                });
                Self::store_task(&self.stderr_task, stderr_task)?;

                let worker_task = tokio::spawn(async move {
                    if let Err(e) = Self::handle_streaming_json(
                        process,
                        events_tx,
                        result_clone,
                        transcript,
                        raw_stdout,
                        exit_code,
                        process_group_owned,
                        secret_redactor,
                    )
                    .await
                    {
                        error.write().await.replace(e);
                    }
                });
                Self::store_task(&self.worker_task, worker_task)?;
            }
            OutputFormat::Json => {
                let worker_task = tokio::spawn(async move {
                    match Self::handle_json(process, process_group_owned).await {
                        Ok((mut parsed, code)) => {
                            for mut event in parsed.events.drain(..) {
                                secret_redactor.json(&mut event.raw);
                                if let Ok(redacted) = RawEvent::from_value(event.raw.clone()) {
                                    event = redacted;
                                }
                                append_bounded_event(&transcript, event).await;
                            }
                            *raw_stdout.write().await =
                                bounded_tail(secret_redactor.text(&parsed.raw_stdout));
                            *stderr_diagnostics.write().await =
                                bounded_tail(secret_redactor.text(&parsed.stderr));
                            exit_code.write().await.replace(code);
                            if let Some(mut parsed_result) = parsed.result {
                                secret_redactor.result(&mut parsed_result);
                                result.write().await.replace(parsed_result);
                            }
                            if let Some(parsed_error) = parsed.error {
                                error.write().await.replace(parsed_error);
                            }
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
                    match Self::handle_text(process, process_group_owned).await {
                        Ok((mut parsed, code)) => {
                            *raw_stdout.write().await =
                                bounded_tail(secret_redactor.text(&parsed.raw_stdout));
                            *stderr_diagnostics.write().await =
                                bounded_tail(secret_redactor.text(&parsed.stderr));
                            secret_redactor.result(&mut parsed.result);
                            result.write().await.replace(parsed.result);
                            exit_code.write().await.replace(code);
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
        mut stderr: tokio::io::BufReader<tokio::process::ChildStderr>,
        diagnostics: Arc<RwLock<String>>,
        secret_redactor: SecretRedactor,
    ) {
        if let Ok((stderr_content, _)) = read_bounded_tail(&mut stderr, DIAGNOSTIC_LIMIT).await {
            *diagnostics.write().await = bounded_tail(secret_redactor.text(&stderr_content));
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "stream worker receives independently owned bounded session state"
    )]
    async fn handle_streaming_json(
        mut process: ProcessHandle,
        events_tx: mpsc::UnboundedSender<Event>,
        result_arc: Arc<RwLock<Option<ClaudeResult>>>,
        transcript: Arc<RwLock<Vec<RawEvent>>>,
        raw_stdout: Arc<RwLock<String>>,
        exit_code: Arc<RwLock<Option<i32>>>,
        process_group_owned: Arc<AtomicBool>,
        secret_redactor: SecretRedactor,
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
        let mut last_parse_error = None;
        let mut public_events_sent = 0_usize;

        while let Some(result) = stream.next().await {
            match result {
                Ok(mut envelope) => {
                    secret_redactor.json(&mut envelope.raw);
                    if let Ok(redacted) = RawEvent::from_value(envelope.raw.clone()) {
                        envelope = redacted;
                    }
                    let event = envelope.event.clone();
                    append_bounded_json(&raw_stdout, &envelope.raw).await;
                    append_bounded_event(&transcript, envelope).await;
                    // Check if this is a result event and store it
                    if let Event::Result(ref result_event) = event {
                        result_arc
                            .write()
                            .await
                            .replace(normalize_result_event(result_event));
                    } else if let Event::Error(ref error_event) = event {
                        result_arc
                            .write()
                            .await
                            .replace(normalize_error_event(error_event));
                    }

                    // Preserve the legacy unbounded receiver type while bounding retained
                    // public events. Reserve one slot so stream termination is always visible.
                    let terminal = matches!(event, Event::Result(_) | Event::Error(_));
                    if (terminal || public_events_sent < EVENT_CHANNEL_LIMIT.saturating_sub(1))
                        && events_tx.send(event).is_ok()
                    {
                        public_events_sent += 1;
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to parse JSON event: {}",
                        secret_redactor.text(&e.to_string())
                    );
                    match &e {
                        ClaudeError::JsonParseError {
                            line: Some(line), ..
                        }
                        | ClaudeError::EventLineTooLong { tail: line, .. } => {
                            append_bounded_text(&raw_stdout, &secret_redactor.text(line)).await;
                        }
                        _ => {}
                    }
                    last_parse_error = Some(e);
                }
            }
        }

        // Explicitly drop the sender to signal end of stream
        drop(events_tx);

        // Wait for process to complete
        let status = process.wait().await?;
        process_group_owned.store(false, Ordering::Release);
        exit_code.write().await.replace(status.code().unwrap_or(-1));

        if result_arc.read().await.is_none() {
            return Err(
                last_parse_error.unwrap_or_else(|| ClaudeError::SessionError {
                    message: "Claude stream ended without a terminal result or error event"
                        .to_string(),
                }),
            );
        }

        Ok(())
    }

    async fn handle_json(
        mut process: ProcessHandle,
        process_group_owned: Arc<AtomicBool>,
    ) -> Result<(crate::stream::ParsedJsonOutput, i32)> {
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
        process_group_owned.store(false, Ordering::Release);
        Ok((result, status.code().unwrap_or(-1)))
    }

    async fn handle_text(
        mut process: ProcessHandle,
        process_group_owned: Arc<AtomicBool>,
    ) -> Result<(crate::stream::ParsedTextOutput, i32)> {
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
        process_group_owned.store(false, Ordering::Release);
        Ok((result, status.code().unwrap_or(-1)))
    }

    /// Wait for the session to complete and return the result
    pub async fn wait(&self) -> Result<ClaudeResult> {
        let outcome = self.complete().await?;
        let code = outcome.exit_code.unwrap_or(-1);
        if code != 0 {
            return Err(Self::outcome_failure(
                outcome,
                format!("Claude process exited with status {code}"),
            ));
        }
        if outcome.result.is_error {
            let message = outcome
                .result
                .error
                .clone()
                .unwrap_or_else(|| "Claude returned a structured terminal error".to_string());
            return Err(Self::outcome_failure(outcome, message));
        }
        Ok(outcome.result)
    }

    fn outcome_failure(outcome: SessionOutcome, message: String) -> ClaudeError {
        ClaudeError::ExecutionFailed {
            failure: Box::new(SessionFailure {
                message,
                transcript: outcome.transcript,
                exit_code: outcome.exit_code,
                raw_stdout: outcome.raw_stdout,
                stderr: outcome.stderr,
                invocation: outcome.invocation,
            }),
        }
    }

    async fn retained_failure(&self, message: String) -> ClaudeError {
        let mut failure = SessionFailure {
            message,
            transcript: self.transcript.read().await.clone(),
            exit_code: *self.exit_code.read().await,
            raw_stdout: self.raw_stdout.read().await.clone(),
            stderr: self.stderr.read().await.clone(),
            invocation: self.invocation.clone(),
        };
        self.secret_redactor.failure(&mut failure);
        ClaudeError::ExecutionFailed {
            failure: Box::new(failure),
        }
    }

    /// Wait once for completion and return the ordered transcript and diagnostics.
    pub async fn complete(&self) -> Result<SessionOutcome> {
        let worker_task = Self::take_task(&self.worker_task)?;
        let stderr_task = Self::take_task(&self.stderr_task)?;

        if let Err(error) = Self::await_task(worker_task, "worker").await {
            return Err(self.retained_failure(error.to_string()).await);
        }
        if let Err(error) = Self::await_task(stderr_task, "stderr").await {
            return Err(self.retained_failure(error.to_string()).await);
        }

        let error = self.error.write().await.take();
        if let Some(error) = error {
            return Err(self.retained_failure(error.to_string()).await);
        }

        let Some(result) = self.result.read().await.clone() else {
            return Err(self
                .retained_failure(
                    "Claude process completed without a terminal result or error event".to_string(),
                )
                .await);
        };
        let mut outcome = SessionOutcome {
            result,
            transcript: self.transcript.read().await.clone(),
            exit_code: *self.exit_code.read().await,
            raw_stdout: self.raw_stdout.read().await.clone(),
            stderr: self.stderr.read().await.clone(),
            invocation: self.invocation.clone(),
        };
        self.secret_redactor.outcome(&mut outcome);
        Ok(outcome)
    }

    pub async fn cancel(&self) -> Result<()> {
        info!(session_id = %self.id, "cancelling Claude session");
        self.kill.graceful_terminate().await?;
        self.process_group_owned.store(false, Ordering::Release);

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
    #[expect(
        clippy::unused_async,
        reason = "async for API consistency with cancel and kill"
    )]
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
    pub fn is_running(&self) -> bool {
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
    #[expect(
        clippy::used_underscore_binding,
        reason = "field exists solely to keep NamedTempFile alive until Session is dropped"
    )]
    pub fn set_mcp_temp_file(&mut self, temp_file: NamedTempFile) {
        self._mcp_temp_file = Some(temp_file);
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if self.process_group_owned.load(Ordering::Acquire) {
            let _ = self.kill.kill_now();
        }

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

    #[tokio::test]
    async fn diagnostics_keep_a_utf8_safe_bounded_tail() {
        let prefix = "discard-me".repeat(DIAGNOSTIC_LIMIT / 5);
        let suffix = "é-tail".repeat(DIAGNOSTIC_LIMIT / 5);
        let bounded = bounded_tail(format!("{prefix}{suffix}"));
        assert!(bounded.len() <= DIAGNOSTIC_LIMIT);
        assert!(bounded.ends_with("é-tail"));
        assert!(!bounded.starts_with("discard-me"));

        let raw = RwLock::new(String::new());
        for index in 0..4_000 {
            append_bounded_json(
                &raw,
                &serde_json::json!({"index": index, "payload": "x".repeat(100)}),
            )
            .await;
        }
        let raw = raw.into_inner();
        assert!(raw.len() <= DIAGNOSTIC_LIMIT);
        assert!(raw.contains("\"index\":3999"));
        assert!(!raw.contains("\"index\":0,"));

        let transcript = RwLock::new(Vec::new());
        for index in 0..(TRANSCRIPT_EVENT_LIMIT + 10) {
            append_bounded_event(
                &transcript,
                RawEvent::from_value(serde_json::json!({"type": "future", "index": index}))
                    .unwrap(),
            )
            .await;
        }
        let transcript = transcript.into_inner();
        assert_eq!(transcript.len(), TRANSCRIPT_EVENT_LIMIT);
        assert_eq!(
            transcript.last().unwrap().raw["index"],
            TRANSCRIPT_EVENT_LIMIT + 9
        );
    }

    #[test]
    fn repeated_short_secret_redaction_reapplies_all_retention_bounds() {
        let redactor = SecretRedactor::from_values(vec!["x".to_string()]).unwrap();
        let mut outcome = SessionOutcome {
            result: ClaudeResult::default(),
            transcript: (0..150_000)
                .map(|index| {
                    RawEvent::from_value(serde_json::json!({
                        "type": "future",
                        "index": index,
                        "payload": "x"
                    }))
                    .unwrap()
                })
                .collect(),
            exit_code: Some(0),
            raw_stdout: "x".repeat(DIAGNOSTIC_LIMIT),
            stderr: "x".repeat(DIAGNOSTIC_LIMIT),
            invocation: InvocationMetadata::default(),
        };

        redactor.outcome(&mut outcome);

        let transcript_bytes = outcome
            .transcript
            .iter()
            .map(|event| event.raw.to_string().len())
            .sum::<usize>();
        assert!(outcome.raw_stdout.len() <= DIAGNOSTIC_LIMIT);
        assert!(outcome.stderr.len() <= DIAGNOSTIC_LIMIT);
        assert!(!outcome.transcript.is_empty());
        assert!(outcome.transcript.len() <= TRANSCRIPT_EVENT_LIMIT);
        assert!(transcript_bytes <= TRANSCRIPT_BYTE_LIMIT);
        assert!(outcome.raw_stdout.ends_with("<redacted>"));
        assert!(outcome.stderr.ends_with("<redacted>"));
        assert_eq!(outcome.transcript.last().unwrap().raw["index"], 149_999);
        assert_ne!(outcome.transcript.first().unwrap().raw["index"], 0);
    }

    #[test]
    fn short_secret_replacements_are_not_redacted_recursively() {
        let redactor =
            SecretRedactor::from_values(vec!["x".to_string(), "e".to_string(), "d".to_string()])
                .unwrap();

        assert_eq!(redactor.text("x"), "<redacted>");
        assert_eq!(redactor.text("xx"), "<redacted><redacted>");
    }
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
            process_group_owned: Arc::new(AtomicBool::new(false)),
            result: Arc::new(RwLock::new(None)),
            error: Arc::new(RwLock::new(Some(ClaudeError::ProcessFailed {
                code: 1,
                stderr: "stderr details".into(),
            }))),
            transcript: Arc::new(RwLock::new(Vec::new())),
            raw_stdout: Arc::new(RwLock::new(String::new())),
            stderr: Arc::new(RwLock::new(String::new())),
            exit_code: Arc::new(RwLock::new(Some(0))),
            invocation: InvocationMetadata::default(),
            secret_redactor: SecretRedactor::default(),
            _mcp_temp_file: None,
        };

        let err = session.wait().await.unwrap_err();
        let ClaudeError::ExecutionFailed { failure } = err else {
            panic!("expected diagnostics-bearing execution failure");
        };
        assert!(failure.message.contains("code 1"));
        assert!(failure.message.contains("stderr details"));
        assert_eq!(failure.exit_code, Some(0));
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
            process_group_owned: Arc::new(AtomicBool::new(false)),
            result: Arc::new(RwLock::new(None)),
            error: Arc::new(RwLock::new(Some(ClaudeError::SessionError {
                message: "custom session error".into(),
            }))),
            transcript: Arc::new(RwLock::new(Vec::new())),
            raw_stdout: Arc::new(RwLock::new(String::new())),
            stderr: Arc::new(RwLock::new(String::new())),
            exit_code: Arc::new(RwLock::new(Some(0))),
            invocation: InvocationMetadata::default(),
            secret_redactor: SecretRedactor::default(),
            _mcp_temp_file: None,
        };

        let err = session.wait().await.unwrap_err();
        let ClaudeError::ExecutionFailed { failure } = err else {
            panic!("expected diagnostics-bearing execution failure");
        };
        assert!(failure.message.contains("custom session error"));
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
            process_group_owned: Arc::new(AtomicBool::new(false)),
            result: Arc::new(RwLock::new(None)),
            error: Arc::new(RwLock::new(Some(io.into()))),
            transcript: Arc::new(RwLock::new(Vec::new())),
            raw_stdout: Arc::new(RwLock::new(String::new())),
            stderr: Arc::new(RwLock::new(String::new())),
            exit_code: Arc::new(RwLock::new(Some(0))),
            invocation: InvocationMetadata::default(),
            secret_redactor: SecretRedactor::default(),
            _mcp_temp_file: None,
        };

        let err = session.wait().await.unwrap_err();
        let ClaudeError::ExecutionFailed { failure } = err else {
            panic!("expected diagnostics-bearing execution failure");
        };
        assert!(failure.message.contains("disk full"));
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
            process_group_owned: Arc::new(AtomicBool::new(false)),
            result: Arc::new(RwLock::new(None)),
            error: Arc::new(RwLock::new(None)),
            transcript: Arc::new(RwLock::new(Vec::new())),
            raw_stdout: Arc::new(RwLock::new(String::new())),
            stderr: Arc::new(RwLock::new(String::new())),
            exit_code: Arc::new(RwLock::new(Some(0))),
            invocation: InvocationMetadata::default(),
            secret_redactor: SecretRedactor::default(),
            _mcp_temp_file: None,
        };

        let err = session.wait().await.unwrap_err();
        let ClaudeError::ExecutionFailed { failure } = err else {
            panic!("expected diagnostics-bearing execution failure");
        };
        assert!(failure.message.contains("without a terminal result"));
    }

    #[tokio::test]
    async fn drop_skips_kill_when_process_group_is_released() {
        let cfg = SessionConfig::builder("test".to_string())
            .output_format(OutputFormat::Text)
            .build()
            .unwrap();

        let mut process = ProcessHandle::spawn(
            Path::new("/bin/sh"),
            vec!["-c".to_string(), "sleep 5".to_string()],
            None,
            None,
        )
        .await
        .unwrap();
        let kill = process.kill_handle().unwrap();

        let session = Session {
            id: "test".into(),
            config: cfg,
            start_time: Utc::now(),
            kill,
            events_tx: None,
            events: None,
            worker_task: std::sync::Mutex::new(None),
            stderr_task: std::sync::Mutex::new(None),
            process_group_owned: Arc::new(AtomicBool::new(false)),
            result: Arc::new(RwLock::new(None)),
            error: Arc::new(RwLock::new(None)),
            transcript: Arc::new(RwLock::new(Vec::new())),
            raw_stdout: Arc::new(RwLock::new(String::new())),
            stderr: Arc::new(RwLock::new(String::new())),
            exit_code: Arc::new(RwLock::new(Some(0))),
            invocation: InvocationMetadata::default(),
            secret_redactor: SecretRedactor::default(),
            _mcp_temp_file: None,
        };

        drop(session);

        assert!(process.try_wait().unwrap().is_none());

        process.kill().await.unwrap();
        process.wait().await.unwrap();
    }
}
