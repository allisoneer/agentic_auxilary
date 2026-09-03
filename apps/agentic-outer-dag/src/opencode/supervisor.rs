use crate::state::CleanupFailure;
use crate::state::InvocationFailure;
use crate::state::LocalTaskDisposition;
use crate::state::OpenCodeDiagnostics;
use crate::state::OpenCodeToolErrorDiagnostics;
use crate::state::ServerAbortDisposition;
use crate::state::TaskDisposition;
use anyhow::Context;
use anyhow::Result;
use opencode_rs::Client;
use opencode_rs::OpencodeError;
use opencode_rs::server::ManagedServer;
use opencode_rs::server::ServerOptions;
use opencode_rs::types::event::Event;
use opencode_rs::types::message::CommandRequest;
use opencode_rs::types::message::Message;
use opencode_rs::types::message::Part;
use opencode_rs::types::message::ToolState;
use opencode_rs::types::permission::PermissionReply;
use opencode_rs::types::permission::PermissionReplyRequest;
use opencode_rs::types::question::QuestionReply;
use opencode_rs::types::session::CreateSessionRequest;
use opencode_rs::types::session::SessionStatusInfo;
use opencode_rs::version;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;

const IDLE_GRACE: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const SSE_READINESS_TIMEOUT: Duration = Duration::from_secs(5);
const TRANSCRIPT_SETTLING_RETRY_BACKOFFS: [Duration; 4] = [
    Duration::from_millis(50),
    Duration::from_millis(100),
    Duration::from_millis(200),
    Duration::from_millis(400),
];
const NESTED_GUARD_NEEDLE: &str = "OPENCODE_ORCHESTRATOR_MANAGED";
const TOOL_ERROR_SUMMARY_LIMIT: usize = 240;

static COMMAND_MESSAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct OpenCodeSupervisor {
    _managed_server: ManagedServer,
    client: Client,
    _directory: PathBuf,
    timeouts: OpenCodeSupervisorTimeouts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenCodeSupervisorTimeouts {
    pub session_deadline: Duration,
    pub inactivity_timeout: Duration,
}

impl OpenCodeSupervisorTimeouts {
    pub fn from_settings(settings: &crate::state::Settings) -> Self {
        Self {
            session_deadline: Duration::from_secs(settings.opencode_session_deadline_seconds),
            inactivity_timeout: Duration::from_secs(settings.opencode_inactivity_timeout_seconds),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisedOutcome {
    Completed {
        session_id: String,
        diagnostics: OpenCodeDiagnostics,
        literal_post_attempts: u32,
        task_disposition: TaskDisposition,
    },
    AcceptedButNotStarted {
        session_id: String,
        diagnostics: OpenCodeDiagnostics,
        literal_post_attempts: u32,
        task_disposition: TaskDisposition,
    },
    PermissionRequired {
        session_id: String,
        request_id: String,
        permission_type: String,
        literal_post_attempts: u32,
    },
    QuestionRequired {
        session_id: String,
        request_id: String,
        prompt: String,
        literal_post_attempts: u32,
    },
    Failed {
        session_id: Option<String>,
        error: String,
        diagnostics: Option<OpenCodeDiagnostics>,
        literal_post_attempts: u32,
        failure: InvocationFailure,
        task_disposition: TaskDisposition,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionSelection {
    Reuse(String),
    Fresh,
}

pub enum PreparedCommandOutcome {
    Prepared(PreparedCommandInvocation),
    Interrupted(SupervisedOutcome),
}

pub struct PreparedCommandInvocation {
    session_id: String,
    command_name: String,
    message: String,
    transcript_window: TranscriptWindow,
    subscription: opencode_rs::sse::SseSubscription<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisionEvent {
    LiteralPostAttempt {
        total: u32,
    },
    AssistantStarted {
        message_id: String,
    },
    Paused {
        kind: crate::state::InterruptionKind,
        request_id: String,
    },
    Resumed {
        assistant_started: bool,
    },
}

#[derive(Debug, Clone)]
pub struct InvocationOwnerContext {
    pub run_id: String,
    pub invocation_id: String,
}

impl PreparedCommandInvocation {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn command_name(&self) -> &str {
        &self.command_name
    }

    pub fn command_message_id(&self) -> &str {
        &self.transcript_window.command_message_id
    }
}

#[derive(Debug, Clone)]
struct TranscriptWindow {
    command_message_id: String,
    baseline_tail_message_id: Option<String>,
}

#[derive(Debug, Clone)]
struct TranscriptAnalysis {
    has_assistant_message: bool,
    final_assistant_message_id: Option<String>,
    final_finish_reason: Option<String>,
    guard_detected: bool,
    final_tool_error: Option<OpenCodeToolErrorDiagnostics>,
    unresolved_tool_calls: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdleGateDecision {
    Finalize,
    WaitForGrace,
    IgnoreUntilDispatchConfirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionStatusObservation {
    Absent,
    Idle,
    BusyLike,
}

fn observe_session_status(
    statuses: &HashMap<String, SessionStatusInfo>,
    session_id: &str,
) -> SessionStatusObservation {
    match statuses.get(session_id) {
        None => SessionStatusObservation::Absent,
        Some(SessionStatusInfo::Idle) => SessionStatusObservation::Idle,
        Some(
            SessionStatusInfo::Busy | SessionStatusInfo::Retry { .. } | SessionStatusInfo::Unknown,
        ) => SessionStatusObservation::BusyLike,
    }
}

fn event_session_matches(event_session_id: Option<&str>, session_id: &str) -> bool {
    event_session_id.is_none_or(|event_session_id| event_session_id == session_id)
}

impl TranscriptAnalysis {
    fn diagnostics(
        &self,
        command_message_id: &str,
        literal_post_attempts: u32,
    ) -> OpenCodeDiagnostics {
        OpenCodeDiagnostics {
            checked_at: chrono::Utc::now().to_rfc3339(),
            literal_post_attempts,
            command_message_id: Some(command_message_id.to_string()),
            final_assistant_message_id: self.final_assistant_message_id.clone(),
            final_finish_reason: self.final_finish_reason.clone(),
            guard_detected: self.guard_detected,
            final_tool_error: self.final_tool_error.clone(),
            command_transport_error: None,
        }
    }
}

fn idle_gate_decision(
    observed_busy: bool,
    idle_grace_deadline: Option<tokio::time::Instant>,
    now: tokio::time::Instant,
) -> IdleGateDecision {
    if observed_busy {
        return IdleGateDecision::Finalize;
    }

    match idle_grace_deadline {
        Some(deadline) if now >= deadline => IdleGateDecision::Finalize,
        Some(_) => IdleGateDecision::WaitForGrace,
        None => IdleGateDecision::IgnoreUntilDispatchConfirmed,
    }
}

fn suspend_supervision_clocks(
    deadline: &mut tokio::time::Instant,
    last_activity: &mut tokio::time::Instant,
    waited: Duration,
) {
    *deadline += waited;
    *last_activity += waited;
}

fn transcript_indicates_dispatch(
    messages: &[Message],
    transcript_window: &TranscriptWindow,
) -> bool {
    if messages
        .iter()
        .any(|message| message.id() == transcript_window.command_message_id)
    {
        return true;
    }

    if let Some(baseline) = transcript_window.baseline_tail_message_id.as_ref() {
        return messages
            .iter()
            .position(|message| message.id() == baseline)
            .is_some_and(|index| index + 1 < messages.len());
    }

    !messages.is_empty()
}

#[derive(Debug)]
enum CompletionValidation {
    Passed(OpenCodeDiagnostics),
    AcceptedButNotStarted(OpenCodeDiagnostics),
    Failed {
        error: String,
        diagnostics: Option<OpenCodeDiagnostics>,
    },
}

type CommandTask = tokio::task::JoinHandle<Result<(), OpencodeError>>;

struct TerminalFailure {
    error: String,
    diagnostics: Option<OpenCodeDiagnostics>,
    literal_post_attempts: u32,
    failure: InvocationFailure,
}

impl OpenCodeSupervisor {
    pub async fn start(directory: &Path, timeouts: OpenCodeSupervisorTimeouts) -> Result<Self> {
        let cwd = directory.canonicalize().with_context(|| {
            format!(
                "failed to canonicalize OpenCode directory {}",
                directory.display()
            )
        })?;
        let launcher_config = resolve_launcher_config(&cwd)
            .context("failed to resolve OpenCode launcher configuration")?;

        tracing::info!(
            binary = %launcher_config.binary,
            launcher_args = ?launcher_config.launcher_args,
            expected_version = %version::PINNED_OPENCODE_VERSION,
            "starting app-local opencode serve"
        );

        let managed = ManagedServer::start(
            ServerOptions::default()
                .binary(&launcher_config.binary)
                .launcher_args(launcher_config.launcher_args)
                .inject_orchestrator_managed_env(false)
                .directory(cwd.clone()),
        )
        .await
        .context("failed to start embedded opencode serve")?;

        let base_url = managed.url().to_string().trim_end_matches('/').to_string();
        let client = Client::builder()
            .base_url(&base_url)
            .directory(cwd.to_string_lossy().to_string())
            .build()
            .context("failed to build opencode client")?;

        let health = client
            .misc()
            .health()
            .await
            .context("failed to fetch /global/health for version validation")?;
        version::validate_exact_version(health.version.as_deref()).with_context(|| {
            format!(
                "embedded OpenCode server did not match pinned stable v{} (binary={})",
                version::PINNED_OPENCODE_VERSION,
                launcher_config.binary
            )
        })?;

        Ok(Self {
            _managed_server: managed,
            client,
            _directory: cwd,
            timeouts,
        })
    }

    pub async fn ensure_commands_present(&self, required: &[&str]) -> Result<()> {
        let commands = self
            .client
            .tools()
            .commands()
            .await
            .context("failed to list OpenCode commands")?;
        for required_name in required {
            if commands.iter().all(|command| {
                command.name != *required_name
                    && command.name.trim_start_matches('/') != required_name.trim_start_matches('/')
            }) {
                anyhow::bail!("required OpenCode command not found: {required_name}");
            }
        }
        Ok(())
    }

    pub async fn prepare_command(
        &self,
        session_selection: SessionSelection,
        command_name: &str,
        message: Option<&str>,
    ) -> Result<PreparedCommandOutcome> {
        let session_id = match session_selection {
            SessionSelection::Reuse(session_id) => {
                self.client
                    .sessions()
                    .get(&session_id)
                    .await
                    .with_context(|| format!("failed to load session {session_id}"))?;
                session_id
            }
            SessionSelection::Fresh => {
                self.client
                    .sessions()
                    .create(&CreateSessionRequest::default())
                    .await
                    .context("failed to create OpenCode session")?
                    .id
            }
        };

        if let Some(outcome) = self.preflight_pending_interruptions(&session_id, 0).await? {
            return Ok(PreparedCommandOutcome::Interrupted(outcome));
        }

        let subscription = self
            .client
            .subscribe_session(&session_id)
            .context("failed to subscribe to session events")?;
        let transcript_window = TranscriptWindow {
            command_message_id: generate_command_message_id(),
            baseline_tail_message_id: self.fetch_transcript_tail_id(&session_id).await?,
        };

        Ok(PreparedCommandOutcome::Prepared(
            PreparedCommandInvocation {
                session_id,
                command_name: command_name.to_string(),
                message: message.unwrap_or_default().to_string(),
                transcript_window,
                subscription,
            },
        ))
    }

    #[cfg(test)]
    async fn run_command_supervised(
        &self,
        existing_session_id: Option<&str>,
        command_name: &str,
        message: Option<&str>,
    ) -> Result<SupervisedOutcome> {
        let selection = existing_session_id.map_or(SessionSelection::Fresh, |session_id| {
            SessionSelection::Reuse(session_id.to_string())
        });
        match self
            .prepare_command(selection, command_name, message)
            .await?
        {
            PreparedCommandOutcome::Prepared(prepared) => {
                self.run_prepared_command(prepared, None, None, |_| Ok(()))
                    .await
            }
            PreparedCommandOutcome::Interrupted(outcome) => Ok(outcome),
        }
    }

    pub async fn run_prepared_command<F>(
        &self,
        prepared: PreparedCommandInvocation,
        owner: Option<&crate::owner::OwnerRuntime>,
        owner_context: Option<&InvocationOwnerContext>,
        mut on_event: F,
    ) -> Result<SupervisedOutcome>
    where
        F: FnMut(SupervisionEvent) -> Result<()>,
    {
        let PreparedCommandInvocation {
            session_id,
            command_name,
            message: dispatch_message,
            transcript_window,
            mut subscription,
        } = prepared;

        tokio::time::timeout(SSE_READINESS_TIMEOUT, subscription.wait_ready())
            .await
            .context(
                "timed out waiting for SSE readiness before initial OpenCode command dispatch",
            )?
            .context(
                "SSE subscription closed before initial OpenCode command dispatch readiness",
            )?;

        let cmd_client = self.client.clone();
        let dispatch_session_id = session_id.clone();
        let dispatch_command = command_name.clone();
        let dispatch_message_id = transcript_window.command_message_id.clone();
        let literal_post_attempts = Arc::new(AtomicU32::new(0));
        let task_literal_post_attempts = Arc::clone(&literal_post_attempts);
        let (attempt_tx, mut attempt_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut command_task = Some(tokio::spawn(async move {
            let request = CommandRequest {
                command: dispatch_command,
                arguments: dispatch_message,
                message_id: Some(dispatch_message_id),
            };
            cmd_client
                .messages()
                .command_with_attempt_observer(&dispatch_session_id, &request, move |_| {
                    let total = task_literal_post_attempts.fetch_add(1, Ordering::Relaxed) + 1;
                    let _ = attempt_tx.send(total);
                })
                .await
                .map(|_| ())
        }));
        let mut observed_local = LocalTaskDisposition::Spawned;

        let mut deadline = tokio::time::Instant::now() + self.timeouts.session_deadline;
        let mut last_activity = tokio::time::Instant::now();
        let mut poll_interval = tokio::time::interval(POLL_INTERVAL);
        poll_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut observed_busy = false;
        let mut assistant_started = false;
        let mut correlated_assistant_ids = HashSet::new();
        let mut handled_interruptions = HashSet::new();
        let mut idle_grace_deadline: Option<tokio::time::Instant> = None;
        let mut awaiting_idle_grace = false;
        let mut sse_active = true;
        let mut command_transport_error: Option<String> = None;

        macro_rules! terminal_runtime_error {
            ($failure:expr, $error:expr) => {{
                let error = $error;
                return Ok(self
                    .terminal_failure(
                        session_id.clone(),
                        TerminalFailure {
                            error: error.to_string(),
                            diagnostics: None,
                            literal_post_attempts: literal_post_attempts.load(Ordering::Relaxed),
                            failure: $failure,
                        },
                        &mut command_task,
                        observed_local,
                    )
                    .await);
            }};
        }

        macro_rules! emit_event {
            ($event:expr) => {
                if let Err(error) = on_event($event) {
                    terminal_runtime_error!(InvocationFailure::Persistence, error);
                }
            };
        }

        loop {
            let now = tokio::time::Instant::now();
            if now.duration_since(last_activity) >= self.timeouts.inactivity_timeout {
                let error = format!(
                    "session idle timeout after {}",
                    describe_duration(self.timeouts.inactivity_timeout)
                );
                return Ok(self
                    .terminal_failure(
                        session_id,
                        TerminalFailure {
                            error,
                            diagnostics: None,
                            literal_post_attempts: literal_post_attempts.load(Ordering::Relaxed),
                            failure: InvocationFailure::InactivityTimeout,
                        },
                        &mut command_task,
                        observed_local,
                    )
                    .await);
            }
            if now >= deadline {
                let error = format!(
                    "session execution timed out after {}",
                    describe_duration(self.timeouts.session_deadline)
                );
                return Ok(self
                    .terminal_failure(
                        session_id,
                        TerminalFailure {
                            error,
                            diagnostics: None,
                            literal_post_attempts: literal_post_attempts.load(Ordering::Relaxed),
                            failure: InvocationFailure::SessionDeadline,
                        },
                        &mut command_task,
                        observed_local,
                    )
                    .await);
            }

            tokio::select! {
                Some(total) = attempt_rx.recv() => {
                    emit_event!(SupervisionEvent::LiteralPostAttempt { total });
                }
                maybe_event = subscription.recv(), if sse_active => {
                    let Some(event) = maybe_event else {
                        sse_active = false;
                        continue;
                    };

                    match event {
                        Event::PermissionAsked { properties } => {
                            if properties.request.session_id != session_id {
                                continue;
                            }
                            if let (Some(owner), Some(owner_context)) = (owner, owner_context) {
                                let request_id = properties.request.id;
                                let interruption_key = (
                                    crate::state::InterruptionKind::Permission,
                                    request_id.clone(),
                                );
                                if handled_interruptions.contains(&interruption_key) {
                                    continue;
                                }
                                let correlation = crate::owner::InterruptionCorrelation {
                                    run_id: owner_context.run_id.clone(),
                                    invocation_id: owner_context.invocation_id.clone(),
                                    session_id: session_id.clone(),
                                    command_message_id: transcript_window.command_message_id.clone(),
                                    kind: crate::state::InterruptionKind::Permission,
                                    request_id: request_id.clone(),
                                };
                                emit_event!(SupervisionEvent::Paused {
                                    kind: crate::state::InterruptionKind::Permission,
                                    request_id: request_id.clone(),
                                });
                                let waited = match self
                                    .continue_permission_with_owner(
                                        owner,
                                        &correlation,
                                        &session_id,
                                        &request_id,
                                    )
                                    .await
                                {
                                    Ok(waited) => waited,
                                    Err(error) => {
                                        terminal_runtime_error!(InvocationFailure::OwnerIpc, error);
                                    }
                                };
                                handled_interruptions.insert(interruption_key);
                                suspend_supervision_clocks(
                                    &mut deadline,
                                    &mut last_activity,
                                    waited,
                                );
                                emit_event!(SupervisionEvent::Resumed { assistant_started });
                                continue;
                            }
                            return Ok(SupervisedOutcome::PermissionRequired {
                                session_id,
                                request_id: properties.request.id,
                                permission_type: properties.request.permission,
                                literal_post_attempts: literal_post_attempts.load(Ordering::Relaxed),
                            });
                        }
                        Event::QuestionAsked { properties } => {
                            if properties.request.session_id != session_id {
                                continue;
                            }
                            if let (Some(owner), Some(owner_context)) = (owner, owner_context) {
                                let request_id = properties.request.id;
                                let interruption_key = (
                                    crate::state::InterruptionKind::Question,
                                    request_id.clone(),
                                );
                                if handled_interruptions.contains(&interruption_key) {
                                    continue;
                                }
                                let correlation = crate::owner::InterruptionCorrelation {
                                    run_id: owner_context.run_id.clone(),
                                    invocation_id: owner_context.invocation_id.clone(),
                                    session_id: session_id.clone(),
                                    command_message_id: transcript_window.command_message_id.clone(),
                                    kind: crate::state::InterruptionKind::Question,
                                    request_id: request_id.clone(),
                                };
                                emit_event!(SupervisionEvent::Paused {
                                    kind: crate::state::InterruptionKind::Question,
                                    request_id: request_id.clone(),
                                });
                                let waited = match self
                                    .continue_question_with_owner(owner, &correlation, &request_id)
                                    .await
                                {
                                    Ok(waited) => waited,
                                    Err(error) => {
                                        terminal_runtime_error!(InvocationFailure::OwnerIpc, error);
                                    }
                                };
                                handled_interruptions.insert(interruption_key);
                                suspend_supervision_clocks(
                                    &mut deadline,
                                    &mut last_activity,
                                    waited,
                                );
                                emit_event!(SupervisionEvent::Resumed { assistant_started });
                                continue;
                            }
                            let prompt = properties
                                .request
                                .questions
                                .first()
                                .map(|question| question.question.clone())
                                .unwrap_or_default();
                            return Ok(SupervisedOutcome::QuestionRequired {
                                session_id,
                                request_id: properties.request.id,
                                prompt,
                                literal_post_attempts: literal_post_attempts.load(Ordering::Relaxed),
                            });
                        }
                        Event::MessageUpdated { properties } => {
                            let info = &properties.info;
                            if event_session_matches(info.session_id.as_deref(), &session_id)
                                && info.role == "assistant"
                                && info.parent_id.as_deref()
                                    == Some(transcript_window.command_message_id.as_str())
                            {
                                if !assistant_started {
                                    emit_event!(SupervisionEvent::AssistantStarted {
                                        message_id: info.id.clone(),
                                    });
                                }
                                correlated_assistant_ids.insert(info.id.clone());
                                assistant_started = true;
                                observed_busy = true;
                                last_activity = tokio::time::Instant::now();
                                awaiting_idle_grace = false;
                            }
                        }
                        Event::MessagePartDelta { properties }
                        | Event::MessagePartUpdated { properties } => {
                            if event_session_matches(properties.session_id.as_deref(), &session_id)
                                && properties.message_id.as_ref().is_some_and(|message_id| {
                                correlated_assistant_ids.contains(message_id)
                            }) {
                                last_activity = tokio::time::Instant::now();
                                awaiting_idle_grace = false;
                            }
                        }
                        Event::SessionIdle { properties } => {
                            if properties.session_id != session_id {
                                continue;
                            }
                            match idle_gate_decision(
                                assistant_started,
                                idle_grace_deadline,
                                tokio::time::Instant::now(),
                            ) {
                                IdleGateDecision::Finalize => {
                                    return Ok(self
                                        .completion_outcome(
                                            session_id,
                                            &transcript_window,
                                            command_transport_error.as_ref(),
                                            literal_post_attempts.load(Ordering::Relaxed),
                                            &mut command_task,
                                            observed_local,
                                        )
                                        .await);
                                }
                                IdleGateDecision::WaitForGrace => {
                                    awaiting_idle_grace = true;
                                }
                                IdleGateDecision::IgnoreUntilDispatchConfirmed => {}
                            }
                        }
                        Event::SessionError { properties } => {
                            if !event_session_matches(properties.session_id.as_deref(), &session_id) {
                                continue;
                            }
                            let outcome_session_id =
                                properties.session_id.unwrap_or_else(|| session_id.clone());
                            return Ok(self
                                .terminal_failure(
                                    outcome_session_id,
                                    TerminalFailure {
                                        error: format!("session error: {:?}", properties.error),
                                        diagnostics: None,
                                        literal_post_attempts: literal_post_attempts
                                            .load(Ordering::Relaxed),
                                        failure: InvocationFailure::SessionError,
                                    },
                                    &mut command_task,
                                    observed_local,
                                )
                                .await);
                        }
                        _ => {}
                    }
                }
                _ = poll_interval.tick() => {
                    let pending_interruption = match self
                        .preflight_pending_interruptions(
                            &session_id,
                            literal_post_attempts.load(Ordering::Relaxed),
                        )
                        .await
                    {
                        Ok(outcome) => outcome,
                        Err(error) => {
                            tracing::warn!(error = %error, "failed to poll pending interruptions");
                            None
                        }
                    };
                    if let Some(outcome) = pending_interruption {
                        match (owner, owner_context, outcome) {
                            (
                                Some(owner),
                                Some(owner_context),
                                SupervisedOutcome::PermissionRequired { request_id, .. },
                            ) if !handled_interruptions.contains(&(
                                crate::state::InterruptionKind::Permission,
                                request_id.clone(),
                            )) => {
                                let interruption_key = (
                                    crate::state::InterruptionKind::Permission,
                                    request_id.clone(),
                                );
                                let correlation = crate::owner::InterruptionCorrelation {
                                    run_id: owner_context.run_id.clone(),
                                    invocation_id: owner_context.invocation_id.clone(),
                                    session_id: session_id.clone(),
                                    command_message_id: transcript_window.command_message_id.clone(),
                                    kind: crate::state::InterruptionKind::Permission,
                                    request_id: request_id.clone(),
                                };
                                emit_event!(SupervisionEvent::Paused {
                                    kind: crate::state::InterruptionKind::Permission,
                                    request_id: request_id.clone(),
                                });
                                let waited = match self
                                    .continue_permission_with_owner(
                                        owner,
                                        &correlation,
                                        &session_id,
                                        &request_id,
                                    )
                                    .await
                                {
                                    Ok(waited) => waited,
                                    Err(error) => {
                                        terminal_runtime_error!(InvocationFailure::OwnerIpc, error);
                                    }
                                };
                                handled_interruptions.insert(interruption_key);
                                suspend_supervision_clocks(
                                    &mut deadline,
                                    &mut last_activity,
                                    waited,
                                );
                                emit_event!(SupervisionEvent::Resumed { assistant_started });
                                continue;
                            }
                            (
                                Some(owner),
                                Some(owner_context),
                                SupervisedOutcome::QuestionRequired { request_id, .. },
                            ) if !handled_interruptions.contains(&(
                                crate::state::InterruptionKind::Question,
                                request_id.clone(),
                            )) => {
                                let interruption_key = (
                                    crate::state::InterruptionKind::Question,
                                    request_id.clone(),
                                );
                                let correlation = crate::owner::InterruptionCorrelation {
                                    run_id: owner_context.run_id.clone(),
                                    invocation_id: owner_context.invocation_id.clone(),
                                    session_id: session_id.clone(),
                                    command_message_id: transcript_window.command_message_id.clone(),
                                    kind: crate::state::InterruptionKind::Question,
                                    request_id: request_id.clone(),
                                };
                                emit_event!(SupervisionEvent::Paused {
                                    kind: crate::state::InterruptionKind::Question,
                                    request_id: request_id.clone(),
                                });
                                let waited = match self
                                    .continue_question_with_owner(owner, &correlation, &request_id)
                                    .await
                                {
                                    Ok(waited) => waited,
                                    Err(error) => {
                                        terminal_runtime_error!(InvocationFailure::OwnerIpc, error);
                                    }
                                };
                                handled_interruptions.insert(interruption_key);
                                suspend_supervision_clocks(
                                    &mut deadline,
                                    &mut last_activity,
                                    waited,
                                );
                                emit_event!(SupervisionEvent::Resumed { assistant_started });
                                continue;
                            }
                            (
                                Some(_),
                                Some(_),
                                SupervisedOutcome::PermissionRequired { .. }
                                | SupervisedOutcome::QuestionRequired { .. },
                            ) => {}
                            (_, _, outcome) => return Ok(outcome),
                        }
                    }

                    match self.client.sessions().status_map().await {
                        Ok(statuses)
                            if matches!(
                                observe_session_status(&statuses, &session_id),
                                SessionStatusObservation::BusyLike
                            ) => {
                            last_activity = tokio::time::Instant::now();
                            observed_busy = true;
                            awaiting_idle_grace = false;
                        }
                        Ok(statuses)
                            if matches!(
                                observe_session_status(&statuses, &session_id),
                                SessionStatusObservation::Absent | SessionStatusObservation::Idle
                            ) => {
                            match idle_gate_decision(
                                assistant_started,
                                idle_grace_deadline,
                                tokio::time::Instant::now(),
                            ) {
                                IdleGateDecision::Finalize => {
                                    return Ok(self
                                        .completion_outcome(
                                            session_id,
                                            &transcript_window,
                                            command_transport_error.as_ref(),
                                            literal_post_attempts.load(Ordering::Relaxed),
                                            &mut command_task,
                                            observed_local,
                                        )
                                        .await);
                                }
                                IdleGateDecision::WaitForGrace => {
                                    awaiting_idle_grace = true;
                                }
                                IdleGateDecision::IgnoreUntilDispatchConfirmed => {}
                            }
                        }
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(error = %error, "failed to poll session status");
                        }
                    }
                }
                result = async {
                    match command_task.as_mut() {
                        Some(task) => Some(task.await),
                        None => std::future::pending::<Option<Result<Result<(), opencode_rs::OpencodeError>, tokio::task::JoinError>>>().await,
                    }
                }, if command_task.is_some() => {
                    match result {
                        Some(Ok(Ok(()))) => {
                            idle_grace_deadline = Some(tokio::time::Instant::now() + IDLE_GRACE);
                            awaiting_idle_grace = true;
                            command_task = None;
                            observed_local = LocalTaskDisposition::Completed;
                        }
                        Some(Ok(Err(error))) => {
                            command_task = None;
                            observed_local = LocalTaskDisposition::ReturnedError;
                            if !matches!(error, OpencodeError::Transport(_)) {
                                return Ok(self
                                    .terminal_failure(
                                        session_id,
                                        TerminalFailure {
                                            error: error.to_string(),
                                            diagnostics: None,
                                            literal_post_attempts: literal_post_attempts
                                                .load(Ordering::Relaxed),
                                            failure: InvocationFailure::CommandTransport,
                                        },
                                        &mut command_task,
                                        observed_local,
                                    )
                                    .await);
                            }

                            let mut start_evidence = observed_busy;

                            if !start_evidence
                                && let Ok(statuses) = self.client.sessions().status_map().await
                                && statuses
                                    .get(&session_id)
                                    .is_some_and(SessionStatusInfo::is_busy_like)
                            {
                                start_evidence = true;
                                observed_busy = true;
                                last_activity = tokio::time::Instant::now();
                            }

                            if !start_evidence {
                                match self.client.messages().list(&session_id).await {
                                    Ok(messages) if transcript_indicates_dispatch(&messages, &transcript_window) => {
                                        start_evidence = true;
                                        idle_grace_deadline
                                            .get_or_insert_with(|| tokio::time::Instant::now() + IDLE_GRACE);
                                        last_activity = tokio::time::Instant::now();
                                    }
                                    Ok(_) => {}
                                    Err(probe_error) => {
                                        tracing::warn!(error = %probe_error, "failed transcript probe after /command transport error");
                                    }
                                }
                            }

                            if start_evidence {
                                idle_grace_deadline
                                    .get_or_insert_with(|| tokio::time::Instant::now() + IDLE_GRACE);
                                awaiting_idle_grace = true;
                                tracing::warn!(
                                    session_id = %session_id,
                                    error = %error,
                                    "POST /session/{session_id}/command transport error after start evidence; continuing supervision via SSE/status"
                                );
                                command_transport_error.get_or_insert_with(|| error.to_string());
                                continue;
                            }

                            let diagnostics = OpenCodeDiagnostics {
                                checked_at: chrono::Utc::now().to_rfc3339(),
                                literal_post_attempts: literal_post_attempts.load(Ordering::Relaxed),
                                command_message_id: Some(transcript_window.command_message_id.clone()),
                                final_assistant_message_id: None,
                                final_finish_reason: None,
                                guard_detected: false,
                                final_tool_error: None,
                                command_transport_error: Some(error.to_string()),
                            };
                            return Ok(self
                                .terminal_failure(
                                    session_id,
                                    TerminalFailure {
                                        error: format!(
                                            "transport error dispatching OpenCode command '{command_name}' (no session start evidence observed): {error}"
                                        ),
                                        diagnostics: Some(diagnostics),
                                        literal_post_attempts: literal_post_attempts
                                            .load(Ordering::Relaxed),
                                        failure: InvocationFailure::CommandTransport,
                                    },
                                    &mut command_task,
                                    observed_local,
                                )
                                .await);
                        }
                        Some(Err(error)) => {
                            command_task = None;
                            observed_local = if error.is_cancelled() {
                                LocalTaskDisposition::JoinCancelled
                            } else {
                                LocalTaskDisposition::JoinPanicked
                            };
                            return Ok(self
                                .terminal_failure(
                                    session_id,
                                    TerminalFailure {
                                        error: format!("command task failed: {error}"),
                                        diagnostics: None,
                                        literal_post_attempts: literal_post_attempts
                                            .load(Ordering::Relaxed),
                                        failure: InvocationFailure::LocalTask,
                                    },
                                    &mut command_task,
                                    observed_local,
                                )
                                .await);
                        }
                        None => unreachable!("command task guard should prevent None"),
                    }
                }
                () = async {
                    match idle_grace_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending::<()>().await,
                    }
                }, if awaiting_idle_grace => {
                    return Ok(self
                        .completion_outcome(
                            session_id,
                            &transcript_window,
                            command_transport_error.as_ref(),
                            literal_post_attempts.load(Ordering::Relaxed),
                            &mut command_task,
                            observed_local,
                        )
                        .await);
                }
            }
        }
    }

    async fn continue_permission_with_owner(
        &self,
        owner: &crate::owner::OwnerRuntime,
        correlation: &crate::owner::InterruptionCorrelation,
        session_id: &str,
        request_id: &str,
    ) -> Result<Duration> {
        owner.publish_pending(correlation)?;
        let wait_started = tokio::time::Instant::now();
        let response = owner.await_response(correlation).await?;
        let crate::owner::InterruptionResponse::Permission { allow } = response else {
            anyhow::bail!("owner returned mismatched permission response");
        };
        self.respond_permission(session_id, request_id, allow)
            .await?;
        Ok(tokio::time::Instant::now().duration_since(wait_started))
    }

    async fn continue_question_with_owner(
        &self,
        owner: &crate::owner::OwnerRuntime,
        correlation: &crate::owner::InterruptionCorrelation,
        request_id: &str,
    ) -> Result<Duration> {
        owner.publish_pending(correlation)?;
        let wait_started = tokio::time::Instant::now();
        let response = owner.await_response(correlation).await?;
        let crate::owner::InterruptionResponse::Question { answers } = response else {
            anyhow::bail!("owner returned mismatched question response");
        };
        self.respond_question_answers(request_id, answers).await?;
        Ok(tokio::time::Instant::now().duration_since(wait_started))
    }

    pub async fn respond_permission(
        &self,
        _session_id: &str,
        request_id: &str,
        allow: bool,
    ) -> Result<()> {
        let reply = if allow {
            PermissionReply::Once
        } else {
            PermissionReply::Reject
        };

        self.client
            .permissions()
            .reply(
                request_id,
                &PermissionReplyRequest {
                    reply,
                    message: None,
                },
            )
            .await
            .with_context(|| format!("failed to respond to permission request {request_id}"))?;
        Ok(())
    }

    async fn terminalize_command_task(
        &self,
        session_id: &str,
        command_task: &mut Option<CommandTask>,
        observed_local: LocalTaskDisposition,
        abort_server: bool,
    ) -> TaskDisposition {
        let server_abort = if abort_server {
            match self.client.sessions().abort(session_id).await {
                Ok(aborted) => ServerAbortDisposition::Succeeded { aborted },
                Err(OpencodeError::Transport(_)) => ServerAbortDisposition::Failed {
                    failure: CleanupFailure::Transport,
                },
                Err(_) => ServerAbortDisposition::Failed {
                    failure: CleanupFailure::Server,
                },
            }
        } else {
            ServerAbortDisposition::NotRequired
        };

        let local_task = match command_task.take() {
            None => observed_local,
            Some(task) if task.is_finished() => match task.await {
                Ok(Ok(())) => LocalTaskDisposition::Completed,
                Ok(Err(_)) => LocalTaskDisposition::ReturnedError,
                Err(error) if error.is_cancelled() => LocalTaskDisposition::JoinCancelled,
                Err(_) => LocalTaskDisposition::JoinPanicked,
            },
            Some(task) => {
                task.abort();
                match task.await {
                    Err(error) if error.is_cancelled() => LocalTaskDisposition::AbortedAndJoined,
                    Err(_) => LocalTaskDisposition::JoinPanicked,
                    Ok(Ok(())) => LocalTaskDisposition::Completed,
                    Ok(Err(_)) => LocalTaskDisposition::ReturnedError,
                }
            }
        };

        TaskDisposition {
            server_abort,
            local_task,
        }
    }

    async fn terminal_failure(
        &self,
        session_id: String,
        terminal: TerminalFailure,
        command_task: &mut Option<CommandTask>,
        observed_local: LocalTaskDisposition,
    ) -> SupervisedOutcome {
        let task_disposition = self
            .terminalize_command_task(&session_id, command_task, observed_local, true)
            .await;
        SupervisedOutcome::Failed {
            session_id: Some(session_id),
            error: terminal.error,
            diagnostics: terminal.diagnostics,
            literal_post_attempts: terminal.literal_post_attempts,
            failure: terminal.failure,
            task_disposition,
        }
    }

    async fn respond_question_answers(
        &self,
        request_id: &str,
        answers: Vec<Vec<String>>,
    ) -> Result<()> {
        self.client
            .question()
            .reply(request_id, &QuestionReply { answers })
            .await
            .with_context(|| format!("failed to respond to question request {request_id}"))?;
        Ok(())
    }

    async fn preflight_pending_interruptions(
        &self,
        session_id: &str,
        literal_post_attempts: u32,
    ) -> Result<Option<SupervisedOutcome>> {
        let permissions = self
            .client
            .permissions()
            .list()
            .await
            .context("failed to list permissions")?;
        if let Some(permission) = permissions
            .into_iter()
            .find(|permission| permission.session_id == session_id)
        {
            return Ok(Some(SupervisedOutcome::PermissionRequired {
                session_id: session_id.to_string(),
                request_id: permission.id,
                permission_type: permission.permission,
                literal_post_attempts,
            }));
        }

        let questions = self
            .client
            .question()
            .list()
            .await
            .context("failed to list questions")?;
        if let Some(question) = questions
            .into_iter()
            .find(|question| question.session_id == session_id)
        {
            return Ok(Some(SupervisedOutcome::QuestionRequired {
                session_id: session_id.to_string(),
                request_id: question.id,
                prompt: question
                    .questions
                    .first()
                    .map(|entry| entry.question.clone())
                    .unwrap_or_default(),
                literal_post_attempts,
            }));
        }

        Ok(None)
    }

    async fn completion_outcome(
        &self,
        session_id: String,
        transcript_window: &TranscriptWindow,
        command_transport_error: Option<&String>,
        literal_post_attempts: u32,
        command_task: &mut Option<CommandTask>,
        observed_local: LocalTaskDisposition,
    ) -> SupervisedOutcome {
        let validation = self
            .validate_completion_with_retries(&session_id, transcript_window, literal_post_attempts)
            .await;
        let abort_server = !matches!(validation, CompletionValidation::Passed(_));
        let task_disposition = self
            .terminalize_command_task(&session_id, command_task, observed_local, abort_server)
            .await;
        let outcome = match validation {
            CompletionValidation::Passed(diagnostics) => SupervisedOutcome::Completed {
                session_id,
                diagnostics,
                literal_post_attempts,
                task_disposition,
            },
            CompletionValidation::AcceptedButNotStarted(diagnostics) => {
                SupervisedOutcome::AcceptedButNotStarted {
                    session_id,
                    diagnostics,
                    literal_post_attempts,
                    task_disposition,
                }
            }
            CompletionValidation::Failed { error, diagnostics } => SupervisedOutcome::Failed {
                session_id: Some(session_id),
                error,
                diagnostics,
                literal_post_attempts,
                failure: InvocationFailure::CompletionValidation,
                task_disposition,
            },
        };

        attach_transport_warning(outcome, command_transport_error)
    }

    async fn fetch_transcript_tail_id(&self, session_id: &str) -> Result<Option<String>> {
        Ok(self
            .client
            .messages()
            .list(session_id)
            .await
            .with_context(|| {
                format!("failed to list transcript messages for session {session_id}")
            })?
            .last()
            .map(|message| message.id().to_string()))
    }

    async fn validate_completion_with_retries(
        &self,
        session_id: &str,
        transcript_window: &TranscriptWindow,
        literal_post_attempts: u32,
    ) -> CompletionValidation {
        for attempt in 0..=TRANSCRIPT_SETTLING_RETRY_BACKOFFS.len() {
            if attempt > 0 {
                tokio::time::sleep(TRANSCRIPT_SETTLING_RETRY_BACKOFFS[attempt - 1]).await;
            }

            let messages = match self.client.messages().list(session_id).await {
                Ok(messages) => messages,
                Err(error) => {
                    return CompletionValidation::Failed {
                        error: format!(
                            "failed to validate completed transcript for session {session_id}: {error}"
                        ),
                        diagnostics: None,
                    };
                }
            };

            let analysis = analyze_transcript_window(&messages, transcript_window);
            let diagnostics =
                analysis.diagnostics(&transcript_window.command_message_id, literal_post_attempts);
            if analysis.guard_detected {
                return CompletionValidation::Failed {
                    error:
                        "completed session transcript contains nested orchestrator guard failure"
                            .to_string(),
                    diagnostics: Some(diagnostics),
                };
            }
            if analysis.final_tool_error.is_some() {
                return CompletionValidation::Failed {
                    error: "completed session transcript ended with a tool error".to_string(),
                    diagnostics: Some(diagnostics),
                };
            }
            if analysis.unresolved_tool_calls > 0 {
                if attempt == TRANSCRIPT_SETTLING_RETRY_BACKOFFS.len() {
                    return CompletionValidation::Failed {
                        error: format!(
                            "completed session transcript still has {} unresolved tool call(s) after settling retries",
                            analysis.unresolved_tool_calls
                        ),
                        diagnostics: Some(diagnostics),
                    };
                }
                continue;
            }
            if analysis.has_assistant_message {
                return CompletionValidation::Passed(diagnostics);
            }
            if attempt == TRANSCRIPT_SETTLING_RETRY_BACKOFFS.len() {
                return CompletionValidation::AcceptedButNotStarted(diagnostics);
            }
        }

        CompletionValidation::Failed {
            error: "completed session transcript validation exited unexpectedly".to_string(),
            diagnostics: None,
        }
    }
}

fn describe_duration(duration: Duration) -> String {
    let seconds = duration.as_secs();
    if seconds.is_multiple_of(3600) {
        let hours = seconds / 3600;
        return format!("{hours} hour{}", if hours == 1 { "" } else { "s" });
    }
    if seconds.is_multiple_of(60) {
        let minutes = seconds / 60;
        return format!("{minutes} minute{}", if minutes == 1 { "" } else { "s" });
    }
    format!("{seconds} second{}", if seconds == 1 { "" } else { "s" })
}

fn generate_command_message_id() -> String {
    let tick = COMMAND_MESSAGE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "msg-outer-dag-{}-{tick}",
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    )
}

fn analyze_transcript_window(
    messages: &[Message],
    transcript_window: &TranscriptWindow,
) -> TranscriptAnalysis {
    let start_index = messages
        .iter()
        .position(|message| message.id() == transcript_window.command_message_id)
        .or_else(|| {
            transcript_window
                .baseline_tail_message_id
                .as_ref()
                .and_then(|baseline| messages.iter().position(|message| message.id() == baseline))
                .map(|index| index + 1)
        });
    let window = start_index.map_or(&[][..], |index| &messages[index.min(messages.len())..]);
    let lineage_assistant = window.iter().rev().find(|message| {
        message.role() == "assistant"
            && message.info.parent_id.as_deref()
                == Some(transcript_window.command_message_id.as_str())
    });
    let final_assistant = lineage_assistant.or_else(|| {
        window
            .iter()
            .rev()
            .find(|message| message.role() == "assistant" && message.info.parent_id.is_none())
    });
    let mut guard_detected = false;
    let mut unresolved_tool_calls = 0;

    for message in window {
        for part in &message.parts {
            match part {
                Part::Text { text, .. } | Part::Reasoning { text, .. } => {
                    if text.contains(NESTED_GUARD_NEEDLE) {
                        guard_detected = true;
                    }
                }
                Part::Tool { state, .. } => {
                    if state.as_ref().is_none_or(|tool_state| {
                        !matches!(tool_state, ToolState::Completed(_) | ToolState::Error(_))
                    }) {
                        unresolved_tool_calls += 1;
                    }

                    if state.as_ref().is_some_and(|tool_state| {
                        tool_state
                            .error()
                            .is_some_and(|error| error.contains(NESTED_GUARD_NEEDLE))
                    }) {
                        guard_detected = true;
                    }
                }
                _ => {}
            }
        }
    }

    let final_tool_error = final_assistant.and_then(|message| {
        message.parts.iter().find_map(|part| {
            let Part::Tool {
                tool,
                state: Some(ToolState::Error(error_state)),
                ..
            } = part
            else {
                return None;
            };
            Some(OpenCodeToolErrorDiagnostics {
                tool: tool.clone(),
                error: truncate_tool_error(&error_state.error),
            })
        })
    });

    TranscriptAnalysis {
        has_assistant_message: final_assistant.is_some(),
        final_assistant_message_id: final_assistant.map(|message| message.id().to_string()),
        final_finish_reason: final_assistant.and_then(|message| {
            message.info.finish.clone().or_else(|| {
                message.parts.iter().rev().find_map(|part| match part {
                    Part::StepFinish { reason, .. } => Some(reason.clone()),
                    _ => None,
                })
            })
        }),
        guard_detected,
        final_tool_error,
        unresolved_tool_calls,
    }
}

fn attach_transport_warning(
    outcome: SupervisedOutcome,
    warning: Option<&String>,
) -> SupervisedOutcome {
    let Some(warning) = warning.cloned() else {
        return outcome;
    };

    match outcome {
        SupervisedOutcome::Completed {
            session_id,
            mut diagnostics,
            literal_post_attempts,
            task_disposition,
        } => {
            diagnostics.command_transport_error.get_or_insert(warning);
            SupervisedOutcome::Completed {
                session_id,
                diagnostics,
                literal_post_attempts,
                task_disposition,
            }
        }
        SupervisedOutcome::AcceptedButNotStarted {
            session_id,
            mut diagnostics,
            literal_post_attempts,
            task_disposition,
        } => {
            diagnostics.command_transport_error.get_or_insert(warning);
            SupervisedOutcome::AcceptedButNotStarted {
                session_id,
                diagnostics,
                literal_post_attempts,
                task_disposition,
            }
        }
        SupervisedOutcome::Failed {
            session_id,
            error,
            diagnostics: Some(mut diagnostics),
            literal_post_attempts,
            failure,
            task_disposition,
        } => {
            diagnostics.command_transport_error.get_or_insert(warning);
            SupervisedOutcome::Failed {
                session_id,
                error,
                diagnostics: Some(diagnostics),
                literal_post_attempts,
                failure,
                task_disposition,
            }
        }
        other => other,
    }
}

fn truncate_tool_error(error: &str) -> String {
    let mut truncated = error
        .chars()
        .take(TOOL_ERROR_SUMMARY_LIMIT)
        .collect::<String>();
    if error.chars().count() > TOOL_ERROR_SUMMARY_LIMIT {
        truncated.push('…');
    }
    truncated
}

#[derive(Debug, Clone)]
struct LauncherConfig {
    binary: String,
    launcher_args: Vec<String>,
}

fn resolve_launcher_config(base_dir: &Path) -> Result<LauncherConfig> {
    let launcher_args = parse_launcher_args();
    if !launcher_args.is_empty() {
        let binary = match std::env::var(version::OPENCODE_BINARY_ENV) {
            Ok(value) => value.trim().to_string(),
            Err(_) => anyhow::bail!(
                "OPENCODE_BINARY_ARGS is set but OPENCODE_BINARY is not set; set OPENCODE_BINARY to the launcher command"
            ),
        };
        if binary.is_empty() {
            anyhow::bail!(
                "OPENCODE_BINARY_ARGS is set but OPENCODE_BINARY is empty; set it to the launcher command"
            );
        }

        return Ok(LauncherConfig {
            binary,
            launcher_args,
        });
    }

    let binary = resolve_opencode_binary(base_dir)?;
    Ok(LauncherConfig {
        binary: binary.to_string_lossy().to_string(),
        launcher_args: Vec::new(),
    })
}

fn resolve_opencode_binary(base_dir: &Path) -> Result<PathBuf> {
    if let Ok(value) = std::env::var(version::OPENCODE_BINARY_ENV) {
        let value = value.trim();
        if !value.is_empty() {
            let path = PathBuf::from(value);
            return path.canonicalize().with_context(|| {
                format!("OPENCODE_BINARY points to missing path: {}", path.display())
            });
        }
    }

    let candidate = base_dir
        .join(".opencode")
        .join("bin")
        .join(format!("opencode-v{}", version::PINNED_OPENCODE_VERSION));
    if candidate.exists() {
        return candidate
            .canonicalize()
            .with_context(|| format!("failed to canonicalize {}", candidate.display()));
    }

    Ok(PathBuf::from("opencode"))
}

fn parse_launcher_args() -> Vec<String> {
    match std::env::var(version::OPENCODE_BINARY_ARGS_ENV) {
        Ok(value) => {
            let value = value.trim();
            if value.is_empty() {
                Vec::new()
            } else {
                value.split_whitespace().map(str::to_string).collect()
            }
        }
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::EnvVarGuard;
    use crate::test_support::process_state_lock;
    use std::process::Stdio;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering as AtomicOrdering;
    use tempfile::TempDir;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::net::TcpStream;
    use tokio::process::Command;
    use tokio::sync::Notify;
    use tokio::time::timeout;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::Request;
    use wiremock::Respond;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    #[derive(Clone)]
    struct SequenceResponder {
        responders: Vec<ResponseTemplate>,
        calls: Arc<AtomicUsize>,
    }

    impl SequenceResponder {
        fn new(responders: Vec<ResponseTemplate>) -> Self {
            assert!(!responders.is_empty(), "responders must not be empty");
            Self {
                responders,
                calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.load(AtomicOrdering::SeqCst)
        }
    }

    impl Respond for SequenceResponder {
        fn respond(&self, _req: &Request) -> ResponseTemplate {
            let idx = self.calls.fetch_add(1, AtomicOrdering::SeqCst);
            self.responders
                .get(idx)
                .cloned()
                .unwrap_or_else(|| self.responders.last().cloned().expect("non-empty"))
        }
    }

    struct RawDispatchBarrierServer {
        base_url: String,
        stream_connections: Arc<AtomicUsize>,
        command_posts: Arc<AtomicUsize>,
        stream_connected: Arc<Notify>,
        command_posted: Arc<Notify>,
        sentinel_released: Arc<AtomicBool>,
        release_sentinel: Arc<Notify>,
        task: tokio::task::JoinHandle<()>,
    }

    impl RawDispatchBarrierServer {
        async fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let stream_connections = Arc::new(AtomicUsize::new(0));
            let command_posts = Arc::new(AtomicUsize::new(0));
            let stream_connected = Arc::new(Notify::new());
            let command_posted = Arc::new(Notify::new());
            let sentinel_released = Arc::new(AtomicBool::new(false));
            let release_sentinel = Arc::new(Notify::new());
            let task_stream_connections = Arc::clone(&stream_connections);
            let task_command_posts = Arc::clone(&command_posts);
            let task_stream_connected = Arc::clone(&stream_connected);
            let task_command_posted = Arc::clone(&command_posted);
            let task_sentinel_released = Arc::clone(&sentinel_released);
            let task_release_sentinel = Arc::clone(&release_sentinel);

            let task = tokio::spawn(async move {
                loop {
                    let (stream, _) = listener.accept().await.unwrap();
                    let stream_connections = Arc::clone(&task_stream_connections);
                    let command_posts = Arc::clone(&task_command_posts);
                    let stream_connected = Arc::clone(&task_stream_connected);
                    let command_posted = Arc::clone(&task_command_posted);
                    let sentinel_released = Arc::clone(&task_sentinel_released);
                    let release_sentinel = Arc::clone(&task_release_sentinel);
                    tokio::spawn(async move {
                        handle_raw_dispatch_request(
                            stream,
                            stream_connections,
                            command_posts,
                            stream_connected,
                            command_posted,
                            sentinel_released,
                            release_sentinel,
                        )
                        .await;
                    });
                }
            });

            Self {
                base_url: format!("http://{address}"),
                stream_connections,
                command_posts,
                stream_connected,
                command_posted,
                sentinel_released,
                release_sentinel,
                task,
            }
        }

        async fn wait_stream_connected(&self) {
            timeout(Duration::from_secs(5), async {
                while self.stream_connections.load(AtomicOrdering::SeqCst) == 0 {
                    self.stream_connected.notified().await;
                }
            })
            .await
            .unwrap();
        }

        fn release_readiness(&self) {
            self.sentinel_released.store(true, AtomicOrdering::SeqCst);
            self.release_sentinel.notify_waiters();
        }

        async fn wait_command_post(&self) {
            timeout(Duration::from_secs(5), async {
                while self.command_posts.load(AtomicOrdering::SeqCst) == 0 {
                    self.command_posted.notified().await;
                }
            })
            .await
            .unwrap();
        }
    }

    impl Drop for RawDispatchBarrierServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn handle_raw_dispatch_request(
        mut stream: TcpStream,
        stream_connections: Arc<AtomicUsize>,
        command_posts: Arc<AtomicUsize>,
        stream_connected: Arc<Notify>,
        command_posted: Arc<Notify>,
        sentinel_released: Arc<AtomicBool>,
        release_sentinel: Arc<Notify>,
    ) {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 2048];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut chunk).await.unwrap();
            if read == 0 {
                return;
            }
            request.extend_from_slice(&chunk[..read]);
        }
        let request = String::from_utf8(request).unwrap();
        let mut request_line = request.lines().next().unwrap().split_whitespace();
        let method = request_line.next().unwrap();
        let path = request_line.next().unwrap().split('?').next().unwrap();

        if method == "GET" && path == "/event" {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n",
                )
                .await
                .unwrap();
            stream.flush().await.unwrap();
            stream_connections.fetch_add(1, AtomicOrdering::SeqCst);
            stream_connected.notify_waiters();
            while !sentinel_released.load(AtomicOrdering::SeqCst) {
                release_sentinel.notified().await;
            }
            stream
                .write_all(b"data: {\"type\":\"server.connected\",\"properties\":{}}\n\n")
                .await
                .unwrap();
            stream.flush().await.unwrap();
            return;
        }

        let (status, body) = if method == "GET" && path == "/session/session-1" {
            (
                200,
                serde_json::json!({
                    "id": "session-1",
                    "slug": "session-1",
                    "projectId": "project-1",
                    "directory": "/tmp",
                    "title": "Barrier test",
                    "version": "test",
                    "time": {"created": 1, "updated": 1}
                })
                .to_string(),
            )
        } else if method == "GET"
            && (path == "/permission"
                || path == "/question"
                || path == "/session/session-1/message")
        {
            (200, "[]".to_string())
        } else if method == "GET" && path == "/session/status" {
            (200, "{}".to_string())
        } else if method == "POST" && path == "/session/session-1/command" {
            command_posts.fetch_add(1, AtomicOrdering::SeqCst);
            command_posted.notify_waiters();
            (200, "{}".to_string())
        } else {
            (404, "{}".to_string())
        };
        let reason = if status == 200 { "OK" } else { "Not Found" };
        stream
            .write_all(
                format!(
                    "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn initial_command_dispatch_waits_for_parsed_sse_readiness() {
        let server = RawDispatchBarrierServer::start().await;
        let child = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let managed = ManagedServer::from_child_for_testing(child, server.base_url.clone(), 1234);
        let client = Client::builder()
            .base_url(&server.base_url)
            .directory("/tmp")
            .build()
            .unwrap();
        let supervisor = OpenCodeSupervisor {
            _managed_server: managed,
            client,
            _directory: PathBuf::from("/tmp"),
            timeouts: test_timeouts(),
        };
        let PreparedCommandOutcome::Prepared(prepared) = supervisor
            .prepare_command(
                SessionSelection::Reuse("session-1".to_string()),
                "implement_plan",
                Some("do it"),
            )
            .await
            .unwrap()
        else {
            panic!("expected prepared command");
        };

        let run = tokio::spawn(async move {
            supervisor
                .run_prepared_command(prepared, None, None, |_| Ok(()))
                .await
        });
        server.wait_stream_connected().await;
        assert_eq!(server.command_posts.load(AtomicOrdering::SeqCst), 0);
        server.release_readiness();
        server.wait_command_post().await;
        assert_eq!(server.command_posts.load(AtomicOrdering::SeqCst), 1);
        run.abort();
        let _ = run.await;
    }

    fn test_timeouts() -> OpenCodeSupervisorTimeouts {
        OpenCodeSupervisorTimeouts {
            session_deadline: Duration::from_hours(8),
            inactivity_timeout: Duration::from_mins(5),
        }
    }

    fn transcript_message(role: &str, id: &str, parts: &serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "info": {
                "id": id,
                "sessionID": "session-1",
                "role": role,
                "time": { "created": 1 },
                "finish": if role == "assistant" { serde_json::json!("stop") } else { serde_json::Value::Null }
            },
            "parts": parts,
        })
    }

    fn transcript_message_with_parent(
        role: &str,
        id: &str,
        parent_id: Option<&str>,
    ) -> serde_json::Value {
        let mut message = transcript_message(role, id, &serde_json::json!([]));
        if let Some(parent_id) = parent_id {
            message["info"]["parentID"] = serde_json::json!(parent_id);
        }
        message
    }

    #[test]
    fn transcript_requires_parent_correlation_when_lineage_is_present() {
        let messages: Vec<Message> = serde_json::from_value(serde_json::json!([
            transcript_message("user", "msg-command", &serde_json::json!([])),
            transcript_message_with_parent("assistant", "msg-wrong", Some("msg-other")),
            transcript_message_with_parent("assistant", "msg-current", Some("msg-command"))
        ]))
        .unwrap();

        let analysis = analyze_transcript_window(
            &messages,
            &TranscriptWindow {
                command_message_id: "msg-command".to_string(),
                baseline_tail_message_id: None,
            },
        );

        assert!(analysis.has_assistant_message);
        assert_eq!(
            analysis.final_assistant_message_id.as_deref(),
            Some("msg-current")
        );
    }

    #[test]
    fn transcript_fallback_accepts_only_lineage_absent_assistant_in_anchored_window() {
        let messages: Vec<Message> = serde_json::from_value(serde_json::json!([
            transcript_message_with_parent("assistant", "msg-before", None),
            transcript_message("user", "msg-baseline", &serde_json::json!([])),
            transcript_message("user", "msg-command", &serde_json::json!([])),
            transcript_message_with_parent("assistant", "msg-wrong", Some("msg-other")),
            transcript_message_with_parent("assistant", "msg-fallback", None)
        ]))
        .unwrap();

        let analysis = analyze_transcript_window(
            &messages,
            &TranscriptWindow {
                command_message_id: "msg-command".to_string(),
                baseline_tail_message_id: Some("msg-baseline".to_string()),
            },
        );

        assert_eq!(
            analysis.final_assistant_message_id.as_deref(),
            Some("msg-fallback")
        );
    }

    #[test]
    fn transcript_without_current_anchor_excludes_unrelated_assistant() {
        let messages: Vec<Message> =
            serde_json::from_value(serde_json::json!([transcript_message_with_parent(
                "assistant",
                "msg-unrelated",
                None
            )]))
            .unwrap();

        let analysis = analyze_transcript_window(
            &messages,
            &TranscriptWindow {
                command_message_id: "msg-missing".to_string(),
                baseline_tail_message_id: Some("baseline-missing".to_string()),
            },
        );

        assert!(!analysis.has_assistant_message);
        assert_eq!(analysis.final_assistant_message_id, None);
    }

    fn parse_messages(value: serde_json::Value) -> Vec<Message> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn idle_gate_ignores_idle_until_dispatch_is_confirmed() {
        assert_eq!(
            idle_gate_decision(false, None, tokio::time::Instant::now()),
            IdleGateDecision::IgnoreUntilDispatchConfirmed
        );
    }

    #[test]
    fn idle_grace_is_strictly_longer_than_poll_interval() {
        assert!(IDLE_GRACE > POLL_INTERVAL);
    }

    #[test]
    fn status_observation_preserves_absence_and_all_present_variants() {
        let mut statuses = HashMap::new();
        assert_eq!(
            observe_session_status(&statuses, "session-1"),
            SessionStatusObservation::Absent
        );

        statuses.insert("session-1".to_string(), SessionStatusInfo::Idle);
        assert_eq!(
            observe_session_status(&statuses, "session-1"),
            SessionStatusObservation::Idle
        );

        for status in [
            SessionStatusInfo::Busy,
            SessionStatusInfo::Retry {
                attempt: 1,
                message: "retrying".to_string(),
                next: 42,
            },
            SessionStatusInfo::Unknown,
        ] {
            statuses.insert("session-1".to_string(), status);
            assert_eq!(
                observe_session_status(&statuses, "session-1"),
                SessionStatusObservation::BusyLike
            );
        }
    }

    #[test]
    fn event_session_correlation_rejects_explicit_wrong_session() {
        assert!(event_session_matches(None, "session-1"));
        assert!(event_session_matches(Some("session-1"), "session-1"));
        assert!(!event_session_matches(Some("session-2"), "session-1"));
    }

    #[test]
    fn authorized_wait_suspends_both_supervision_clocks() {
        let original_deadline = tokio::time::Instant::now() + Duration::from_mins(1);
        let original_activity = tokio::time::Instant::now();
        let mut deadline = original_deadline;
        let mut last_activity = original_activity;

        suspend_supervision_clocks(&mut deadline, &mut last_activity, Duration::from_secs(45));

        assert_eq!(deadline, original_deadline + Duration::from_secs(45));
        assert_eq!(last_activity, original_activity + Duration::from_secs(45));
    }

    #[test]
    fn idle_gate_finalizes_after_observed_busy() {
        let future_deadline = tokio::time::Instant::now() + Duration::from_mins(1);
        assert_eq!(
            idle_gate_decision(true, Some(future_deadline), tokio::time::Instant::now()),
            IdleGateDecision::Finalize
        );
    }

    #[test]
    fn idle_gate_waits_for_grace_before_deadline() {
        let now = tokio::time::Instant::now();
        assert_eq!(
            idle_gate_decision(false, Some(now + Duration::from_millis(50)), now),
            IdleGateDecision::WaitForGrace
        );
    }

    #[test]
    fn idle_gate_finalizes_after_grace_deadline_elapses() {
        let now = tokio::time::Instant::now();
        assert_eq!(
            idle_gate_decision(false, Some(now), now),
            IdleGateDecision::Finalize
        );
    }

    #[tokio::test]
    async fn preflight_returns_pending_permission() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/permission"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "perm-1",
                    "sessionID": "session-1",
                    "permission": "file.write",
                    "patterns": ["src/**/*.rs"]
                }
            ])))
            .mount(&mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/question"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock)
            .await;

        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let outcome = supervisor
            .preflight_pending_interruptions("session-1", 0)
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            Some(SupervisedOutcome::PermissionRequired { request_id, .. }) if request_id == "perm-1"
        ));
    }

    #[tokio::test]
    async fn preflight_returns_pending_question() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/permission"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/question"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "question-1",
                    "sessionID": "session-2",
                    "questions": [{ "question": "Continue?" }]
                }
            ])))
            .mount(&mock)
            .await;

        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let outcome = supervisor
            .preflight_pending_interruptions("session-2", 0)
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            Some(SupervisedOutcome::QuestionRequired { request_id, .. }) if request_id == "question-1"
        ));
    }

    #[test]
    fn detects_final_tool_error_as_failure() {
        let messages = parse_messages(serde_json::json!([
            transcript_message("user", "msg-user", &serde_json::json!([])),
            transcript_message(
                "assistant",
                "msg-assistant",
                &serde_json::json!([
                    {
                        "type": "tool",
                        "callID": "call-1",
                        "tool": "read",
                        "state": {
                            "status": "error",
                            "input": {},
                            "error": "permission denied",
                            "time": { "start": 1, "end": 2 }
                        }
                    }
                ])
            )
        ]));

        let analysis = analyze_transcript_window(
            &messages,
            &TranscriptWindow {
                command_message_id: "msg-user".to_string(),
                baseline_tail_message_id: None,
            },
        );

        assert_eq!(
            analysis.final_assistant_message_id.as_deref(),
            Some("msg-assistant")
        );
        assert_eq!(
            analysis
                .final_tool_error
                .as_ref()
                .map(|error| error.tool.as_str()),
            Some("read")
        );
    }

    #[test]
    fn detects_guard_text_as_failure() {
        let messages = parse_messages(serde_json::json!([
            transcript_message("user", "msg-command", &serde_json::json!([])),
            transcript_message(
                "assistant",
                "msg-assistant",
                &serde_json::json!([
                    {
                        "type": "text",
                        "text": "nested launch blocked by OPENCODE_ORCHESTRATOR_MANAGED"
                    }
                ])
            )
        ]));

        let analysis = analyze_transcript_window(
            &messages,
            &TranscriptWindow {
                command_message_id: "msg-command".to_string(),
                baseline_tail_message_id: None,
            },
        );

        assert!(analysis.guard_detected);
    }

    #[test]
    fn describe_duration_uses_human_friendly_units() {
        assert_eq!(describe_duration(Duration::from_hours(8)), "8 hours");
        assert_eq!(describe_duration(Duration::from_mins(5)), "5 minutes");
        assert_eq!(describe_duration(Duration::from_secs(45)), "45 seconds");
    }

    #[test]
    fn requires_assistant_message_after_dispatch_window() {
        let messages = parse_messages(serde_json::json!([
            transcript_message("assistant", "msg-before", &serde_json::json!([])),
            transcript_message("user", "msg-baseline", &serde_json::json!([]))
        ]));

        let analysis = analyze_transcript_window(
            &messages,
            &TranscriptWindow {
                command_message_id: "msg-missing".to_string(),
                baseline_tail_message_id: Some("msg-baseline".to_string()),
            },
        );

        assert!(!analysis.has_assistant_message);
    }

    #[test]
    fn counts_unresolved_tool_states_conservatively() {
        let messages = parse_messages(serde_json::json!([transcript_message(
            "assistant",
            "msg-assistant",
            &serde_json::json!([
                {
                    "type": "tool",
                    "callID": "call-pending",
                    "tool": "read",
                    "state": {
                        "status": "pending",
                        "input": {},
                        "raw": "read"
                    }
                },
                {
                    "type": "tool",
                    "callID": "call-running",
                    "tool": "grep",
                    "state": {
                        "status": "running",
                        "input": {},
                        "time": { "start": 1 }
                    }
                },
                {
                    "type": "tool",
                    "callID": "call-none",
                    "tool": "write"
                },
                {
                    "type": "tool",
                    "callID": "call-unknown",
                    "tool": "edit",
                    "state": { "status": "paused" }
                },
                {
                    "type": "tool",
                    "callID": "call-completed",
                    "tool": "done",
                    "state": {
                        "status": "completed",
                        "input": {},
                        "output": "ok",
                        "title": "done",
                        "metadata": {},
                        "time": { "start": 1, "end": 2 }
                    }
                },
                {
                    "type": "tool",
                    "callID": "call-error",
                    "tool": "fail",
                    "state": {
                        "status": "error",
                        "input": {},
                        "error": "boom",
                        "time": { "start": 1, "end": 2 }
                    }
                }
            ]),
        )]));

        let analysis = analyze_transcript_window(
            &messages,
            &TranscriptWindow {
                command_message_id: "msg-assistant".to_string(),
                baseline_tail_message_id: None,
            },
        );

        assert_eq!(analysis.unresolved_tool_calls, 4);
    }

    #[tokio::test]
    async fn fetches_and_validates_completed_transcript() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/session/session-1/message"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                transcript_message("user", "msg-dispatch", &serde_json::json!([])),
                transcript_message(
                    "assistant",
                    "msg-assistant",
                    &serde_json::json!([
                        { "type": "text", "text": "done" },
                        { "type": "step-finish", "reason": "stop", "cost": 0.0 }
                    ])
                )
            ])))
            .mount(&mock)
            .await;

        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let validation = supervisor
            .validate_completion_with_retries(
                "session-1",
                &TranscriptWindow {
                    command_message_id: "msg-dispatch".to_string(),
                    baseline_tail_message_id: None,
                },
                1,
            )
            .await;

        let CompletionValidation::Passed(diagnostics) = validation else {
            panic!("expected transcript validation success");
        };
        assert_eq!(
            diagnostics.final_assistant_message_id.as_deref(),
            Some("msg-assistant")
        );
        assert_eq!(diagnostics.final_finish_reason.as_deref(), Some("stop"));
    }

    #[tokio::test]
    async fn missing_assistant_after_settling_is_accepted_but_not_started() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/session/session-1/message"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                transcript_message("user", "msg-dispatch", &serde_json::json!([]))
            ])))
            .mount(&mock)
            .await;

        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let validation = supervisor
            .validate_completion_with_retries(
                "session-1",
                &TranscriptWindow {
                    command_message_id: "msg-dispatch".to_string(),
                    baseline_tail_message_id: None,
                },
                1,
            )
            .await;

        let CompletionValidation::AcceptedButNotStarted(diagnostics) = validation else {
            panic!("expected accepted-but-not-started classification without assistant");
        };
        assert_eq!(diagnostics.final_assistant_message_id, None);
    }

    #[tokio::test]
    async fn assistant_appears_after_settling_retry() {
        let mock = MockServer::start().await;
        let transcript_seq = SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([transcript_message(
                "user",
                "msg-dispatch",
                &serde_json::json!([])
            )])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([
                transcript_message("user", "msg-dispatch", &serde_json::json!([])),
                transcript_message(
                    "assistant",
                    "msg-assistant",
                    &serde_json::json!([
                        { "type": "text", "text": "done" },
                        { "type": "step-finish", "reason": "stop", "cost": 0.0 }
                    ])
                )
            ])),
        ]);
        let transcript_seq_for_assert = transcript_seq.clone();
        Mock::given(method("GET"))
            .and(path("/session/session-1/message"))
            .respond_with(transcript_seq)
            .mount(&mock)
            .await;

        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let validation = supervisor
            .validate_completion_with_retries(
                "session-1",
                &TranscriptWindow {
                    command_message_id: "msg-dispatch".to_string(),
                    baseline_tail_message_id: None,
                },
                1,
            )
            .await;

        let CompletionValidation::Passed(diagnostics) = validation else {
            panic!("expected transcript validation success after assistant retry");
        };
        assert_eq!(
            diagnostics.final_assistant_message_id.as_deref(),
            Some("msg-assistant")
        );
        assert!(transcript_seq_for_assert.call_count() >= 2);
    }

    #[tokio::test]
    async fn unresolved_tool_state_retries_until_resolved() {
        let mock = MockServer::start().await;
        let transcript_seq = SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([
                transcript_message("user", "msg-dispatch", &serde_json::json!([])),
                transcript_message(
                    "assistant",
                    "msg-assistant",
                    &serde_json::json!([
                        {
                            "type": "tool",
                            "callID": "call-1",
                            "tool": "read",
                            "state": {
                                "status": "pending",
                                "input": {},
                                "raw": "read"
                            }
                        },
                        { "type": "step-finish", "reason": "stop", "cost": 0.0 }
                    ])
                )
            ])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([
                transcript_message("user", "msg-dispatch", &serde_json::json!([])),
                transcript_message(
                    "assistant",
                    "msg-assistant",
                    &serde_json::json!([
                        {
                            "type": "tool",
                            "callID": "call-1",
                            "tool": "read",
                            "state": {
                                "status": "completed",
                                "input": {},
                                "output": "ok",
                                "title": "read",
                                "metadata": {},
                                "time": { "start": 1, "end": 2 }
                            }
                        },
                        { "type": "step-finish", "reason": "stop", "cost": 0.0 }
                    ])
                )
            ])),
        ]);
        let transcript_seq_for_assert = transcript_seq.clone();
        Mock::given(method("GET"))
            .and(path("/session/session-1/message"))
            .respond_with(transcript_seq)
            .mount(&mock)
            .await;

        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let validation = supervisor
            .validate_completion_with_retries(
                "session-1",
                &TranscriptWindow {
                    command_message_id: "msg-dispatch".to_string(),
                    baseline_tail_message_id: None,
                },
                1,
            )
            .await;

        assert!(matches!(validation, CompletionValidation::Passed(_)));
        assert!(transcript_seq_for_assert.call_count() >= 2);
    }

    #[tokio::test]
    async fn unresolved_tool_state_after_settling_fails() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/session/session-1/message"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                transcript_message("user", "msg-dispatch", &serde_json::json!([])),
                transcript_message(
                    "assistant",
                    "msg-assistant",
                    &serde_json::json!([
                        {
                            "type": "tool",
                            "callID": "call-1",
                            "tool": "read"
                        },
                        { "type": "step-finish", "reason": "stop", "cost": 0.0 }
                    ])
                )
            ])))
            .mount(&mock)
            .await;

        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let validation = supervisor
            .validate_completion_with_retries(
                "session-1",
                &TranscriptWindow {
                    command_message_id: "msg-dispatch".to_string(),
                    baseline_tail_message_id: None,
                },
                1,
            )
            .await;

        let CompletionValidation::Failed { error, .. } = validation else {
            panic!("expected unresolved tool state to fail after retries");
        };
        assert!(error.contains("unresolved tool call"));
    }

    #[tokio::test]
    async fn run_command_supervised_does_not_complete_before_dispatch_confirmation() {
        let mock = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/session/session-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "session-1",
                "slug": "session-1",
                "projectId": "proj-1",
                "directory": "/tmp",
                "path": null,
                "title": "Test Session",
                "version": "1.0",
                "time": { "created": 1, "updated": 1 }
            })))
            .mount(&mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/permission"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/question"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/event"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                "data: {\"type\":\"server.connected\",\"properties\":{}}\n\n",
                "text/event-stream",
            ))
            .mount(&mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/session/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/session/session-1/command"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_secs(2))
                    .set_body_json(serde_json::json!({})),
            )
            .mount(&mock)
            .await;

        let transcript_seq = SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([transcript_message(
                "user",
                "msg-baseline",
                &serde_json::json!([])
            )])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([
                transcript_message("user", "msg-baseline", &serde_json::json!([])),
                transcript_message(
                    "assistant",
                    "msg-assistant",
                    &serde_json::json!([
                        { "type": "text", "text": "done" },
                        { "type": "step-finish", "reason": "stop", "cost": 0.0 }
                    ]),
                )
            ])),
        ]);
        let transcript_seq_for_assert = transcript_seq.clone();
        Mock::given(method("GET"))
            .and(path("/session/session-1/message"))
            .respond_with(transcript_seq)
            .mount(&mock)
            .await;

        let temp_dir = TempDir::new().unwrap();
        let supervisor = test_supervisor(&mock, temp_dir.path());
        let mut handle = tokio::spawn(async move {
            supervisor
                .run_command_supervised(Some("session-1"), "implement_plan", Some("do it"))
                .await
        });

        assert!(
            timeout(Duration::from_millis(1200), &mut handle)
                .await
                .is_err(),
            "supervisor should still be waiting before dispatch is confirmed"
        );

        let outcome = timeout(Duration::from_secs(5), &mut handle)
            .await
            .expect("supervisor should eventually complete")
            .expect("join should succeed")
            .expect("run should succeed");

        let SupervisedOutcome::Completed {
            diagnostics,
            literal_post_attempts,
            ..
        } = outcome
        else {
            panic!("expected completed outcome");
        };
        assert_eq!(literal_post_attempts, 1);
        assert_eq!(diagnostics.literal_post_attempts, 1);
        assert!(
            transcript_seq_for_assert.call_count() >= 2,
            "expected baseline and completion transcript fetches"
        );
    }

    #[tokio::test]
    async fn command_transport_error_after_busy_observed_continues_until_idle_and_completes() {
        let mock = MockServer::start().await;

        mount_existing_session(&mock).await;
        mount_empty_interruptions(&mock).await;
        Mock::given(method("GET"))
            .and(path("/event"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(
                        "data: {\"type\":\"server.connected\",\"properties\":{}}\n\ndata: {\"type\":\"message.part.updated\",\"properties\":{\"sessionID\":\"session-1\"}}\n\n",
                        "text/event-stream",
                    ),
            )
            .mount(&mock)
            .await;

        let status_seq = SequenceResponder::new(vec![
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"session-1": {"type": "busy"}})),
            ResponseTemplate::new(200).set_body_json(serde_json::json!({})),
        ]);
        Mock::given(method("GET"))
            .and(path("/session/status"))
            .respond_with(status_seq)
            .mount(&mock)
            .await;

        Mock::given(method("POST"))
            .and(path("/session/session-1/command"))
            .respond_with(
                ResponseTemplate::new(307)
                    .set_delay(Duration::from_millis(50))
                    .insert_header("location", "http://127.0.0.1:1/redirected-command"),
            )
            .mount(&mock)
            .await;

        let transcript_seq = SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([transcript_message(
                "user",
                "msg-baseline",
                &serde_json::json!([])
            )])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([
                transcript_message("user", "msg-baseline", &serde_json::json!([])),
                transcript_message(
                    "assistant",
                    "msg-assistant",
                    &serde_json::json!([
                        { "type": "text", "text": "done" },
                        { "type": "step-finish", "reason": "stop", "cost": 0.0 }
                    ]),
                )
            ])),
        ]);
        Mock::given(method("GET"))
            .and(path("/session/session-1/message"))
            .respond_with(transcript_seq)
            .mount(&mock)
            .await;

        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let outcome = supervisor
            .run_command_supervised(Some("session-1"), "implement_plan", Some("do it"))
            .await
            .expect("run should succeed");

        let SupervisedOutcome::Completed { diagnostics, .. } = outcome else {
            panic!("expected completed outcome after post-start transport error");
        };
        assert!(diagnostics.command_transport_error.is_some());
    }

    #[tokio::test]
    async fn attach_transport_warning_sets_completed_diagnostics_warning() {
        let warning = Some("Transport error: timeout".to_string());
        let outcome = attach_transport_warning(
            SupervisedOutcome::Completed {
                session_id: "session-1".to_string(),
                diagnostics: OpenCodeDiagnostics {
                    checked_at: "2026-01-01T00:00:00Z".to_string(),
                    literal_post_attempts: 1,
                    command_message_id: Some("msg-dispatch".to_string()),
                    final_assistant_message_id: Some("msg-assistant".to_string()),
                    final_finish_reason: Some("stop".to_string()),
                    guard_detected: false,
                    final_tool_error: None,
                    command_transport_error: None,
                },
                literal_post_attempts: 1,
                task_disposition: TaskDisposition {
                    server_abort: ServerAbortDisposition::NotRequired,
                    local_task: LocalTaskDisposition::Completed,
                },
            },
            warning.as_ref(),
        );

        let SupervisedOutcome::Completed { diagnostics, .. } = outcome else {
            panic!("expected completed outcome");
        };
        assert_eq!(
            diagnostics.command_transport_error.as_deref(),
            Some("Transport error: timeout")
        );
    }

    #[tokio::test]
    async fn uncertain_terminalization_aborts_server_then_joins_local_task() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/session/session-1/abort"))
            .respond_with(ResponseTemplate::new(200).set_body_json(true))
            .expect(1)
            .mount(&mock)
            .await;
        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let mut task = Some(tokio::spawn(async {
            std::future::pending::<()>().await;
            Ok::<(), OpencodeError>(())
        }));

        let disposition = supervisor
            .terminalize_command_task("session-1", &mut task, LocalTaskDisposition::Spawned, true)
            .await;

        assert_eq!(
            disposition.server_abort,
            ServerAbortDisposition::Succeeded { aborted: true }
        );
        assert_eq!(
            disposition.local_task,
            LocalTaskDisposition::AbortedAndJoined
        );
        assert!(task.is_none());
    }

    #[tokio::test]
    async fn normal_terminalization_never_aborts_server() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/session/session-1/abort"))
            .respond_with(ResponseTemplate::new(200).set_body_json(true))
            .expect(0)
            .mount(&mock)
            .await;
        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let mut task = Some(tokio::spawn(async {
            std::future::pending::<()>().await;
            Ok::<(), OpencodeError>(())
        }));

        let disposition = supervisor
            .terminalize_command_task("session-1", &mut task, LocalTaskDisposition::Spawned, false)
            .await;

        assert_eq!(
            disposition.server_abort,
            ServerAbortDisposition::NotRequired
        );
        assert_eq!(
            disposition.local_task,
            LocalTaskDisposition::AbortedAndJoined
        );
    }

    #[tokio::test]
    async fn persistence_callback_failure_aborts_server_and_joins_command_task() {
        let mock = MockServer::start().await;
        mount_existing_session(&mock).await;
        mount_empty_interruptions(&mock).await;
        mount_stalled_event_stream(&mock).await;
        Mock::given(method("GET"))
            .and(path("/session/session-1/message"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/session/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/session/session-1/command"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_secs(30))
                    .set_body_json(serde_json::json!({})),
            )
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/session/session-1/abort"))
            .respond_with(ResponseTemplate::new(200).set_body_json(true))
            .expect(1)
            .mount(&mock)
            .await;

        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let PreparedCommandOutcome::Prepared(prepared) = supervisor
            .prepare_command(
                SessionSelection::Reuse("session-1".to_string()),
                "implement_plan",
                Some("do it"),
            )
            .await
            .unwrap()
        else {
            panic!("expected prepared command");
        };
        let outcome = supervisor
            .run_prepared_command(prepared, None, None, |event| match event {
                SupervisionEvent::LiteralPostAttempt { .. } => {
                    anyhow::bail!("state persistence failed")
                }
                _ => Ok(()),
            })
            .await
            .unwrap();

        let SupervisedOutcome::Failed {
            failure,
            task_disposition,
            ..
        } = outcome
        else {
            panic!("expected terminal failure");
        };
        assert_eq!(failure, InvocationFailure::Persistence);
        assert_eq!(
            task_disposition.server_abort,
            ServerAbortDisposition::Succeeded { aborted: true }
        );
        assert_eq!(
            task_disposition.local_task,
            LocalTaskDisposition::AbortedAndJoined
        );
    }

    async fn run_owner_pause_continuation_case(
        interruption_event: serde_json::Value,
        reply_path: &str,
        response: crate::owner::InterruptionResponse,
        pending_repetitions: usize,
        interleave_sse_after_poll: bool,
    ) {
        let mock = MockServer::start().await;
        mount_existing_session(&mock).await;
        if interleave_sse_after_poll {
            Mock::given(method("GET"))
                .and(path("/event"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_delay(Duration::from_millis(300))
                        .set_body_raw(
                            format!(
                                "data: {{\"type\":\"server.connected\",\"properties\":{{}}}}\n\ndata: {interruption_event}\n\n"
                            ),
                            "text/event-stream",
                        ),
                )
                .mount(&mock)
                .await;
        } else {
            mount_stalled_event_stream(&mock).await;
        }
        let properties = interruption_event
            .get("properties")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let empty = ResponseTemplate::new(200).set_body_json(serde_json::json!([]));
        let pending =
            ResponseTemplate::new(200).set_body_json(serde_json::Value::Array(vec![properties]));
        let mut pending_sequence = vec![empty.clone()];
        pending_sequence.extend(std::iter::repeat_n(pending, pending_repetitions));
        pending_sequence.push(empty.clone());
        match interruption_event
            .get("type")
            .and_then(serde_json::Value::as_str)
        {
            Some("permission.asked") => {
                Mock::given(method("GET"))
                    .and(path("/permission"))
                    .respond_with(SequenceResponder::new(pending_sequence))
                    .mount(&mock)
                    .await;
                Mock::given(method("GET"))
                    .and(path("/question"))
                    .respond_with(empty.clone())
                    .mount(&mock)
                    .await;
            }
            Some("question.asked") => {
                Mock::given(method("GET"))
                    .and(path("/permission"))
                    .respond_with(empty.clone())
                    .mount(&mock)
                    .await;
                Mock::given(method("GET"))
                    .and(path("/question"))
                    .respond_with(SequenceResponder::new(pending_sequence))
                    .mount(&mock)
                    .await;
            }
            other => panic!("unsupported interruption event type: {other:?}"),
        }
        Mock::given(method("GET"))
            .and(path("/session/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/session/session-1/command"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(100))
                    .set_body_json(serde_json::json!({})),
            )
            .expect(1)
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path(reply_path))
            .respond_with(ResponseTemplate::new(200).set_body_json(true))
            .expect(1)
            .mount(&mock)
            .await;
        Mock::given(method("POST"))
            .and(path("/session/session-1/abort"))
            .respond_with(ResponseTemplate::new(200).set_body_json(true))
            .expect(0)
            .mount(&mock)
            .await;
        let transcript_seq = SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([transcript_message(
                "user",
                "msg-baseline",
                &serde_json::json!([])
            )])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([
                transcript_message("user", "msg-baseline", &serde_json::json!([])),
                transcript_message(
                    "assistant",
                    "msg-assistant",
                    &serde_json::json!([
                        { "type": "text", "text": "done" },
                        { "type": "step-finish", "reason": "stop", "cost": 0.0 }
                    ])
                )
            ])),
        ]);
        Mock::given(method("GET"))
            .and(path("/session/session-1/message"))
            .respond_with(transcript_seq)
            .mount(&mock)
            .await;

        let worktree = TempDir::new().unwrap();
        let owner = crate::owner::OwnerRuntime::acquire(worktree.path()).unwrap();
        let supervisor = test_supervisor(&mock, worktree.path());
        let PreparedCommandOutcome::Prepared(prepared) = supervisor
            .prepare_command(
                SessionSelection::Reuse("session-1".to_string()),
                "implement_plan",
                Some("do it"),
            )
            .await
            .unwrap()
        else {
            panic!("expected prepared command");
        };
        let owner_context = InvocationOwnerContext {
            run_id: "run-1".to_string(),
            invocation_id: "inv-1".to_string(),
        };
        let observed_events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let callback_events = Arc::clone(&observed_events);
        let run = tokio::spawn(async move {
            supervisor
                .run_prepared_command(prepared, Some(&owner), Some(&owner_context), move |event| {
                    callback_events.lock().unwrap().push(match event {
                        SupervisionEvent::Paused { .. } => "paused",
                        SupervisionEvent::Resumed { .. } => "resumed",
                        SupervisionEvent::LiteralPostAttempt { .. } => "post",
                        SupervisionEvent::AssistantStarted { .. } => "assistant",
                    });
                    Ok(())
                })
                .await
        });

        let mut sent = false;
        for _ in 0..100 {
            if crate::owner::send_response(worktree.path(), response.clone())
                .await
                .is_ok()
            {
                sent = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(sent, "responder should reach the live foreground owner");
        let outcome = timeout(Duration::from_secs(5), run)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(outcome, SupervisedOutcome::Completed { .. }));
        let events = observed_events.lock().unwrap();
        assert!(events.contains(&"post"));
        assert_eq!(events.iter().filter(|event| **event == "paused").count(), 1);
        assert_eq!(
            events.iter().filter(|event| **event == "resumed").count(),
            1
        );
    }

    #[tokio::test]
    async fn permission_pause_continues_same_one_post_invocation() {
        run_owner_pause_continuation_case(
            serde_json::json!({
                "type": "permission.asked",
                "properties": {
                    "id": "permission-1",
                    "sessionID": "session-1",
                    "permission": "file.write",
                    "patterns": ["**/*.rs"]
                }
            }),
            "/permission/permission-1/reply",
            crate::owner::InterruptionResponse::Permission { allow: true },
            1,
            false,
        )
        .await;
    }

    #[tokio::test]
    async fn question_pause_continues_same_one_post_invocation() {
        run_owner_pause_continuation_case(
            serde_json::json!({
                "type": "question.asked",
                "properties": {
                    "id": "question-1",
                    "sessionID": "session-1",
                    "questions": [{"question": "Continue?"}]
                }
            }),
            "/question/question-1/reply",
            crate::owner::InterruptionResponse::Question {
                answers: vec![vec!["Yes".to_string()]],
            },
            1,
            false,
        )
        .await;
    }

    #[tokio::test]
    async fn duplicate_polled_permission_is_handled_once() {
        run_owner_pause_continuation_case(
            serde_json::json!({
                "type": "permission.asked",
                "properties": {
                    "id": "permission-1",
                    "sessionID": "session-1",
                    "permission": "file.write",
                    "patterns": ["**/*.rs"]
                }
            }),
            "/permission/permission-1/reply",
            crate::owner::InterruptionResponse::Permission { allow: true },
            2,
            false,
        )
        .await;
    }

    #[tokio::test]
    async fn polled_question_then_duplicate_sse_event_is_handled_once() {
        run_owner_pause_continuation_case(
            serde_json::json!({
                "type": "question.asked",
                "properties": {
                    "id": "question-1",
                    "sessionID": "session-1",
                    "questions": [{"question": "Continue?"}]
                }
            }),
            "/question/question-1/reply",
            crate::owner::InterruptionResponse::Question {
                answers: vec![vec!["Yes".to_string()]],
            },
            1,
            true,
        )
        .await;
    }

    #[test]
    fn interruption_deduplication_distinguishes_kinds_for_same_request_id() {
        let mut handled = HashSet::new();
        handled.insert((
            crate::state::InterruptionKind::Permission,
            "request-1".to_string(),
        ));

        assert!(!handled.contains(&(
            crate::state::InterruptionKind::Question,
            "request-1".to_string(),
        )));
    }

    #[tokio::test]
    async fn command_transport_error_before_any_start_evidence_fails_fast() {
        let mock = MockServer::start().await;

        mount_existing_session(&mock).await;
        mount_empty_interruptions(&mock).await;
        mount_stalled_event_stream(&mock).await;

        Mock::given(method("GET"))
            .and(path("/session/status"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&mock)
            .await;

        Mock::given(method("POST"))
            .and(path("/session/session-1/command"))
            .respond_with(
                ResponseTemplate::new(307)
                    .set_delay(Duration::from_secs(2))
                    .insert_header("location", "http://127.0.0.1:1/redirected-command"),
            )
            .mount(&mock)
            .await;

        let transcript_seq = SequenceResponder::new(vec![
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
            ResponseTemplate::new(200).set_body_json(serde_json::json!([])),
        ]);
        Mock::given(method("GET"))
            .and(path("/session/session-1/message"))
            .respond_with(transcript_seq)
            .mount(&mock)
            .await;

        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let outcome = supervisor
            .run_command_supervised(Some("session-1"), "implement_plan", Some("do it"))
            .await
            .expect("run should return classified failure");

        let SupervisedOutcome::Failed {
            error,
            diagnostics: Some(diagnostics),
            ..
        } = outcome
        else {
            panic!("expected failed outcome without start evidence");
        };
        assert!(error.contains("no session start evidence observed"));
        assert!(diagnostics.command_transport_error.is_some());
    }

    #[tokio::test]
    async fn command_transport_error_before_busy_but_transcript_shows_dispatch_window_continues() {
        let messages = parse_messages(serde_json::json!([
            transcript_message("assistant", "msg-baseline", &serde_json::json!([])),
            transcript_message("user", "msg-dispatch-window", &serde_json::json!([])),
        ]));

        assert!(transcript_indicates_dispatch(
            &messages,
            &TranscriptWindow {
                command_message_id: "msg-missing".to_string(),
                baseline_tail_message_id: Some("msg-baseline".to_string()),
            },
        ));
    }

    fn test_supervisor(mock: &MockServer, directory: &Path) -> OpenCodeSupervisor {
        let child = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let managed = ManagedServer::from_child_for_testing(child, mock.uri(), 1234);
        let client = Client::builder()
            .base_url(mock.uri())
            .directory(directory.display().to_string())
            .build()
            .unwrap();
        OpenCodeSupervisor {
            _managed_server: managed,
            client,
            _directory: directory.to_path_buf(),
            timeouts: test_timeouts(),
        }
    }

    async fn mount_existing_session(mock: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/session/session-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": "session-1",
                "slug": "session-1",
                "projectId": "proj-1",
                "directory": "/tmp",
                "path": null,
                "title": "Test Session",
                "version": "1.0",
                "time": { "created": 1, "updated": 1 }
            })))
            .mount(mock)
            .await;
    }

    async fn mount_empty_interruptions(mock: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/permission"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/question"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(mock)
            .await;
    }

    async fn mount_stalled_event_stream(mock: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/event"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                "data: {\"type\":\"server.connected\",\"properties\":{}}\n\n",
                "text/event-stream",
            ))
            .mount(mock)
            .await;
    }

    #[test]
    fn resolve_launcher_config_errors_when_args_set_but_binary_missing() {
        let _guard = process_state_lock().lock().unwrap();
        let _binary = EnvVarGuard::remove(version::OPENCODE_BINARY_ENV);
        let _args = EnvVarGuard::set(version::OPENCODE_BINARY_ARGS_ENV, "serve --help");

        let err = resolve_launcher_config(Path::new("/tmp/project"))
            .expect_err("missing launcher binary should fail");

        assert!(
            err.to_string()
                .contains("OPENCODE_BINARY_ARGS is set but OPENCODE_BINARY is not set")
        );
    }

    #[test]
    fn resolve_launcher_config_errors_when_args_set_but_binary_empty() {
        let _guard = process_state_lock().lock().unwrap();
        let _binary = EnvVarGuard::set(version::OPENCODE_BINARY_ENV, "   ");
        let _args = EnvVarGuard::set(version::OPENCODE_BINARY_ARGS_ENV, "serve --help");

        let err = resolve_launcher_config(Path::new("/tmp/project"))
            .expect_err("empty launcher binary should fail");

        assert!(err.to_string().contains("OPENCODE_BINARY is empty"));
    }

    #[test]
    fn resolve_launcher_config_accepts_explicit_binary_with_args() {
        let _guard = process_state_lock().lock().unwrap();
        let _binary = EnvVarGuard::set(version::OPENCODE_BINARY_ENV, "bunx");
        let _args = EnvVarGuard::set(
            version::OPENCODE_BINARY_ARGS_ENV,
            "--yes opencode-ai@1.17.4",
        );

        let config = resolve_launcher_config(Path::new("/tmp/project"))
            .expect("explicit launcher binary should succeed");

        assert_eq!(config.binary, "bunx");
        assert_eq!(config.launcher_args, vec!["--yes", "opencode-ai@1.17.4"]);
    }
}
