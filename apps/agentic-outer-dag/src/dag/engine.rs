use crate::dag::stages;
use crate::github::coderabbit::CodeRabbitClient;
use crate::github::coderabbit::CodeRabbitPoll;
use crate::github::coderabbit::skip_reason_indicates_draft;
use crate::github::pr::DetectedPrLookup;
use crate::github::pr::GitHubPrClient;
use crate::linear;
use crate::opencode::supervisor::OpenCodeSupervisor;
use crate::opencode::supervisor::SupervisedOutcome;
use crate::state;
use crate::state::RunState;
use crate::state::StageKind;
use crate::state::store::ThoughtsStateStore;
use crate::worktree::freshness;
use anyhow::Result;
use std::fmt::Write as _;
use std::future::Future;
use std::time::Duration;

const DETECTING_PR_BACKOFFS: [Duration; 4] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
];
const DETECTING_PR_MAX_ATTEMPTS: usize = DETECTING_PR_BACKOFFS.len() + 1;

pub struct DagEngine {
    supervisor: Option<OpenCodeSupervisor>,
    github: GitHubPrClient,
    coderabbit: CodeRabbitClient,
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
            summary: "Freshness gate before ticket_to_pr (fetch/rebase)",
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
            summary: "Freshness gate before CodeRabbit wait (fetch/rebase)",
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
            id: "stop.ready_for_human_review",
            summary: "Stop at ready_for_human_review",
        },
    ]
}

fn poll_interval_sleep_duration(poll_interval_seconds: u64) -> std::time::Duration {
    std::time::Duration::from_secs(poll_interval_seconds.max(1))
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

fn transition_to_missing_pr_after_lookup(state: &mut RunState, context: &str) {
    let message = if let Some(lookup) = state.pr.last_lookup.as_ref() {
        let mut message = format!(
            "no open PR found for branch '{}' in {}/{} {context}",
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
        format!("no open PR found {context}")
    };

    transition_to_stopped_failed(state, message);
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

enum DraftSkipRecovery {
    ContinueWaiting,
    TerminalStop { message: String },
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
            github: GitHubPrClient::new()?,
            coderabbit: CodeRabbitClient::new()?,
        })
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
                    state.counters.ticket_to_pr_runs += 1;
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
                        ensure_pr_ready_for_review(
                            &mut state,
                            &pr,
                            "post_ticket_to_pr_detection",
                            |pr| async move { github.mark_ready_for_review(&pr).await },
                        )
                        .await?;
                        state.stage.kind = StageKind::FreshnessBeforeCoderabbitWait;
                        state.stage.details = None;
                    } else {
                        transition_to_missing_pr_after_lookup(
                            &mut state,
                            "after ticket_to_pr run; inspect status.pr_lookup for lookup context",
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
                    let head_sha = state
                        .pr
                        .head_sha
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("missing PR head SHA in state"))?;
                    let mut started_at = chrono::Utc::now();
                    let timeout_seconds = i64::try_from(state.settings.coderabbit_timeout_seconds)
                        .map_err(|_| anyhow::anyhow!("coderabbit timeout exceeds i64 range"))?;
                    loop {
                        match self.coderabbit.poll_once(pr_number, &head_sha).await? {
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
                                if (chrono::Utc::now() - started_at).num_seconds()
                                    >= timeout_seconds
                                {
                                    state.stage.kind = StageKind::StoppedTimedOut;
                                    state.stage.details = Some(
                                        "timed out waiting for CodeRabbit completion".to_string(),
                                    );
                                    ThoughtsStateStore::save(&state)?;
                                    return Ok(());
                                }
                                tokio::time::sleep(poll_interval_sleep_duration(
                                    state.settings.poll_interval_seconds,
                                ))
                                .await;
                            }
                        }
                    }
                }
                StageKind::DispatchingResolvePrComments => {
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
                    state.counters.resolve_comments_runs += 1;
                    state.stage.kind = StageKind::StoppedReadyForHumanReview;
                    state.stage.details =
                        Some("completed one CodeRabbit resolve cycle".to_string());
                    ThoughtsStateStore::save(&state)?;
                    return Ok(());
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
                | StageKind::StoppedFailed => return Ok(()),
            }
        }
    }

    async fn advance_freshness(
        &self,
        state: &mut RunState,
        next_stage: StageKind,
        resume_stage: StageKind,
    ) -> Result<()> {
        let outcome = freshness::run(&state.worktree.base_ref, state.settings.dry_run)?;
        state.freshness.last_checked_at = Some(chrono::Utc::now().to_rfc3339());
        state.opencode.resume_stage = Some(resume_stage);
        match outcome {
            freshness::FreshnessOutcome::UpToDate => {
                state.freshness.last_result = Some("up_to_date".to_string());
                state.stage.kind = next_stage;
                state.stage.details = None;
            }
            freshness::FreshnessOutcome::Rebases { old_head, new_head } => {
                state.freshness.last_result = Some(format!("rebased:{old_head}->{new_head}"));
                state.stage.kind = next_stage;
                state.stage.details = Some("rebased onto base ref".to_string());
                if state.pr.head_sha.is_some() {
                    state.pr.head_sha = Some(new_head.clone());
                    state.pr.last_observed_head_sha = Some(new_head);
                }
            }
            freshness::FreshnessOutcome::Conflict => {
                let message = "rebase conflict requires manual handoff".to_string();
                state.freshness.last_result = Some("conflict".to_string());
                state.stage.kind = StageKind::StoppedRebaseConflict;
                state.stage.details = Some(message);
                let message = persist_stop_state_before_handoff(state, ThoughtsStateStore::save)?;
                linear::post_handoff_once(state, &message).await?;
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
            .ensure_commands_present(&[command_name, "linear_ticket_2_pr", "resolve_pr_comments"])
            .await?;
        state.opencode.resume_stage = Some(resume_stage.clone());
        state.opencode.dispatch_attempt += 1;
        state.opencode.last_command = Some(command_name.to_string());
        ThoughtsStateStore::save(state)?;

        let outcome = self
            .supervisor(&state.settings)
            .await?
            .run_command_supervised(
                state.opencode.active_session_id.as_deref(),
                command_name,
                message,
            )
            .await?;
        match outcome {
            SupervisedOutcome::Completed {
                session_id,
                diagnostics,
            } => {
                state.opencode.active_session_id = Some(session_id);
                state.opencode.last_diagnostics = Some(diagnostics);
                state.opencode.pending_permission = None;
                state.opencode.pending_question = None;
                state.stage.kind = resume_stage;
                state.stage.details = None;
            }
            SupervisedOutcome::PermissionRequired {
                session_id,
                request_id,
                permission_type,
            } => {
                state.opencode.active_session_id = Some(session_id);
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
            } => {
                state.opencode.active_session_id = Some(session_id);
                state.opencode.pending_question =
                    Some(state::PendingQuestion { request_id, prompt });
                state.stage.kind = StageKind::StoppedQuestionRequired;
                state.stage.details = Some("OpenCode question response required".to_string());
            }
            SupervisedOutcome::Failed {
                session_id,
                error,
                diagnostics,
            } => {
                state.opencode.active_session_id = session_id;
                state.opencode.last_diagnostics = diagnostics;
                transition_to_stopped_failed(state, error);
            }
        }
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
    use super::DagEngine;
    use super::DraftSkipRecovery;
    use super::detecting_pr_retry_attempt_number;
    use super::ensure_pr_ready_for_review;
    use super::persist_stop_state_before_handoff;
    use super::planned_actions_for_start;
    use super::poll_interval_sleep_duration;
    use super::record_pr_lookup;
    use super::recover_from_draft_review_skip;
    use super::should_reset_coderabbit_timeout_baseline;
    use super::stage_kind_label;
    use super::transition_to_dispatch_disabled;
    use super::transition_to_missing_pr_after_lookup;
    use super::transition_to_stopped_failed;
    use crate::github::pr::DetectedPrLookup;
    use crate::state::RunState;
    use crate::state::StageKind;
    use crate::test_support::process_state_lock;
    use crate::worktree::TargetWorktree;
    use pr_comments::models::PrRef;
    use std::sync::Mutex;
    use std::time::Duration;

    fn sample_pr(is_draft: bool) -> PrRef {
        PrRef {
            number: 258,
            url: "https://example.invalid/pr/258".to_string(),
            head_sha: "abc123".to_string(),
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

    #[test]
    fn poll_interval_sleep_duration_clamps_to_one_second_minimum() {
        assert_eq!(poll_interval_sleep_duration(0), Duration::from_secs(1));
        assert_eq!(poll_interval_sleep_duration(1), Duration::from_secs(1));
        assert_eq!(poll_interval_sleep_duration(5), Duration::from_secs(5));
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
                "stop.ready_for_human_review",
            ]
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
    fn transition_to_missing_pr_after_lookup_uses_lookup_context() {
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

        transition_to_missing_pr_after_lookup(&mut state, "after ticket_to_pr run");

        assert!(state
            .last_error
            .as_deref()
            .expect("last error should exist")
            .contains("no open PR found for branch 'feature/eng-992' in allisoneer/agentic_auxilary after ticket_to_pr run"));
        assert!(
            state
                .last_error
                .as_deref()
                .expect("last error should exist")
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
