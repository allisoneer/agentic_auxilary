//! Tool implementations for orchestrator MCP server.

use crate::server::OrchestratorServer;
use crate::token_tracker::TokenTracker;
use crate::types::{
    CommandInfo, ListCommandsInput, ListCommandsOutput, ListSessionsInput, ListSessionsOutput,
    OrchestratorRunInput, OrchestratorRunOutput, PermissionReply, RespondPermissionInput,
    RespondPermissionOutput, RunStatus, SessionSummary,
};
use agentic_tools_core::{Tool, ToolContext, ToolError, ToolRegistry};
use futures::future::BoxFuture;
use opencode_rs::types::event::Event;
use opencode_rs::types::message::{CommandRequest, PromptPart, PromptRequest};
use opencode_rs::types::permission::PermissionReply as ApiPermissionReply;
use opencode_rs::types::permission::PermissionReplyRequest;
use opencode_rs::types::session::{CreateSessionRequest, SessionStatusInfo, SummarizeRequest};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OnceCell;
use tokio::task::JoinHandle;

// ============================================================================
// orchestrator_run
// ============================================================================

/// Tool for starting or resuming `OpenCode` sessions.
///
/// Handles session creation, prompt/command execution, SSE event monitoring,
/// and permission request detection. Returns when the session completes or
/// when a permission is requested.
#[derive(Clone)]
pub struct OrchestratorRunTool {
    server: Arc<OnceCell<OrchestratorServer>>,
}

impl OrchestratorRunTool {
    /// Create a new `OrchestratorRunTool` with the given server cell.
    pub fn new(server: Arc<OnceCell<OrchestratorServer>>) -> Self {
        Self { server }
    }

    /// Finalize a completed session by fetching messages and optionally triggering summarization.
    ///
    /// This is called when we detect the session is idle, either via SSE `SessionIdle` event
    /// or via polling `sessions().status()`.
    ///
    /// Uses bounded retry with backoff (0/50/100/200/400ms) if assistant text is not immediately
    /// available, handling the race condition where the session becomes idle before messages
    /// are fully persisted.
    async fn finalize_completed(
        client: &opencode_rs::Client,
        session_id: String,
        token_tracker: &TokenTracker,
        mut warnings: Vec<String>,
    ) -> Result<OrchestratorRunOutput, ToolError> {
        // Bounded backoff delays for message extraction retry (~750ms total budget)
        const BACKOFFS_MS: &[u64] = &[0, 50, 100, 200, 400];

        let mut response: Option<String> = None;

        for (attempt, &delay_ms) in BACKOFFS_MS.iter().enumerate() {
            if delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }

            let messages = client
                .messages()
                .list(&session_id)
                .await
                .map_err(|e| ToolError::Internal(format!("Failed to list messages: {e}")))?;

            response = OrchestratorServer::extract_assistant_text(&messages);

            if response.is_some() {
                if attempt > 0 {
                    tracing::debug!(
                        session_id = %session_id,
                        attempt,
                        "assistant response became available after retry"
                    );
                }
                break;
            }
        }

        if response.is_none() {
            tracing::debug!(
                session_id = %session_id,
                "no assistant response found after bounded retry"
            );
        }

        // Handle context limit summarization if needed
        if token_tracker.compaction_needed
            && let (Some(pid), Some(mid)) = (&token_tracker.provider_id, &token_tracker.model_id)
        {
            let summarize_req = SummarizeRequest {
                provider_id: pid.clone(),
                model_id: mid.clone(),
                auto: None,
            };

            match client
                .sessions()
                .summarize(&session_id, &summarize_req)
                .await
            {
                Ok(_) => {
                    tracing::info!(session_id = %session_id, "context summarization triggered");
                    warnings.push("Context limit reached; summarization triggered".into());
                }
                Err(e) => {
                    tracing::warn!(session_id = %session_id, error = %e, "summarization failed");
                    warnings.push(format!("Summarization failed: {e}"));
                }
            }
        }

        Ok(OrchestratorRunOutput {
            session_id,
            status: RunStatus::Completed,
            response,
            partial_response: None,
            permission_request_id: None,
            permission_type: None,
            permission_patterns: vec![],
            warnings,
        })
    }

    pub async fn run_impl(
        &self,
        input: OrchestratorRunInput,
    ) -> Result<OrchestratorRunOutput, ToolError> {
        // Input validation
        if input.session_id.is_none() && input.message.is_none() && input.command.is_none() {
            return Err(ToolError::InvalidInput(
                "Either session_id (to resume/check status) or message/command (to start work) is required"
                    .into(),
            ));
        }

        if input.command.is_some() && input.message.is_none() {
            return Err(ToolError::InvalidInput(
                "message is required when command is specified (becomes $ARGUMENTS for template expansion)"
                    .into(),
            ));
        }

        // Trim and validate message content
        let message = input.message.map(|m| m.trim().to_string());
        if let Some(ref m) = message
            && m.is_empty()
        {
            return Err(ToolError::InvalidInput(
                "message cannot be empty or whitespace-only".into(),
            ));
        }

        let wait_for_activity = input.wait_for_activity.unwrap_or(false);

        // Lazy initialization: spawn server on first tool call
        let server = self
            .server
            .get_or_try_init(OrchestratorServer::start_lazy)
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?;

        let client = server.client();

        tracing::debug!(
            command = ?input.command,
            message = ?message,
            session_id = ?input.session_id,
            "orchestrator_run: starting"
        );

        // 1. Resolve session: validate existing or create new
        let session_id = if let Some(sid) = input.session_id {
            // Validate session exists
            client.sessions().get(&sid).await.map_err(|e| {
                if e.is_not_found() {
                    ToolError::InvalidInput(format!(
                        "Session '{sid}' not found. Use orchestrator_list_sessions to discover sessions, \
                         or omit session_id to create a new session."
                    ))
                } else {
                    ToolError::Internal(format!("Failed to get session: {e}"))
                }
            })?;
            sid
        } else {
            // Create new session
            let session = client
                .sessions()
                .create(&CreateSessionRequest::default())
                .await
                .map_err(|e| ToolError::Internal(format!("Failed to create session: {e}")))?;
            session.id
        };

        tracing::info!(session_id = %session_id, "orchestrator_run: session resolved");

        // 2. Check if session is already idle (for resume-only case)
        let status = client
            .sessions()
            .status_for(&session_id)
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to get session status: {e}")))?;

        let is_idle = matches!(status, SessionStatusInfo::Idle);

        // 3. Check for pending permissions before doing anything else
        let pending_permissions = client
            .permissions()
            .list()
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to list permissions: {e}")))?;

        let my_permission = pending_permissions
            .into_iter()
            .find(|p| p.session_id == session_id);

        if let Some(perm) = my_permission {
            tracing::info!(
                session_id = %session_id,
                permission_type = %perm.permission,
                "orchestrator_run: pending permission found"
            );
            return Ok(OrchestratorRunOutput {
                session_id,
                status: RunStatus::PermissionRequired,
                response: None,
                partial_response: None,
                permission_request_id: Some(perm.id),
                permission_type: Some(perm.permission),
                permission_patterns: perm.patterns,
                warnings: vec![],
            });
        }

        // 4. If no message/command and session is idle, just return current state
        // Uses finalize_completed to get retry logic for message extraction
        if message.is_none() && input.command.is_none() && is_idle && !wait_for_activity {
            let token_tracker = TokenTracker::new();
            return Self::finalize_completed(client, session_id, &token_tracker, vec![]).await;
        }

        // 5. Subscribe to SSE BEFORE sending prompt/command
        let mut subscription = client
            .subscribe_session(&session_id)
            .map_err(|e| ToolError::Internal(format!("Failed to subscribe to session: {e}")))?;

        // 6. Kick off the work
        let mut command_task: Option<JoinHandle<Result<(), String>>> = None;
        let mut command_name_for_logging: Option<String> = None;

        if let Some(command) = &input.command {
            command_name_for_logging = Some(command.clone());

            let cmd_client = client.clone();
            let cmd_session_id = session_id.clone();
            let cmd_name = command.clone();
            let cmd_arguments = message.clone().unwrap_or_default();

            command_task = Some(tokio::spawn(async move {
                let req = CommandRequest {
                    command: cmd_name,
                    arguments: cmd_arguments,
                };

                cmd_client
                    .messages()
                    .command(&cmd_session_id, &req)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }));
        } else if let Some(msg) = &message {
            // Send prompt asynchronously
            let req = PromptRequest {
                parts: vec![PromptPart::Text {
                    text: msg.clone(),
                    synthetic: None,
                    ignored: None,
                    metadata: None,
                }],
                message_id: None,
                model: None,
                agent: None,
                no_reply: None,
                system: None,
                variant: None,
            };

            client
                .messages()
                .prompt_async(&session_id, &req)
                .await
                .map_err(|e| ToolError::Internal(format!("Failed to send prompt: {e}")))?;
        }

        // 7. Event loop: wait for completion or permission
        // Overall timeout to prevent infinite hangs (1 hour)
        let deadline = tokio::time::Instant::now() + Duration::from_secs(3600);
        let inactivity_timeout = Duration::from_secs(300);
        let mut last_activity_time = tokio::time::Instant::now();

        tracing::debug!(session_id = %session_id, "orchestrator_run: entering event loop");
        let mut token_tracker = TokenTracker::new();
        let mut partial_response = String::new();
        let warnings = Vec::new();

        let mut poll_interval = tokio::time::interval(Duration::from_secs(1));
        poll_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Track whether this call is dispatching new work (command or message)
        // vs just resuming/monitoring an existing session
        let dispatched_new_work = input.command.is_some() || message.is_some() || wait_for_activity;

        // Track whether we've observed the session as busy at least once.
        // This prevents completing immediately if we call run_impl on an already-idle
        // session before our new work has started processing.
        let mut observed_busy = false;

        // Track whether SSE is still active. If the stream closes, we fall back
        // to polling-only mode rather than returning an error.
        let mut sse_active = true;

        // === Post-subscribe status re-check (latency optimization) ===
        // If we're just monitoring (no new work dispatched), check if session is already idle.
        // This handles the race where session completed between our initial status check
        // and SSE subscription becoming ready.
        if !dispatched_new_work
            && let Ok(status) = client.sessions().status_for(&session_id).await
            && matches!(status, SessionStatusInfo::Idle)
        {
            tracing::debug!(
                session_id = %session_id,
                "session already idle on post-subscribe check"
            );
            return Self::finalize_completed(client, session_id, &token_tracker, warnings).await;
        }
        // If check fails or session is busy, continue to event loop

        loop {
            // Check timeout before processing
            let now = tokio::time::Instant::now();

            if now.duration_since(last_activity_time) >= inactivity_timeout {
                return Err(ToolError::Internal(format!(
                    "Session idle timeout: no activity for 5 minutes (session_id={session_id}). \
                     The session may still be running; use orchestrator_run(session_id=...) to check status."
                )));
            }

            if now >= deadline {
                return Err(ToolError::Internal(
                    "Session execution timed out after 1 hour. \
                     The session may still be running; use orchestrator_run with the session_id to check status."
                        .into(),
                ));
            }

            let command_task_active = command_task.is_some();

            tokio::select! {
                maybe_event = subscription.recv(), if sse_active => {
                    let Some(event) = maybe_event else {
                        // SSE stream closed - this can happen due to network issues,
                        // server restarts, or connection timeouts. Fall back to polling
                        // rather than failing immediately.
                        tracing::warn!(
                            session_id = %session_id,
                            "SSE stream closed unexpectedly; falling back to polling-only mode"
                        );
                        sse_active = false;
                        continue; // The poll_interval branch will now drive completion detection
                    };

                    // Track tokens (server is already initialized at this point)
                    token_tracker.observe_event(&event, |pid, mid| {
                        server.context_limit(pid, mid)
                    });

                    match event {
                        Event::PermissionAsked { properties } => {
                            tracing::info!(
                                session_id = %session_id,
                                permission_type = %properties.request.permission,
                                "orchestrator_run: permission requested"
                            );
                            return Ok(OrchestratorRunOutput {
                                session_id,
                                status: RunStatus::PermissionRequired,
                                response: None,
                                partial_response: if partial_response.is_empty() {
                                    None
                                } else {
                                    Some(partial_response)
                                },
                                permission_request_id: Some(properties.request.id),
                                permission_type: Some(properties.request.permission),
                                permission_patterns: properties.request.patterns,
                                warnings,
                            });
                        }

                        Event::MessagePartUpdated { properties } => {
                            last_activity_time = tokio::time::Instant::now();
                            // Message streaming means session is actively processing
                            observed_busy = true;
                            // Collect streaming text
                            if let Some(delta) = &properties.delta {
                                partial_response.push_str(delta);
                            }
                        }

                        Event::SessionError { properties } => {
                            let error_msg = properties
                                .error
                                .map_or_else(|| "Unknown error".to_string(), |e| format!("{e:?}"));
                            tracing::error!(
                                session_id = %session_id,
                                error = %error_msg,
                                "orchestrator_run: session error"
                            );
                            return Err(ToolError::Internal(format!("Session error: {error_msg}")));
                        }

                        Event::SessionIdle { .. } => {
                            tracing::debug!(session_id = %session_id, "received SessionIdle event");
                            return Self::finalize_completed(client, session_id, &token_tracker, warnings).await;
                        }

                        _ => {
                            // Other events - continue
                        }
                    }
                }

                _ = poll_interval.tick() => {
                    // === 1. Permission fallback (check first, permissions take priority) ===
                    let pending = match client.permissions().list().await {
                        Ok(p) => p,
                        Err(e) => {
                            // Log but continue - permission list failure shouldn't block completion detection
                            tracing::warn!(
                                session_id = %session_id,
                                error = %e,
                                "failed to list permissions during poll fallback"
                            );
                            vec![]
                        }
                    };

                    if let Some(perm) = pending.into_iter().find(|p| p.session_id == session_id) {
                        tracing::debug!(
                            session_id = %session_id,
                            permission_id = %perm.id,
                            "detected pending permission via polling fallback"
                        );
                        return Ok(OrchestratorRunOutput {
                            session_id,
                            status: RunStatus::PermissionRequired,
                            response: None,
                            partial_response: if partial_response.is_empty() {
                                None
                            } else {
                                Some(partial_response)
                            },
                            permission_request_id: Some(perm.id),
                            permission_type: Some(perm.permission),
                            permission_patterns: perm.patterns,
                            warnings,
                        });
                    }

                    // === 2. Session idle detection fallback (NEW) ===
                    // This is the key fix for race conditions. If SSE missed SessionIdle,
                    // we detect completion via polling sessions().status_for(session_id).
                    match client.sessions().status_for(&session_id).await {
                        Ok(SessionStatusInfo::Busy | SessionStatusInfo::Retry { .. }) => {
                            last_activity_time = tokio::time::Instant::now();
                            observed_busy = true;
                            tracing::trace!(
                                session_id = %session_id,
                                "our session is busy/retry, waiting"
                            );
                        }
                        Ok(SessionStatusInfo::Idle) => {
                            if !dispatched_new_work || observed_busy {
                                // Session is idle AND either:
                                // - We didn't dispatch new work (just monitoring), OR
                                // - We did dispatch work and have seen it become busy at least once
                                //
                                // This guards against completing before our work starts processing.
                                tracing::debug!(
                                    session_id = %session_id,
                                    dispatched_new_work = dispatched_new_work,
                                    observed_busy = observed_busy,
                                    "detected session idle via polling fallback"
                                );
                                return Self::finalize_completed(client, session_id, &token_tracker, warnings).await;
                            }

                            // Session is idle but we dispatched work and haven't seen busy yet.
                            // This likely means our work hasn't started processing.
                            // Wait for next poll tick.
                            tracing::trace!(
                                session_id = %session_id,
                                "session idle but work may not have started yet, waiting"
                            );
                        }
                        Err(e) => {
                            // Log but continue - status check failure shouldn't block the loop
                            tracing::warn!(
                                session_id = %session_id,
                                error = %e,
                                "failed to get session status during poll fallback"
                            );
                        }
                    }
                }

                cmd_result = async {
                    match command_task.as_mut() {
                        Some(handle) => Some(handle.await),
                        None => {
                            std::future::pending::<
                                Option<Result<Result<(), String>, tokio::task::JoinError>>,
                            >()
                            .await
                        }
                    }
                }, if command_task_active => {
                    match cmd_result {
                        Some(Ok(Ok(()))) => {
                            tracing::debug!(
                                session_id = %session_id,
                                command = ?command_name_for_logging,
                                "orchestrator_run: command dispatch completed successfully"
                            );
                            command_task = None;
                        }
                        Some(Ok(Err(e))) => {
                            tracing::error!(
                                session_id = %session_id,
                                command = ?command_name_for_logging,
                                error = %e,
                                "orchestrator_run: command dispatch failed"
                            );
                            return Err(ToolError::Internal(format!(
                                "Failed to execute command '{}': {e}",
                                command_name_for_logging.as_deref().unwrap_or("unknown")
                            )));
                        }
                        Some(Err(join_err)) => {
                            tracing::error!(
                                session_id = %session_id,
                                command = ?command_name_for_logging,
                                error = %join_err,
                                "orchestrator_run: command task panicked"
                            );
                            return Err(ToolError::Internal(format!("Command task panicked: {join_err}")));
                        }
                        None => {
                            unreachable!("command_task_active guard should prevent None");
                        }
                    }
                }
            }
        }
    }
}

impl Tool for OrchestratorRunTool {
    type Input = OrchestratorRunInput;
    type Output = OrchestratorRunOutput;
    const NAME: &'static str = "orchestrator_run";
    const DESCRIPTION: &'static str = r#"Start or resume an OpenCode session. Optionally run a named command or send a raw prompt.

Returns when:
- status=completed: Session finished executing. Response contains final assistant output.
- status=permission_required: Session needs permission approval. Call orchestrator_respond_permission to continue.

Parameters:
- session_id: Existing session to resume (omit to create new)
- command: OpenCode command name (e.g., "research", "implement_plan")
- message: Prompt text or $ARGUMENTS for command template

Examples:
- New session with prompt: orchestrator_run(message="explain this code")
- New session with command: orchestrator_run(command="research", message="caching strategies")
- Resume session: orchestrator_run(session_id="...", message="continue")
- Check status: orchestrator_run(session_id="...")"#;

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let this = self.clone();
        Box::pin(async move { this.run_impl(input).await })
    }
}

// ============================================================================
// orchestrator_list_sessions
// ============================================================================

/// Tool for listing available `OpenCode` sessions in the current directory.
#[derive(Clone)]
pub struct ListSessionsTool {
    server: Arc<OnceCell<OrchestratorServer>>,
}

impl ListSessionsTool {
    /// Create a new `ListSessionsTool` with the given server cell.
    pub fn new(server: Arc<OnceCell<OrchestratorServer>>) -> Self {
        Self { server }
    }
}

impl Tool for ListSessionsTool {
    type Input = ListSessionsInput;
    type Output = ListSessionsOutput;
    const NAME: &'static str = "orchestrator_list_sessions";
    const DESCRIPTION: &'static str =
        "List available OpenCode sessions in the current directory context.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let server_cell = Arc::clone(&self.server);
        Box::pin(async move {
            let server = server_cell
                .get_or_try_init(OrchestratorServer::start_lazy)
                .await
                .map_err(|e| ToolError::Internal(e.to_string()))?;

            let sessions = server
                .client()
                .sessions()
                .list()
                .await
                .map_err(|e| ToolError::Internal(format!("Failed to list sessions: {e}")))?;

            let limit = input.limit.unwrap_or(20);
            let summaries: Vec<SessionSummary> = sessions
                .into_iter()
                .take(limit)
                .map(|s| SessionSummary {
                    id: s.id,
                    title: s.title,
                    updated: s.time.as_ref().map(|t| t.updated),
                })
                .collect();

            Ok(ListSessionsOutput {
                sessions: summaries,
            })
        })
    }
}

// ============================================================================
// orchestrator_list_commands
// ============================================================================

/// Tool for listing available `OpenCode` commands that can be executed.
#[derive(Clone)]
pub struct ListCommandsTool {
    server: Arc<OnceCell<OrchestratorServer>>,
}

impl ListCommandsTool {
    /// Create a new `ListCommandsTool` with the given server cell.
    pub fn new(server: Arc<OnceCell<OrchestratorServer>>) -> Self {
        Self { server }
    }
}

impl Tool for ListCommandsTool {
    type Input = ListCommandsInput;
    type Output = ListCommandsOutput;
    const NAME: &'static str = "orchestrator_list_commands";
    const DESCRIPTION: &'static str =
        "List available OpenCode commands that can be used with orchestrator_run.";

    fn call(
        &self,
        _input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let server_cell = Arc::clone(&self.server);
        Box::pin(async move {
            let server = server_cell
                .get_or_try_init(OrchestratorServer::start_lazy)
                .await
                .map_err(|e| ToolError::Internal(e.to_string()))?;

            let commands = server
                .client()
                .tools()
                .commands()
                .await
                .map_err(|e| ToolError::Internal(format!("Failed to list commands: {e}")))?;

            let command_infos: Vec<CommandInfo> = commands
                .into_iter()
                .map(|c| CommandInfo {
                    name: c.name,
                    description: c.description,
                })
                .collect();

            Ok(ListCommandsOutput {
                commands: command_infos,
            })
        })
    }
}

// ============================================================================
// orchestrator_respond_permission
// ============================================================================

/// Tool for responding to permission requests from `OpenCode` sessions.
///
/// After sending the reply, continues monitoring the session and returns
/// when the session completes or another permission is requested.
#[derive(Clone)]
pub struct RespondPermissionTool {
    server: Arc<OnceCell<OrchestratorServer>>,
}

impl RespondPermissionTool {
    /// Create a new `RespondPermissionTool` with the given server cell.
    pub fn new(server: Arc<OnceCell<OrchestratorServer>>) -> Self {
        Self { server }
    }
}

impl Tool for RespondPermissionTool {
    type Input = RespondPermissionInput;
    type Output = RespondPermissionOutput;
    const NAME: &'static str = "orchestrator_respond_permission";
    const DESCRIPTION: &'static str = r#"Respond to a permission request from an OpenCode session.

After responding, continues monitoring the session and returns when complete or when another permission is required.

Parameters:
- session_id: Session with pending permission
- reply: "once" (allow this request), "always" (allow for matching patterns), or "reject" (deny)
- message: Optional message to include with reply"#;

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let server_cell = Arc::clone(&self.server);
        Box::pin(async move {
            let server = server_cell
                .get_or_try_init(OrchestratorServer::start_lazy)
                .await
                .map_err(|e| ToolError::Internal(e.to_string()))?;

            let client = server.client();

            // Find the pending permission for this session
            let pending = client
                .permissions()
                .list()
                .await
                .map_err(|e| ToolError::Internal(format!("Failed to list permissions: {e}")))?;

            let perm = pending
                .into_iter()
                .find(|p| p.session_id == input.session_id)
                .ok_or_else(|| {
                    ToolError::InvalidInput(format!(
                        "No pending permission found for session '{}'. \
                         The permission may have already been responded to.",
                        input.session_id
                    ))
                })?;

            // Track if this is a rejection for post-processing
            let is_reject = matches!(input.reply, PermissionReply::Reject);

            // Capture permission details for warning message
            let permission_type = perm.permission.clone();
            let permission_patterns = perm.patterns.clone();

            // Capture baseline assistant text BEFORE sending reject
            // This lets us detect stale text after rejection
            let mut pre_warnings: Vec<String> = Vec::new();
            let baseline = if is_reject {
                match client.messages().list(&input.session_id).await {
                    Ok(msgs) => OrchestratorServer::extract_assistant_text(&msgs),
                    Err(e) => {
                        pre_warnings.push(format!("Failed to fetch baseline messages: {e}"));
                        None
                    }
                }
            } else {
                None
            };

            // Convert our reply type to API type
            let api_reply = match input.reply {
                PermissionReply::Once => ApiPermissionReply::Once,
                PermissionReply::Always => ApiPermissionReply::Always,
                PermissionReply::Reject => ApiPermissionReply::Reject,
            };

            // Send the reply
            client
                .permissions()
                .reply(
                    &perm.id,
                    &PermissionReplyRequest {
                        reply: api_reply,
                        message: input.message,
                    },
                )
                .await
                .map_err(|e| ToolError::Internal(format!("Failed to reply to permission: {e}")))?;

            // Now continue monitoring the session using orchestrator_run logic
            let run_tool = OrchestratorRunTool::new(Arc::clone(&server_cell));
            let wait_for_activity = (!is_reject).then_some(true);
            let mut out = run_tool
                .run_impl(OrchestratorRunInput {
                    session_id: Some(input.session_id),
                    command: None,
                    message: None,
                    wait_for_activity,
                })
                .await?;

            // Merge pre-warnings
            out.warnings.extend(pre_warnings);

            // Apply rejection-aware output mutation
            if is_reject && matches!(out.status, RunStatus::Completed) {
                let final_resp = out.response.as_deref();
                let baseline_resp = baseline.as_deref();

                // If response unchanged or None, it's stale pre-rejection text
                if final_resp.is_none() || final_resp == baseline_resp {
                    out.response = None;
                    let patterns_str = if permission_patterns.is_empty() {
                        "(none)".to_string()
                    } else {
                        permission_patterns.join(", ")
                    };
                    out.warnings.push(format!(
                        "Permission rejected for '{permission_type}'. Patterns: {patterns_str}. \
                         Session stopped without generating a new assistant response."
                    ));
                    tracing::debug!(
                        permission_type = %permission_type,
                        "rejection override applied: response set to None"
                    );
                }
            }

            Ok(out)
        })
    }
}

// ============================================================================
// Registry builder
// ============================================================================

/// Build the tool registry with all orchestrator tools.
///
/// The server cell is lazily initialized on first tool call.
pub fn build_registry(server: &Arc<OnceCell<OrchestratorServer>>) -> ToolRegistry {
    ToolRegistry::builder()
        .register::<OrchestratorRunTool, ()>(OrchestratorRunTool::new(Arc::clone(server)))
        .register::<ListSessionsTool, ()>(ListSessionsTool::new(Arc::clone(server)))
        .register::<ListCommandsTool, ()>(ListCommandsTool::new(Arc::clone(server)))
        .register::<RespondPermissionTool, ()>(RespondPermissionTool::new(Arc::clone(server)))
        .finish()
}
