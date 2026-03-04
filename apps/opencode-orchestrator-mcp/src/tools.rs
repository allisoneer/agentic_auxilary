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
use opencode_rs::types::session::{CreateSessionRequest, SummarizeRequest};
use std::sync::Arc;
use std::time::Duration;
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
    server: Arc<OrchestratorServer>,
}

impl OrchestratorRunTool {
    /// Create a new `OrchestratorRunTool` with the given server.
    pub fn new(server: Arc<OrchestratorServer>) -> Self {
        Self { server }
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

        let client = self.server.client();

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
            .status()
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to get session status: {e}")))?;

        let is_idle = !status.busy;

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
        if message.is_none() && input.command.is_none() && is_idle {
            // Fetch and return last assistant message
            let messages = client
                .messages()
                .list(&session_id)
                .await
                .map_err(|e| ToolError::Internal(format!("Failed to list messages: {e}")))?;

            let response = OrchestratorServer::extract_assistant_text(&messages);

            return Ok(OrchestratorRunOutput {
                session_id,
                status: RunStatus::Completed,
                response,
                partial_response: None,
                permission_request_id: None,
                permission_type: None,
                permission_patterns: vec![],
                warnings: vec![],
            });
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

        tracing::debug!(session_id = %session_id, "orchestrator_run: entering event loop");
        let mut token_tracker = TokenTracker::new();
        let mut partial_response = String::new();
        let mut warnings = Vec::new();

        let mut poll_interval = tokio::time::interval(Duration::from_secs(1));
        poll_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            // Check timeout before processing
            if tokio::time::Instant::now() >= deadline {
                return Err(ToolError::Internal(
                    "Session execution timed out after 1 hour. \
                     The session may still be running; use orchestrator_run with the session_id to check status."
                        .into(),
                ));
            }

            let command_task_active = command_task.is_some();

            tokio::select! {
                maybe_event = subscription.recv() => {
                    let Some(event) = maybe_event else {
                        // SSE stream closed unexpectedly
                        return Err(ToolError::Internal("SSE stream closed unexpectedly".into()));
                    };

                    // Track tokens
                    token_tracker.observe_event(&event, |pid, mid| {
                        self.server.context_limit(pid, mid)
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
                            tracing::info!(session_id = %session_id, "orchestrator_run: session idle, completing");
                            // Session completed - reconcile via HTTP
                            let messages = client
                                .messages()
                                .list(&session_id)
                                .await
                                .map_err(|e| ToolError::Internal(format!("Failed to list messages: {e}")))?;

                            let response = OrchestratorServer::extract_assistant_text(&messages);

                            // Trigger summarization if needed
                            if token_tracker.compaction_needed
                                && let (Some(pid), Some(mid)) =
                                    (&token_tracker.provider_id, &token_tracker.model_id)
                            {
                                let summarize_req = SummarizeRequest {
                                    provider_id: pid.clone(),
                                    model_id: mid.clone(),
                                    auto: None,
                                };

                                match client.sessions().summarize(&session_id, &summarize_req).await {
                                    Ok(_) => {
                                        warnings.push("Context limit reached; summarization triggered".into());
                                    }
                                    Err(e) => {
                                        warnings.push(format!("Summarization failed: {e}"));
                                    }
                                }
                            }

                            return Ok(OrchestratorRunOutput {
                                session_id,
                                status: RunStatus::Completed,
                                response,
                                partial_response: None,
                                permission_request_id: None,
                                permission_type: None,
                                permission_patterns: vec![],
                                warnings,
                            });
                        }

                        _ => {
                            // Other events - continue
                        }
                    }
                }

                _ = poll_interval.tick() => {
                    // Fallback: poll for permissions in case SSE missed it
                    let pending = client
                        .permissions()
                        .list()
                        .await
                        .unwrap_or_default();

                    if let Some(perm) = pending.into_iter().find(|p| p.session_id == session_id) {
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
    server: Arc<OrchestratorServer>,
}

impl ListSessionsTool {
    /// Create a new `ListSessionsTool` with the given server.
    pub fn new(server: Arc<OrchestratorServer>) -> Self {
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
        let server = Arc::clone(&self.server);
        Box::pin(async move {
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
    server: Arc<OrchestratorServer>,
}

impl ListCommandsTool {
    /// Create a new `ListCommandsTool` with the given server.
    pub fn new(server: Arc<OrchestratorServer>) -> Self {
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
        let server = Arc::clone(&self.server);
        Box::pin(async move {
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
    server: Arc<OrchestratorServer>,
}

impl RespondPermissionTool {
    /// Create a new `RespondPermissionTool` with the given server.
    pub fn new(server: Arc<OrchestratorServer>) -> Self {
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
        let server = Arc::clone(&self.server);
        Box::pin(async move {
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
            let run_tool = OrchestratorRunTool::new(server);
            run_tool
                .run_impl(OrchestratorRunInput {
                    session_id: Some(input.session_id),
                    command: None,
                    message: None,
                })
                .await
        })
    }
}

// ============================================================================
// Registry builder
// ============================================================================

/// Build the tool registry with all orchestrator tools.
pub fn build_registry(server: &Arc<OrchestratorServer>) -> ToolRegistry {
    ToolRegistry::builder()
        .register::<OrchestratorRunTool, ()>(OrchestratorRunTool::new(Arc::clone(server)))
        .register::<ListSessionsTool, ()>(ListSessionsTool::new(Arc::clone(server)))
        .register::<ListCommandsTool, ()>(ListCommandsTool::new(Arc::clone(server)))
        .register::<RespondPermissionTool, ()>(RespondPermissionTool::new(Arc::clone(server)))
        .finish()
}
