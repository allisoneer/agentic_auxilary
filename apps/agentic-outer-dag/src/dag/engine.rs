use crate::dag::stages;
use crate::github::coderabbit::CodeRabbitClient;
use crate::github::coderabbit::CodeRabbitPoll;
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
use std::path::Path;

pub struct DagEngine {
    supervisor: OpenCodeSupervisor,
    github: GitHubPrClient,
    coderabbit: CodeRabbitClient,
}

fn poll_interval_sleep_duration(poll_interval_seconds: u64) -> std::time::Duration {
    std::time::Duration::from_secs(poll_interval_seconds.max(1))
}

impl DagEngine {
    pub async fn for_current_dir() -> Result<Self> {
        let supervisor = OpenCodeSupervisor::start(Path::new(".")).await?;
        Ok(Self {
            supervisor,
            github: GitHubPrClient::new()?,
            coderabbit: CodeRabbitClient::new()?,
        })
    }

    pub async fn run_until_stop(&self) -> Result<()> {
        loop {
            let mut state = ThoughtsStateStore::load()?
                .ok_or_else(|| anyhow::anyhow!("no state; run start first"))?;

            if stages::is_terminal(&state.stage.kind) || stages::is_paused(&state.stage.kind) {
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
                    if let Some(pr) = self
                        .github
                        .detect_open_pr_from_branch(&state.worktree.branch)
                        .await?
                    {
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
                    if let Some(pr) = self
                        .github
                        .detect_open_pr_from_branch(&state.worktree.branch)
                        .await?
                    {
                        state.pr.number = Some(pr.number);
                        state.pr.url = Some(pr.url);
                        state.pr.head_sha = Some(pr.head_sha.clone());
                        state.pr.last_observed_head_sha = Some(pr.head_sha);
                        state.stage.kind = StageKind::FreshnessBeforeCoderabbitWait;
                        state.stage.details = None;
                    } else {
                        state.stage.kind = StageKind::StoppedFailed;
                        state.stage.details =
                            Some("no open PR found for branch after ticket_to_pr run".to_string());
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
        &self,
        state: &mut RunState,
        resume_stage: StageKind,
        command_name: &str,
        message: Option<&str>,
    ) -> Result<()> {
        self.supervisor
            .ensure_commands_present(&[command_name, "linear_ticket_2_pr", "resolve_pr_comments"])
            .await?;
        state.opencode.resume_stage = Some(resume_stage.clone());
        state.opencode.dispatch_attempt += 1;
        state.opencode.last_command = Some(command_name.to_string());
        ThoughtsStateStore::save(state)?;

        let outcome = self
            .supervisor
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
                state.last_error = Some(error.clone());
                state.stage.kind = StageKind::StoppedFailed;
                state.stage.details = Some(error);
            }
        }
        ThoughtsStateStore::save(state)
    }
}

#[cfg(test)]
mod tests {
    use super::poll_interval_sleep_duration;
    use std::time::Duration;

    #[test]
    fn poll_interval_sleep_duration_clamps_to_one_second_minimum() {
        assert_eq!(poll_interval_sleep_duration(0), Duration::from_secs(1));
        assert_eq!(poll_interval_sleep_duration(1), Duration::from_secs(1));
        assert_eq!(poll_interval_sleep_duration(5), Duration::from_secs(5));
    }
}
