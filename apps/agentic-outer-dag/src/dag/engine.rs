use crate::dag::stages;
use crate::github::coderabbit::CodeRabbitClient;
use crate::github::coderabbit::CodeRabbitPoll;
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
                        state.pr.number = Some(pr.number);
                        state.pr.url = Some(pr.url);
                        state.pr.head_sha = Some(pr.head_sha.clone());
                        state.pr.last_observed_head_sha = Some(pr.head_sha);
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
                    let lookup = self
                        .github
                        .detect_open_pr_from_branch(&state.worktree.branch)
                        .await?;
                    record_pr_lookup(&mut state, StageKind::DetectingPr, &lookup);
                    if let Some(pr) = lookup.pr {
                        state.pr.number = Some(pr.number);
                        state.pr.url = Some(pr.url);
                        state.pr.head_sha = Some(pr.head_sha.clone());
                        state.pr.last_observed_head_sha = Some(pr.head_sha);
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
                    let started_at = chrono::Utc::now();
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
                                state.stage.kind = StageKind::StoppedReviewSkipped;
                                state.stage.details = Some(reason);
                                ThoughtsStateStore::save(&state)?;
                                return Ok(());
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
                _ => return Ok(()),
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
                linear::post_handoff_once(state, &message).await?;
                state.freshness.last_result = Some("conflict".to_string());
                state.stage.kind = StageKind::StoppedRebaseConflict;
                state.stage.details = Some(message);
            }
            freshness::FreshnessOutcome::DirtyTree => {
                let message = "dirty worktree blocks freshness gate".to_string();
                linear::post_handoff_once(state, &message).await?;
                state.freshness.last_result = Some("dirty_tree".to_string());
                state.stage.kind = StageKind::StoppedDirtyTree;
                state.stage.details = Some(message);
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

        self.supervisor()
            .await?
            .ensure_commands_present(&[command_name, "linear_ticket_2_pr", "resolve_pr_comments"])
            .await?;
        state.opencode.resume_stage = Some(resume_stage.clone());
        state.opencode.dispatch_attempt += 1;
        state.opencode.last_command = Some(command_name.to_string());
        ThoughtsStateStore::save(state)?;

        let outcome = self
            .supervisor()
            .await?
            .run_command_supervised(
                state.opencode.active_session_id.as_deref(),
                command_name,
                message,
            )
            .await?;
        match outcome {
            SupervisedOutcome::Completed { session_id } => {
                state.opencode.active_session_id = Some(session_id);
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
            SupervisedOutcome::Failed { session_id, error } => {
                state.opencode.active_session_id = session_id;
                transition_to_stopped_failed(state, error);
            }
        }
        ThoughtsStateStore::save(state)
    }

    async fn supervisor(&mut self) -> Result<&OpenCodeSupervisor> {
        if self.supervisor.is_none() {
            self.supervisor = Some(OpenCodeSupervisor::start(std::path::Path::new(".")).await?);
        }

        self.supervisor
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("supervisor should be initialized before use"))
    }
}

#[cfg(test)]
mod tests {
    use super::DagEngine;
    use super::planned_actions_for_start;
    use super::poll_interval_sleep_duration;
    use super::record_pr_lookup;
    use super::stage_kind_label;
    use super::transition_to_dispatch_disabled;
    use super::transition_to_missing_pr_after_lookup;
    use super::transition_to_stopped_failed;
    use crate::github::pr::DetectedPrLookup;
    use crate::state::RunState;
    use crate::state::StageKind;
    use crate::worktree::TargetWorktree;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use std::time::Duration;

    static OPENCODE_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

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
                pr: None,
            },
        );

        transition_to_missing_pr_after_lookup(&mut state, "after ticket_to_pr run");

        assert!(state
            .last_error
            .as_deref()
            .expect("last error should exist")
            .contains("no open PR found for branch 'feature/eng-992' in allisoneer/agentic_auxilary after ticket_to_pr run"));
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
        let _guard = opencode_env_lock().lock().unwrap();
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

    fn opencode_env_lock() -> &'static Mutex<()> {
        OPENCODE_ENV_LOCK.get_or_init(|| Mutex::new(()))
    }
}
