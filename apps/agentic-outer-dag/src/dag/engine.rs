use crate::dag::stages;
use crate::github::ci::GhCheck;
use crate::github::ci::RequiredCiClient;
use crate::github::ci::RequiredCiPoll;
use crate::github::coderabbit::CodeRabbitClient;
use crate::github::coderabbit::CodeRabbitPoll;
use crate::github::coderabbit::skip_reason_indicates_draft;
use crate::github::pr::DetectedPrLookup;
use crate::github::pr::GitHubPrClient;
use crate::linear;
use crate::opencode::supervisor::InvocationOwnerContext;
use crate::opencode::supervisor::OpenCodeSupervisor;
use crate::opencode::supervisor::PreparedCommandOutcome;
use crate::opencode::supervisor::SessionSelection;
use crate::opencode::supervisor::SupervisedOutcome;
use crate::opencode::supervisor::SupervisionEvent;
use crate::state;
use crate::state::RunState;
use crate::state::StageKind;
use crate::state::store::ThoughtsStateStore;
use crate::worktree::freshness;
use anyhow::Result;
use pr_comments::github::GitHubRestError;
use std::fmt::Write as _;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

const DETECTING_PR_BACKOFFS: [Duration; 4] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
];
const DETECTING_PR_MAX_ATTEMPTS: usize = DETECTING_PR_BACKOFFS.len() + 1;
const SYNC_WITH_MAIN_COMMAND: &str = "sync_with_main_and_resolve_conflicts";
const RESOLVE_PR_CI_COMMAND: &str = "resolve_pr_ci_failures";
const REPRESENTATIVE_BOT_THREAD_LIMIT: usize = 5;
const CI_FAILURE_GRACE_POLLS: u32 = 3;
const MAX_RESOLVE_START_RETRIES: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReviewThreadRef {
    comment_id: u64,
    url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnresolvedReviewThreadsSnapshot {
    total_unresolved: usize,
    bot_unresolved: usize,
    representative_bot_refs: Vec<ReviewThreadRef>,
}

impl UnresolvedReviewThreadsSnapshot {
    fn human_unresolved_threads(&self) -> usize {
        self.total_unresolved.saturating_sub(self.bot_unresolved)
    }
}

fn format_review_thread_refs(review_thread_refs: &[ReviewThreadRef]) -> String {
    if review_thread_refs.is_empty() {
        return "[]".to_string();
    }

    review_thread_refs
        .iter()
        .map(|thread| format!("#{} {}", thread.comment_id, thread.url))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolvePreDispatchAction {
    ReadyForHumanReview { details: String },
    ManualHandoff { details: String },
    DispatchResolve,
}

fn decide_resolve_pre_dispatch(
    before: &UnresolvedReviewThreadsSnapshot,
    resolve_comments_runs: u32,
    max_cycles: u32,
) -> ResolvePreDispatchAction {
    if before.bot_unresolved == 0 {
        return ResolvePreDispatchAction::ReadyForHumanReview {
            details: format!(
                "no unresolved bot review threads; skipping resolve_pr_comments (total unresolved threads: {}, human unresolved threads: {})",
                before.total_unresolved,
                before.human_unresolved_threads()
            ),
        };
    }

    if max_cycles == 0 || resolve_comments_runs >= max_cycles {
        return ResolvePreDispatchAction::ManualHandoff {
            details: format!(
                "max_review_cycles exhausted before resolve_pr_comments: resolve_comments_runs={resolve_comments_runs}/{max_cycles}; unresolved bot threads={}; unresolved human threads={}; representative bot thread parents: {}",
                before.bot_unresolved,
                before.human_unresolved_threads(),
                format_review_thread_refs(&before.representative_bot_refs),
            ),
        };
    }

    ResolvePreDispatchAction::DispatchResolve
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolveInvocationClass {
    Completed,
    AcceptedButNotStarted,
    FailedOrCancelled,
    CleanupUncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResolveAttemptAction {
    AdvanceExternalComplete,
    LoopExternalProgress,
    RetryStart,
    ExhaustedStartRetries,
    ExecutedNoProgress,
    ManualHandoff,
}

fn decide_resolve_attempt(
    before: &UnresolvedReviewThreadsSnapshot,
    after: &UnresolvedReviewThreadsSnapshot,
    lifecycle: ResolveInvocationClass,
    start_retries: u32,
) -> ResolveAttemptAction {
    if after.bot_unresolved == 0 {
        return ResolveAttemptAction::AdvanceExternalComplete;
    }
    if after.bot_unresolved < before.bot_unresolved {
        return ResolveAttemptAction::LoopExternalProgress;
    }

    match lifecycle {
        ResolveInvocationClass::AcceptedButNotStarted
            if start_retries < MAX_RESOLVE_START_RETRIES =>
        {
            ResolveAttemptAction::RetryStart
        }
        ResolveInvocationClass::AcceptedButNotStarted => {
            ResolveAttemptAction::ExhaustedStartRetries
        }
        ResolveInvocationClass::Completed => ResolveAttemptAction::ExecutedNoProgress,
        ResolveInvocationClass::FailedOrCancelled | ResolveInvocationClass::CleanupUncertain => {
            ResolveAttemptAction::ManualHandoff
        }
    }
}

fn current_resolve_invocation_class(state: &RunState) -> ResolveInvocationClass {
    let Some(invocation) = state.opencode.current_invocation.as_ref() else {
        return ResolveInvocationClass::CleanupUncertain;
    };
    let cleanup_uncertain = invocation
        .task_disposition
        .as_ref()
        .is_none_or(|disposition| !task_disposition_is_certain(disposition));
    if cleanup_uncertain {
        return ResolveInvocationClass::CleanupUncertain;
    }

    match invocation.lifecycle_result.as_ref() {
        Some(state::InvocationLifecycleResult::Completed) => ResolveInvocationClass::Completed,
        Some(state::InvocationLifecycleResult::AcceptedButNotStarted) => {
            ResolveInvocationClass::AcceptedButNotStarted
        }
        Some(
            state::InvocationLifecycleResult::Failed { .. }
            | state::InvocationLifecycleResult::Cancelled { .. },
        ) => ResolveInvocationClass::FailedOrCancelled,
        None => ResolveInvocationClass::CleanupUncertain,
    }
}

fn session_selection(active_session_id: Option<&str>) -> SessionSelection {
    active_session_id.map_or(SessionSelection::Fresh, |session_id| {
        SessionSelection::Reuse(session_id.to_string())
    })
}

fn schedule_resolve_start_retry(state: &mut RunState) {
    state.opencode.resolve_start_retries = state.opencode.resolve_start_retries.saturating_add(1);
    state.opencode.active_session_id = None;
    state.opencode.last_resolve_workflow_outcome =
        Some(state::ResolveWorkflowOutcome::RetryScheduled);
    state.stage.kind = StageKind::DispatchingResolvePrComments;
    state.stage.details = Some(format!(
        "resolve_pr_comments was accepted but not started; scheduling fresh-session start retry {} of {}",
        state.opencode.resolve_start_retries, MAX_RESOLVE_START_RETRIES
    ));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CiFailureFlow {
    ContinueWaiting,
    DispatchResolve,
    StopManualHandoff,
}

async fn fetch_unresolved_review_threads_snapshot(
    pr_number: u64,
    representative_limit: usize,
) -> Result<UnresolvedReviewThreadsSnapshot> {
    use pr_comments::PrComments;
    use pr_comments::models::CommentSourceType;

    let pr_comments = PrComments::new()?;

    let mut total_unresolved = None;
    let mut bot_unresolved = 0usize;
    let mut representative_bot_refs = Vec::new();

    loop {
        let page = pr_comments
            .get_comments(Some(pr_number), Some(CommentSourceType::All), Some(false))
            .await?;

        total_unresolved.get_or_insert(page.total_threads);

        for parent in page
            .comments
            .iter()
            .filter(|comment| comment.in_reply_to_id.is_none())
        {
            if parent.is_bot {
                bot_unresolved += 1;
                if representative_bot_refs.len() < representative_limit {
                    representative_bot_refs.push(ReviewThreadRef {
                        comment_id: parent.id,
                        url: parent.html_url.clone(),
                    });
                }
            }
        }

        if !page.has_more {
            break;
        }
    }

    Ok(UnresolvedReviewThreadsSnapshot {
        total_unresolved: total_unresolved.unwrap_or(0),
        bot_unresolved,
        representative_bot_refs,
    })
}

pub struct DagEngine {
    supervisor: Option<OpenCodeSupervisor>,
    owner: Option<Arc<crate::owner::OwnerRuntime>>,
    github: GitHubPrClient,
    coderabbit: CodeRabbitClient,
    ci: RequiredCiClient,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct PlannedAction {
    pub id: &'static str,
    pub summary: &'static str,
}

pub fn planned_actions_for_start() -> Vec<PlannedAction> {
    vec![
        PlannedAction {
            id: "worktree.resolve",
            summary: "Resolve target worktree (create if missing)",
        },
        PlannedAction {
            id: "state.check_existing",
            summary: "Check for existing outer DAG state file",
        },
        PlannedAction {
            id: "state.write_initial",
            summary: "Persist initial outer DAG run state",
        },
        PlannedAction {
            id: "freshness.before_ticket_to_pr",
            summary: "Freshness gate before ticket_to_pr (fetch/check; auto-sync if behind)",
        },
        PlannedAction {
            id: "github.pr.detect_existing",
            summary: "Detect existing open PR for branch",
        },
        PlannedAction {
            id: "opencode.run.linear_ticket_2_pr",
            summary: "If no PR, lazily start OpenCode and run linear_ticket_2_pr",
        },
        PlannedAction {
            id: "github.pr.detect_after_ticket_to_pr",
            summary: "Detect open PR after ticket_to_pr",
        },
        PlannedAction {
            id: "freshness.before_coderabbit_wait",
            summary: "Freshness gate before CodeRabbit wait (fetch/check; auto-sync if behind)",
        },
        PlannedAction {
            id: "github.coderabbit.wait",
            summary: "Poll GitHub until CodeRabbit completes",
        },
        PlannedAction {
            id: "opencode.run.resolve_pr_comments",
            summary: "Lazily start OpenCode if needed and run resolve_pr_comments",
        },
        PlannedAction {
            id: "opencode.run.describe_pr_refresh",
            summary: "Re-check PR head SHA and rerun describe_pr when needed before completion",
        },
        PlannedAction {
            id: "github.ci.wait",
            summary: "Poll GitHub until required non-CodeRabbit CI completes",
        },
        PlannedAction {
            id: "opencode.run.resolve_pr_ci_failures",
            summary: "Run resolve_pr_ci_failures when required CI fails",
        },
        PlannedAction {
            id: "stop.ready_for_human_review",
            summary: "Stop at ready_for_human_review",
        },
    ]
}

fn poll_interval_sleep_duration(poll_interval_seconds: u64) -> std::time::Duration {
    std::time::Duration::from_secs(poll_interval_seconds.max(1))
}

fn coderabbit_waiting_details(
    pr_number: u64,
    head_sha: &str,
    cycle: u32,
    elapsed_seconds: i64,
    timeout_seconds: i64,
    poll_interval_seconds: u64,
) -> String {
    let remaining = (timeout_seconds - elapsed_seconds).max(0);
    format!(
        "waiting for CodeRabbit completion (cycle={cycle}, pr=#{pr_number}, head={head_sha}); elapsed={elapsed_seconds}s; timeout_in={remaining}s; next_poll_in={poll_interval_seconds}s"
    )
}

fn should_reset_coderabbit_timeout_baseline(was_recovered: bool, now_recovered: bool) -> bool {
    !was_recovered && now_recovered
}

fn detecting_pr_retry_attempt_number(attempt_index: usize) -> usize {
    attempt_index + 2
}

fn transition_to_stopped_failed(state: &mut RunState, message: impl Into<String>) {
    let message = message.into();
    state.last_error = Some(message.clone());
    state.stage.kind = StageKind::StoppedFailed;
    state.stage.details = Some(message);
}

fn record_pr_lookup(state: &mut RunState, stage_kind: StageKind, lookup: &DetectedPrLookup) {
    state.pr.last_lookup = Some(state::PrLookupDiagnostics {
        checked_at: chrono::Utc::now().to_rfc3339(),
        stage: stage_kind,
        requested_branch: lookup.requested_branch.clone(),
        current_branch: lookup.current_branch.clone(),
        repo_owner: lookup.repo_owner.clone(),
        repo_name: lookup.repo_name.clone(),
        token_source: lookup.token_source.clone(),
        empty_result_reason: lookup.empty_result_reason.clone(),
        pr_number: lookup.pr.as_ref().map(|pr| pr.number),
        pr_is_draft: lookup.pr.as_ref().map(|pr| pr.is_draft),
        outcome: if lookup.pr.is_some() {
            "found".to_string()
        } else {
            "not_found".to_string()
        },
    });
}

fn transition_to_dispatch_disabled(
    state: &mut RunState,
    resume_stage: &StageKind,
    command_name: &str,
) {
    let mut message = format!(
        "OpenCode dispatch disabled; refusing to run {command_name} at stage {}",
        stage_kind_label(resume_stage)
    );

    if let Some(lookup) = state.pr.last_lookup.as_ref() {
        let _ = write!(
            message,
            " after PR lookup outcome={} for branch '{}' in {}/{}",
            lookup.outcome, lookup.requested_branch, lookup.repo_owner, lookup.repo_name,
        );
        if let Some(current_branch) = lookup.current_branch.as_deref() {
            let _ = write!(message, " (git HEAD: '{current_branch}')");
        }
        if let Some(token_source) = lookup.token_source.as_deref() {
            let _ = write!(message, "; token source={token_source}");
        }
        if let Some(empty_result_reason) = lookup.empty_result_reason.as_deref() {
            let _ = write!(message, "; diagnostic={empty_result_reason}");
        }
    }

    transition_to_stopped_failed(state, message);
}

fn transition_to_ticket_to_pr_no_pr_handoff(state: &mut RunState, context: &str) {
    let message = if let Some(lookup) = state.pr.last_lookup.as_ref() {
        let mut message = format!(
            "ticket_to_pr completed but no open PR found for branch '{}' in {}/{} {context}; stopping for human handoff. Inspect status.pr_lookup for lookup context",
            lookup.requested_branch, lookup.repo_owner, lookup.repo_name,
        );
        if let Some(current_branch) = lookup.current_branch.as_deref() {
            let _ = write!(message, " (git HEAD: '{current_branch}')");
        }
        if let Some(token_source) = lookup.token_source.as_deref() {
            let _ = write!(message, "; token source={token_source}");
        }
        if let Some(empty_result_reason) = lookup.empty_result_reason.as_deref() {
            let _ = write!(message, "; diagnostic={empty_result_reason}");
        }
        message
    } else {
        format!("ticket_to_pr completed but no open PR found {context}; stopping for human handoff")
    };

    state.stage.kind = StageKind::StoppedTicketToPrNoPrHandoff;
    state.stage.details = Some(message);
}

fn stage_kind_label(kind: &StageKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{kind:?}"))
}

fn persist_detected_pr(state: &mut RunState, pr: &pr_comments::models::PrRef) {
    state.pr.number = Some(pr.number);
    state.pr.url = Some(pr.url.clone());
    state.pr.head_sha = Some(pr.head_sha.clone());
    state.pr.last_observed_head_sha = Some(pr.head_sha.clone());
    state.pr.is_draft = Some(pr.is_draft);
}

fn baseline_last_described_head_sha_after_pr_create(state: &mut RunState, head_sha: &str) {
    if state.counters.ticket_to_pr > 0 && state.pr.last_described_head_sha.is_none() {
        state.pr.last_described_head_sha = Some(head_sha.to_string());
    }
}

fn transition_to_ready_for_human_review(state: &mut RunState, details: impl Into<String>) {
    state.stage.kind = StageKind::StoppedReadyForHumanReview;
    state.stage.details = Some(details.into());
}

fn format_ci_checks(checks: &[GhCheck]) -> String {
    if checks.is_empty() {
        return "[]".to_string();
    }

    checks
        .iter()
        .map(|check| {
            let url = check.details_url.as_deref().unwrap_or("");
            if url.is_empty() {
                format!("{} ({})", check.name, check.state)
            } else {
                format!("{} ({}) {url}", check.name, check.state)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn reset_ci_tracking(state: &mut RunState) {
    state.ci.last_remediated_head_sha = None;
    state.ci.last_remediated_fingerprint = None;
    state.ci.grace_polls_remaining = 0;
}

fn handle_ci_gate_head_sha_change(
    state: &mut RunState,
    previous_head_sha: Option<&str>,
    refreshed_head_sha: &str,
) -> bool {
    let Some(previous_head_sha) = previous_head_sha else {
        return false;
    };
    if previous_head_sha == refreshed_head_sha {
        return false;
    }

    reset_ci_tracking(state);
    state.stage.kind = StageKind::FreshnessBeforeCoderabbitWait;
    state.stage.details = Some(format!(
        "PR head SHA changed during CI gate ({previous_head_sha}→{refreshed_head_sha}); restarting CodeRabbit wait"
    ));
    true
}

fn apply_required_ci_failure(
    state: &mut RunState,
    head_sha: &str,
    fingerprint: &str,
    failing: &[GhCheck],
) -> CiFailureFlow {
    let max_cycles = state.settings.max_review_cycles;
    if max_cycles == 0 || state.counters.resolve_ci >= max_cycles {
        state.stage.kind = StageKind::StoppedManualHandoff;
        state.stage.details = Some(format!(
            "required CI failing but max_review_cycles exhausted for CI remediation: resolve_ci_runs={}/{max_cycles}; head={head_sha}; failing checks: {}",
            state.counters.resolve_ci,
            format_ci_checks(failing),
        ));
        return CiFailureFlow::StopManualHandoff;
    }

    let same_head = state.ci.last_remediated_head_sha.as_deref() == Some(head_sha);
    let same_fingerprint = state.ci.last_remediated_fingerprint.as_deref() == Some(fingerprint);

    if same_head && same_fingerprint {
        if state.ci.grace_polls_remaining == 0 {
            state.stage.kind = StageKind::StoppedManualHandoff;
            state.stage.details = Some(format!(
                "required CI still failing with same fingerprint after remediation and grace exhausted; head={head_sha}; fingerprint={fingerprint}; failing checks: {}",
                format_ci_checks(failing),
            ));
            return CiFailureFlow::StopManualHandoff;
        }

        state.ci.grace_polls_remaining = state.ci.grace_polls_remaining.saturating_sub(1);
        if state.ci.grace_polls_remaining == 0 {
            state.stage.kind = StageKind::StoppedManualHandoff;
            state.stage.details = Some(format!(
                "required CI still failing with same fingerprint after remediation; grace exhausted on this poll; head={head_sha}; fingerprint={fingerprint}; failing checks: {}",
                format_ci_checks(failing),
            ));
            return CiFailureFlow::StopManualHandoff;
        }

        state.stage.kind = StageKind::WaitingForCi;
        state.stage.details = Some(format!(
            "required CI still failing with same fingerprint after remediation; grace polls remaining={}; head={head_sha}; failing checks: {}",
            state.ci.grace_polls_remaining,
            format_ci_checks(failing),
        ));
        return CiFailureFlow::ContinueWaiting;
    }

    state.ci.last_remediated_head_sha = Some(head_sha.to_string());
    state.ci.last_remediated_fingerprint = Some(fingerprint.to_string());
    state.ci.grace_polls_remaining = CI_FAILURE_GRACE_POLLS;
    state.stage.kind = StageKind::DispatchingResolvePrCiFailures;
    state.stage.details = Some(format!(
        "required CI failed; dispatching {RESOLVE_PR_CI_COMMAND}; head={head_sha}; resolve_ci_runs={}/{}; failing checks: {}",
        state.counters.resolve_ci,
        max_cycles,
        format_ci_checks(failing),
    ));
    CiFailureFlow::DispatchResolve
}

fn route_after_ci_remediation(
    state: &mut RunState,
    remediated_head_sha: &str,
    redetected_head_sha: &str,
) {
    if redetected_head_sha == remediated_head_sha {
        state.stage.kind = StageKind::WaitingForCi;
        state.stage.details = Some(format!(
            "CI remediation completed without changing PR head SHA ({remediated_head_sha}); continuing CI gate"
        ));
    } else {
        reset_ci_tracking(state);
        state.stage.kind = StageKind::FreshnessBeforeCoderabbitWait;
        state.stage.details = Some(format!(
            "CI remediation changed PR head SHA ({remediated_head_sha}→{redetected_head_sha}); restarting CodeRabbit wait"
        ));
    }
}

enum DescribePrRefreshDecision {
    Stop,
    Rerun { head_sha: String },
}

fn prepare_dispatch_describe_pr_stage(
    state: &mut RunState,
    lookup: DetectedPrLookup,
) -> DescribePrRefreshDecision {
    record_pr_lookup(state, StageKind::DispatchingDescribePr, &lookup);

    let Some(pr) = lookup.pr else {
        transition_to_stopped_failed(
            state,
            format!(
                "describe_pr refresh could not re-detect an open PR for branch '{}'",
                state.worktree.branch
            ),
        );
        return DescribePrRefreshDecision::Stop;
    };

    let head_sha = pr.head_sha.clone();
    persist_detected_pr(state, &pr);

    if state.pr.last_described_head_sha.as_deref() == Some(head_sha.as_str()) {
        state.stage.kind = StageKind::WaitingForCi;
        state.stage.details = Some(
            "completed one CodeRabbit resolve cycle; describe_pr already covered current PR head SHA; entering CI gate"
                .to_string(),
        );
        return DescribePrRefreshDecision::Stop;
    }

    DescribePrRefreshDecision::Rerun { head_sha }
}

fn finish_dispatch_describe_pr_stage_after_rerun(
    state: &mut RunState,
    head_sha: String,
) -> DescribePrRefreshDecision {
    if stages::is_paused(&state.stage.kind) || matches!(state.stage.kind, StageKind::StoppedFailed)
    {
        return DescribePrRefreshDecision::Stop;
    }

    state.pr.last_described_head_sha = Some(head_sha);
    state.stage.kind = StageKind::WaitingForCi;
    state.stage.details = Some(
        "completed one CodeRabbit resolve cycle, refreshed PR description, and entered CI gate"
            .to_string(),
    );
    DescribePrRefreshDecision::Stop
}

async fn ensure_pr_ready_for_review<MarkReady, MarkReadyFut>(
    state: &mut RunState,
    pr: &pr_comments::models::PrRef,
    context: &str,
    mark_ready: MarkReady,
) -> Result<pr_comments::models::PrRef>
where
    MarkReady: FnOnce(pr_comments::models::PrRef) -> MarkReadyFut,
    MarkReadyFut: Future<Output = Result<pr_comments::models::PrRef>>,
{
    persist_detected_pr(state, pr);
    if !pr.is_draft {
        state.pr.ready_for_review.last_result = Some(format!("already_ready:{context}"));
        return Ok(pr.clone());
    }

    state.pr.ready_for_review.attempts += 1;
    state.pr.ready_for_review.last_attempted_at = Some(chrono::Utc::now().to_rfc3339());
    let updated_pr = mark_ready(pr.clone()).await.map_err(|error| {
        anyhow::anyhow!(
            "draft PR #{} must be ready for review before proceeding ({context}): {error}",
            pr.number
        )
    })?;
    persist_detected_pr(state, &updated_pr);
    state.pr.is_draft = Some(false);
    state.pr.ready_for_review.last_result = Some(format!("marked_ready:{context}"));
    Ok(updated_pr)
}

#[derive(Debug)]
enum DraftSkipRecovery {
    ContinueWaiting,
    TerminalStop { message: String },
}

#[derive(Debug)]
enum CheckSuite422Recovery {
    NotApplicable,
    ContinueWaiting,
    TerminalStop { message: String },
}

fn is_check_suites_no_commit_found_422(err: &GitHubRestError, head_sha: &str) -> bool {
    if err.status != 422 {
        return false;
    }

    let expected_path = format!("/commits/{head_sha}/check-suites");
    if !err.url.contains(&expected_path) {
        return false;
    }

    let body = err.body.to_ascii_lowercase();
    body.contains("no commit found for sha") && body.contains(&head_sha.to_ascii_lowercase())
}

fn check_suite_422_footer(rest: &GitHubRestError) -> String {
    format!("\n\nURL: {}\nBody: {}", rest.url, rest.body)
}

async fn recover_from_draft_review_skip<DetectPr, DetectPrFut, MarkReady, MarkReadyFut>(
    state: &mut RunState,
    reason: &str,
    detect_pr: DetectPr,
    mark_ready: MarkReady,
) -> Result<DraftSkipRecovery>
where
    DetectPr: FnOnce() -> DetectPrFut,
    DetectPrFut: Future<Output = Result<DetectedPrLookup>>,
    MarkReady: FnOnce(pr_comments::models::PrRef) -> MarkReadyFut,
    MarkReadyFut: Future<Output = Result<pr_comments::models::PrRef>>,
{
    if !skip_reason_indicates_draft(reason) {
        return Ok(DraftSkipRecovery::TerminalStop {
            message: reason.to_string(),
        });
    }

    if state.pr.ready_for_review.coderabbit_draft_skip_recovered {
        state.stage.kind = StageKind::WaitingForCoderabbit;
        state.stage.details = Some(
            "CodeRabbit still reports the earlier draft-skip comment after recovery; treating it as stale and continuing to wait"
                .to_string(),
        );
        return Ok(DraftSkipRecovery::ContinueWaiting);
    }

    let lookup = detect_pr().await?;
    record_pr_lookup(state, StageKind::WaitingForCoderabbit, &lookup);
    let Some(pr) = lookup.pr else {
        return Ok(DraftSkipRecovery::TerminalStop {
            message: format!(
                "CodeRabbit reported draft-detected skip, but no open PR could be re-detected for branch '{}'",
                state.worktree.branch
            ),
        });
    };

    ensure_pr_ready_for_review(state, &pr, "coderabbit_draft_skip_recovery", mark_ready).await?;
    state.pr.ready_for_review.coderabbit_draft_skip_recovered = true;
    state.stage.kind = StageKind::WaitingForCoderabbit;
    state.stage.details = Some(
        "CodeRabbit skipped review because the PR was draft; marked ready for review and continuing to wait"
            .to_string(),
    );
    Ok(DraftSkipRecovery::ContinueWaiting)
}

async fn recover_from_check_suite_no_commit_found_422<DetectPr, DetectPrFut>(
    state: &mut RunState,
    head_sha: &str,
    poll_error: &anyhow::Error,
    detect_pr: DetectPr,
) -> Result<CheckSuite422Recovery>
where
    DetectPr: FnOnce() -> DetectPrFut,
    DetectPrFut: Future<Output = Result<DetectedPrLookup>>,
{
    let Some(rest) = poll_error
        .chain()
        .find_map(|cause| cause.downcast_ref::<GitHubRestError>())
        .filter(|rest| is_check_suites_no_commit_found_422(rest, head_sha))
    else {
        return Ok(CheckSuite422Recovery::NotApplicable);
    };

    let branch = state.worktree.branch.clone();
    let lookup = match detect_pr().await {
        Ok(lookup) => lookup,
        Err(error) => {
            return Ok(CheckSuite422Recovery::TerminalStop {
                message: format!(
                    "GitHub check-suites returned 422 (No commit found for SHA) for {head_sha}, and PR re-detection failed for branch '{branch}': {error}{}",
                    check_suite_422_footer(rest)
                ),
            });
        }
    };

    record_pr_lookup(state, StageKind::WaitingForCoderabbit, &lookup);

    let Some(pr) = lookup.pr else {
        return Ok(CheckSuite422Recovery::TerminalStop {
            message: format!(
                "GitHub check-suites returned 422 (No commit found for SHA) for {head_sha}, but no open PR could be re-detected for branch '{branch}'.{}",
                check_suite_422_footer(rest)
            ),
        });
    };

    if pr.head_sha == head_sha {
        return Ok(CheckSuite422Recovery::TerminalStop {
            message: format!(
                "GitHub check-suites returned 422 (No commit found for SHA) for {head_sha}. Re-detected remote PR head is unchanged ({head_sha}); stopping for manual handoff.{}",
                check_suite_422_footer(rest)
            ),
        });
    }

    let old = head_sha.to_string();
    let new = pr.head_sha.clone();
    persist_detected_pr(state, &pr);
    state.stage.kind = StageKind::WaitingForCoderabbit;
    state.stage.details = Some(format!(
        "Recovered from GitHub check-suites 422 (No commit found for SHA) for stale head {old}; remote PR head is now {new}. Updated stored head SHA and continuing to wait."
    ));

    Ok(CheckSuite422Recovery::ContinueWaiting)
}

fn persist_stop_state_before_handoff<Save>(state: &RunState, save: Save) -> Result<String>
where
    Save: FnMut(&RunState) -> Result<()>,
{
    let mut save = save;
    save(state)?;
    state
        .stage
        .details
        .clone()
        .ok_or_else(|| anyhow::anyhow!("missing stop detail before Linear handoff"))
}

impl DagEngine {
    pub fn for_current_dir() -> Result<Self> {
        Ok(Self {
            supervisor: None,
            owner: None,
            github: GitHubPrClient::new()?,
            coderabbit: CodeRabbitClient::new()?,
            ci: RequiredCiClient::new(),
        })
    }

    pub fn for_current_dir_with_owner(owner: Arc<crate::owner::OwnerRuntime>) -> Result<Self> {
        let mut engine = Self::for_current_dir()?;
        engine.owner = Some(owner);
        Ok(engine)
    }

    pub async fn run_until_stop(&mut self, stop_after: Option<StageKind>) -> Result<()> {
        loop {
            let mut state = ThoughtsStateStore::load()?
                .ok_or_else(|| anyhow::anyhow!("no state; run start first"))?;

            if stages::is_terminal(&state.stage.kind) || stages::is_paused(&state.stage.kind) {
                return Ok(());
            }

            if let Some(stop_after) = stop_after.as_ref()
                && stages::is_beyond_stop_after(&state.stage.kind, stop_after)
            {
                return Ok(());
            }

            match state.stage.kind.clone() {
                StageKind::FreshnessBeforeTicketToPr => {
                    self.advance_freshness(
                        &mut state,
                        StageKind::DispatchingTicketToPr,
                        StageKind::FreshnessBeforeTicketToPr,
                    )
                    .await?;
                }
                StageKind::DispatchingTicketToPr => {
                    // INVARIANT: duplicate-PR guard must remain before linear_ticket_2_pr dispatch.
                    let lookup = self
                        .github
                        .detect_open_pr_from_branch(&state.worktree.branch)
                        .await?;
                    record_pr_lookup(&mut state, StageKind::DispatchingTicketToPr, &lookup);
                    if let Some(pr) = lookup.pr {
                        let github = &self.github;
                        ensure_pr_ready_for_review(
                            &mut state,
                            &pr,
                            "existing_pr_guard",
                            |pr| async move { github.mark_ready_for_review(&pr).await },
                        )
                        .await?;
                        state.stage.kind = StageKind::FreshnessBeforeCoderabbitWait;
                        state.stage.details = None;
                        ThoughtsStateStore::save(&state)?;
                        continue;
                    }

                    let ticket_key = state.ticket.linear_key.clone();
                    self.run_supervised_command(
                        &mut state,
                        StageKind::DispatchingTicketToPr,
                        "linear_ticket_2_pr",
                        Some(ticket_key.as_str()),
                    )
                    .await?;
                    if stages::is_paused(&state.stage.kind)
                        || matches!(state.stage.kind, StageKind::StoppedFailed)
                    {
                        return Ok(());
                    }
                    state.stage.kind = StageKind::DetectingPr;
                    state.stage.details = None;
                    state.counters.ticket_to_pr += 1;
                    ThoughtsStateStore::save(&state)?;
                }
                StageKind::DetectingPr => {
                    let branch = state.worktree.branch.clone();
                    let lookup = detect_pr_with_retry(
                        || self.github.detect_open_pr_from_branch(&branch),
                        |next_attempt, backoff, lookup| {
                            record_pr_lookup(&mut state, StageKind::DetectingPr, lookup);
                            state.stage.details = Some(format!(
                                "no PR visible yet after ticket_to_pr; retry {next_attempt}/{DETECTING_PR_MAX_ATTEMPTS} in {}s",
                                backoff.as_secs()
                            ));
                            ThoughtsStateStore::save(&state)
                        },
                        tokio::time::sleep,
                    )
                    .await?;
                    record_pr_lookup(&mut state, StageKind::DetectingPr, &lookup);
                    if let Some(pr) = lookup.pr {
                        let github = &self.github;
                        let pr = ensure_pr_ready_for_review(
                            &mut state,
                            &pr,
                            "post_ticket_to_pr_detection",
                            |pr| async move { github.mark_ready_for_review(&pr).await },
                        )
                        .await?;
                        baseline_last_described_head_sha_after_pr_create(&mut state, &pr.head_sha);
                        state.stage.kind = StageKind::FreshnessBeforeCoderabbitWait;
                        state.stage.details = None;
                    } else {
                        transition_to_ticket_to_pr_no_pr_handoff(
                            &mut state,
                            "after ticket_to_pr run",
                        );
                    }
                    ThoughtsStateStore::save(&state)?;
                }
                StageKind::FreshnessBeforeCoderabbitWait => {
                    self.advance_freshness(
                        &mut state,
                        StageKind::WaitingForCoderabbit,
                        StageKind::FreshnessBeforeCoderabbitWait,
                    )
                    .await?;
                }
                StageKind::WaitingForCoderabbit => {
                    let pr_number = state
                        .pr
                        .number
                        .ok_or_else(|| anyhow::anyhow!("missing PR number in state"))?;
                    let mut started_at = chrono::Utc::now();
                    let timeout_seconds = i64::try_from(state.settings.coderabbit_timeout_seconds)
                        .map_err(|_| anyhow::anyhow!("coderabbit timeout exceeds i64 range"))?;
                    loop {
                        let head_sha = state
                            .pr
                            .head_sha
                            .clone()
                            .ok_or_else(|| anyhow::anyhow!("missing PR head SHA in state"))?;
                        let poll = match self.coderabbit.poll_once(pr_number, &head_sha).await {
                            Ok(poll) => poll,
                            Err(err) => {
                                let branch = state.worktree.branch.clone();
                                let github = &self.github;

                                match recover_from_check_suite_no_commit_found_422(
                                    &mut state,
                                    &head_sha,
                                    &err,
                                    || github.detect_open_pr_from_branch(&branch),
                                )
                                .await?
                                {
                                    CheckSuite422Recovery::NotApplicable => return Err(err),
                                    CheckSuite422Recovery::ContinueWaiting => {
                                        started_at = chrono::Utc::now();
                                        ThoughtsStateStore::save(&state)?;
                                        tokio::time::sleep(poll_interval_sleep_duration(
                                            state.settings.poll_interval_seconds,
                                        ))
                                        .await;
                                        continue;
                                    }
                                    CheckSuite422Recovery::TerminalStop { message } => {
                                        state.stage.kind = StageKind::StoppedManualHandoff;
                                        state.stage.details = Some(message);
                                        let message = persist_stop_state_before_handoff(
                                            &state,
                                            ThoughtsStateStore::save,
                                        )?;
                                        linear::post_handoff_once(&mut state, &message).await?;
                                        ThoughtsStateStore::save(&state)?;
                                        return Ok(());
                                    }
                                }
                            }
                        };

                        match poll {
                            CodeRabbitPoll::Completed => {
                                state.coderabbit.current_cycle += 1;
                                state.stage.kind = StageKind::DispatchingResolvePrComments;
                                state.stage.details = None;
                                ThoughtsStateStore::save(&state)?;
                                break;
                            }
                            CodeRabbitPoll::Skipped { reason } => {
                                let branch = state.worktree.branch.clone();
                                let github = &self.github;
                                let recovered_before =
                                    state.pr.ready_for_review.coderabbit_draft_skip_recovered;
                                match recover_from_draft_review_skip(
                                    &mut state,
                                    &reason,
                                    || github.detect_open_pr_from_branch(&branch),
                                    |pr| async move { github.mark_ready_for_review(&pr).await },
                                )
                                .await?
                                {
                                    DraftSkipRecovery::ContinueWaiting => {
                                        if should_reset_coderabbit_timeout_baseline(
                                            recovered_before,
                                            state
                                                .pr
                                                .ready_for_review
                                                .coderabbit_draft_skip_recovered,
                                        ) {
                                            started_at = chrono::Utc::now();
                                        }
                                        ThoughtsStateStore::save(&state)?;
                                        tokio::time::sleep(poll_interval_sleep_duration(
                                            state.settings.poll_interval_seconds,
                                        ))
                                        .await;
                                    }
                                    DraftSkipRecovery::TerminalStop { message } => {
                                        state.stage.kind = StageKind::StoppedReviewSkipped;
                                        state.stage.details = Some(message);
                                        ThoughtsStateStore::save(&state)?;
                                        return Ok(());
                                    }
                                }
                            }
                            CodeRabbitPoll::Waiting => {
                                let elapsed_seconds =
                                    (chrono::Utc::now() - started_at).num_seconds().max(0);

                                if elapsed_seconds >= timeout_seconds {
                                    state.stage.kind = StageKind::StoppedTimedOut;
                                    state.stage.details = Some(
                                        "timed out waiting for CodeRabbit completion".to_string(),
                                    );
                                    ThoughtsStateStore::save(&state)?;
                                    return Ok(());
                                }

                                state.stage.kind = StageKind::WaitingForCoderabbit;
                                state.stage.details = Some(coderabbit_waiting_details(
                                    pr_number,
                                    &head_sha,
                                    state.coderabbit.current_cycle,
                                    elapsed_seconds,
                                    timeout_seconds,
                                    state.settings.poll_interval_seconds,
                                ));
                                ThoughtsStateStore::save(&state)?;

                                tokio::time::sleep(poll_interval_sleep_duration(
                                    state.settings.poll_interval_seconds,
                                ))
                                .await;
                            }
                        }
                    }
                }
                StageKind::DispatchingResolvePrComments => {
                    let Some(pr_number) = state.pr.number else {
                        transition_to_stopped_failed(
                            &mut state,
                            "missing PR number in state before resolve_pr_comments",
                        );
                        ThoughtsStateStore::save(&state)?;
                        return Ok(());
                    };

                    let max_cycles = state.settings.max_review_cycles;

                    loop {
                        let before = match fetch_unresolved_review_threads_snapshot(
                            pr_number,
                            REPRESENTATIVE_BOT_THREAD_LIMIT,
                        )
                        .await
                        {
                            Ok(snapshot) => snapshot,
                            Err(error) => {
                                state.stage.kind = StageKind::StoppedManualHandoff;
                                state.stage.details = Some(format!(
                                    "failed to measure unresolved PR review threads before resolve_pr_comments: {error}"
                                ));
                                ThoughtsStateStore::save(&state)?;
                                return Ok(());
                            }
                        };

                        match decide_resolve_pre_dispatch(
                            &before,
                            state.counters.resolve_comments,
                            max_cycles,
                        ) {
                            ResolvePreDispatchAction::ReadyForHumanReview { details } => {
                                state.opencode.resolve_start_retries = 0;
                                state.opencode.last_resolve_workflow_outcome =
                                    Some(state::ResolveWorkflowOutcome::Skipped);
                                state.stage.kind = StageKind::DispatchingDescribePr;
                                state.stage.details = Some(details);
                                ThoughtsStateStore::save(&state)?;
                                break;
                            }
                            ResolvePreDispatchAction::ManualHandoff { details } => {
                                state.opencode.resolve_start_retries = 0;
                                state.opencode.last_resolve_workflow_outcome =
                                    Some(state::ResolveWorkflowOutcome::ManualHandoff);
                                state.stage.kind = StageKind::StoppedManualHandoff;
                                state.stage.details = Some(details);
                                ThoughtsStateStore::save(&state)?;
                                return Ok(());
                            }
                            ResolvePreDispatchAction::DispatchResolve => {}
                        }

                        self.run_supervised_command(
                            &mut state,
                            StageKind::DispatchingResolvePrComments,
                            "resolve_pr_comments",
                            None,
                        )
                        .await?;
                        if stages::is_paused(&state.stage.kind)
                            || matches!(state.stage.kind, StageKind::StoppedFailed)
                        {
                            return Ok(());
                        }

                        let after = match fetch_unresolved_review_threads_snapshot(
                            pr_number,
                            REPRESENTATIVE_BOT_THREAD_LIMIT,
                        )
                        .await
                        {
                            Ok(snapshot) => snapshot,
                            Err(error) => {
                                state.stage.kind = StageKind::StoppedManualHandoff;
                                state.stage.details = Some(format!(
                                    "failed to measure unresolved PR review threads after resolve_pr_comments: {error}"
                                ));
                                ThoughtsStateStore::save(&state)?;
                                return Ok(());
                            }
                        };

                        let lifecycle = current_resolve_invocation_class(&state);
                        match decide_resolve_attempt(
                            &before,
                            &after,
                            lifecycle,
                            state.opencode.resolve_start_retries,
                        ) {
                            ResolveAttemptAction::AdvanceExternalComplete => {
                                state.opencode.resolve_start_retries = 0;
                                state.opencode.last_resolve_workflow_outcome =
                                    Some(state::ResolveWorkflowOutcome::ExternalProgress);
                                state.stage.kind = StageKind::DispatchingDescribePr;
                                state.stage.details = Some(format!(
                                    "CodeRabbit postcondition reached zero unresolved bot threads after resolve_pr_comments (before: {}, after: {}; lifecycle: {lifecycle:?})",
                                    before.bot_unresolved, after.bot_unresolved
                                ));
                                ThoughtsStateStore::save(&state)?;
                                break;
                            }
                            ResolveAttemptAction::LoopExternalProgress => {
                                state.counters.resolve_comments += 1;
                                state.opencode.resolve_start_retries = 0;
                                state.opencode.last_resolve_workflow_outcome =
                                    Some(state::ResolveWorkflowOutcome::ExternalProgress);
                                state.stage.kind = StageKind::DispatchingResolvePrComments;
                                state.stage.details = Some(format!(
                                    "CodeRabbit unresolved bot threads decreased (before: {}, after: {}; lifecycle: {lifecycle:?}); continuing resolve cycle {} of {}",
                                    before.bot_unresolved,
                                    after.bot_unresolved,
                                    state.counters.resolve_comments,
                                    max_cycles
                                ));
                                ThoughtsStateStore::save(&state)?;
                            }
                            ResolveAttemptAction::RetryStart => {
                                schedule_resolve_start_retry(&mut state);
                                ThoughtsStateStore::save(&state)?;
                            }
                            ResolveAttemptAction::ExhaustedStartRetries => {
                                state.opencode.last_resolve_workflow_outcome =
                                    Some(state::ResolveWorkflowOutcome::RetriesExhausted);
                                state.opencode.resolve_start_retries = 0;
                                state.stage.kind = StageKind::StoppedManualHandoff;
                                state.stage.details = Some(format!(
                                    "resolve_pr_comments exhausted {} fresh-session start retries after the initial attempt; unresolved bot threads unchanged at {}",
                                    MAX_RESOLVE_START_RETRIES, after.bot_unresolved
                                ));
                                ThoughtsStateStore::save(&state)?;
                                return Ok(());
                            }
                            ResolveAttemptAction::ExecutedNoProgress => {
                                state.opencode.resolve_start_retries = 0;
                                state.opencode.last_resolve_workflow_outcome =
                                    Some(state::ResolveWorkflowOutcome::ExecutedNoProgress);
                                state.stage.kind = StageKind::StoppedManualHandoff;
                                state.stage.details = Some(format!(
                                    "resolve_pr_comments executed with current-command assistant evidence but unresolved bot threads did not decrease (before: {}, after: {})",
                                    before.bot_unresolved, after.bot_unresolved
                                ));
                                ThoughtsStateStore::save(&state)?;
                                return Ok(());
                            }
                            ResolveAttemptAction::ManualHandoff => {
                                state.opencode.resolve_start_retries = 0;
                                state.opencode.last_resolve_workflow_outcome =
                                    Some(state::ResolveWorkflowOutcome::ManualHandoff);
                                state.stage.kind = StageKind::StoppedManualHandoff;
                                state.stage.details = Some(format!(
                                    "resolve_pr_comments lifecycle failed or cleanup was uncertain; unresolved bot threads unchanged at {}",
                                    after.bot_unresolved
                                ));
                                ThoughtsStateStore::save(&state)?;
                                return Ok(());
                            }
                        }
                    }
                }
                StageKind::DispatchingDescribePr => {
                    let branch = state.worktree.branch.clone();
                    let lookup = self.github.detect_open_pr_from_branch(&branch).await?;
                    match prepare_dispatch_describe_pr_stage(&mut state, lookup) {
                        DescribePrRefreshDecision::Stop => {}
                        DescribePrRefreshDecision::Rerun { head_sha } => {
                            self.run_supervised_command(
                                &mut state,
                                StageKind::DispatchingDescribePr,
                                "describe_pr",
                                None,
                            )
                            .await?;
                            finish_dispatch_describe_pr_stage_after_rerun(&mut state, head_sha);
                        }
                    }
                    ThoughtsStateStore::save(&state)?;
                }
                StageKind::WaitingForCi => {
                    let pr_number = state.pr.number.ok_or_else(|| {
                        anyhow::anyhow!("missing PR number in state before CI wait")
                    })?;
                    let branch = state.worktree.branch.clone();
                    loop {
                        let previous_head_sha = state.pr.head_sha.clone();
                        let lookup = self.github.detect_open_pr_from_branch(&branch).await?;
                        record_pr_lookup(&mut state, StageKind::WaitingForCi, &lookup);
                        let Some(pr) = lookup.pr else {
                            let worktree_branch = state.worktree.branch.clone();
                            transition_to_stopped_failed(
                                &mut state,
                                format!(
                                    "CI gate could not re-detect an open PR for branch '{worktree_branch}'"
                                ),
                            );
                            ThoughtsStateStore::save(&state)?;
                            return Ok(());
                        };
                        let refreshed_head_sha = pr.head_sha.clone();
                        persist_detected_pr(&mut state, &pr);

                        if handle_ci_gate_head_sha_change(
                            &mut state,
                            previous_head_sha.as_deref(),
                            &refreshed_head_sha,
                        ) {
                            ThoughtsStateStore::save(&state)?;
                            break;
                        }

                        match self.ci.poll_once(pr_number)? {
                            RequiredCiPoll::Waiting {
                                pending,
                                fingerprint,
                            } => {
                                state.stage.kind = StageKind::WaitingForCi;
                                state.stage.details = Some(format!(
                                    "waiting for required CI on head {refreshed_head_sha}; pending checks: {}; fingerprint={fingerprint}",
                                    format_ci_checks(&pending),
                                ));
                                ThoughtsStateStore::save(&state)?;
                                tokio::time::sleep(poll_interval_sleep_duration(
                                    state.settings.poll_interval_seconds,
                                ))
                                .await;
                            }
                            RequiredCiPoll::Passed { fingerprint } => {
                                reset_ci_tracking(&mut state);
                                transition_to_ready_for_human_review(
                                    &mut state,
                                    format!(
                                        "required CI passed; head={refreshed_head_sha}; fingerprint={fingerprint}"
                                    ),
                                );
                                ThoughtsStateStore::save(&state)?;
                                return Ok(());
                            }
                            RequiredCiPoll::Failed {
                                failing,
                                fingerprint,
                            } => {
                                match apply_required_ci_failure(
                                    &mut state,
                                    &refreshed_head_sha,
                                    &fingerprint,
                                    &failing,
                                ) {
                                    CiFailureFlow::ContinueWaiting => {
                                        ThoughtsStateStore::save(&state)?;
                                        tokio::time::sleep(poll_interval_sleep_duration(
                                            state.settings.poll_interval_seconds,
                                        ))
                                        .await;
                                    }
                                    CiFailureFlow::DispatchResolve => {
                                        ThoughtsStateStore::save(&state)?;
                                        break;
                                    }
                                    CiFailureFlow::StopManualHandoff => {
                                        let message = persist_stop_state_before_handoff(
                                            &state,
                                            ThoughtsStateStore::save,
                                        )?;
                                        linear::post_handoff_once(&mut state, &message).await?;
                                        ThoughtsStateStore::save(&state)?;
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                }
                StageKind::DispatchingResolvePrCiFailures => {
                    self.run_supervised_command(
                        &mut state,
                        StageKind::DispatchingResolvePrCiFailures,
                        RESOLVE_PR_CI_COMMAND,
                        None,
                    )
                    .await?;
                    if stages::is_paused(&state.stage.kind)
                        || matches!(state.stage.kind, StageKind::StoppedFailed)
                    {
                        return Ok(());
                    }

                    state.counters.resolve_ci += 1;
                    let remediated_head_sha = state
                        .ci
                        .last_remediated_head_sha
                        .clone()
                        .or_else(|| state.pr.head_sha.clone())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "missing remediated head SHA in state after CI remediation"
                            )
                        })?;
                    let branch = state.worktree.branch.clone();
                    let lookup = self.github.detect_open_pr_from_branch(&branch).await?;
                    record_pr_lookup(
                        &mut state,
                        StageKind::DispatchingResolvePrCiFailures,
                        &lookup,
                    );
                    let Some(pr) = lookup.pr else {
                        let worktree_branch = state.worktree.branch.clone();
                        transition_to_stopped_failed(
                            &mut state,
                            format!(
                                "CI remediation completed but PR could not be re-detected for branch '{worktree_branch}'"
                            ),
                        );
                        ThoughtsStateStore::save(&state)?;
                        return Ok(());
                    };
                    let redetected_head_sha = pr.head_sha.clone();
                    persist_detected_pr(&mut state, &pr);
                    route_after_ci_remediation(
                        &mut state,
                        &remediated_head_sha,
                        &redetected_head_sha,
                    );
                    ThoughtsStateStore::save(&state)?;
                }
                StageKind::Init
                | StageKind::StoppedPermissionRequired
                | StageKind::StoppedQuestionRequired
                | StageKind::StoppedDirtyTree
                | StageKind::StoppedRebaseConflict
                | StageKind::StoppedManualHandoff
                | StageKind::StoppedReviewSkipped
                | StageKind::StoppedTimedOut
                | StageKind::StoppedReadyForHumanReview
                | StageKind::StoppedTicketToPrNoPrHandoff
                | StageKind::StoppedFailed => return Ok(()),
            }
        }
    }

    async fn advance_freshness(
        &mut self,
        state: &mut RunState,
        next_stage: StageKind,
        resume_stage: StageKind,
    ) -> Result<()> {
        let outcome = freshness::run(&state.worktree.base_ref, state.settings.dry_run)?;
        state.freshness.last_checked_at = Some(chrono::Utc::now().to_rfc3339());
        state.opencode.resume_stage = Some(resume_stage.clone());
        match outcome {
            freshness::FreshnessOutcome::UpToDate => {
                state.freshness.last_result = Some("up_to_date".to_string());
                state.stage.kind = next_stage;
                state.stage.details = None;
            }
            freshness::FreshnessOutcome::Behind { head_sha, base_sha } => {
                state.freshness.last_result = Some(format!("behind:{head_sha}..{base_sha}"));

                let message = format!(
                    "OuterDAG freshness gate: branch '{}' is behind {} (HEAD={head_sha}, base={base_sha}). Run merge-based sync, resolve only bounded mechanical conflicts, verify, and push normally (no force).",
                    state.worktree.branch, state.worktree.base_ref,
                );

                self.run_supervised_command(
                    state,
                    resume_stage.clone(),
                    SYNC_WITH_MAIN_COMMAND,
                    Some(message.as_str()),
                )
                .await?;

                if stages::is_paused(&state.stage.kind)
                    || matches!(state.stage.kind, StageKind::StoppedFailed)
                {
                    return Ok(());
                }

                let post = freshness::run(&state.worktree.base_ref, state.settings.dry_run)?;
                state.freshness.last_checked_at = Some(chrono::Utc::now().to_rfc3339());

                match post {
                    freshness::FreshnessOutcome::UpToDate => {
                        state.freshness.last_result = Some("up_to_date".to_string());

                        if state.pr.number.is_some() {
                            let branch = state.worktree.branch.clone();
                            let lookup = self.github.detect_open_pr_from_branch(&branch).await?;
                            record_pr_lookup(state, resume_stage.clone(), &lookup);
                            let Some(pr) = lookup.pr else {
                                state.stage.kind = StageKind::StoppedManualHandoff;
                                state.stage.details = Some(format!(
                                    "sync completed but PR could not be re-detected for branch '{branch}'; stopping for human handoff"
                                ));
                                let message = persist_stop_state_before_handoff(
                                    state,
                                    ThoughtsStateStore::save,
                                )?;
                                linear::post_handoff_once(state, &message).await?;
                                return ThoughtsStateStore::save(state);
                            };

                            let github = &self.github;
                            let pr = ensure_pr_ready_for_review(
                                state,
                                &pr,
                                "post_sync_refresh",
                                |pr| async move { github.mark_ready_for_review(&pr).await },
                            )
                            .await?;
                            persist_detected_pr(state, &pr);
                        }

                        state.stage.kind = next_stage;
                        state.stage.details = Some(format!(
                            "synced with {} via {SYNC_WITH_MAIN_COMMAND}",
                            state.worktree.base_ref
                        ));
                    }
                    freshness::FreshnessOutcome::Behind { head_sha, base_sha } => {
                        state.freshness.last_result =
                            Some(format!("behind:{head_sha}..{base_sha}"));
                        state.stage.kind = StageKind::StoppedManualHandoff;
                        state.stage.details = Some(format!(
                            "{SYNC_WITH_MAIN_COMMAND} completed but branch is still behind {}; stopping to avoid infinite redispatch. Operator: run the sync command manually and re-run outer-dag from this worktree.",
                            state.worktree.base_ref
                        ));
                        let message =
                            persist_stop_state_before_handoff(state, ThoughtsStateStore::save)?;
                        linear::post_handoff_once(state, &message).await?;
                    }
                    freshness::FreshnessOutcome::DirtyTree => {
                        let message = "dirty worktree blocks freshness gate".to_string();
                        state.freshness.last_result = Some("dirty_tree".to_string());
                        state.stage.kind = StageKind::StoppedDirtyTree;
                        state.stage.details = Some(message);
                        let message =
                            persist_stop_state_before_handoff(state, ThoughtsStateStore::save)?;
                        linear::post_handoff_once(state, &message).await?;
                    }
                }
            }
            freshness::FreshnessOutcome::DirtyTree => {
                let message = "dirty worktree blocks freshness gate".to_string();
                state.freshness.last_result = Some("dirty_tree".to_string());
                state.stage.kind = StageKind::StoppedDirtyTree;
                state.stage.details = Some(message);
                let message = persist_stop_state_before_handoff(state, ThoughtsStateStore::save)?;
                linear::post_handoff_once(state, &message).await?;
            }
        }
        ThoughtsStateStore::save(state)
    }

    async fn run_supervised_command(
        &mut self,
        state: &mut RunState,
        resume_stage: StageKind,
        command_name: &str,
        message: Option<&str>,
    ) -> Result<()> {
        if !state.settings.opencode_dispatch_enabled {
            transition_to_dispatch_disabled(state, &resume_stage, command_name);
            return ThoughtsStateStore::save(state);
        }

        self.supervisor(&state.settings)
            .await?
            .ensure_commands_present(&[
                command_name,
                "linear_ticket_2_pr",
                "resolve_pr_comments",
                RESOLVE_PR_CI_COMMAND,
                SYNC_WITH_MAIN_COMMAND,
            ])
            .await?;
        state.opencode.resume_stage = Some(resume_stage.clone());
        state.opencode.last_command = Some(command_name.to_string());
        let session_selection = session_selection(state.opencode.active_session_id.as_deref());
        let prepared = match self
            .supervisor(&state.settings)
            .await?
            .prepare_command(session_selection, command_name, message)
            .await?
        {
            PreparedCommandOutcome::Prepared(prepared) => prepared,
            PreparedCommandOutcome::Interrupted(outcome) => {
                apply_preflight_interruption(state, resume_stage, command_name, outcome);
                return ThoughtsStateStore::save(state);
            }
        };

        state.opencode.active_session_id = Some(prepared.session_id().to_string());
        state.opencode.current_invocation = Some(state::OpenCodeInvocationState {
            invocation_id: format!("invocation-{}", prepared.command_message_id()),
            command: prepared.command_name().to_string(),
            session_id: prepared.session_id().to_string(),
            command_message_id: prepared.command_message_id().to_string(),
            phase: state::OpenCodeInvocationPhase::Prepared,
            literal_post_attempts: 0,
            lifecycle_result: None,
            task_disposition: None,
            pending_interruption: None,
        });
        ThoughtsStateStore::save(state)?;
        if let Some(invocation) = state.opencode.current_invocation.as_mut() {
            invocation.phase = state::OpenCodeInvocationPhase::PostAttempted;
        }
        ThoughtsStateStore::save(state)?;

        let settings = state.settings.clone();
        let owner = self.owner.clone();
        let owner_context = state
            .opencode
            .current_invocation
            .as_ref()
            .map(|invocation| InvocationOwnerContext {
                run_id: state.run_id.clone(),
                invocation_id: invocation.invocation_id.clone(),
            });
        let event_resume_stage = resume_stage.clone();
        let outcome = self
            .supervisor(&settings)
            .await?
            .run_prepared_command(
                prepared,
                owner.as_deref(),
                owner_context.as_ref(),
                |event| {
                let invocation = state.opencode.current_invocation.as_mut().ok_or_else(|| {
                    anyhow::anyhow!("current invocation disappeared during supervision")
                })?;
                match event {
                    SupervisionEvent::LiteralPostAttempt { total } => {
                        let new_attempts = total.saturating_sub(invocation.literal_post_attempts);
                        state.opencode.dispatch_attempt =
                            state.opencode.dispatch_attempt.saturating_add(new_attempts);
                        invocation.literal_post_attempts = total;
                    }
                    SupervisionEvent::AssistantStarted { .. } => {
                        invocation.phase = state::OpenCodeInvocationPhase::RunningAssistantStarted;
                    }
                    SupervisionEvent::Paused { kind, request_id } => {
                        state.opencode.pending_permission = None;
                        state.opencode.pending_question = None;
                        invocation.phase = match kind {
                            state::InterruptionKind::Permission => {
                                state.stage.kind = StageKind::StoppedPermissionRequired;
                                state.stage.details = Some(
                                    "foreground OpenCode invocation is awaiting an authenticated permission response"
                                        .to_string(),
                                );
                                state::OpenCodeInvocationPhase::PausedPermission
                            }
                            state::InterruptionKind::Question => {
                                state.stage.kind = StageKind::StoppedQuestionRequired;
                                state.stage.details = Some(
                                    "foreground OpenCode invocation is awaiting an authenticated question response"
                                        .to_string(),
                                );
                                state::OpenCodeInvocationPhase::PausedQuestion
                            }
                        };
                        invocation.pending_interruption =
                            Some(state::PendingInterruptionIdentity { kind, request_id });
                    }
                    SupervisionEvent::Resumed { assistant_started } => {
                        state.stage.kind = event_resume_stage.clone();
                        state.stage.details = None;
                        state.opencode.pending_permission = None;
                        state.opencode.pending_question = None;
                        invocation.phase = if assistant_started {
                            state::OpenCodeInvocationPhase::RunningAssistantStarted
                        } else {
                            state::OpenCodeInvocationPhase::PostAttempted
                        };
                        invocation.pending_interruption = None;
                    }
                }
                ThoughtsStateStore::save(state)
            },
            )
            .await?;
        apply_supervised_outcome(state, resume_stage, command_name, outcome);
        ThoughtsStateStore::save(state)
    }

    async fn supervisor(&mut self, settings: &state::Settings) -> Result<&OpenCodeSupervisor> {
        if self.supervisor.is_none() {
            self.supervisor = Some(
                OpenCodeSupervisor::start(
                    std::path::Path::new("."),
                    crate::opencode::supervisor::OpenCodeSupervisorTimeouts::from_settings(
                        settings,
                    ),
                )
                .await?,
            );
        }

        self.supervisor
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("supervisor should be initialized before use"))
    }
}

fn apply_preflight_interruption(
    state: &mut RunState,
    resume_stage: StageKind,
    command_name: &str,
    outcome: SupervisedOutcome,
) {
    state.opencode.current_invocation = None;
    apply_supervised_outcome(state, resume_stage, command_name, outcome);
}

fn apply_supervised_outcome(
    state: &mut RunState,
    resume_stage: StageKind,
    command_name: &str,
    outcome: SupervisedOutcome,
) {
    match outcome {
        SupervisedOutcome::Completed {
            session_id,
            diagnostics,
            literal_post_attempts,
            task_disposition,
        } => {
            state.opencode.active_session_id = Some(session_id);
            state.opencode.last_diagnostics = Some(diagnostics);
            terminalize_persisted_invocation(
                state,
                literal_post_attempts,
                state::InvocationLifecycleResult::Completed,
                task_disposition,
            );
            state.opencode.pending_permission = None;
            state.opencode.pending_question = None;
            state.stage.kind = resume_stage;
            state.stage.details = None;
        }
        SupervisedOutcome::AcceptedButNotStarted {
            session_id: _,
            diagnostics,
            literal_post_attempts,
            task_disposition,
        } => {
            let invocation_id = state
                .opencode
                .current_invocation
                .as_ref()
                .map(|invocation| invocation.invocation_id.clone())
                .unwrap_or_default();
            state.opencode.active_session_id = None;
            state.opencode.last_diagnostics = Some(diagnostics);
            terminalize_persisted_invocation(
                state,
                literal_post_attempts,
                state::InvocationLifecycleResult::AcceptedButNotStarted,
                task_disposition,
            );
            state.opencode.last_lifecycle_anomaly = Some(state::InvocationLifecycleAnomaly {
                invocation_id,
                observed_at: chrono::Utc::now().to_rfc3339(),
                kind: state::InvocationLifecycleAnomalyKind::AcceptedButNotStarted,
            });
            if command_name == "resolve_pr_comments" {
                state.stage.kind = resume_stage;
                state.stage.details = Some(
                        "OpenCode accepted resolve_pr_comments but no current-command assistant execution started"
                            .to_string(),
                    );
            } else {
                transition_to_stopped_failed(
                    state,
                    format!(
                        "OpenCode accepted command '{command_name}' but no current-command assistant execution started"
                    ),
                );
            }
        }
        SupervisedOutcome::PermissionRequired {
            session_id,
            request_id,
            permission_type,
            literal_post_attempts,
        } => {
            state.opencode.active_session_id = Some(session_id);
            if let Some(invocation) = state.opencode.current_invocation.as_mut() {
                invocation.phase = state::OpenCodeInvocationPhase::PausedPermission;
                invocation.literal_post_attempts = literal_post_attempts;
                invocation.pending_interruption = Some(state::PendingInterruptionIdentity {
                    kind: state::InterruptionKind::Permission,
                    request_id: request_id.clone(),
                });
            }
            state.opencode.pending_permission = Some(state::PendingPermission {
                request_id,
                permission_type,
            });
            state.stage.kind = StageKind::StoppedPermissionRequired;
            state.stage.details = Some("OpenCode permission response required".to_string());
        }
        SupervisedOutcome::QuestionRequired {
            session_id,
            request_id,
            prompt,
            literal_post_attempts,
        } => {
            state.opencode.active_session_id = Some(session_id);
            if let Some(invocation) = state.opencode.current_invocation.as_mut() {
                invocation.phase = state::OpenCodeInvocationPhase::PausedQuestion;
                invocation.literal_post_attempts = literal_post_attempts;
                invocation.pending_interruption = Some(state::PendingInterruptionIdentity {
                    kind: state::InterruptionKind::Question,
                    request_id: request_id.clone(),
                });
            }
            state.opencode.pending_question = Some(state::PendingQuestion { request_id, prompt });
            state.stage.kind = StageKind::StoppedQuestionRequired;
            state.stage.details = Some("OpenCode question response required".to_string());
        }
        SupervisedOutcome::Failed {
            session_id,
            error,
            diagnostics,
            literal_post_attempts,
            failure,
            task_disposition,
        } => {
            let cleanup_proven = task_disposition_is_certain(&task_disposition);
            let invocation_id = state
                .opencode
                .current_invocation
                .as_ref()
                .map(|invocation| invocation.invocation_id.clone())
                .unwrap_or_default();
            state.opencode.active_session_id = session_id;
            state.opencode.last_diagnostics = diagnostics;
            terminalize_persisted_invocation(
                state,
                literal_post_attempts,
                state::InvocationLifecycleResult::Failed {
                    failure: failure.clone(),
                },
                task_disposition,
            );
            state.opencode.last_lifecycle_anomaly = Some(state::InvocationLifecycleAnomaly {
                invocation_id,
                observed_at: chrono::Utc::now().to_rfc3339(),
                kind: state::InvocationLifecycleAnomalyKind::Failed { failure },
            });
            if command_name == "resolve_pr_comments" && cleanup_proven {
                state.stage.kind = resume_stage;
                state.stage.details = Some(format!(
                    "resolve_pr_comments failed after explicit terminal cleanup; checking CodeRabbit postconditions: {error}"
                ));
            } else {
                transition_to_stopped_failed(state, error);
            }
        }
    }
}

fn terminalize_persisted_invocation(
    state: &mut RunState,
    literal_post_attempts: u32,
    lifecycle_result: state::InvocationLifecycleResult,
    task_disposition: state::TaskDisposition,
) {
    let persisted_attempts = state
        .opencode
        .current_invocation
        .as_ref()
        .map_or(0, |invocation| invocation.literal_post_attempts);
    state.opencode.dispatch_attempt = state
        .opencode
        .dispatch_attempt
        .saturating_add(literal_post_attempts.saturating_sub(persisted_attempts));
    if let Some(invocation) = state.opencode.current_invocation.as_mut() {
        invocation.phase = state::OpenCodeInvocationPhase::Terminal;
        invocation.literal_post_attempts = literal_post_attempts;
        invocation.lifecycle_result = Some(lifecycle_result);
        invocation.task_disposition = Some(task_disposition);
        invocation.pending_interruption = None;
    }
}

fn task_disposition_is_certain(disposition: &state::TaskDisposition) -> bool {
    !matches!(
        disposition.server_abort,
        state::ServerAbortDisposition::Failed { .. }
    ) && !matches!(
        disposition.local_task,
        state::LocalTaskDisposition::NotStarted | state::LocalTaskDisposition::Spawned
    )
}

async fn detect_pr_with_retry<Lookup, LookupFut, OnRetry, Sleep, SleepFut>(
    mut lookup: Lookup,
    mut on_retry: OnRetry,
    mut sleep: Sleep,
) -> Result<DetectedPrLookup>
where
    Lookup: FnMut() -> LookupFut,
    LookupFut: Future<Output = Result<DetectedPrLookup>>,
    OnRetry: FnMut(usize, Duration, &DetectedPrLookup) -> Result<()>,
    Sleep: FnMut(Duration) -> SleepFut,
    SleepFut: Future<Output = ()>,
{
    for (attempt_index, backoff) in DETECTING_PR_BACKOFFS.into_iter().enumerate() {
        let lookup_result = lookup().await?;
        if lookup_result.pr.is_some() {
            return Ok(lookup_result);
        }

        on_retry(
            detecting_pr_retry_attempt_number(attempt_index),
            backoff,
            &lookup_result,
        )?;
        sleep(backoff).await;
    }

    let lookup_result = lookup().await?;
    Ok(lookup_result)
}

#[cfg(test)]
mod tests {
    use super::CI_FAILURE_GRACE_POLLS;
    use super::CheckSuite422Recovery;
    use super::CiFailureFlow;
    use super::DagEngine;
    use super::DescribePrRefreshDecision;
    use super::DraftSkipRecovery;
    use super::MAX_RESOLVE_START_RETRIES;
    use super::ResolveAttemptAction;
    use super::ResolveInvocationClass;
    use super::ResolvePreDispatchAction;
    use super::UnresolvedReviewThreadsSnapshot;
    use super::apply_preflight_interruption;
    use super::baseline_last_described_head_sha_after_pr_create;
    use super::coderabbit_waiting_details;
    use super::decide_resolve_attempt;
    use super::decide_resolve_pre_dispatch;
    use super::detecting_pr_retry_attempt_number;
    use super::ensure_pr_ready_for_review;
    use super::finish_dispatch_describe_pr_stage_after_rerun;
    use super::handle_ci_gate_head_sha_change;
    use super::is_check_suites_no_commit_found_422;
    use super::persist_stop_state_before_handoff;
    use super::planned_actions_for_start;
    use super::poll_interval_sleep_duration;
    use super::prepare_dispatch_describe_pr_stage;
    use super::record_pr_lookup;
    use super::recover_from_check_suite_no_commit_found_422;
    use super::recover_from_draft_review_skip;
    use super::route_after_ci_remediation;
    use super::schedule_resolve_start_retry;
    use super::session_selection;
    use super::should_reset_coderabbit_timeout_baseline;
    use super::stage_kind_label;
    use super::terminalize_persisted_invocation;
    use super::transition_to_dispatch_disabled;
    use super::transition_to_stopped_failed;
    use super::transition_to_ticket_to_pr_no_pr_handoff;
    use crate::github::ci::GhCheck;
    use crate::github::pr::DetectedPrLookup;
    use crate::opencode::supervisor::SessionSelection;
    use crate::opencode::supervisor::SupervisedOutcome;
    use crate::state::RunState;
    use crate::state::StageKind;
    use crate::test_support::process_state_lock;
    use crate::worktree::TargetWorktree;
    use pr_comments::github::GitHubRestError;
    use pr_comments::models::PrRef;
    use std::sync::Mutex;
    use std::time::Duration;

    fn sample_pr(is_draft: bool) -> PrRef {
        sample_pr_with_head_sha("abc123", is_draft)
    }

    fn sample_pr_with_head_sha(head_sha: &str, is_draft: bool) -> PrRef {
        PrRef {
            number: 258,
            url: "https://example.invalid/pr/258".to_string(),
            head_sha: head_sha.to_string(),
            node_id: "PR_258".to_string(),
            is_draft,
        }
    }

    fn sample_state() -> RunState {
        RunState::for_start(
            "ENG-992",
            &TargetWorktree {
                path: std::env::current_dir().expect("cwd available for test"),
                branch: "feature/eng-992".to_string(),
                base_ref: "origin/main".to_string(),
            },
            false,
        )
        .expect("sample state builds")
    }

    fn set_previous_terminal_invocation(state: &mut RunState) {
        state.opencode.current_invocation = Some(crate::state::OpenCodeInvocationState {
            invocation_id: "previous-invocation".to_string(),
            command: "previous_command".to_string(),
            session_id: "previous-session".to_string(),
            command_message_id: "previous-message".to_string(),
            phase: crate::state::OpenCodeInvocationPhase::Terminal,
            literal_post_attempts: 1,
            lifecycle_result: Some(crate::state::InvocationLifecycleResult::Completed),
            task_disposition: Some(crate::state::TaskDisposition {
                server_abort: crate::state::ServerAbortDisposition::NotRequired,
                local_task: crate::state::LocalTaskDisposition::Completed,
            }),
            pending_interruption: None,
        });
    }

    #[test]
    fn permission_preflight_does_not_mutate_previous_invocation() {
        let mut state = sample_state();
        set_previous_terminal_invocation(&mut state);

        apply_preflight_interruption(
            &mut state,
            StageKind::DispatchingTicketToPr,
            "linear_ticket_2_pr",
            SupervisedOutcome::PermissionRequired {
                session_id: "next-session".to_string(),
                request_id: "permission-1".to_string(),
                permission_type: "file.write".to_string(),
                literal_post_attempts: 0,
            },
        );

        assert!(state.opencode.current_invocation.is_none());
        assert_eq!(
            state
                .opencode
                .pending_permission
                .as_ref()
                .map(|pending| pending.request_id.as_str()),
            Some("permission-1")
        );
        assert_eq!(state.stage.kind, StageKind::StoppedPermissionRequired);
    }

    #[test]
    fn question_preflight_does_not_mutate_previous_invocation() {
        let mut state = sample_state();
        set_previous_terminal_invocation(&mut state);

        apply_preflight_interruption(
            &mut state,
            StageKind::DispatchingResolvePrComments,
            "resolve_pr_comments",
            SupervisedOutcome::QuestionRequired {
                session_id: "next-session".to_string(),
                request_id: "question-1".to_string(),
                prompt: "Continue?".to_string(),
                literal_post_attempts: 0,
            },
        );

        assert!(state.opencode.current_invocation.is_none());
        assert_eq!(
            state
                .opencode
                .pending_question
                .as_ref()
                .map(|pending| pending.request_id.as_str()),
            Some("question-1")
        );
        assert_eq!(state.stage.kind, StageKind::StoppedQuestionRequired);
    }

    fn check_suites_422_error(head_sha: &str) -> anyhow::Error {
        GitHubRestError {
            status: 422,
            url: format!(
                "https://api.github.com/repos/owner/repo/commits/{head_sha}/check-suites?page=1&per_page=100"
            ),
            body: format!("{{\"message\":\"No commit found for SHA: {head_sha}\"}}"),
        }
        .into()
    }

    fn sample_unresolved_snapshot(
        total_unresolved_threads: usize,
        bot_unresolved_threads: usize,
    ) -> UnresolvedReviewThreadsSnapshot {
        UnresolvedReviewThreadsSnapshot {
            total_unresolved: total_unresolved_threads,
            bot_unresolved: bot_unresolved_threads,
            representative_bot_refs: Vec::new(),
        }
    }

    fn sample_ci_check(name: &str, state: &str) -> GhCheck {
        GhCheck {
            name: name.to_string(),
            state: state.to_string(),
            conclusion: None,
            details_url: Some(format!("https://example.invalid/{name}")),
        }
    }

    #[test]
    fn poll_interval_sleep_duration_clamps_to_one_second_minimum() {
        assert_eq!(poll_interval_sleep_duration(0), Duration::from_secs(1));
        assert_eq!(poll_interval_sleep_duration(1), Duration::from_secs(1));
        assert_eq!(poll_interval_sleep_duration(5), Duration::from_secs(5));
    }

    #[test]
    fn resolve_attempt_routes_external_zero_before_lifecycle_anomaly() {
        let before = sample_unresolved_snapshot(5, 3);
        let after = sample_unresolved_snapshot(2, 0);

        assert_eq!(
            decide_resolve_attempt(
                &before,
                &after,
                ResolveInvocationClass::AcceptedButNotStarted,
                MAX_RESOLVE_START_RETRIES,
            ),
            ResolveAttemptAction::AdvanceExternalComplete
        );
    }

    #[test]
    fn resolve_attempt_routes_external_decrease_before_failed_lifecycle() {
        let before = sample_unresolved_snapshot(5, 4);
        let after = sample_unresolved_snapshot(4, 3);

        assert_eq!(
            decide_resolve_attempt(
                &before,
                &after,
                ResolveInvocationClass::FailedOrCancelled,
                1,
            ),
            ResolveAttemptAction::LoopExternalProgress
        );
    }

    #[test]
    fn resolve_attempt_allows_exactly_two_start_retries_then_exhausts() {
        let before = sample_unresolved_snapshot(4, 3);
        let after = sample_unresolved_snapshot(4, 3);

        assert_eq!(
            decide_resolve_attempt(
                &before,
                &after,
                ResolveInvocationClass::AcceptedButNotStarted,
                0,
            ),
            ResolveAttemptAction::RetryStart
        );
        assert_eq!(
            decide_resolve_attempt(
                &before,
                &after,
                ResolveInvocationClass::AcceptedButNotStarted,
                1,
            ),
            ResolveAttemptAction::RetryStart
        );
        assert_eq!(
            decide_resolve_attempt(
                &before,
                &after,
                ResolveInvocationClass::AcceptedButNotStarted,
                2,
            ),
            ResolveAttemptAction::ExhaustedStartRetries
        );
    }

    #[test]
    fn resolve_attempt_executed_without_external_progress_hands_off() {
        let before = sample_unresolved_snapshot(4, 3);
        let after = sample_unresolved_snapshot(4, 3);

        assert_eq!(
            decide_resolve_attempt(&before, &after, ResolveInvocationClass::Completed, 0),
            ResolveAttemptAction::ExecutedNoProgress
        );
    }

    #[test]
    fn resolve_attempt_uncertain_cleanup_never_retries() {
        let before = sample_unresolved_snapshot(4, 3);
        let after = sample_unresolved_snapshot(4, 3);

        assert_eq!(
            decide_resolve_attempt(&before, &after, ResolveInvocationClass::CleanupUncertain, 0,),
            ResolveAttemptAction::ManualHandoff
        );
    }

    #[test]
    fn terminal_persistence_reconciles_unobserved_literal_post_attempts_once() {
        let mut state = sample_state();
        state.opencode.dispatch_attempt = 7;
        state.opencode.current_invocation = Some(crate::state::OpenCodeInvocationState {
            invocation_id: "inv-1".to_string(),
            command: "resolve_pr_comments".to_string(),
            session_id: "session-1".to_string(),
            command_message_id: "msg-1".to_string(),
            phase: crate::state::OpenCodeInvocationPhase::PostAttempted,
            literal_post_attempts: 1,
            lifecycle_result: None,
            task_disposition: None,
            pending_interruption: None,
        });
        let disposition = crate::state::TaskDisposition {
            server_abort: crate::state::ServerAbortDisposition::NotRequired,
            local_task: crate::state::LocalTaskDisposition::Completed,
        };

        terminalize_persisted_invocation(
            &mut state,
            2,
            crate::state::InvocationLifecycleResult::Completed,
            disposition.clone(),
        );
        assert_eq!(state.opencode.dispatch_attempt, 8);
        assert_eq!(
            state
                .opencode
                .current_invocation
                .as_ref()
                .map(|invocation| invocation.literal_post_attempts),
            Some(2)
        );

        terminalize_persisted_invocation(
            &mut state,
            2,
            crate::state::InvocationLifecycleResult::Completed,
            disposition,
        );
        assert_eq!(state.opencode.dispatch_attempt, 8);
    }

    #[test]
    fn resolve_start_retry_forces_fresh_session_without_charging_cycle_counters() {
        let mut state = sample_state();
        state.opencode.active_session_id = Some("old-session".to_string());
        state.opencode.resolve_start_retries = 1;
        state.opencode.dispatch_attempt = 9;
        state.counters.resolve_comments = 3;
        state.opencode.last_lifecycle_anomaly = Some(crate::state::InvocationLifecycleAnomaly {
            invocation_id: "inv-1".to_string(),
            observed_at: "2026-01-01T00:00:00Z".to_string(),
            kind: crate::state::InvocationLifecycleAnomalyKind::AcceptedButNotStarted,
        });

        schedule_resolve_start_retry(&mut state);

        assert_eq!(state.opencode.resolve_start_retries, 2);
        assert_eq!(state.opencode.active_session_id, None);
        assert_eq!(state.opencode.dispatch_attempt, 9);
        assert_eq!(state.counters.resolve_comments, 3);
        assert!(state.opencode.last_lifecycle_anomaly.is_some());
        assert_eq!(
            session_selection(state.opencode.active_session_id.as_deref()),
            SessionSelection::Fresh
        );
    }

    #[test]
    fn normal_session_selection_reuses_only_explicit_active_session() {
        assert_eq!(session_selection(None), SessionSelection::Fresh);
        assert_eq!(
            session_selection(Some("session-1")),
            SessionSelection::Reuse("session-1".to_string())
        );
    }

    #[test]
    fn unresolved_review_threads_snapshot_reports_human_thread_count() {
        let snapshot = sample_unresolved_snapshot(7, 5);

        assert_eq!(snapshot.human_unresolved_threads(), 2);
    }

    #[test]
    fn resolve_loop_stops_ready_for_human_review_when_no_bot_threads_precheck() {
        let before = sample_unresolved_snapshot(3, 0);

        let action = decide_resolve_pre_dispatch(&before, 2, 5);

        assert!(matches!(
            action,
            ResolvePreDispatchAction::ReadyForHumanReview { .. }
        ));
    }

    #[test]
    fn resolve_loop_enforces_max_review_cycles_exhaustion() {
        let before = UnresolvedReviewThreadsSnapshot {
            total_unresolved: 4,
            bot_unresolved: 3,
            representative_bot_refs: Vec::new(),
        };

        let action = decide_resolve_pre_dispatch(&before, 5, 5);

        let ResolvePreDispatchAction::ManualHandoff { details } = action else {
            panic!("expected manual handoff");
        };
        assert!(details.contains("max_review_cycles exhausted"));
    }

    #[test]
    fn should_reset_coderabbit_timeout_baseline_only_on_false_to_true_transition() {
        assert!(should_reset_coderabbit_timeout_baseline(false, true));
        assert!(!should_reset_coderabbit_timeout_baseline(false, false));
        assert!(!should_reset_coderabbit_timeout_baseline(true, true));
        assert!(!should_reset_coderabbit_timeout_baseline(true, false));
    }

    #[test]
    fn detecting_pr_retry_attempt_number_starts_from_second_attempt() {
        assert_eq!(detecting_pr_retry_attempt_number(0), 2);
        assert_eq!(detecting_pr_retry_attempt_number(1), 3);
        assert_eq!(detecting_pr_retry_attempt_number(3), 5);
    }

    #[test]
    fn coderabbit_waiting_details_includes_required_context() {
        let details = coderabbit_waiting_details(42, "abc123", 3, 120, 600, 30);

        assert!(details.contains("cycle=3"));
        assert!(details.contains("pr=#42"));
        assert!(details.contains("head=abc123"));
        assert!(details.contains("elapsed=120s"));
        assert!(details.contains("timeout_in=480s"));
        assert!(details.contains("next_poll_in=30s"));
    }

    #[test]
    fn planned_actions_for_start_returns_expected_ordered_ids() {
        let ids: Vec<_> = planned_actions_for_start()
            .into_iter()
            .map(|action| action.id)
            .collect();

        assert_eq!(
            ids,
            vec![
                "worktree.resolve",
                "state.check_existing",
                "state.write_initial",
                "freshness.before_ticket_to_pr",
                "github.pr.detect_existing",
                "opencode.run.linear_ticket_2_pr",
                "github.pr.detect_after_ticket_to_pr",
                "freshness.before_coderabbit_wait",
                "github.coderabbit.wait",
                "opencode.run.resolve_pr_comments",
                "opencode.run.describe_pr_refresh",
                "github.ci.wait",
                "opencode.run.resolve_pr_ci_failures",
                "stop.ready_for_human_review",
            ]
        );
    }

    #[test]
    fn baseline_last_described_head_sha_sets_once_after_pr_create() {
        let mut state = sample_state();
        state.counters.ticket_to_pr = 1;

        baseline_last_described_head_sha_after_pr_create(&mut state, "abc123");

        assert_eq!(state.pr.last_described_head_sha.as_deref(), Some("abc123"));
    }

    #[test]
    fn baseline_last_described_head_sha_does_not_overwrite_existing_value() {
        let mut state = sample_state();
        state.counters.ticket_to_pr = 1;
        state.pr.last_described_head_sha = Some("already-set".to_string());

        baseline_last_described_head_sha_after_pr_create(&mut state, "abc123");

        assert_eq!(
            state.pr.last_described_head_sha.as_deref(),
            Some("already-set")
        );
    }

    #[test]
    fn prepare_dispatch_describe_pr_stage_skips_when_head_sha_matches_baseline() {
        let mut state = sample_state();
        state.pr.last_described_head_sha = Some("abc123".to_string());

        let decision = prepare_dispatch_describe_pr_stage(
            &mut state,
            DetectedPrLookup {
                requested_branch: "feature/eng-992".to_string(),
                current_branch: Some("feature/eng-992".to_string()),
                repo_owner: "allisoneer".to_string(),
                repo_name: "agentic_auxilary".to_string(),
                token_source: Some("GH_TOKEN".to_string()),
                empty_result_reason: None,
                pr: Some(sample_pr(false)),
            },
        );

        assert!(matches!(decision, DescribePrRefreshDecision::Stop));
        assert_eq!(state.stage.kind, StageKind::WaitingForCi);
        assert_eq!(state.pr.last_described_head_sha.as_deref(), Some("abc123"));
    }

    #[test]
    fn prepare_dispatch_describe_pr_stage_requests_rerun_when_head_sha_differs() {
        let mut state = sample_state();
        state.pr.last_described_head_sha = Some("old-sha".to_string());

        let decision = prepare_dispatch_describe_pr_stage(
            &mut state,
            DetectedPrLookup {
                requested_branch: "feature/eng-992".to_string(),
                current_branch: Some("feature/eng-992".to_string()),
                repo_owner: "allisoneer".to_string(),
                repo_name: "agentic_auxilary".to_string(),
                token_source: Some("GH_TOKEN".to_string()),
                empty_result_reason: None,
                pr: Some(sample_pr_with_head_sha("new-sha", false)),
            },
        );

        assert!(matches!(
            decision,
            DescribePrRefreshDecision::Rerun { ref head_sha } if head_sha == "new-sha"
        ));
        assert_eq!(state.pr.last_described_head_sha.as_deref(), Some("old-sha"));
        assert_eq!(state.pr.head_sha.as_deref(), Some("new-sha"));
    }

    #[test]
    fn prepare_dispatch_describe_pr_stage_reruns_when_baseline_is_unknown() {
        let mut state = sample_state();

        let decision = prepare_dispatch_describe_pr_stage(
            &mut state,
            DetectedPrLookup {
                requested_branch: "feature/eng-992".to_string(),
                current_branch: Some("feature/eng-992".to_string()),
                repo_owner: "allisoneer".to_string(),
                repo_name: "agentic_auxilary".to_string(),
                token_source: Some("GH_TOKEN".to_string()),
                empty_result_reason: None,
                pr: Some(sample_pr_with_head_sha("new-sha", false)),
            },
        );

        assert!(matches!(
            decision,
            DescribePrRefreshDecision::Rerun { ref head_sha } if head_sha == "new-sha"
        ));
        assert_eq!(state.pr.last_described_head_sha, None);
    }

    #[test]
    fn finish_dispatch_describe_pr_stage_after_rerun_preserves_stopped_failed() {
        let mut state = sample_state();
        state.pr.last_described_head_sha = Some("old-sha".to_string());
        transition_to_stopped_failed(&mut state, "describe_pr failed");

        let decision =
            finish_dispatch_describe_pr_stage_after_rerun(&mut state, "new-sha".to_string());

        assert!(matches!(decision, DescribePrRefreshDecision::Stop));
        assert_eq!(state.stage.kind, StageKind::StoppedFailed);
        assert_eq!(state.pr.last_described_head_sha.as_deref(), Some("old-sha"));
    }

    #[test]
    fn finish_dispatch_describe_pr_stage_after_rerun_updates_baseline_and_enters_ci_gate() {
        let mut state = sample_state();
        state.stage.kind = StageKind::DispatchingDescribePr;

        let decision =
            finish_dispatch_describe_pr_stage_after_rerun(&mut state, "new-sha".to_string());

        assert!(matches!(decision, DescribePrRefreshDecision::Stop));
        assert_eq!(state.stage.kind, StageKind::WaitingForCi);
        assert_eq!(state.pr.last_described_head_sha.as_deref(), Some("new-sha"));
    }

    #[test]
    fn ci_failure_dispatches_remediation_with_tracking() {
        let mut state = sample_state();

        let flow = super::apply_required_ci_failure(
            &mut state,
            "abc123",
            "fp1",
            &[sample_ci_check("unit", "failure")],
        );

        assert_eq!(flow, CiFailureFlow::DispatchResolve);
        assert_eq!(state.stage.kind, StageKind::DispatchingResolvePrCiFailures);
        assert_eq!(state.ci.last_remediated_head_sha.as_deref(), Some("abc123"));
        assert_eq!(state.ci.last_remediated_fingerprint.as_deref(), Some("fp1"));
        assert_eq!(state.ci.grace_polls_remaining, CI_FAILURE_GRACE_POLLS);
    }

    #[test]
    fn ci_failure_stops_when_ceiling_reached() {
        let mut state = sample_state();
        state.settings.max_review_cycles = 2;
        state.counters.resolve_ci = 2;

        let flow = super::apply_required_ci_failure(
            &mut state,
            "abc123",
            "fp1",
            &[sample_ci_check("unit", "failure")],
        );

        assert_eq!(flow, CiFailureFlow::StopManualHandoff);
        assert_eq!(state.stage.kind, StageKind::StoppedManualHandoff);
        assert!(
            state
                .stage
                .details
                .as_deref()
                .unwrap()
                .contains("max_review_cycles exhausted")
        );
    }

    #[test]
    fn ci_failure_stops_after_grace_exhaustion() {
        let mut state = sample_state();
        state.ci.last_remediated_head_sha = Some("abc123".to_string());
        state.ci.last_remediated_fingerprint = Some("fp1".to_string());
        state.ci.grace_polls_remaining = 1;

        let flow = super::apply_required_ci_failure(
            &mut state,
            "abc123",
            "fp1",
            &[sample_ci_check("unit", "failure")],
        );

        assert_eq!(flow, CiFailureFlow::StopManualHandoff);
        assert_eq!(state.stage.kind, StageKind::StoppedManualHandoff);
        assert!(
            state
                .stage
                .details
                .as_deref()
                .unwrap()
                .contains("grace exhausted")
        );
    }

    #[test]
    fn ci_head_change_restarts_coderabbit_wait_and_resets_tracking() {
        let mut state = sample_state();
        state.stage.kind = StageKind::WaitingForCi;
        state.ci.last_remediated_head_sha = Some("old".to_string());
        state.ci.last_remediated_fingerprint = Some("fp1".to_string());
        state.ci.grace_polls_remaining = 2;

        let changed = handle_ci_gate_head_sha_change(&mut state, Some("old"), "new");

        assert!(changed);
        assert_eq!(state.stage.kind, StageKind::FreshnessBeforeCoderabbitWait);
        assert_eq!(state.ci.last_remediated_head_sha, None);
        assert_eq!(state.ci.last_remediated_fingerprint, None);
        assert_eq!(state.ci.grace_polls_remaining, 0);
    }

    #[test]
    fn ci_same_head_after_remediation_returns_to_ci_wait() {
        let mut state = sample_state();
        route_after_ci_remediation(&mut state, "abc123", "abc123");
        assert_eq!(state.stage.kind, StageKind::WaitingForCi);
    }

    #[test]
    fn ci_new_head_after_remediation_restarts_coderabbit_wait() {
        let mut state = sample_state();
        state.ci.last_remediated_head_sha = Some("abc123".to_string());
        state.ci.last_remediated_fingerprint = Some("fp1".to_string());
        state.ci.grace_polls_remaining = 2;

        route_after_ci_remediation(&mut state, "abc123", "def456");

        assert_eq!(state.stage.kind, StageKind::FreshnessBeforeCoderabbitWait);
        assert_eq!(state.ci.last_remediated_head_sha, None);
        assert_eq!(state.ci.last_remediated_fingerprint, None);
        assert_eq!(state.ci.grace_polls_remaining, 0);
    }

    #[test]
    fn prepare_dispatch_describe_pr_stage_stops_failed_when_pr_cannot_be_redetected() {
        let mut state = sample_state();

        let decision = prepare_dispatch_describe_pr_stage(
            &mut state,
            DetectedPrLookup {
                requested_branch: "feature/eng-992".to_string(),
                current_branch: Some("feature/eng-992".to_string()),
                repo_owner: "allisoneer".to_string(),
                repo_name: "agentic_auxilary".to_string(),
                token_source: Some("GH_TOKEN".to_string()),
                empty_result_reason: Some("no_open_pull_requests_matched_branch".to_string()),
                pr: None,
            },
        );

        assert!(matches!(decision, DescribePrRefreshDecision::Stop));
        assert_eq!(state.stage.kind, StageKind::StoppedFailed);
        assert!(
            state
                .stage
                .details
                .as_deref()
                .expect("failure detail should exist")
                .contains("could not re-detect an open PR")
        );
    }

    #[test]
    fn transition_to_stopped_failed_sets_last_error_and_stage_details() {
        let mut state = sample_state();

        transition_to_stopped_failed(
            &mut state,
            "no open PR found for branch after ticket_to_pr run",
        );

        assert_eq!(state.stage.kind, StageKind::StoppedFailed);
        assert_eq!(
            state.stage.details.as_deref(),
            Some("no open PR found for branch after ticket_to_pr run")
        );
        assert_eq!(
            state.last_error.as_deref(),
            Some("no open PR found for branch after ticket_to_pr run")
        );
    }

    #[test]
    fn record_pr_lookup_persists_safe_context_for_status_debugging() {
        let mut state = sample_state();

        record_pr_lookup(
            &mut state,
            StageKind::DispatchingTicketToPr,
            &DetectedPrLookup {
                requested_branch: "feature/eng-992".to_string(),
                current_branch: Some("feature/eng-992".to_string()),
                repo_owner: "allisoneer".to_string(),
                repo_name: "agentic_auxilary".to_string(),
                token_source: Some("GH_TOKEN".to_string()),
                empty_result_reason: Some("no_open_pull_requests_matched_branch".to_string()),
                pr: None,
            },
        );

        let lookup = state
            .pr
            .last_lookup
            .as_ref()
            .expect("lookup diagnostics should be stored");
        assert_eq!(lookup.stage, StageKind::DispatchingTicketToPr);
        assert_eq!(lookup.outcome, "not_found");
        assert_eq!(lookup.repo_owner, "allisoneer");
        assert_eq!(lookup.repo_name, "agentic_auxilary");
        assert_eq!(lookup.token_source.as_deref(), Some("GH_TOKEN"));
        assert_eq!(
            lookup.empty_result_reason.as_deref(),
            Some("no_open_pull_requests_matched_branch")
        );
        assert_eq!(lookup.pr_number, None);
        assert_eq!(lookup.pr_is_draft, None);
    }

    #[test]
    fn record_pr_lookup_persists_detected_draft_state() {
        let mut state = sample_state();

        record_pr_lookup(
            &mut state,
            StageKind::DetectingPr,
            &DetectedPrLookup {
                requested_branch: "feature/eng-992".to_string(),
                current_branch: Some("feature/eng-992".to_string()),
                repo_owner: "allisoneer".to_string(),
                repo_name: "agentic_auxilary".to_string(),
                token_source: Some("GH_TOKEN".to_string()),
                empty_result_reason: None,
                pr: Some(sample_pr(true)),
            },
        );

        let lookup = state.pr.last_lookup.as_ref().expect("lookup stored");
        assert_eq!(lookup.pr_number, Some(258));
        assert_eq!(lookup.pr_is_draft, Some(true));
    }

    #[test]
    fn transition_to_dispatch_disabled_sets_clear_failure_without_dispatch() {
        let mut state = sample_state();
        state.settings.opencode_dispatch_enabled = false;
        record_pr_lookup(
            &mut state,
            StageKind::DispatchingTicketToPr,
            &DetectedPrLookup {
                requested_branch: "feature/eng-992".to_string(),
                current_branch: Some("feature/eng-992".to_string()),
                repo_owner: "allisoneer".to_string(),
                repo_name: "agentic_auxilary".to_string(),
                token_source: Some("gh-config".to_string()),
                empty_result_reason: Some("graphql_response_missing_data".to_string()),
                pr: None,
            },
        );

        transition_to_dispatch_disabled(
            &mut state,
            &StageKind::DispatchingTicketToPr,
            "linear_ticket_2_pr",
        );

        assert_eq!(state.stage.kind, StageKind::StoppedFailed);
        assert!(
            state
                .stage
                .details
                .as_deref()
                .expect("failure message should exist")
                .contains("OpenCode dispatch disabled; refusing to run linear_ticket_2_pr")
        );
        assert!(
            state
                .last_error
                .as_deref()
                .expect("last error should exist")
                .contains("allisoneer/agentic_auxilary")
        );
        assert!(
            state
                .last_error
                .as_deref()
                .expect("last error should exist")
                .contains("token source=gh-config")
        );
    }

    #[test]
    fn transition_to_ticket_to_pr_no_pr_handoff_uses_lookup_context_and_does_not_set_last_error() {
        let mut state = sample_state();
        record_pr_lookup(
            &mut state,
            StageKind::DetectingPr,
            &DetectedPrLookup {
                requested_branch: "feature/eng-992".to_string(),
                current_branch: Some("feature/eng-992".to_string()),
                repo_owner: "allisoneer".to_string(),
                repo_name: "agentic_auxilary".to_string(),
                token_source: Some("GH_TOKEN".to_string()),
                empty_result_reason: Some("no_open_pull_requests_matched_branch".to_string()),
                pr: None,
            },
        );

        transition_to_ticket_to_pr_no_pr_handoff(&mut state, "after ticket_to_pr run");

        assert_eq!(state.stage.kind, StageKind::StoppedTicketToPrNoPrHandoff);
        assert_eq!(state.last_error, None);
        assert!(
            state
                .stage
                .details
                .as_deref()
                .expect("stage details should exist")
                .contains("ticket_to_pr completed but no open PR found for branch 'feature/eng-992' in allisoneer/agentic_auxilary after ticket_to_pr run")
        );
        assert!(
            state
                .stage
                .details
                .as_deref()
                .expect("stage details should exist")
                .contains("diagnostic=no_open_pull_requests_matched_branch")
        );
    }

    #[test]
    fn stage_kind_label_uses_snake_case_status_names() {
        assert_eq!(
            stage_kind_label(&StageKind::DispatchingResolvePrComments),
            "dispatching_resolve_pr_comments"
        );
    }

    #[test]
    fn for_current_dir_does_not_start_opencode_eagerly() {
        let _guard = process_state_lock().lock().unwrap();
        let previous = std::env::var_os("OPENCODE_BINARY");

        // SAFETY: this test serializes OPENCODE_BINARY mutation with a process-wide mutex.
        unsafe { std::env::set_var("OPENCODE_BINARY", "/definitely/not/opencode") };

        let result = DagEngine::for_current_dir();

        match previous {
            Some(value) => {
                // SAFETY: this test serializes OPENCODE_BINARY mutation with a process-wide mutex.
                unsafe { std::env::set_var("OPENCODE_BINARY", value) };
            }
            None => {
                // SAFETY: this test serializes OPENCODE_BINARY mutation with a process-wide mutex.
                unsafe { std::env::remove_var("OPENCODE_BINARY") };
            }
        }

        assert!(result.is_ok(), "engine construction should stay lazy");
    }

    #[tokio::test]
    async fn detect_pr_with_retry_stops_after_pr_appears() {
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen_sleeps = std::sync::Arc::new(Mutex::new(Vec::new()));
        let seen_attempt_numbers = std::sync::Arc::new(Mutex::new(Vec::new()));

        let lookup_attempts = std::sync::Arc::clone(&attempts);
        let sleep_log = std::sync::Arc::clone(&seen_sleeps);
        let attempt_log = std::sync::Arc::clone(&seen_attempt_numbers);
        let result = super::detect_pr_with_retry(
            move || {
                let lookup_attempts = std::sync::Arc::clone(&lookup_attempts);
                async move {
                    let attempt =
                        lookup_attempts.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    Ok(DetectedPrLookup {
                        requested_branch: "feature/eng-992".to_string(),
                        current_branch: Some("feature/eng-992".to_string()),
                        repo_owner: "allisoneer".to_string(),
                        repo_name: "agentic_auxilary".to_string(),
                        token_source: Some("GH_TOKEN".to_string()),
                        empty_result_reason: (attempt < 1)
                            .then_some("no_open_pull_requests_matched_branch".to_string()),
                        pr: (attempt >= 1).then_some(sample_pr(false)),
                    })
                }
            },
            move |attempt_number, _, _| {
                attempt_log.lock().unwrap().push(attempt_number);
                Ok(())
            },
            move |duration| {
                let sleep_log = std::sync::Arc::clone(&sleep_log);
                async move {
                    sleep_log.lock().unwrap().push(duration);
                }
            },
        )
        .await
        .expect("retry should succeed once PR appears");

        assert_eq!(result.pr.as_ref().map(|pr| pr.number), Some(258));
        assert_eq!(attempts.load(std::sync::atomic::Ordering::Relaxed), 2);
        assert_eq!(
            seen_sleeps.lock().unwrap().as_slice(),
            &[Duration::from_secs(1)]
        );
        assert_eq!(seen_attempt_numbers.lock().unwrap().as_slice(), &[2]);
    }

    #[tokio::test]
    async fn detect_pr_with_retry_exhausts_backoff_schedule() {
        let seen_sleeps = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sleep_log = std::sync::Arc::clone(&seen_sleeps);

        let result = super::detect_pr_with_retry(
            || async {
                Ok(DetectedPrLookup {
                    requested_branch: "feature/eng-992".to_string(),
                    current_branch: Some("feature/eng-992".to_string()),
                    repo_owner: "allisoneer".to_string(),
                    repo_name: "agentic_auxilary".to_string(),
                    token_source: Some("GH_TOKEN".to_string()),
                    empty_result_reason: Some("no_open_pull_requests_matched_branch".to_string()),
                    pr: None,
                })
            },
            |_, _, _| Ok(()),
            move |duration| {
                let sleep_log = std::sync::Arc::clone(&sleep_log);
                async move {
                    sleep_log.lock().unwrap().push(duration);
                }
            },
        )
        .await
        .expect("retry helper should return final empty lookup after exhaustion");

        assert!(result.pr.is_none());
        assert_eq!(
            seen_sleeps.lock().unwrap().as_slice(),
            &[
                Duration::from_secs(1),
                Duration::from_secs(2),
                Duration::from_secs(4),
                Duration::from_secs(8)
            ]
        );
    }

    #[tokio::test]
    async fn ensure_pr_ready_for_review_marks_draft_before_coderabbit_wait() {
        let mut state = sample_state();

        let updated = ensure_pr_ready_for_review(
            &mut state,
            &sample_pr(true),
            "existing_pr_guard",
            |_| async { Ok(sample_pr(false)) },
        )
        .await
        .expect("draft PR should be marked ready");

        assert!(!updated.is_draft);
        assert_eq!(state.pr.number, Some(258));
        assert_eq!(state.pr.is_draft, Some(false));
        assert_eq!(
            state.pr.ready_for_review.last_result.as_deref(),
            Some("marked_ready:existing_pr_guard")
        );
        assert_eq!(state.pr.ready_for_review.attempts, 1);
    }

    #[tokio::test]
    async fn recover_from_draft_review_skip_continues_waiting_after_readying_pr() {
        let mut state = sample_state();

        let recovery = recover_from_draft_review_skip(
            &mut state,
            "Review skipped. Draft detected.",
            || async {
                Ok(DetectedPrLookup {
                    requested_branch: "feature/eng-992".to_string(),
                    current_branch: Some("feature/eng-992".to_string()),
                    repo_owner: "allisoneer".to_string(),
                    repo_name: "agentic_auxilary".to_string(),
                    token_source: Some("GH_TOKEN".to_string()),
                    empty_result_reason: None,
                    pr: Some(sample_pr(true)),
                })
            },
            |_| async { Ok(sample_pr(false)) },
        )
        .await
        .expect("recovery should succeed");

        assert!(matches!(recovery, DraftSkipRecovery::ContinueWaiting));
        assert_eq!(state.stage.kind, StageKind::WaitingForCoderabbit);
        assert!(
            state
                .stage
                .details
                .as_deref()
                .expect("recovery detail")
                .contains("marked ready for review")
        );
        assert!(state.pr.ready_for_review.coderabbit_draft_skip_recovered);
        assert_eq!(state.pr.is_draft, Some(false));
    }

    #[tokio::test]
    async fn recover_from_draft_review_skip_treats_repeat_draft_skip_as_stale() {
        let mut state = sample_state();
        state.pr.ready_for_review.coderabbit_draft_skip_recovered = true;

        let recovery = recover_from_draft_review_skip(
            &mut state,
            "Review skipped. Draft detected.",
            || async { panic!("repeat draft skip should not trigger another PR lookup") },
            |_| async { panic!("repeat draft skip should not try to ready the PR again") },
        )
        .await
        .expect("repeat draft skip should continue waiting");

        assert!(matches!(recovery, DraftSkipRecovery::ContinueWaiting));
        assert_eq!(state.stage.kind, StageKind::WaitingForCoderabbit);
        assert!(
            state
                .stage
                .details
                .as_deref()
                .expect("stale detail")
                .contains("treating it as stale")
        );
    }

    #[test]
    fn check_suite_422_classifier_requires_status_url_and_body_match() {
        let matching = GitHubRestError {
            status: 422,
            url: "https://api.github.com/repos/owner/repo/commits/abc123/check-suites?page=1"
                .to_string(),
            body: "No commit found for SHA: abc123".to_string(),
        };
        assert!(is_check_suites_no_commit_found_422(&matching, "abc123"));

        let wrong_status = GitHubRestError {
            status: 404,
            ..matching.clone()
        };
        assert!(!is_check_suites_no_commit_found_422(
            &wrong_status,
            "abc123"
        ));

        let wrong_url = GitHubRestError {
            url: "https://api.github.com/repos/owner/repo/issues/1/comments".to_string(),
            ..matching.clone()
        };
        assert!(!is_check_suites_no_commit_found_422(&wrong_url, "abc123"));

        let wrong_body = GitHubRestError {
            body: "validation failed".to_string(),
            ..matching
        };
        assert!(!is_check_suites_no_commit_found_422(&wrong_body, "abc123"));
    }

    #[tokio::test]
    async fn recover_from_check_suites_422_continues_when_remote_head_changed() {
        let mut state = sample_state();
        state.pr.head_sha = Some("abc123".to_string());

        let recovery = recover_from_check_suite_no_commit_found_422(
            &mut state,
            "abc123",
            &check_suites_422_error("abc123"),
            || async {
                Ok(DetectedPrLookup {
                    requested_branch: "feature/eng-992".to_string(),
                    current_branch: Some("feature/eng-992".to_string()),
                    repo_owner: "allisoneer".to_string(),
                    repo_name: "agentic_auxilary".to_string(),
                    token_source: Some("GH_TOKEN".to_string()),
                    empty_result_reason: None,
                    pr: Some(PrRef {
                        head_sha: "def456".to_string(),
                        ..sample_pr(false)
                    }),
                })
            },
        )
        .await
        .expect("recovery should succeed");

        assert!(matches!(recovery, CheckSuite422Recovery::ContinueWaiting));
        assert_eq!(state.pr.head_sha.as_deref(), Some("def456"));
        assert_eq!(state.pr.last_observed_head_sha.as_deref(), Some("def456"));
        assert_eq!(state.stage.kind, StageKind::WaitingForCoderabbit);
        assert!(
            state
                .stage
                .details
                .as_deref()
                .expect("recovery detail")
                .contains("Updated stored head SHA and continuing to wait")
        );
    }

    #[tokio::test]
    async fn recover_from_check_suites_422_stops_when_remote_head_unchanged() {
        let mut state = sample_state();

        let recovery = recover_from_check_suite_no_commit_found_422(
            &mut state,
            "abc123",
            &check_suites_422_error("abc123"),
            || async {
                Ok(DetectedPrLookup {
                    requested_branch: "feature/eng-992".to_string(),
                    current_branch: Some("feature/eng-992".to_string()),
                    repo_owner: "allisoneer".to_string(),
                    repo_name: "agentic_auxilary".to_string(),
                    token_source: Some("GH_TOKEN".to_string()),
                    empty_result_reason: None,
                    pr: Some(sample_pr(false)),
                })
            },
        )
        .await
        .expect("recovery should succeed");

        match recovery {
            CheckSuite422Recovery::TerminalStop { message } => {
                assert!(message.contains("Re-detected remote PR head is unchanged (abc123)"));
                assert!(message.contains("/commits/abc123/check-suites"));
                assert!(message.contains("No commit found for SHA"));
            }
            other => panic!("expected terminal stop, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn recover_from_check_suites_422_stops_when_pr_not_redetected() {
        let mut state = sample_state();

        let recovery = recover_from_check_suite_no_commit_found_422(
            &mut state,
            "abc123",
            &check_suites_422_error("abc123"),
            || async {
                Ok(DetectedPrLookup {
                    requested_branch: "feature/eng-992".to_string(),
                    current_branch: Some("feature/eng-992".to_string()),
                    repo_owner: "allisoneer".to_string(),
                    repo_name: "agentic_auxilary".to_string(),
                    token_source: Some("GH_TOKEN".to_string()),
                    empty_result_reason: Some("no_open_pull_requests_matched_branch".to_string()),
                    pr: None,
                })
            },
        )
        .await
        .expect("recovery should succeed");

        match recovery {
            CheckSuite422Recovery::TerminalStop { message } => {
                assert!(message.contains("no open PR could be re-detected"));
                assert!(message.contains("No commit found for SHA"));
            }
            other => panic!("expected terminal stop, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn recover_from_check_suites_422_ignores_non_matching_422() {
        let mut state = sample_state();

        let err: anyhow::Error = GitHubRestError {
            status: 422,
            url: "https://api.github.com/repos/owner/repo/issues/1/comments".to_string(),
            body: "No commit found for SHA: abc123".to_string(),
        }
        .into();

        let recovery =
            recover_from_check_suite_no_commit_found_422(&mut state, "abc123", &err, || async {
                panic!("non-matching 422 should not trigger PR re-detection")
            })
            .await
            .expect("classification should succeed");

        assert!(matches!(recovery, CheckSuite422Recovery::NotApplicable));
    }

    #[tokio::test]
    async fn persist_stop_state_before_handoff_saves_before_posting() {
        let mut state = sample_state();
        state.stage.kind = StageKind::StoppedDirtyTree;
        state.stage.details = Some("dirty worktree blocks freshness gate".to_string());

        let events = std::sync::Arc::new(Mutex::new(Vec::new()));
        let message = persist_stop_state_before_handoff(&state, {
            let events = std::sync::Arc::clone(&events);
            move |saved_state| {
                events
                    .lock()
                    .unwrap()
                    .push(format!("save:{:?}", saved_state.stage.kind));
                Ok(())
            }
        })
        .expect("save-before-post helper should succeed");

        events
            .lock()
            .unwrap()
            .push(format!("post:{:?}:{}", state.stage.kind, message));

        assert_eq!(
            events.lock().unwrap().as_slice(),
            &[
                "save:StoppedDirtyTree".to_string(),
                "post:StoppedDirtyTree:dirty worktree blocks freshness gate".to_string(),
            ]
        );
    }
}
