//! Tool implementations for orchestrator MCP server.

use crate::config;
use crate::logging;
use crate::server::AgentPolicyDecision;
use crate::server::CommandPolicyDecision;
use crate::server::OrchestratorServer;
use crate::server::OrchestratorServerHandle;
use crate::token_tracker::TokenTracker;
use crate::types::AgentInfo;
use crate::types::ChangeStats;
use crate::types::CommandInfo;
use crate::types::GetSessionStateInput;
use crate::types::GetSessionStateOutput;
use crate::types::ListAgentsInput;
use crate::types::ListAgentsOutput;
use crate::types::ListCommandsInput;
use crate::types::ListCommandsOutput;
use crate::types::ListSessionsInput;
use crate::types::ListSessionsOutput;
use crate::types::OrchestratorRunInput;
use crate::types::OrchestratorRunOutput;
use crate::types::PermissionReply;
use crate::types::QuestionAction;
use crate::types::QuestionInfoView;
use crate::types::QuestionOptionView;
use crate::types::RespondPermissionInput;
use crate::types::RespondPermissionOutput;
use crate::types::RespondQuestionInput;
use crate::types::RespondQuestionOutput;
use crate::types::RunStatus;
use crate::types::SessionStatusSummary;
use crate::types::SessionSummary;
use crate::types::ToolCallSummary;
use crate::types::ToolStateSummary;
use agentic_logging::CallTimer;
use agentic_logging::ToolCallRecord;
use agentic_tools_core::Tool;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use agentic_tools_core::ToolRegistry;
use agentic_tools_core::fmt::TextFormat;
use agentic_tools_core::fmt::TextOptions;
use futures::future::BoxFuture;
use opencode_rs::OpencodeError;
use opencode_rs::types::event::Event;
use opencode_rs::types::event::MessagePartEventProps;
use opencode_rs::types::event::MessageUpdatedProps;
use opencode_rs::types::message::CommandRequest;
use opencode_rs::types::message::Message;
use opencode_rs::types::message::Part;
use opencode_rs::types::message::PromptPart;
use opencode_rs::types::message::PromptRequest;
use opencode_rs::types::message::ToolState;
use opencode_rs::types::new_message_id;
use opencode_rs::types::permission::PermissionReply as ApiPermissionReply;
use opencode_rs::types::permission::PermissionReplyRequest;
use opencode_rs::types::permission::PermissionRequest;
use opencode_rs::types::question::QuestionReply;
use opencode_rs::types::question::QuestionRequest;
use opencode_rs::types::session::CreateSessionRequest;
use opencode_rs::types::session::SessionStatusInfo;
use opencode_rs::types::session::SummarizeRequest;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

const SERVER_NAME: &str = "opencode-orchestrator-mcp";
const PRECORRELATION_TOTAL_BYTES: usize = 64 * 1024;
const PRECORRELATION_MESSAGE_BYTES: usize = 16 * 1024;
const PRECORRELATION_MESSAGE_IDS: usize = 32;

#[derive(Debug, Clone, Default)]
struct ToolLogMeta {
    token_usage: Option<agentic_logging::TokenUsage>,
    token_usage_saturated: bool,
}

struct RunOutcome {
    output: OrchestratorRunOutput,
    log_meta: ToolLogMeta,
}

#[derive(Clone, Copy, Debug)]
enum PermissionPreflightMode {
    Strict,
    RespondPermissionContinuation,
}

#[derive(Clone, Copy, Debug)]
enum BlockerDetectionSource {
    Preflight(PermissionPreflightMode),
    Fallback,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum CallerResponseBlockerKind {
    Permission,
    Question,
}

#[derive(Clone, Debug)]
enum CallerResponseBlockerPayload {
    Permission(PermissionRequest),
    Question(QuestionRequest),
}

#[derive(Clone, Debug)]
struct CallerResponseBlocker {
    root_session_id: String,
    owner_session_id: String,
    owner_depth: usize,
    request_id: String,
    kind: CallerResponseBlockerKind,
    payload: CallerResponseBlockerPayload,
}

#[derive(Clone, Debug)]
enum BlockerScanResult {
    Found(Box<CallerResponseBlocker>),
    Clear,
    Inconclusive,
}

const LATCH_CLEAR_POLLS: u8 = 2;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct LatchedCallerResponseKey {
    kind: CallerResponseBlockerKind,
    owner_session_id: String,
    request_id: String,
}

#[derive(Debug, Default)]
struct CallerResponseEventLatch {
    clear_polls_by_request: HashMap<LatchedCallerResponseKey, u8>,
}

#[derive(Debug, Default)]
struct IdleCompletionMonitor {
    completion_candidate_pending: bool,
}

enum IdleCompletionOutcome {
    Continue,
    Blocker(Box<CallerResponseBlocker>),
    Complete,
}

impl IdleCompletionMonitor {
    fn reset_on_root_progress(&mut self) {
        self.completion_candidate_pending = false;
    }

    fn reset_completion_candidate(&mut self) {
        self.completion_candidate_pending = false;
    }
}

impl CallerResponseEventLatch {
    fn latch(&mut self, kind: CallerResponseBlockerKind, owner_session_id: &str, request_id: &str) {
        self.clear_polls_by_request.insert(
            LatchedCallerResponseKey {
                kind,
                owner_session_id: owner_session_id.to_string(),
                request_id: request_id.to_string(),
            },
            0,
        );
    }

    fn remove(
        &mut self,
        kind: CallerResponseBlockerKind,
        owner_session_id: &str,
        request_id: &str,
    ) {
        self.clear_polls_by_request
            .remove(&LatchedCallerResponseKey {
                kind,
                owner_session_id: owner_session_id.to_string(),
                request_id: request_id.to_string(),
            });
    }

    fn observe_poll_scan(&mut self, result: &BlockerScanResult) {
        match result {
            BlockerScanResult::Clear => {
                self.clear_polls_by_request.retain(|_, clear_polls| {
                    *clear_polls = clear_polls.saturating_add(1);
                    *clear_polls < LATCH_CLEAR_POLLS
                });
            }
            BlockerScanResult::Inconclusive => {
                for clear_polls in self.clear_polls_by_request.values_mut() {
                    *clear_polls = 0;
                }
            }
            BlockerScanResult::Found(_) => {}
        }
    }

    fn is_empty(&self) -> bool {
        self.clear_polls_by_request.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OwnerEligibility {
    Eligible(usize),
    Ineligible,
    Inconclusive,
}

#[derive(Debug, Default)]
struct SessionLineageResolver {
    sessions_by_id: HashMap<String, opencode_rs::types::session::Session>,
    eligible_depth_by_owner: HashMap<String, usize>,
}

impl SessionLineageResolver {
    async fn eligible_owner_depth(
        &mut self,
        client: &opencode_rs::Client,
        root_session_id: &str,
        owner_session_id: &str,
    ) -> Result<Option<usize>, OpencodeError> {
        if owner_session_id == root_session_id {
            return Ok(Some(0));
        }

        if let Some(depth) = self.eligible_depth_by_owner.get(owner_session_id) {
            return Ok(Some(*depth));
        }

        let mut visited = HashSet::from([owner_session_id.to_string()]);
        let mut current_session_id = owner_session_id.to_string();
        let mut depth = 0_usize;

        loop {
            let session = if let Some(session) = self.sessions_by_id.get(&current_session_id) {
                session.clone()
            } else {
                match client.sessions().get(&current_session_id).await {
                    Ok(session) => {
                        self.sessions_by_id
                            .insert(current_session_id.clone(), session.clone());
                        session
                    }
                    Err(error) if error.is_not_found() => return Ok(None),
                    Err(error) => return Err(error),
                }
            };

            let Some(parent_session_id) = session.parent_id else {
                return Ok(None);
            };
            depth = depth.saturating_add(1);

            if parent_session_id == root_session_id {
                self.eligible_depth_by_owner
                    .insert(owner_session_id.to_string(), depth);
                return Ok(Some(depth));
            }

            if !visited.insert(parent_session_id.clone()) {
                return Ok(None);
            }
            current_session_id = parent_session_id;
        }
    }
}

fn compare_caller_response_blockers(
    left: &CallerResponseBlocker,
    right: &CallerResponseBlocker,
) -> std::cmp::Ordering {
    let kind_rank = |kind| match kind {
        CallerResponseBlockerKind::Permission => 0_u8,
        CallerResponseBlockerKind::Question => 1_u8,
    };

    kind_rank(left.kind)
        .cmp(&kind_rank(right.kind))
        .then_with(|| left.owner_depth.cmp(&right.owner_depth))
        .then_with(|| left.owner_session_id.cmp(&right.owner_session_id))
        .then_with(|| left.request_id.cmp(&right.request_id))
}

async fn eligible_owner_depth_for_scan(
    lineage: &mut SessionLineageResolver,
    client: &opencode_rs::Client,
    root_session_id: &str,
    owner_session_id: &str,
) -> OwnerEligibility {
    match lineage
        .eligible_owner_depth(client, root_session_id, owner_session_id)
        .await
    {
        Ok(Some(depth)) => OwnerEligibility::Eligible(depth),
        Ok(None) => OwnerEligibility::Ineligible,
        Err(error) => {
            tracing::warn!(
                root_session_id,
                owner_session_id,
                error = %error,
                "failed to verify pending blocker owner ancestry"
            );
            OwnerEligibility::Inconclusive
        }
    }
}

async fn scan_pending_caller_response_blocker(
    client: &opencode_rs::Client,
    root_session_id: &str,
    lineage: &mut SessionLineageResolver,
    source: BlockerDetectionSource,
    warnings: &mut Vec<String>,
) -> Result<BlockerScanResult, ToolError> {
    let mut inconclusive = false;
    let permissions = match client.permissions().list().await {
        Ok(permissions) => permissions,
        Err(error) => match source {
            BlockerDetectionSource::Preflight(permission_mode) if error.is_validation_error() => {
                match permission_mode {
                    PermissionPreflightMode::RespondPermissionContinuation => {
                        tracing::warn!(
                            session_id = %root_session_id,
                            error = %error,
                            "failed to list permissions during respond_permission continuation; falling back to polling"
                        );
                        warnings.push(format!(
                            "Permission refresh failed after reply ({error}); permission state could not be listed and may be stale or malformed. Continuing with polling fallback."
                        ));
                    }
                    PermissionPreflightMode::Strict => {
                        tracing::warn!(
                            session_id = %root_session_id,
                            error = %error,
                            "failed to list permissions during run preflight; continuing without permission preflight"
                        );
                        warnings.push(
                            "Permission state could not be listed during preflight (HTTP 400). Permission state may be stale or malformed; pending permissions may be stale or undiscoverable."
                                .to_string(),
                        );
                    }
                }
                inconclusive = true;
                Vec::new()
            }
            BlockerDetectionSource::Preflight(_) => {
                return Err(ToolError::Internal(format!(
                    "Failed to list permissions: {error}"
                )));
            }
            BlockerDetectionSource::Fallback => {
                tracing::warn!(
                    session_id = %root_session_id,
                    error = %error,
                    "failed to list permissions during blocker scan fallback"
                );
                inconclusive = true;
                Vec::new()
            }
        },
    };

    let mut blockers = Vec::new();
    for permission in permissions {
        match eligible_owner_depth_for_scan(
            lineage,
            client,
            root_session_id,
            &permission.session_id,
        )
        .await
        {
            OwnerEligibility::Eligible(owner_depth) => {
                blockers.push(CallerResponseBlocker {
                    root_session_id: root_session_id.to_string(),
                    owner_session_id: permission.session_id.clone(),
                    owner_depth,
                    request_id: permission.id.clone(),
                    kind: CallerResponseBlockerKind::Permission,
                    payload: CallerResponseBlockerPayload::Permission(permission),
                });
            }
            OwnerEligibility::Ineligible => {}
            OwnerEligibility::Inconclusive => inconclusive = true,
        }
    }
    blockers.sort_by(compare_caller_response_blockers);
    if let Some(blocker) = blockers.into_iter().next() {
        return Ok(BlockerScanResult::Found(Box::new(blocker)));
    }

    let questions = match client.question().list().await {
        Ok(questions) => questions,
        Err(error) => match source {
            BlockerDetectionSource::Preflight(_) => {
                return Err(ToolError::Internal(format!(
                    "Failed to list questions: {error}"
                )));
            }
            BlockerDetectionSource::Fallback => {
                tracing::warn!(
                    session_id = %root_session_id,
                    error = %error,
                    "failed to list questions during blocker scan fallback"
                );
                inconclusive = true;
                Vec::new()
            }
        },
    };

    let mut blockers = Vec::new();
    for question in questions {
        match eligible_owner_depth_for_scan(lineage, client, root_session_id, &question.session_id)
            .await
        {
            OwnerEligibility::Eligible(owner_depth) => {
                blockers.push(CallerResponseBlocker {
                    root_session_id: root_session_id.to_string(),
                    owner_session_id: question.session_id.clone(),
                    owner_depth,
                    request_id: question.id.clone(),
                    kind: CallerResponseBlockerKind::Question,
                    payload: CallerResponseBlockerPayload::Question(question),
                });
            }
            OwnerEligibility::Ineligible => {}
            OwnerEligibility::Inconclusive => inconclusive = true,
        }
    }
    blockers.sort_by(compare_caller_response_blockers);
    if let Some(blocker) = blockers.into_iter().next() {
        Ok(BlockerScanResult::Found(Box::new(blocker)))
    } else if inconclusive {
        Ok(BlockerScanResult::Inconclusive)
    } else {
        Ok(BlockerScanResult::Clear)
    }
}

struct IdleCompletionContext<'a> {
    sse_active: bool,
    latch: &'a mut CallerResponseEventLatch,
    client: &'a opencode_rs::Client,
    root_session_id: &'a str,
    lineage: &'a mut SessionLineageResolver,
    warnings: &'a mut Vec<String>,
}

async fn evaluate_idle_completion(
    monitor: &mut IdleCompletionMonitor,
    initial_scan: BlockerScanResult,
    poll_driven: bool,
    may_complete: bool,
    context: IdleCompletionContext<'_>,
) -> Result<IdleCompletionOutcome, ToolError> {
    if poll_driven {
        context.latch.observe_poll_scan(&initial_scan);
    }

    match initial_scan {
        BlockerScanResult::Found(blocker) => {
            monitor.reset_on_root_progress();
            return Ok(IdleCompletionOutcome::Blocker(blocker));
        }
        BlockerScanResult::Inconclusive => {
            monitor.reset_completion_candidate();
            return Ok(IdleCompletionOutcome::Continue);
        }
        BlockerScanResult::Clear => {}
    }

    if !may_complete {
        return Ok(IdleCompletionOutcome::Continue);
    }

    if !context.latch.is_empty() {
        monitor.reset_completion_candidate();
        return Ok(IdleCompletionOutcome::Continue);
    }

    if context.sse_active && !monitor.completion_candidate_pending {
        monitor.completion_candidate_pending = true;
        return Ok(IdleCompletionOutcome::Continue);
    }

    let final_scan = scan_pending_caller_response_blocker(
        context.client,
        context.root_session_id,
        context.lineage,
        BlockerDetectionSource::Fallback,
        context.warnings,
    )
    .await?;
    match final_scan {
        BlockerScanResult::Found(blocker) => {
            monitor.reset_on_root_progress();
            Ok(IdleCompletionOutcome::Blocker(blocker))
        }
        BlockerScanResult::Inconclusive => Ok(IdleCompletionOutcome::Continue),
        BlockerScanResult::Clear if context.latch.is_empty() => {
            monitor.reset_on_root_progress();
            Ok(IdleCompletionOutcome::Complete)
        }
        BlockerScanResult::Clear => {
            monitor.reset_completion_candidate();
            Ok(IdleCompletionOutcome::Continue)
        }
    }
}

#[derive(Debug, Clone)]
struct CommandTranscriptWindow {
    command_message_id: String,
}

#[derive(Debug)]
struct BufferedDelta {
    message_id: String,
    text: String,
}

#[derive(Debug, Default)]
struct PreCorrelationDeltaBuffer {
    deltas: VecDeque<BufferedDelta>,
    bytes_by_message: HashMap<String, usize>,
    total_bytes: usize,
}

impl PreCorrelationDeltaBuffer {
    fn push(&mut self, message_id: &str, delta: &str) {
        if message_id.is_empty() || delta.is_empty() {
            return;
        }

        let delta_bytes = delta.len();
        if delta_bytes > PRECORRELATION_MESSAGE_BYTES || delta_bytes > PRECORRELATION_TOTAL_BYTES {
            return;
        }

        if !self.bytes_by_message.contains_key(message_id)
            && self.bytes_by_message.len() >= PRECORRELATION_MESSAGE_IDS
        {
            self.evict_oldest_message();
        }

        let message_bytes = self
            .bytes_by_message
            .get(message_id)
            .copied()
            .unwrap_or_default();
        if message_bytes.saturating_add(delta_bytes) > PRECORRELATION_MESSAGE_BYTES {
            return;
        }

        while self.total_bytes.saturating_add(delta_bytes) > PRECORRELATION_TOTAL_BYTES {
            let Some(oldest_message_id) = self.deltas.front().map(|delta| delta.message_id.clone())
            else {
                return;
            };
            if oldest_message_id == message_id {
                return;
            }
            self.evict_message(&oldest_message_id);
        }

        self.deltas.push_back(BufferedDelta {
            message_id: message_id.to_string(),
            text: delta.to_string(),
        });
        self.bytes_by_message
            .insert(message_id.to_string(), message_bytes + delta_bytes);
        self.total_bytes += delta_bytes;
    }

    fn take(&mut self, message_id: &str) -> String {
        let Some(removed_bytes) = self.bytes_by_message.remove(message_id) else {
            return String::new();
        };

        let mut text = String::with_capacity(removed_bytes);
        self.deltas.retain(|delta| {
            if delta.message_id == message_id {
                text.push_str(&delta.text);
                false
            } else {
                true
            }
        });
        self.total_bytes -= removed_bytes;
        text
    }

    fn evict_oldest_message(&mut self) {
        if let Some(message_id) = self.deltas.front().map(|delta| delta.message_id.clone()) {
            self.evict_message(&message_id);
        }
    }

    fn evict_message(&mut self, message_id: &str) {
        if let Some(removed_bytes) = self.bytes_by_message.remove(message_id) {
            self.deltas.retain(|delta| delta.message_id != message_id);
            self.total_bytes -= removed_bytes;
        }
    }
}

fn blocked_command_error(command: &str, decision: CommandPolicyDecision) -> ToolError {
    let reason = match decision {
        CommandPolicyDecision::Allowed => {
            return ToolError::Internal("command unexpectedly allowed".into());
        }
        CommandPolicyDecision::DeniedByAllowlist => {
            "it is not present in orchestrator.commands.allow"
        }
        CommandPolicyDecision::DeniedByDenylist => "it is blocked by orchestrator.commands.deny",
    };

    ToolError::InvalidInput(format!(
        "Command '{command}' cannot be run because {reason}."
    ))
}

fn blocked_agent_error(agent: &str, decision: AgentPolicyDecision) -> ToolError {
    let reason = match decision {
        AgentPolicyDecision::Allowed => {
            return ToolError::Internal("agent unexpectedly allowed".into());
        }
        AgentPolicyDecision::DeniedByAllowlist => "it is not present in orchestrator.agents.allow",
        AgentPolicyDecision::DeniedByDenylist => "it is blocked by orchestrator.agents.deny",
    };

    ToolError::InvalidInput(format!("Agent '{agent}' cannot be used because {reason}."))
}

impl RunOutcome {
    fn without_tokens(output: OrchestratorRunOutput) -> Self {
        Self {
            output,
            log_meta: ToolLogMeta::default(),
        }
    }

    fn with_tracker(output: OrchestratorRunOutput, token_tracker: &TokenTracker) -> Self {
        let (token_usage, token_usage_saturated) = token_tracker.to_log_token_usage();
        Self {
            output,
            log_meta: ToolLogMeta {
                token_usage,
                token_usage_saturated,
            },
        }
    }
}

async fn abort_command_task(task: &mut Option<JoinHandle<Result<(), OpencodeError>>>) {
    if let Some(handle) = task.take() {
        handle.abort();
        let _ = handle.await;
    }
}

fn transcript_indicates_command_dispatch(
    messages: &[Message],
    transcript_window: &CommandTranscriptWindow,
) -> bool {
    messages
        .iter()
        .any(|message| message.id() == transcript_window.command_message_id)
}

fn assistant_update_matches_command(
    transcript_window: Option<&CommandTranscriptWindow>,
    role: &str,
    parent_id: Option<&str>,
) -> bool {
    transcript_window.map_or(role == "assistant", |window| {
        role == "assistant" && parent_id == Some(window.command_message_id.as_str())
    })
}

fn part_event_matches_command(
    transcript_window: Option<&CommandTranscriptWindow>,
    message_id: Option<&str>,
    correlated_assistant_ids: &HashSet<String>,
) -> bool {
    transcript_window.is_none()
        || message_id.is_some_and(|message_id| correlated_assistant_ids.contains(message_id))
}

fn record_message_part_delta(
    properties: &MessagePartEventProps,
    transcript_window: Option<&CommandTranscriptWindow>,
    correlated_assistant_ids: &HashSet<String>,
    precorrelation_deltas: &mut Option<PreCorrelationDeltaBuffer>,
    partial_response: &mut String,
) -> bool {
    let correlated = part_event_matches_command(
        transcript_window,
        properties.message_id.as_deref(),
        correlated_assistant_ids,
    );
    if correlated && let Some(delta) = &properties.delta {
        partial_response.push_str(delta);
    } else if let (Some(buffer), Some(message_id), Some(delta)) = (
        precorrelation_deltas.as_mut(),
        properties.message_id.as_deref().filter(|id| !id.is_empty()),
        properties.delta.as_deref(),
    ) {
        buffer.push(message_id, delta);
    }
    correlated
}

fn record_message_updated(
    properties: &MessageUpdatedProps,
    transcript_window: Option<&CommandTranscriptWindow>,
    correlated_assistant_ids: &mut HashSet<String>,
    precorrelation_deltas: &mut Option<PreCorrelationDeltaBuffer>,
    partial_response: &mut String,
) -> bool {
    let correlated = assistant_update_matches_command(
        transcript_window,
        &properties.info.role,
        properties.info.parent_id.as_deref(),
    );
    let message_id = &properties.info.id;
    if correlated {
        correlated_assistant_ids.insert(message_id.clone());
        if let Some(buffer) = precorrelation_deltas.as_mut() {
            partial_response.push_str(&buffer.take(message_id));
        }
    } else if let Some(buffer) = precorrelation_deltas.as_mut() {
        buffer.evict_message(message_id);
    }
    correlated
}

fn request_json<T: Serialize>(request: &T) -> serde_json::Value {
    serde_json::to_value(request)
        .unwrap_or_else(|error| serde_json::json!({"serialization_error": error.to_string()}))
}

fn log_tool_success<TReq: Serialize, TOut: TextFormat>(
    timer: &CallTimer,
    tool: &str,
    request: &TReq,
    output: &TOut,
    log_meta: ToolLogMeta,
    write_markdown: bool,
) {
    let (completed_at, duration_ms) = timer.finish();
    let rendered = output.fmt_text(&TextOptions::default());
    let response_file = write_markdown
        .then(|| logging::write_markdown_best_effort(completed_at, &timer.call_id, &rendered))
        .flatten();

    let record = ToolCallRecord {
        call_id: timer.call_id.clone(),
        server: SERVER_NAME.into(),
        tool: tool.into(),
        started_at: timer.started_at,
        completed_at,
        duration_ms,
        request: request_json(request),
        response_file,
        success: true,
        error: None,
        failure_kind: None,
        model: None,
        token_usage: log_meta.token_usage,
        summary: log_meta
            .token_usage_saturated
            .then(|| serde_json::json!({"token_usage_saturated": true})),
    };

    logging::append_record_best_effort(&record);
}

fn log_tool_error<TReq: Serialize>(
    timer: &CallTimer,
    tool: &str,
    request: &TReq,
    error: &ToolError,
) {
    let (completed_at, duration_ms) = timer.finish();
    let error = error.to_string();
    let record = ToolCallRecord {
        call_id: timer.call_id.clone(),
        server: SERVER_NAME.into(),
        tool: tool.into(),
        started_at: timer.started_at,
        completed_at,
        duration_ms,
        request: request_json(request),
        response_file: None,
        success: false,
        error: Some(error.clone()),
        failure_kind: agentic_logging::classify_failure_kind(false, Some(&error)),
        model: None,
        token_usage: None,
        summary: None,
    };

    logging::append_record_best_effort(&record);
}

// ============================================================================
// run
// ============================================================================

/// Tool for starting or resuming `OpenCode` sessions.
///
/// Handles session creation, prompt/command execution, SSE event monitoring,
/// and permission request detection. Returns when the session completes or
/// when a permission is requested.
#[derive(Clone)]
pub struct OrchestratorRunTool {
    server: Arc<OrchestratorServerHandle>,
}

impl OrchestratorRunTool {
    /// Create a new `OrchestratorRunTool` with the shared server handle.
    pub fn new(server: Arc<OrchestratorServerHandle>) -> Self {
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
            question_request_id: None,
            questions: vec![],
            warnings,
        })
    }

    fn map_questions(req: &QuestionRequest) -> Vec<QuestionInfoView> {
        req.questions
            .iter()
            .map(|question| QuestionInfoView {
                question: question.question.clone(),
                header: question.header.clone(),
                options: question
                    .options
                    .iter()
                    .map(|option| QuestionOptionView {
                        label: option.label.clone(),
                        description: option.description.clone(),
                    })
                    .collect(),
                multiple: question.multiple,
                custom: question.custom,
            })
            .collect()
    }

    fn question_required_output(
        session_id: String,
        partial_response: Option<String>,
        request: &QuestionRequest,
        warnings: Vec<String>,
    ) -> OrchestratorRunOutput {
        OrchestratorRunOutput {
            session_id,
            status: RunStatus::QuestionRequired,
            response: None,
            partial_response,
            permission_request_id: None,
            permission_type: None,
            permission_patterns: vec![],
            question_request_id: Some(request.id.clone()),
            questions: Self::map_questions(request),
            warnings,
        }
    }

    fn caller_response_blocker_output(
        blocker: CallerResponseBlocker,
        partial_response: Option<String>,
        warnings: Vec<String>,
    ) -> OrchestratorRunOutput {
        match blocker.payload {
            CallerResponseBlockerPayload::Permission(permission) => OrchestratorRunOutput {
                session_id: blocker.root_session_id,
                status: RunStatus::PermissionRequired,
                response: None,
                partial_response,
                permission_request_id: Some(permission.id),
                permission_type: Some(permission.permission),
                permission_patterns: permission.patterns,
                question_request_id: None,
                questions: vec![],
                warnings,
            },
            CallerResponseBlockerPayload::Question(question) => Self::question_required_output(
                blocker.root_session_id,
                partial_response,
                &question,
                warnings,
            ),
        }
    }

    async fn apply_idle_completion_outcome(
        outcome: IdleCompletionOutcome,
        client: &opencode_rs::Client,
        session_id: &str,
        partial_response: &str,
        warnings: &mut Vec<String>,
        token_tracker: &TokenTracker,
    ) -> Result<Option<RunOutcome>, ToolError> {
        match outcome {
            IdleCompletionOutcome::Continue => Ok(None),
            IdleCompletionOutcome::Blocker(blocker) => {
                let partial_response =
                    (!partial_response.is_empty()).then(|| partial_response.to_string());
                let output = Self::caller_response_blocker_output(
                    *blocker,
                    partial_response,
                    std::mem::take(warnings),
                );
                Ok(Some(RunOutcome::with_tracker(output, token_tracker)))
            }
            IdleCompletionOutcome::Complete => {
                let output = Self::finalize_completed(
                    client,
                    session_id.to_string(),
                    token_tracker,
                    std::mem::take(warnings),
                )
                .await?;
                Ok(Some(RunOutcome::with_tracker(output, token_tracker)))
            }
        }
    }

    async fn run_impl_outcome(
        &self,
        input: OrchestratorRunInput,
        ctx: &ToolContext,
        permission_preflight_mode: PermissionPreflightMode,
    ) -> Result<RunOutcome, ToolError> {
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

        if input.command.is_some() && input.agent.is_some() {
            return Err(ToolError::InvalidInput(
                "agent cannot be provided when command is specified".into(),
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

        let agent = input.agent.map(|value| value.trim().to_string());
        if let Some(ref agent) = agent
            && agent.is_empty()
        {
            return Err(ToolError::InvalidInput(
                "agent cannot be empty or whitespace-only".into(),
            ));
        }

        let wait_for_activity = input.wait_for_activity.unwrap_or(false);

        // Lazy initialization: spawn server on first tool call
        let server = self
            .server
            .acquire()
            .await
            .map_err(|e| ToolError::Internal(e.to_string()))?;

        if let Some(command) = input.command.as_deref() {
            let decision = server.command_policy_decision(command);
            if !decision.is_allowed() {
                return Err(blocked_command_error(command, decision));
            }
        }

        if let Some(agent) = agent.as_deref() {
            let decision = server.agent_policy_decision(agent);
            if !decision.is_allowed() {
                return Err(blocked_agent_error(agent, decision));
            }
        }

        let client = server.client();

        tracing::debug!(
            command = ?input.command,
            has_message = message.is_some(),
            message_len = message.as_ref().map(String::len),
            session_id = ?input.session_id,
            "run: starting"
        );

        // 1. Resolve session: validate existing or create new
        let session_id = if let Some(sid) = input.session_id {
            // Validate session exists
            client.sessions().get(&sid).await.map_err(|e| {
                if e.is_not_found() {
                    ToolError::InvalidInput(format!(
                        "Session '{sid}' not found. Use list_sessions to discover sessions, \
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

            {
                let mut spawned = server.spawned_sessions().write().await;
                spawned.insert(session.id.clone());
            }

            session.id
        };

        tracing::info!(session_id = %session_id, "run: session resolved");

        let mut warnings = Vec::new();
        let mut lineage = SessionLineageResolver::default();

        // 2. Check if session is already idle (for resume-only case)
        let status = client
            .sessions()
            .status_for(&session_id)
            .await
            .map_err(|e| ToolError::Internal(format!("Failed to get session status: {e}")))?;

        let is_idle = matches!(status, SessionStatusInfo::Idle);

        // 3. Check for pending caller-response blockers before doing anything else
        match scan_pending_caller_response_blocker(
            client,
            &session_id,
            &mut lineage,
            BlockerDetectionSource::Preflight(permission_preflight_mode),
            &mut warnings,
        )
        .await?
        {
            BlockerScanResult::Found(blocker) => {
                tracing::info!(
                    session_id = %session_id,
                    owner_session_id = %blocker.owner_session_id,
                    request_id = %blocker.request_id,
                    blocker_kind = ?blocker.kind,
                    "run: pending caller-response blocker found"
                );
                return Ok(RunOutcome::without_tokens(
                    Self::caller_response_blocker_output(*blocker, None, warnings),
                ));
            }
            BlockerScanResult::Clear | BlockerScanResult::Inconclusive => {}
        }
        tracing::trace!(
            session_id = %session_id,
            initial_idle = is_idle,
            "initial status observed; completion delegated to the idle completion monitor"
        );

        // 4. Subscribe to SSE BEFORE sending prompt/command
        let mut subscription = client
            .subscribe()
            .map_err(|e| ToolError::Internal(format!("Failed to subscribe to events: {e}")))?;

        // Track whether this call is dispatching new work (command or message)
        // vs just resuming/monitoring an existing session.
        let dispatched_new_work = input.command.is_some() || message.is_some() || wait_for_activity;
        let idle_grace = config::idle_grace();
        let mut idle_grace_deadline: Option<tokio::time::Instant> = None;
        let mut awaiting_idle_grace_check = false;

        if wait_for_activity && input.command.is_none() && message.is_none() {
            idle_grace_deadline = Some(tokio::time::Instant::now() + idle_grace);
        }

        // 5. Kick off the work
        let mut command_task: Option<JoinHandle<Result<(), OpencodeError>>> = None;
        let mut command_name_for_logging: Option<String> = None;
        let mut command_transcript_window: Option<CommandTranscriptWindow> = None;

        if let Some(command) = &input.command {
            command_name_for_logging = Some(command.clone());

            let command_message_id = new_message_id();
            command_transcript_window = Some(CommandTranscriptWindow {
                command_message_id: command_message_id.clone(),
            });

            let cmd_client = client.clone();
            let cmd_session_id = session_id.clone();
            let cmd_name = command.clone();
            let cmd_arguments = message.clone().unwrap_or_default();

            command_task = Some(tokio::spawn(async move {
                // OpenCode's /command request is completion-coupled rather than fire-and-forget.
                // Once start evidence exists, later transport failures are ambiguous and we must
                // continue supervision via SSE + status polling instead of retrying blindly.
                let req = CommandRequest {
                    command: cmd_name,
                    arguments: cmd_arguments,
                    message_id: Some(command_message_id),
                };

                cmd_client
                    .messages()
                    .command(&cmd_session_id, &req)
                    .await
                    .map(|_| ())
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
                agent: agent.clone(),
                no_reply: None,
                system: None,
                variant: None,
            };

            client
                .messages()
                .prompt_async(&session_id, &req)
                .await
                .map_err(|e| ToolError::Internal(format!("Failed to send prompt: {e}")))?;

            idle_grace_deadline = Some(tokio::time::Instant::now() + idle_grace);
        }

        // 6. Event loop: wait for completion or permission
        // Overall timeout to prevent infinite hangs (configurable, default 1 hour)
        let deadline = tokio::time::Instant::now() + server.session_deadline();
        let inactivity_timeout = server.inactivity_timeout();
        let mut last_activity_time = tokio::time::Instant::now();

        tracing::debug!(session_id = %session_id, "run: entering event loop");
        let mut token_tracker = TokenTracker::with_threshold(server.compaction_threshold());
        let mut partial_response = String::new();

        let mut poll_interval = tokio::time::interval(Duration::from_secs(1));
        poll_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        // Track whether we've observed the session as busy at least once.
        // This prevents completing immediately if we call run_impl on an already-idle
        // session before our new work has started processing.
        let mut observed_busy = false;
        let mut correlated_assistant_ids = HashSet::new();
        let mut precorrelation_deltas = command_transcript_window
            .as_ref()
            .map(|_| PreCorrelationDeltaBuffer::default());

        // Track whether SSE is still active. If the stream closes, we fall back
        // to polling-only mode rather than returning an error.
        let mut sse_active = true;
        let mut blocker_event_latch = CallerResponseEventLatch::default();
        let mut idle_completion_monitor = IdleCompletionMonitor::default();

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
            let blocker_scan = scan_pending_caller_response_blocker(
                client,
                &session_id,
                &mut lineage,
                BlockerDetectionSource::Fallback,
                &mut warnings,
            )
            .await?;
            match evaluate_idle_completion(
                &mut idle_completion_monitor,
                blocker_scan,
                false,
                false,
                IdleCompletionContext {
                    sse_active,
                    latch: &mut blocker_event_latch,
                    client,
                    root_session_id: &session_id,
                    lineage: &mut lineage,
                    warnings: &mut warnings,
                },
            )
            .await?
            {
                IdleCompletionOutcome::Blocker(blocker) => {
                    tracing::debug!(
                        session_id = %session_id,
                        owner_session_id = %blocker.owner_session_id,
                        request_id = %blocker.request_id,
                        blocker_kind = ?blocker.kind,
                        "detected pending caller-response blocker before post-subscribe finalization"
                    );
                    return Ok(RunOutcome::with_tracker(
                        Self::caller_response_blocker_output(*blocker, None, warnings),
                        &token_tracker,
                    ));
                }
                IdleCompletionOutcome::Continue => {}
                IdleCompletionOutcome::Complete => {
                    return Err(ToolError::Internal(
                        "idle completion monitor unexpectedly completed during post-subscribe observation"
                            .to_string(),
                    ));
                }
            }
        }
        // If check fails or session is busy, continue to event loop

        loop {
            // Check timeout before processing
            let now = tokio::time::Instant::now();

            if now.duration_since(last_activity_time) >= inactivity_timeout {
                return Err(ToolError::Internal(format!(
                    "Session idle timeout: no activity for 5 minutes (session_id={session_id}). \
                     The session may still be running; use run(session_id=...) to check status."
                )));
            }

            if now >= deadline {
                return Err(ToolError::Internal(
                    "Session execution timed out after 1 hour. \
                     The session may still be running; use run with the session_id to check status."
                        .into(),
                ));
            }

            let command_task_active = command_task.is_some();

            tokio::select! {
                () = ctx.cancelled() => {
                    abort_command_task(&mut command_task).await;
                    return Err(ToolError::cancelled(None));
                }

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

                    match &event {
                        Event::PermissionReplied { properties }
                        | Event::PermissionRepliedNext { properties } => {
                            blocker_event_latch.remove(
                                CallerResponseBlockerKind::Permission,
                                &properties.session_id,
                                &properties.request_id,
                            );
                            continue;
                        }
                        Event::QuestionReplied { properties } => {
                            blocker_event_latch.remove(
                                CallerResponseBlockerKind::Question,
                                &properties.session_id,
                                &properties.request_id,
                            );
                            continue;
                        }
                        Event::QuestionRejected { properties } => {
                            blocker_event_latch.remove(
                                CallerResponseBlockerKind::Question,
                                &properties.session_id,
                                &properties.request_id,
                            );
                            continue;
                        }
                        _ => {}
                    }

                    let asked_request = match &event {
                        Event::PermissionAsked { properties } => Some((
                            CallerResponseBlockerKind::Permission,
                            properties.request.session_id.as_str(),
                            properties.request.id.as_str(),
                        )),
                        Event::QuestionAsked { properties } => Some((
                            CallerResponseBlockerKind::Question,
                            properties.request.session_id.as_str(),
                            properties.request.id.as_str(),
                        )),
                        _ => None,
                    };
                    if let Some((kind, owner_session_id, request_id)) = asked_request {
                        match lineage
                            .eligible_owner_depth(client, &session_id, owner_session_id)
                            .await
                        {
                            Ok(Some(_)) => {
                                blocker_event_latch.latch(kind, owner_session_id, request_id);
                                idle_completion_monitor.reset_completion_candidate();
                            }
                            Ok(None) => {
                                tracing::trace!(
                                    session_id = %session_id,
                                    owner_session_id,
                                    request_id,
                                    "ignoring unrelated caller-response SSE event"
                                );
                                continue;
                            }
                            Err(error) => {
                                tracing::warn!(
                                    session_id = %session_id,
                                    owner_session_id,
                                    request_id,
                                    error = %error,
                                    "failed to validate caller-response SSE event owner; latching until polling resolves it"
                                );
                                blocker_event_latch.latch(kind, owner_session_id, request_id);
                                idle_completion_monitor.reset_completion_candidate();
                            }
                        }

                        let blocker_scan = scan_pending_caller_response_blocker(
                            client,
                            &session_id,
                            &mut lineage,
                            BlockerDetectionSource::Fallback,
                            &mut warnings,
                        ).await?;
                        if let BlockerScanResult::Found(blocker) = blocker_scan {
                                tracing::info!(
                                    session_id = %session_id,
                                    owner_session_id = %blocker.owner_session_id,
                                    request_id = %blocker.request_id,
                                    blocker_kind = ?blocker.kind,
                                    "detected pending caller-response blocker after SSE event"
                                );
                                let partial_response =
                                    (!partial_response.is_empty()).then_some(partial_response);
                                return Ok(RunOutcome::with_tracker(
                                    Self::caller_response_blocker_output(
                                        *blocker,
                                        partial_response,
                                        warnings,
                                    ),
                                    &token_tracker,
                                ));
                        }
                        continue;
                    }

                    if event.session_id() != Some(session_id.as_str()) {
                        continue;
                    }

                    // Track tokens only for the monitored root session.
                    token_tracker.observe_event(&event, |pid, mid| {
                        server.context_limit(pid, mid)
                    });

                    match event {
                        Event::MessagePartDelta { properties } => {
                            let correlated = record_message_part_delta(
                                &properties,
                                command_transcript_window.as_ref(),
                                &correlated_assistant_ids,
                                &mut precorrelation_deltas,
                                &mut partial_response,
                            );
                            if correlated {
                                last_activity_time = tokio::time::Instant::now();
                                observed_busy = true;
                                awaiting_idle_grace_check = false;
                                idle_completion_monitor.reset_on_root_progress();
                            }
                        }

                        Event::MessageUpdated { properties } => {
                            let correlated = record_message_updated(
                                &properties,
                                command_transcript_window.as_ref(),
                                &mut correlated_assistant_ids,
                                &mut precorrelation_deltas,
                                &mut partial_response,
                            );
                            if correlated {
                                last_activity_time = tokio::time::Instant::now();
                                observed_busy = true;
                                awaiting_idle_grace_check = false;
                                idle_completion_monitor.reset_on_root_progress();
                            }
                        }

                        Event::MessagePartUpdated { properties } => {
                            let correlated = part_event_matches_command(
                                command_transcript_window.as_ref(),
                                properties.message_id.as_deref(),
                                &correlated_assistant_ids,
                            );
                            if correlated {
                                last_activity_time = tokio::time::Instant::now();
                                observed_busy = true;
                                awaiting_idle_grace_check = false;
                                idle_completion_monitor.reset_on_root_progress();
                            }
                        }

                        Event::SessionError { properties } => {
                            let error_msg = properties
                                .error
                                .map_or_else(|| "Unknown error".to_string(), |e| format!("{e:?}"));
                            tracing::error!(
                                session_id = %session_id,
                                error = %error_msg,
                                "run: session error"
                            );
                            return Err(ToolError::Internal(format!("Session error: {error_msg}")));
                        }

                        Event::SessionIdle { .. } => {
                            tracing::debug!(session_id = %session_id, "received SessionIdle event");
                            let blocker_scan = scan_pending_caller_response_blocker(
                                client,
                                &session_id,
                                &mut lineage,
                                BlockerDetectionSource::Fallback,
                                &mut warnings,
                            ).await?;
                            let completion_outcome = evaluate_idle_completion(
                                &mut idle_completion_monitor,
                                blocker_scan,
                                false,
                                false,
                                IdleCompletionContext {
                                    sse_active,
                                    latch: &mut blocker_event_latch,
                                    client,
                                    root_session_id: &session_id,
                                    lineage: &mut lineage,
                                    warnings: &mut warnings,
                                },
                            ).await?;
                            if let Some(outcome) = Self::apply_idle_completion_outcome(
                                completion_outcome,
                                client,
                                &session_id,
                                &partial_response,
                                &mut warnings,
                                &token_tracker,
                            ).await? {
                                return Ok(outcome);
                            }
                        }

                        _ => {
                            // Other events - continue
                        }
                    }
                }

                _ = poll_interval.tick() => {
                    // === 1. Caller-response blocker fallback ===
                    let blocker_scan = scan_pending_caller_response_blocker(
                        client,
                        &session_id,
                        &mut lineage,
                        BlockerDetectionSource::Fallback,
                        &mut warnings,
                    ).await?;
                    let blocker_scan = match blocker_scan {
                        BlockerScanResult::Found(blocker) => {
                            tracing::debug!(
                                session_id = %session_id,
                                owner_session_id = %blocker.owner_session_id,
                                request_id = %blocker.request_id,
                                blocker_kind = ?blocker.kind,
                                "detected pending caller-response blocker via polling fallback"
                            );
                            let partial_response = (!partial_response.is_empty()).then_some(partial_response);
                            return Ok(RunOutcome::with_tracker(
                                Self::caller_response_blocker_output(
                                    *blocker,
                                    partial_response,
                                    warnings,
                                ),
                                &token_tracker,
                            ));
                        }
                        blocker_scan => blocker_scan,
                    };

                    // === 2. Session idle detection fallback (NEW) ===
                    // This is the key fix for race conditions. If SSE missed SessionIdle,
                    // we detect completion via polling sessions().status_for(session_id).
                    match client.sessions().status_for(&session_id).await {
                        Ok(
                            SessionStatusInfo::Busy
                            | SessionStatusInfo::Retry { .. }
                            | SessionStatusInfo::Unknown,
                        ) => {
                            blocker_event_latch.observe_poll_scan(&blocker_scan);
                            last_activity_time = tokio::time::Instant::now();
                            observed_busy = true;
                            awaiting_idle_grace_check = false;
                            idle_completion_monitor.reset_on_root_progress();
                            tracing::trace!(
                                session_id = %session_id,
                                "our session is busy/retry, waiting"
                            );
                        }
                        Ok(SessionStatusInfo::Idle) => {
                            let now = tokio::time::Instant::now();
                            let may_complete = if !dispatched_new_work || observed_busy {
                                true
                            } else if let Some(deadline) = idle_grace_deadline {
                                if now >= deadline {
                                    true
                                } else {
                                    awaiting_idle_grace_check = true;
                                    tracing::trace!(
                                        session_id = %session_id,
                                        remaining_ms = (deadline - now).as_millis(),
                                        "idle detected before busy; waiting for idle-grace deadline"
                                    );
                                    false
                                }
                            } else {
                                tracing::trace!(
                                    session_id = %session_id,
                                    command_task_active = command_task_active,
                                    "idle seen before dispatch confirmed; waiting"
                                );
                                false
                            };

                            let completion_outcome = evaluate_idle_completion(
                                &mut idle_completion_monitor,
                                blocker_scan,
                                true,
                                may_complete,
                                IdleCompletionContext {
                                    sse_active,
                                    latch: &mut blocker_event_latch,
                                    client,
                                    root_session_id: &session_id,
                                    lineage: &mut lineage,
                                    warnings: &mut warnings,
                                },
                            ).await?;
                            if let Some(outcome) = Self::apply_idle_completion_outcome(
                                completion_outcome,
                                client,
                                &session_id,
                                &partial_response,
                                &mut warnings,
                                &token_tracker,
                            ).await? {
                                return Ok(outcome);
                            }
                        }
                        Err(e) => {
                            // Log but continue - status check failure shouldn't block the loop
                            tracing::warn!(
                                session_id = %session_id,
                                error = %e,
                                "failed to get session status during poll fallback"
                            );
                            blocker_event_latch.observe_poll_scan(&blocker_scan);
                            idle_completion_monitor.reset_completion_candidate();
                        }
                    }
                }

                () = async {
                    match idle_grace_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending::<()>().await,
                    }
                }, if awaiting_idle_grace_check => {
                    awaiting_idle_grace_check = false;

                    match client.sessions().status_for(&session_id).await {
                        Ok(SessionStatusInfo::Idle) => {
                            tracing::debug!(
                                session_id = %session_id,
                                "idle-grace deadline reached; rechecking blockers before polling completion"
                            );
                            let blocker_scan = scan_pending_caller_response_blocker(
                                client,
                                &session_id,
                                &mut lineage,
                                BlockerDetectionSource::Fallback,
                                &mut warnings,
                            ).await?;
                            match evaluate_idle_completion(
                                &mut idle_completion_monitor,
                                blocker_scan,
                                false,
                                false,
                                IdleCompletionContext {
                                    sse_active,
                                    latch: &mut blocker_event_latch,
                                    client,
                                    root_session_id: &session_id,
                                    lineage: &mut lineage,
                                    warnings: &mut warnings,
                                },
                            ).await? {
                                IdleCompletionOutcome::Blocker(blocker) => {
                                    tracing::debug!(
                                        session_id = %session_id,
                                        owner_session_id = %blocker.owner_session_id,
                                        request_id = %blocker.request_id,
                                        blocker_kind = ?blocker.kind,
                                        "detected pending caller-response blocker before idle-grace finalization"
                                    );
                                    let partial_response =
                                        (!partial_response.is_empty()).then_some(partial_response);
                                    return Ok(RunOutcome::with_tracker(
                                        Self::caller_response_blocker_output(
                                            *blocker,
                                            partial_response,
                                            warnings,
                                        ),
                                        &token_tracker,
                                    ));
                                }
                                IdleCompletionOutcome::Continue => {}
                                IdleCompletionOutcome::Complete => {
                                    return Err(ToolError::Internal(
                                        "idle completion monitor unexpectedly completed during idle-grace observation"
                                            .to_string(),
                                    ));
                                }
                            }
                        }
                        Ok(
                            SessionStatusInfo::Busy
                            | SessionStatusInfo::Retry { .. }
                            | SessionStatusInfo::Unknown,
                        ) => {
                            last_activity_time = tokio::time::Instant::now();
                            observed_busy = true;
                            idle_completion_monitor.reset_on_root_progress();
                        }
                        Err(e) => {
                            tracing::warn!(
                                session_id = %session_id,
                                error = %e,
                                "status check failed at idle-grace deadline"
                            );
                        }
                    }
                }

                cmd_result = async {
                    match command_task.as_mut() {
                        Some(handle) => Some(handle.await),
                        None => {
                            std::future::pending::<
                                Option<Result<Result<(), OpencodeError>, tokio::task::JoinError>>,
                            >()
                            .await
                        }
                    }
                }, if command_task_active => {
                    match cmd_result {
                        Some(Ok(Ok(()))) => {
                            idle_grace_deadline = Some(tokio::time::Instant::now() + idle_grace);
                            tracing::debug!(
                                session_id = %session_id,
                                command = ?command_name_for_logging,
                                "run: command dispatch completed successfully"
                            );
                            command_task = None;
                        }
                        Some(Ok(Err(error))) => {
                            let command_name = command_name_for_logging.as_deref().unwrap_or("unknown");

                            if error.is_validation_error() || error.is_not_found() {
                                tracing::warn!(
                                    session_id = %session_id,
                                    command = command_name,
                                    error = %error,
                                    "run: command dispatch rejected before start"
                                );
                                return Err(ToolError::InvalidInput(format!(
                                    "Failed to dispatch command '{command_name}': {error}"
                                )));
                            }

                            if !matches!(error, OpencodeError::Transport(_)) {
                                tracing::error!(
                                    session_id = %session_id,
                                    command = command_name,
                                    error = %error,
                                    "run: command dispatch failed"
                                );
                                return Err(ToolError::Internal(format!(
                                    "Failed to dispatch command '{command_name}': {error}"
                                )));
                            }

                            let mut start_evidence = observed_busy;

                            if !start_evidence
                                && let Ok(status) = client.sessions().status_for(&session_id).await
                                && status.is_busy_like()
                            {
                                start_evidence = true;
                                observed_busy = true;
                                last_activity_time = tokio::time::Instant::now();
                                awaiting_idle_grace_check = false;
                            }

                            if !start_evidence
                                && let Some(transcript_window) = command_transcript_window.as_ref()
                            {
                                // This bounded transcript probe only decides whether dispatch
                                // already started after an ambiguous /command transport failure.
                                match client.messages().list(&session_id).await {
                                    Ok(messages)
                                        if transcript_indicates_command_dispatch(
                                            &messages,
                                            transcript_window,
                                        ) =>
                                    {
                                        start_evidence = true;
                                        idle_grace_deadline.get_or_insert_with(|| {
                                            tokio::time::Instant::now() + idle_grace
                                        });
                                        last_activity_time = tokio::time::Instant::now();
                                    }
                                    Ok(_) => {}
                                    Err(probe_error) => {
                                        tracing::warn!(
                                            session_id = %session_id,
                                            command = command_name,
                                            error = %probe_error,
                                            "run: transcript probe failed after command transport error"
                                        );
                                    }
                                }
                            }

                            if start_evidence {
                                tracing::warn!(
                                    session_id = %session_id,
                                    command = command_name,
                                    error = %error,
                                    "run: command transport error after start evidence; continuing supervision"
                                );
                                warnings.push(format!(
                                    "OpenCode command transport error after start evidence; continuing supervision: {error}"
                                ));
                                command_task = None;
                                continue;
                            }

                            tracing::error!(
                                session_id = %session_id,
                                command = command_name,
                                error = %error,
                                "run: command transport error before start evidence"
                            );
                            return Err(ToolError::Internal(format!(
                                "Failed to dispatch command '{command_name}': transport error before session start evidence: {error}"
                            )));
                        }
                        Some(Err(join_err)) => {
                            tracing::error!(
                                session_id = %session_id,
                                command = ?command_name_for_logging,
                                error = %join_err,
                                "run: command task panicked"
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
    const NAME: &'static str = "run";
    const DESCRIPTION: &'static str = r#"Start or resume an OpenCode session. Optionally run a named command or send a raw prompt.

Returns when:
- status=completed: Session finished executing. Response contains final assistant output.
- status=permission_required: Session needs permission approval. Call respond_permission to continue.
- status=question_required: Session needs question answers. Call respond_question to continue.

Parameters:
- session_id: Existing session to resume (omit to create new)
- command: OpenCode command name (e.g., "research", "implement_plan")
- agent: Explicit agent for raw-prompt execution only. Invalid with command.
- message: Prompt text or $ARGUMENTS for command template

Examples:
- New session with prompt: run(message="explain this code")
- New session with prompt and agent: run(message="explain this code", agent="Plan")
- New session with command: run(command="research", message="caching strategies")
- Resume session: run(session_id="...", message="continue")
- Check status: run(session_id="...")"#;

    fn call(
        &self,
        input: Self::Input,
        ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let this = self.clone();
        let ctx = ctx.clone();
        Box::pin(async move {
            let timer = CallTimer::start();
            match this
                .run_impl_outcome(input.clone(), &ctx, PermissionPreflightMode::Strict)
                .await
            {
                Ok(outcome) => {
                    log_tool_success(
                        &timer,
                        Self::NAME,
                        &input,
                        &outcome.output,
                        outcome.log_meta,
                        true,
                    );
                    Ok(outcome.output)
                }
                Err(error) => {
                    log_tool_error(&timer, Self::NAME, &input, &error);
                    Err(error)
                }
            }
        })
    }
}

// ============================================================================
// list_sessions
// ============================================================================

/// Tool for listing available `OpenCode` sessions in the current directory.
#[derive(Clone)]
pub struct ListSessionsTool {
    server: Arc<OrchestratorServerHandle>,
}

impl ListSessionsTool {
    /// Create a new `ListSessionsTool` with the shared server handle.
    pub fn new(server: Arc<OrchestratorServerHandle>) -> Self {
        Self { server }
    }
}

impl Tool for ListSessionsTool {
    type Input = ListSessionsInput;
    type Output = ListSessionsOutput;
    const NAME: &'static str = "list_sessions";
    const DESCRIPTION: &'static str =
        "List available OpenCode sessions in the current directory context.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let server_handle = Arc::clone(&self.server);
        Box::pin(async move {
            let timer = CallTimer::start();
            let result: Result<ListSessionsOutput, ToolError> = async {
                let server = server_handle
                    .acquire()
                    .await
                    .map_err(|e| ToolError::Internal(e.to_string()))?;

                let sessions =
                    // Intentionally keep zero-arg list() so SDK directory context preserves launch-directory scoping.
                    server.client().sessions().list().await.map_err(|e| {
                        ToolError::Internal(format!("Failed to list sessions: {e}"))
                    })?;
                let status_map = server.client().sessions().status_map().await.ok();
                let spawned = server.spawned_sessions().read().await;

                let limit = input.limit.unwrap_or(20);
                let summaries: Vec<SessionSummary> = sessions
                    .into_iter()
                    .take(limit)
                    .map(|s| {
                        let status =
                            status_map
                                .as_ref()
                                .map(|status_map| match status_map.get(&s.id) {
                                    Some(SessionStatusInfo::Busy | SessionStatusInfo::Unknown) => {
                                        SessionStatusSummary::Busy
                                    }
                                    Some(SessionStatusInfo::Retry {
                                        attempt,
                                        message,
                                        next,
                                    }) => SessionStatusSummary::Retry {
                                        attempt: *attempt,
                                        message: message.clone(),
                                        next: *next,
                                    },
                                    Some(SessionStatusInfo::Idle) | None => {
                                        SessionStatusSummary::Idle
                                    }
                                });

                        let change_stats = s.summary.as_ref().map(|summary| ChangeStats {
                            additions: summary.additions,
                            deletions: summary.deletions,
                            files: summary.files,
                        });

                        SessionSummary {
                            launched_by_you: spawned.contains(&s.id),
                            created: s.time.as_ref().map(|t| t.created),
                            updated: s.time.as_ref().map(|t| t.updated),
                            directory: s.directory,
                            path: s.path,
                            title: s.title,
                            id: s.id,
                            status,
                            change_stats,
                        }
                    })
                    .collect();

                Ok(ListSessionsOutput {
                    sessions: summaries,
                })
            }
            .await;

            match result {
                Ok(output) => {
                    log_tool_success(
                        &timer,
                        Self::NAME,
                        &input,
                        &output,
                        ToolLogMeta::default(),
                        false,
                    );
                    Ok(output)
                }
                Err(error) => {
                    log_tool_error(&timer, Self::NAME, &input, &error);
                    Err(error)
                }
            }
        })
    }
}

fn count_pending_messages(messages: &[Message]) -> usize {
    let mut pending = 0;

    for message in messages.iter().rev() {
        if message.role() == "user" {
            pending += 1;
        } else if message.role() == "assistant" {
            break;
        }
    }

    pending
}

fn get_last_activity_time(messages: &[Message], fallback: Option<i64>) -> Option<i64> {
    messages.last().map_or(fallback, |message| {
        Some(
            message
                .info
                .time
                .completed
                .unwrap_or(message.info.time.created),
        )
    })
}

fn extract_recent_tool_calls(messages: &[Message], limit: usize) -> Vec<ToolCallSummary> {
    let mut tool_calls = Vec::new();

    for message in messages.iter().rev() {
        for part in message.parts.iter().rev() {
            if let Part::Tool {
                call_id,
                tool,
                state,
                ..
            } = part
            {
                let (state, started_at, completed_at) = match state {
                    Some(ToolState::Running(running)) => {
                        (ToolStateSummary::Running, Some(running.time.start), None)
                    }
                    Some(ToolState::Completed(completed)) => (
                        ToolStateSummary::Completed,
                        Some(completed.time.start),
                        Some(completed.time.end),
                    ),
                    Some(ToolState::Error(error)) => (
                        ToolStateSummary::Error {
                            message: error.error.clone(),
                        },
                        Some(error.time.start),
                        Some(error.time.end),
                    ),
                    _ => (ToolStateSummary::Pending, None, None),
                };

                tool_calls.push(ToolCallSummary {
                    call_id: call_id.clone(),
                    tool_name: tool.clone(),
                    state,
                    started_at,
                    completed_at,
                });

                if tool_calls.len() >= limit {
                    return tool_calls;
                }
            }
        }
    }

    tool_calls
}

/// Tool for getting detailed state of a specific `OpenCode` session.
#[derive(Clone)]
pub struct GetSessionStateTool {
    server: Arc<OrchestratorServerHandle>,
}

impl GetSessionStateTool {
    pub fn new(server: Arc<OrchestratorServerHandle>) -> Self {
        Self { server }
    }
}

impl Tool for GetSessionStateTool {
    type Input = GetSessionStateInput;
    type Output = GetSessionStateOutput;
    const NAME: &'static str = "get_session_state";
    const DESCRIPTION: &'static str = "Get detailed state of a specific session including status, pending messages, and recent tool calls.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let server_handle = Arc::clone(&self.server);
        Box::pin(async move {
            let timer = CallTimer::start();
            let result: Result<GetSessionStateOutput, ToolError> = async {
                let server = server_handle
                    .acquire()
                    .await
                    .map_err(|e| ToolError::Internal(e.to_string()))?;

                let client = server.client();
                let session_id = &input.session_id;

                let session = client.sessions().get(session_id).await.map_err(|e| {
                    if e.is_not_found() {
                        ToolError::InvalidInput(format!(
                            "Session '{session_id}' not found. Use list_sessions to discover available sessions."
                        ))
                    } else {
                        ToolError::Internal(format!("Failed to get session: {e}"))
                    }
                })?;

                let status = match client.sessions().status_for(session_id).await.map_err(|e| {
                    ToolError::Internal(format!("Failed to get session status: {e}"))
                })? {
                    SessionStatusInfo::Busy | SessionStatusInfo::Unknown => {
                        SessionStatusSummary::Busy
                    }
                    SessionStatusInfo::Retry {
                        attempt,
                        message,
                        next,
                    } => SessionStatusSummary::Retry {
                        attempt,
                        message,
                        next,
                    },
                    SessionStatusInfo::Idle => SessionStatusSummary::Idle,
                };

                let messages = client.messages().list(session_id).await.map_err(|e| {
                    ToolError::Internal(format!("Failed to list messages: {e}"))
                })?;
                let pending_message_count = count_pending_messages(&messages);
                let last_activity = get_last_activity_time(
                    &messages,
                    session.time.as_ref().map(|time| time.updated),
                );
                let recent_tool_calls = extract_recent_tool_calls(&messages, 10);

                let spawned = server.spawned_sessions().read().await;
                let launched_by_you = spawned.contains(session_id);

                Ok(GetSessionStateOutput {
                    session_id: session.id,
                    title: session.title,
                    directory: session.directory,
                    path: session.path,
                    status,
                    launched_by_you,
                    pending_message_count,
                    last_activity,
                    recent_tool_calls,
                })
            }
            .await;

            match result {
                Ok(output) => {
                    log_tool_success(
                        &timer,
                        Self::NAME,
                        &input,
                        &output,
                        ToolLogMeta::default(),
                        false,
                    );
                    Ok(output)
                }
                Err(error) => {
                    log_tool_error(&timer, Self::NAME, &input, &error);
                    Err(error)
                }
            }
        })
    }
}

// ============================================================================
// list_agents
// ============================================================================

/// Tool for listing available visible `OpenCode` agents that can be selected explicitly.
#[derive(Clone)]
pub struct ListAgentsTool {
    server: Arc<OrchestratorServerHandle>,
}

impl ListAgentsTool {
    /// Create a new `ListAgentsTool` with the shared server handle.
    pub fn new(server: Arc<OrchestratorServerHandle>) -> Self {
        Self { server }
    }
}

impl Tool for ListAgentsTool {
    type Input = ListAgentsInput;
    type Output = ListAgentsOutput;
    const NAME: &'static str = "list_agents";
    const DESCRIPTION: &'static str =
        "List available visible OpenCode agents that can be used with run(message=..., agent=...).";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let server_handle = Arc::clone(&self.server);
        Box::pin(async move {
            let timer = CallTimer::start();
            let result: Result<ListAgentsOutput, ToolError> =
                async {
                    let server = server_handle
                        .acquire()
                        .await
                        .map_err(|e| ToolError::Internal(e.to_string()))?;

                    let agents =
                        server.client().tools().agents().await.map_err(|e| {
                            ToolError::Internal(format!("Failed to list agents: {e}"))
                        })?;

                    let agent_infos: Vec<AgentInfo> = agents
                        .into_iter()
                        .filter(|agent| !agent.hidden.unwrap_or(false))
                        .filter(|agent| server.is_agent_allowed(&agent.name))
                        .map(|agent| AgentInfo {
                            name: agent.name,
                            description: agent.description,
                        })
                        .collect();

                    Ok(ListAgentsOutput {
                        agents: agent_infos,
                    })
                }
                .await;

            match result {
                Ok(output) => {
                    log_tool_success(
                        &timer,
                        Self::NAME,
                        &input,
                        &output,
                        ToolLogMeta::default(),
                        false,
                    );
                    Ok(output)
                }
                Err(error) => {
                    log_tool_error(&timer, Self::NAME, &input, &error);
                    Err(error)
                }
            }
        })
    }
}

// ============================================================================
// list_commands
// ============================================================================

/// Tool for listing available `OpenCode` commands that can be executed.
#[derive(Clone)]
pub struct ListCommandsTool {
    server: Arc<OrchestratorServerHandle>,
}

impl ListCommandsTool {
    /// Create a new `ListCommandsTool` with the shared server handle.
    pub fn new(server: Arc<OrchestratorServerHandle>) -> Self {
        Self { server }
    }
}

impl Tool for ListCommandsTool {
    type Input = ListCommandsInput;
    type Output = ListCommandsOutput;
    const NAME: &'static str = "list_commands";
    const DESCRIPTION: &'static str = "List available OpenCode commands that can be used with run.";

    fn call(
        &self,
        input: Self::Input,
        _ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let server_handle = Arc::clone(&self.server);
        Box::pin(async move {
            let timer = CallTimer::start();
            let result: Result<ListCommandsOutput, ToolError> = async {
                let server = server_handle
                    .acquire()
                    .await
                    .map_err(|e| ToolError::Internal(e.to_string()))?;

                let commands =
                    server.client().tools().commands().await.map_err(|e| {
                        ToolError::Internal(format!("Failed to list commands: {e}"))
                    })?;

                let command_infos: Vec<CommandInfo> = commands
                    .into_iter()
                    .filter(|command| server.is_command_allowed(&command.name))
                    .map(|c| CommandInfo {
                        name: c.name,
                        description: c.description,
                    })
                    .collect();

                Ok(ListCommandsOutput {
                    commands: command_infos,
                })
            }
            .await;

            match result {
                Ok(output) => {
                    log_tool_success(
                        &timer,
                        Self::NAME,
                        &input,
                        &output,
                        ToolLogMeta::default(),
                        false,
                    );
                    Ok(output)
                }
                Err(error) => {
                    log_tool_error(&timer, Self::NAME, &input, &error);
                    Err(error)
                }
            }
        })
    }
}

// ============================================================================
// respond_permission
// ============================================================================

/// Tool for responding to permission requests from `OpenCode` sessions.
///
/// After sending the reply, continues monitoring the session and returns
/// when the session completes or another permission is requested.
#[derive(Clone)]
pub struct RespondPermissionTool {
    server: Arc<OrchestratorServerHandle>,
}

impl RespondPermissionTool {
    /// Create a new `RespondPermissionTool` with the shared server handle.
    pub fn new(server: Arc<OrchestratorServerHandle>) -> Self {
        Self { server }
    }
}

impl Tool for RespondPermissionTool {
    type Input = RespondPermissionInput;
    type Output = RespondPermissionOutput;
    const NAME: &'static str = "respond_permission";
    const DESCRIPTION: &'static str = r#"Respond to a permission request from an OpenCode session.

After responding, continues monitoring the session and returns when complete or when another permission is required.

Parameters:
- session_id: Monitored root session; the permission may belong to it or an eligible descendant
- reply: "once" (allow this request), "always" (allow for matching patterns), or "reject" (deny)
- message: Optional message to include with reply"#;

    fn call(
        &self,
        input: Self::Input,
        ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let server_handle = Arc::clone(&self.server);
        let ctx = ctx.clone();
        Box::pin(async move {
            let timer = CallTimer::start();
            let request = input.clone();
            let result: Result<(RespondPermissionOutput, ToolLogMeta), ToolError> = async {
                let server = server_handle
                    .acquire()
                    .await
                    .map_err(|e| ToolError::Internal(e.to_string()))?;

                let client = server.client();
                let mut pre_warnings: Vec<String> = Vec::new();
                let mut lineage = SessionLineageResolver::default();

                let (permission_request_id, permission_type, permission_patterns) =
                    if let Some(req_id) = input.permission_request_id.as_deref() {
                        match client.permissions().list().await {
                            Ok(mut pending) => {
                                let idx = pending.iter().position(|p| p.id == req_id).ok_or_else(|| {
                                    ToolError::InvalidInput(format!(
                                        "No pending permission found with id '{req_id}'. \
                                     (session_id='{}')",
                                        input.session_id
                                    ))
                                })?;

                                let perm = pending.remove(idx);
                                let owner_depth = lineage
                                    .eligible_owner_depth(
                                        client,
                                        &input.session_id,
                                        &perm.session_id,
                                    )
                                    .await
                                    .map_err(|error| {
                                        ToolError::Internal(format!(
                                            "Failed to verify permission request '{req_id}' owner ancestry: {error}"
                                        ))
                                    })?;
                                if owner_depth.is_none() {
                                    return Err(ToolError::InvalidInput(format!(
                                        "Permission request '{req_id}' belongs to session '{}', which is not session '{}' or an eligible descendant.",
                                        perm.session_id, input.session_id
                                    )));
                                }

                                (perm.id, perm.permission, perm.patterns)
                            }
                            Err(error) if error.is_validation_error() => {
                                tracing::warn!(
                                    session_id = %input.session_id,
                                    permission_request_id = %req_id,
                                    error = %error,
                                    "permission validation failed for known request id; proceeding with provided id"
                                );
                                pre_warnings.push(format!(
                                    "Permission validation failed for request '{req_id}' ({error}); submitting reply using the provided request id."
                                ));
                                (req_id.to_string(), "unknown".to_string(), Vec::new())
                            }
                            Err(error) => {
                                return Err(ToolError::Internal(format!(
                                    "Failed to list permissions: {error}"
                                )));
                            }
                        }
                    } else {
                        // Find the pending permission for this session when discovery is required.
                        let pending = match client.permissions().list().await {
                            Ok(pending) => pending,
                            Err(error) if error.is_validation_error() => {
                                return Err(ToolError::InvalidInput(format!(
                                    "Unable to discover a pending permission for session '{}': permission state could not be listed (HTTP 400). Permission state may be stale or malformed. Retry with permission_request_id (from run) if available; otherwise reopen the session or start a new one.",
                                    input.session_id
                                )));
                            }
                            Err(error) => {
                                return Err(ToolError::Internal(format!(
                                    "Failed to list permissions: {error}"
                                )));
                            }
                        };

                        let mut perms = Vec::new();
                        let mut unresolved = Vec::new();
                        for permission in pending {
                            match lineage
                                .eligible_owner_depth(
                                    client,
                                    &input.session_id,
                                    &permission.session_id,
                                )
                                .await
                            {
                                Ok(Some(owner_depth)) => perms.push((owner_depth, permission)),
                                Ok(None) => {}
                                Err(error) => {
                                    tracing::warn!(
                                        root_session_id = %input.session_id,
                                        owner_session_id = %permission.session_id,
                                        error = %error,
                                        "failed to verify permission owner during discovery"
                                    );
                                    unresolved.push((
                                        permission.session_id,
                                        permission.id,
                                        error.to_string(),
                                    ));
                                }
                            }
                        }
                        unresolved.sort_by(|left, right| {
                            left.0
                                .cmp(&right.0)
                                .then_with(|| left.1.cmp(&right.1))
                        });
                        if !unresolved.is_empty() {
                            let details = unresolved
                                .iter()
                                .map(|(owner_session_id, request_id, error)| {
                                    format!(
                                        "request '{request_id}' owned by session '{owner_session_id}': {error}"
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("; ");
                            return Err(ToolError::Internal(format!(
                                "Unable to safely discover a pending permission for root session '{}': request ancestry could not be verified ({details}). Retry after the OpenCode session endpoint recovers, or provide permission_request_id returned by run.",
                                input.session_id
                            )));
                        }
                        perms.sort_by(|(left_depth, left), (right_depth, right)| {
                            left_depth
                                .cmp(right_depth)
                                .then_with(|| left.session_id.cmp(&right.session_id))
                                .then_with(|| left.id.cmp(&right.id))
                        });

                        match perms.as_slice() {
                            [] => {
                                return Err(ToolError::InvalidInput(format!(
                                    "No pending permission found for session '{}'. \
                                 The permission may have already been responded to.",
                                    input.session_id
                                )));
                            }
                            [_single] => {
                                let (_, perm) = perms.swap_remove(0);
                                (perm.id, perm.permission, perm.patterns)
                            }
                            multiple => {
                                let ids = multiple
                                    .iter()
                                    .map(|(_, permission)| permission.id.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ");
                                return Err(ToolError::InvalidInput(format!(
                                    "Multiple pending permissions found for session '{}': {ids}. \
                                 Please retry with permission_request_id (returned by run).",
                                    input.session_id
                                )));
                            }
                        }
                    };

                // Track if this is a rejection for post-processing
                let is_reject = matches!(input.reply, PermissionReply::Reject);

                // Capture baseline assistant text BEFORE sending reject
                // This lets us detect stale text after rejection
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
                let session_id_for_error = input.session_id.clone();

                match client
                    .permissions()
                    .reply(
                        &permission_request_id,
                        &PermissionReplyRequest {
                            reply: api_reply,
                            message: input.message,
                        },
                    )
                    .await
                {
                    Ok(_) => {}
                    Err(error) if error.is_not_found() => {
                        return Err(ToolError::InvalidInput(format!(
                            "Permission request '{permission_request_id}' was not found or is no longer pending (HTTP 404). It may be stale. Re-run orchestrator_run(session_id='{session_id_for_error}') to get the latest permission_request_id and retry."
                        )));
                    }
                    Err(error) => {
                        return Err(ToolError::Internal(format!(
                            "Failed to reply to permission: {error}"
                        )));
                    }
                }

                // Now continue monitoring the session using run logic
                let run_tool = OrchestratorRunTool::new(Arc::clone(&server_handle));
                let wait_for_activity = (!is_reject).then_some(true);
                let outcome = run_tool
                    .run_impl_outcome(
                        OrchestratorRunInput {
                            session_id: Some(input.session_id),
                            command: None,
                            agent: None,
                            message: None,
                            wait_for_activity,
                        },
                        &ctx,
                        PermissionPreflightMode::RespondPermissionContinuation,
                    )
                    .await?;
                let mut out = outcome.output;

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

                Ok((out, outcome.log_meta))
            }
            .await;

            match result {
                Ok((output, log_meta)) => {
                    log_tool_success(&timer, Self::NAME, &request, &output, log_meta, true);
                    Ok(output)
                }
                Err(error) => {
                    log_tool_error(&timer, Self::NAME, &request, &error);
                    Err(error)
                }
            }
        })
    }
}

// ============================================================================
// respond_question
// ============================================================================

#[derive(Clone)]
pub struct RespondQuestionTool {
    server: Arc<OrchestratorServerHandle>,
}

impl RespondQuestionTool {
    pub fn new(server: Arc<OrchestratorServerHandle>) -> Self {
        Self { server }
    }
}

impl Tool for RespondQuestionTool {
    type Input = RespondQuestionInput;
    type Output = RespondQuestionOutput;
    const NAME: &'static str = "respond_question";
    const DESCRIPTION: &'static str = r#"Respond to a question request from an OpenCode session.

After replying, continues monitoring the session and returns when complete or when another interruption is required.

Parameters:
- session_id: Monitored root session; the question may belong to it or an eligible descendant
- action: "reply" or "reject"
- answers: Required when action=reply; one list per question"#;

    fn call(
        &self,
        input: Self::Input,
        ctx: &ToolContext,
    ) -> BoxFuture<'static, Result<Self::Output, ToolError>> {
        let server_handle = Arc::clone(&self.server);
        let ctx = ctx.clone();
        Box::pin(async move {
            let timer = CallTimer::start();
            let request = input.clone();
            let result: Result<(RespondQuestionOutput, ToolLogMeta), ToolError> = async {
            let server = server_handle
                .acquire()
                .await
                .map_err(|e| ToolError::Internal(e.to_string()))?;

            let client = server.client();
            let mut lineage = SessionLineageResolver::default();
            let mut pending = client
                .question()
                .list()
                .await
                .map_err(|e| ToolError::Internal(format!("Failed to list questions: {e}")))?;

            let question = if let Some(req_id) = input.question_request_id.as_deref() {
                let idx = pending
                    .iter()
                    .position(|question| question.id == req_id)
                    .ok_or_else(|| {
                        ToolError::InvalidInput(format!(
                            "No pending question found with id '{req_id}'. (session_id='{}')",
                            input.session_id
                        ))
                    })?;

                let question = pending.remove(idx);
                let owner_depth = lineage
                    .eligible_owner_depth(client, &input.session_id, &question.session_id)
                    .await
                    .map_err(|error| {
                        ToolError::Internal(format!(
                            "Failed to verify question request '{req_id}' owner ancestry: {error}"
                        ))
                    })?;
                if owner_depth.is_none() {
                    return Err(ToolError::InvalidInput(format!(
                        "Question request '{req_id}' belongs to session '{}', which is not session '{}' or an eligible descendant.",
                        question.session_id, input.session_id
                    )));
                }

                question
            } else {
                let mut questions = Vec::new();
                let mut unresolved = Vec::new();
                for question in pending {
                    match lineage
                        .eligible_owner_depth(client, &input.session_id, &question.session_id)
                        .await
                    {
                        Ok(Some(owner_depth)) => questions.push((owner_depth, question)),
                        Ok(None) => {}
                        Err(error) => {
                            tracing::warn!(
                                root_session_id = %input.session_id,
                                owner_session_id = %question.session_id,
                                error = %error,
                                "failed to verify question owner during discovery"
                            );
                            unresolved.push((
                                question.session_id,
                                question.id,
                                error.to_string(),
                            ));
                        }
                    }
                }
                unresolved.sort_by(|left, right| {
                    left.0
                        .cmp(&right.0)
                        .then_with(|| left.1.cmp(&right.1))
                });
                if !unresolved.is_empty() {
                    let details = unresolved
                        .iter()
                        .map(|(owner_session_id, request_id, error)| {
                            format!(
                                "request '{request_id}' owned by session '{owner_session_id}': {error}"
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("; ");
                    return Err(ToolError::Internal(format!(
                        "Unable to safely discover a pending question for root session '{}': request ancestry could not be verified ({details}). Retry after the OpenCode session endpoint recovers, or provide question_request_id returned by run.",
                        input.session_id
                    )));
                }
                questions.sort_by(|(left_depth, left), (right_depth, right)| {
                    left_depth
                        .cmp(right_depth)
                        .then_with(|| left.session_id.cmp(&right.session_id))
                        .then_with(|| left.id.cmp(&right.id))
                });

                match questions.as_slice() {
                    [] => {
                        return Err(ToolError::InvalidInput(format!(
                            "No pending question found for session '{}'. The question may have already been responded to.",
                            input.session_id
                        )));
                    }
                    [_single] => questions.swap_remove(0).1,
                    multiple => {
                        let ids = multiple
                            .iter()
                            .map(|(_, question)| question.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ");
                        return Err(ToolError::InvalidInput(format!(
                            "Multiple pending questions found for session '{}': {ids}. Please retry with question_request_id (returned by run).",
                            input.session_id
                        )));
                    }
                }
            };

            match input.action {
                QuestionAction::Reply => {
                    if input.answers.is_empty() {
                        return Err(ToolError::InvalidInput(
                            "answers is required when action=reply".into(),
                        ));
                    }

                    client
                        .question()
                        .reply(
                            &question.id,
                            &QuestionReply {
                                answers: input.answers,
                            },
                        )
                        .await
                        .map_err(|e| {
                            ToolError::Internal(format!("Failed to reply to question: {e}"))
                        })?;

                    let outcome = OrchestratorRunTool::new(Arc::clone(&server_handle))
                        .run_impl_outcome(OrchestratorRunInput {
                            session_id: Some(input.session_id),
                            command: None,
                            agent: None,
                            message: None,
                            wait_for_activity: Some(true),
                        }, &ctx, PermissionPreflightMode::Strict)
                        .await?;
                    Ok((outcome.output, outcome.log_meta))
                }
                QuestionAction::Reject => {
                    client.question().reject(&question.id).await.map_err(|e| {
                        ToolError::Internal(format!("Failed to reject question: {e}"))
                    })?;

                    let outcome = OrchestratorRunTool::new(Arc::clone(&server_handle))
                        .run_impl_outcome(OrchestratorRunInput {
                            session_id: Some(input.session_id),
                            command: None,
                            agent: None,
                            message: None,
                            wait_for_activity: None,
                        }, &ctx, PermissionPreflightMode::Strict)
                        .await?;
                    Ok((outcome.output, outcome.log_meta))
                }
            }
        }
        .await;

            match result {
                Ok((output, log_meta)) => {
                    log_tool_success(&timer, Self::NAME, &request, &output, log_meta, true);
                    Ok(output)
                }
                Err(error) => {
                    log_tool_error(&timer, Self::NAME, &request, &error);
                    Err(error)
                }
            }
        })
    }
}

// ============================================================================
// Registry builder
// ============================================================================

/// Build the tool registry with all orchestrator tools.
///
/// The shared handle lazily starts or recovers the server on tool entry.
pub fn build_registry(server: &Arc<OrchestratorServerHandle>) -> ToolRegistry {
    ToolRegistry::builder()
        .register::<OrchestratorRunTool, ()>(OrchestratorRunTool::new(Arc::clone(server)))
        .register::<ListSessionsTool, ()>(ListSessionsTool::new(Arc::clone(server)))
        .register::<GetSessionStateTool, ()>(GetSessionStateTool::new(Arc::clone(server)))
        .register::<ListAgentsTool, ()>(ListAgentsTool::new(Arc::clone(server)))
        .register::<ListCommandsTool, ()>(ListCommandsTool::new(Arc::clone(server)))
        .register::<RespondPermissionTool, ()>(RespondPermissionTool::new(Arc::clone(server)))
        .register::<RespondQuestionTool, ()>(RespondQuestionTool::new(Arc::clone(server)))
        .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentic_tools_core::Tool;

    fn test_blocker(
        kind: CallerResponseBlockerKind,
        owner_session_id: &str,
        owner_depth: usize,
        request_id: &str,
    ) -> CallerResponseBlocker {
        let payload = match kind {
            CallerResponseBlockerKind::Permission => {
                CallerResponseBlockerPayload::Permission(PermissionRequest {
                    id: request_id.to_string(),
                    session_id: owner_session_id.to_string(),
                    permission: "file.read".to_string(),
                    patterns: vec![],
                    metadata: None,
                    always: vec![],
                    tool: None,
                })
            }
            CallerResponseBlockerKind::Question => {
                CallerResponseBlockerPayload::Question(QuestionRequest {
                    id: request_id.to_string(),
                    session_id: owner_session_id.to_string(),
                    questions: vec![],
                    tool: None,
                    extra: serde_json::Value::Null,
                })
            }
        };

        CallerResponseBlocker {
            root_session_id: "root".to_string(),
            owner_session_id: owner_session_id.to_string(),
            owner_depth,
            request_id: request_id.to_string(),
            kind,
            payload,
        }
    }

    #[test]
    fn tool_names_are_short() {
        assert_eq!(<OrchestratorRunTool as Tool>::NAME, "run");
        assert_eq!(<ListSessionsTool as Tool>::NAME, "list_sessions");
        assert_eq!(<GetSessionStateTool as Tool>::NAME, "get_session_state");
        assert_eq!(<ListCommandsTool as Tool>::NAME, "list_commands");
        assert_eq!(<RespondPermissionTool as Tool>::NAME, "respond_permission");
        assert_eq!(<RespondQuestionTool as Tool>::NAME, "respond_question");
    }

    #[test]
    fn caller_response_blocker_order_prefers_permission_before_question() {
        let mut blockers = [
            test_blocker(CallerResponseBlockerKind::Question, "root", 0, "question"),
            test_blocker(
                CallerResponseBlockerKind::Permission,
                "deep-child",
                3,
                "permission",
            ),
        ];

        blockers.sort_by(compare_caller_response_blockers);
        assert_eq!(blockers[0].kind, CallerResponseBlockerKind::Permission);
    }

    #[test]
    fn caller_response_blocker_order_prefers_shallower_owner() {
        let mut blockers = [
            test_blocker(
                CallerResponseBlockerKind::Permission,
                "deep",
                2,
                "request-1",
            ),
            test_blocker(
                CallerResponseBlockerKind::Permission,
                "shallow",
                1,
                "request-2",
            ),
        ];

        blockers.sort_by(compare_caller_response_blockers);
        assert_eq!(blockers[0].owner_session_id, "shallow");
    }

    #[test]
    fn caller_response_blocker_order_uses_owner_then_request_id() {
        let mut blockers = [
            test_blocker(
                CallerResponseBlockerKind::Question,
                "owner-b",
                1,
                "request-a",
            ),
            test_blocker(
                CallerResponseBlockerKind::Question,
                "owner-a",
                1,
                "request-z",
            ),
            test_blocker(
                CallerResponseBlockerKind::Question,
                "owner-a",
                1,
                "request-a",
            ),
        ];

        blockers.sort_by(compare_caller_response_blockers);
        assert_eq!(blockers[0].owner_session_id, "owner-a");
        assert_eq!(blockers[0].request_id, "request-a");
        assert_eq!(blockers[1].owner_session_id, "owner-a");
        assert_eq!(blockers[1].request_id, "request-z");
        assert_eq!(blockers[2].owner_session_id, "owner-b");
    }

    #[test]
    fn last_activity_falls_back_to_session_timestamp_when_messages_are_empty() {
        assert_eq!(get_last_activity_time(&[], Some(1_234)), Some(1_234));
    }

    #[test]
    fn command_activity_requires_current_assistant_lineage() {
        let window = CommandTranscriptWindow {
            command_message_id: "msg-command".to_string(),
        };
        assert!(!assistant_update_matches_command(
            Some(&window),
            "user",
            Some("msg-command")
        ));
        assert!(!assistant_update_matches_command(
            Some(&window),
            "assistant",
            Some("msg-other")
        ));
        assert!(assistant_update_matches_command(
            Some(&window),
            "assistant",
            Some("msg-command")
        ));
    }

    #[test]
    fn command_part_activity_requires_correlated_assistant_id() {
        let window = CommandTranscriptWindow {
            command_message_id: "msg-command".to_string(),
        };
        let correlated = HashSet::from(["msg-assistant".to_string()]);
        assert!(!part_event_matches_command(
            Some(&window),
            Some("msg-command"),
            &correlated
        ));
        assert!(part_event_matches_command(
            Some(&window),
            Some("msg-assistant"),
            &correlated
        ));
        assert!(part_event_matches_command(None, None, &correlated));
    }

    #[test]
    fn precorrelation_deltas_flush_only_after_matching_assistant_lineage() {
        let window = CommandTranscriptWindow {
            command_message_id: "msg-command".to_string(),
        };
        let mut correlated_assistant_ids = HashSet::new();
        let mut buffer = Some(PreCorrelationDeltaBuffer::default());
        let mut partial_response = String::new();
        let delta: Event = serde_json::from_value(serde_json::json!({
            "type": "message.part.delta",
            "properties": {
                "sessionID": "session-1",
                "messageID": "msg-assistant",
                "delta": "buffered prefix"
            }
        }))
        .unwrap();
        let update: Event = serde_json::from_value(serde_json::json!({
            "type": "message.updated",
            "properties": {
                "info": {
                    "id": "msg-assistant",
                    "sessionID": "session-1",
                    "role": "assistant",
                    "parentID": "msg-command",
                    "time": { "created": 1 }
                }
            }
        }))
        .unwrap();

        let Event::MessagePartDelta { properties } = delta else {
            panic!("expected message delta event");
        };
        assert!(!record_message_part_delta(
            &properties,
            Some(&window),
            &correlated_assistant_ids,
            &mut buffer,
            &mut partial_response,
        ));
        assert!(partial_response.is_empty());

        let Event::MessageUpdated { properties } = update else {
            panic!("expected message update event");
        };
        assert!(record_message_updated(
            &properties,
            Some(&window),
            &mut correlated_assistant_ids,
            &mut buffer,
            &mut partial_response,
        ));
        assert_eq!(partial_response, "buffered prefix");
    }

    #[test]
    fn precorrelation_deltas_do_not_flush_for_other_lineage() {
        let window = CommandTranscriptWindow {
            command_message_id: "msg-command".to_string(),
        };
        let mut buffer = PreCorrelationDeltaBuffer::default();
        buffer.push("msg-assistant", "must remain unconfirmed");

        assert!(!assistant_update_matches_command(
            Some(&window),
            "assistant",
            Some("msg-other")
        ));
        buffer.evict_message("msg-assistant");
        assert!(!buffer.bytes_by_message.contains_key("msg-assistant"));
        assert_eq!(buffer.total_bytes, 0);
    }

    #[test]
    fn precorrelation_delta_buffer_enforces_byte_and_message_caps() {
        let mut buffer = PreCorrelationDeltaBuffer::default();
        let per_message = "x".repeat(PRECORRELATION_MESSAGE_BYTES);
        buffer.push("full", &per_message);
        buffer.push("full", "overflow");
        assert_eq!(buffer.take("full"), per_message);

        for index in 0..PRECORRELATION_MESSAGE_IDS {
            buffer.push(&format!("id-{index}"), "x");
        }
        buffer.push("newest", "y");
        assert!(!buffer.bytes_by_message.contains_key("id-0"));
        assert_eq!(buffer.bytes_by_message.len(), PRECORRELATION_MESSAGE_IDS);
        assert_eq!(buffer.take("newest"), "y");

        let mut total_capped = PreCorrelationDeltaBuffer::default();
        for index in 0..(PRECORRELATION_TOTAL_BYTES / PRECORRELATION_MESSAGE_BYTES) {
            total_capped.push(&format!("chunk-{index}"), &per_message);
        }
        assert_eq!(total_capped.total_bytes, PRECORRELATION_TOTAL_BYTES);
        total_capped.push("replacement", "z");
        assert!(!total_capped.bytes_by_message.contains_key("chunk-0"));
        assert_eq!(
            total_capped.total_bytes,
            PRECORRELATION_TOTAL_BYTES - PRECORRELATION_MESSAGE_BYTES + 1
        );

        let oversized = "x".repeat(PRECORRELATION_MESSAGE_BYTES + 1);
        total_capped.push("oversized", &oversized);
        assert!(!total_capped.bytes_by_message.contains_key("oversized"));
    }
}
