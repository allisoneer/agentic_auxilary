use crate::dag::engine::planned_actions_for_start;
use crate::state::RunState;
use crate::state::StageKind;
use crate::state::store::ThoughtsStateStore;
use std::collections::HashMap;
use std::time::Duration;

pub const PROGRESS_PREFIX: &str = "outer-dag:";

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProgressSnapshot {
    stage_kind: StageKind,
    stage_details: Option<String>,
    opencode_dispatch_attempt: u32,
    opencode_last_command: Option<String>,
    pending_permission_type: Option<String>,
    pending_question_prompt: Option<String>,
    pr_url: Option<String>,
    updated_at: String,
}

pub struct ProgressRenderer {
    actions: HashMap<&'static str, (usize, &'static str)>,
    total_actions: usize,
    last: Option<ProgressSnapshot>,
    last_output_at: tokio::time::Instant,
    warned_load_error: bool,
}

impl Default for ProgressRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressRenderer {
    pub fn new() -> Self {
        let actions_vec = planned_actions_for_start();
        let total_actions = actions_vec.len();
        let mut actions = HashMap::new();
        for (idx, action) in actions_vec.into_iter().enumerate() {
            actions.insert(action.id, (idx, action.summary));
        }

        Self {
            actions,
            total_actions,
            last: None,
            last_output_at: tokio::time::Instant::now(),
            warned_load_error: false,
        }
    }

    pub fn poll_interval() -> Duration {
        POLL_INTERVAL
    }

    pub fn tick_best_effort(&mut self) {
        let loaded = match ThoughtsStateStore::load() {
            Ok(state) => {
                self.warned_load_error = false;
                state
            }
            Err(err) => {
                if !self.warned_load_error {
                    eprintln!(
                        "{PROGRESS_PREFIX} progress: failed to load state (will retry): {err}"
                    );
                    self.warned_load_error = true;
                }
                return;
            }
        };

        let Some(state) = loaded else {
            return;
        };

        let snapshot = ProgressSnapshot::from_state(&state);
        self.maybe_render(&snapshot);
    }

    fn maybe_render(&mut self, snap: &ProgressSnapshot) {
        let now = tokio::time::Instant::now();
        let changed = self.last.as_ref() != Some(snap);
        let should_heartbeat = now.duration_since(self.last_output_at) >= HEARTBEAT_INTERVAL;

        if changed {
            self.render_stage_line(snap);
            self.render_pr_line_if_needed(snap);
            self.render_opencode_line_if_needed(snap);
            Self::render_interrupt_details_if_needed(snap);
            self.last_output_at = now;
            self.last = Some(snap.clone());
            return;
        }

        if should_heartbeat {
            Self::render_heartbeat(snap);
            self.last_output_at = now;
        }
    }

    fn render_stage_line(&self, snap: &ProgressSnapshot) {
        let kind = stage_kind_label(&snap.stage_kind);
        let details_suffix = snap
            .stage_details
            .as_ref()
            .map(|details| format!(": {details}"))
            .unwrap_or_default();

        if let Some(action_id) = planned_action_id_for_stage(&snap.stage_kind)
            && let Some((idx, summary)) = self.actions.get(action_id)
        {
            eprintln!(
                "{PROGRESS_PREFIX} [{}/{}] {} — {}{}",
                idx + 1,
                self.total_actions,
                kind,
                summary,
                details_suffix,
            );
            return;
        }

        eprintln!("{PROGRESS_PREFIX} {kind}{details_suffix}");
    }

    fn render_pr_line_if_needed(&self, snap: &ProgressSnapshot) {
        let pr_changed = if let Some(last) = self.last.as_ref() {
            last.pr_url != snap.pr_url
        } else {
            snap.pr_url.is_some()
        };

        if pr_changed && let Some(url) = snap.pr_url.as_deref() {
            eprintln!("{PROGRESS_PREFIX} pr: {url}");
        }
    }

    fn render_opencode_line_if_needed(&self, snap: &ProgressSnapshot) {
        let changed = match self.last.as_ref() {
            Some(last) => {
                last.opencode_dispatch_attempt != snap.opencode_dispatch_attempt
                    || last.opencode_last_command != snap.opencode_last_command
            }
            None => snap.opencode_last_command.is_some(),
        };
        if !changed {
            return;
        }

        let Some(command) = snap.opencode_last_command.as_deref() else {
            return;
        };

        if let Some(action_id) = planned_action_id_for_command(command)
            && let Some((idx, summary)) = self.actions.get(action_id)
        {
            eprintln!(
                "{PROGRESS_PREFIX} [{}/{}] opencode — {} (attempt={}): {}",
                idx + 1,
                self.total_actions,
                command,
                snap.opencode_dispatch_attempt,
                summary,
            );
            return;
        }

        eprintln!(
            "{PROGRESS_PREFIX} opencode — {} (attempt={})",
            command, snap.opencode_dispatch_attempt,
        );
    }

    fn render_interrupt_details_if_needed(snap: &ProgressSnapshot) {
        if matches!(snap.stage_kind, StageKind::StoppedPermissionRequired)
            && let Some(permission_type) = snap.pending_permission_type.as_deref()
        {
            eprintln!("{PROGRESS_PREFIX} opencode: permission required ({permission_type})");
        }

        if matches!(snap.stage_kind, StageKind::StoppedQuestionRequired)
            && let Some(prompt) = snap.pending_question_prompt.as_deref()
        {
            eprintln!("{PROGRESS_PREFIX} opencode: question required: {prompt}");
        }
    }

    fn render_heartbeat(snap: &ProgressSnapshot) {
        eprintln!(
            "{PROGRESS_PREFIX} (still running) stage={} last_update={}",
            stage_kind_label(&snap.stage_kind),
            snap.updated_at,
        );
    }
}

impl ProgressSnapshot {
    fn from_state(state: &RunState) -> Self {
        Self {
            stage_kind: state.stage.kind.clone(),
            stage_details: normalize_details(state.stage.details.as_deref()),
            opencode_dispatch_attempt: state.opencode.dispatch_attempt,
            opencode_last_command: state.opencode.last_command.clone(),
            pending_permission_type: state
                .opencode
                .pending_permission
                .as_ref()
                .map(|pending| pending.permission_type.clone()),
            pending_question_prompt: state
                .opencode
                .pending_question
                .as_ref()
                .map(|pending| pending.prompt.clone()),
            pr_url: state.pr.url.clone(),
            updated_at: state.updated_at.clone(),
        }
    }
}

fn normalize_details(details: Option<&str>) -> Option<String> {
    let details = details?;
    let collapsed = details.split_whitespace().collect::<Vec<_>>().join(" ");
    (!collapsed.is_empty()).then_some(collapsed)
}

fn stage_kind_label(kind: &StageKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| format!("{kind:?}"))
}

fn planned_action_id_for_stage(kind: &StageKind) -> Option<&'static str> {
    match kind {
        StageKind::FreshnessBeforeTicketToPr => Some("freshness.before_ticket_to_pr"),
        StageKind::DispatchingTicketToPr => Some("github.pr.detect_existing"),
        StageKind::DetectingPr => Some("github.pr.detect_after_ticket_to_pr"),
        StageKind::FreshnessBeforeCoderabbitWait => Some("freshness.before_coderabbit_wait"),
        StageKind::WaitingForCoderabbit => Some("github.coderabbit.wait"),
        StageKind::DispatchingResolvePrComments => Some("opencode.run.resolve_pr_comments"),
        StageKind::DispatchingDescribePr => Some("opencode.run.describe_pr_refresh"),
        StageKind::WaitingForCi => Some("github.ci.wait"),
        StageKind::DispatchingResolvePrCiFailures => Some("opencode.run.resolve_pr_ci_failures"),
        StageKind::StoppedReadyForHumanReview => Some("stop.ready_for_human_review"),
        _ => None,
    }
}

fn planned_action_id_for_command(command: &str) -> Option<&'static str> {
    match command {
        "linear_ticket_2_pr" => Some("opencode.run.linear_ticket_2_pr"),
        "resolve_pr_comments" => Some("opencode.run.resolve_pr_comments"),
        "describe_pr" => Some("opencode.run.describe_pr_refresh"),
        "resolve_pr_ci_failures" => Some("opencode.run.resolve_pr_ci_failures"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::ProgressSnapshot;
    use super::normalize_details;
    use super::planned_action_id_for_command;
    use super::planned_action_id_for_stage;
    use super::stage_kind_label;
    use crate::state;
    use crate::worktree::TargetWorktree;

    fn sample_state() -> state::RunState {
        let mut state = state::RunState::for_start(
            "ENG-992",
            &TargetWorktree {
                path: std::env::current_dir().expect("cwd available for progress test"),
                branch: "feature/eng-992".to_string(),
                base_ref: "origin/main".to_string(),
            },
            false,
        )
        .expect("sample state builds");
        state.stage.kind = state::StageKind::WaitingForCoderabbit;
        state.stage.details = Some("  waiting\nfor   CodeRabbit  ".to_string());
        state.opencode.dispatch_attempt = 3;
        state.opencode.last_command = Some("resolve_pr_comments".to_string());
        state.opencode.pending_permission = Some(state::PendingPermission {
            request_id: "perm-1".to_string(),
            permission_type: "git_push".to_string(),
        });
        state.opencode.pending_question = Some(state::PendingQuestion {
            request_id: "question-1".to_string(),
            prompt: "Proceed?".to_string(),
        });
        state.pr.url = Some("https://example.invalid/pr/1".to_string());
        state.updated_at = "2026-01-01T00:00:00Z".to_string();
        state
    }

    #[test]
    fn normalize_details_collapses_whitespace_to_single_line() {
        assert_eq!(
            normalize_details(Some("  line one\n\tline   two  ")),
            Some("line one line two".to_string())
        );
        assert_eq!(normalize_details(Some("   \n\t  ")), None);
        assert_eq!(normalize_details(None), None);
    }

    #[test]
    fn stage_and_command_mappings_cover_expected_progress_surfaces() {
        assert_eq!(
            planned_action_id_for_stage(&state::StageKind::WaitingForCoderabbit),
            Some("github.coderabbit.wait")
        );
        assert_eq!(
            planned_action_id_for_command("resolve_pr_ci_failures"),
            Some("opencode.run.resolve_pr_ci_failures")
        );
        assert_eq!(planned_action_id_for_command("unknown"), None);
    }

    #[test]
    fn stage_kind_label_uses_snake_case_serialization() {
        assert_eq!(
            stage_kind_label(&state::StageKind::StoppedPermissionRequired),
            "stopped_permission_required"
        );
    }

    #[test]
    fn snapshot_extraction_uses_normalized_state_fields() {
        let state = sample_state();
        let snapshot = ProgressSnapshot::from_state(&state);

        assert_eq!(snapshot.stage_kind, state::StageKind::WaitingForCoderabbit);
        assert_eq!(
            snapshot.stage_details,
            Some("waiting for CodeRabbit".to_string())
        );
        assert_eq!(snapshot.opencode_dispatch_attempt, 3);
        assert_eq!(
            snapshot.opencode_last_command.as_deref(),
            Some("resolve_pr_comments")
        );
        assert_eq!(
            snapshot.pending_permission_type.as_deref(),
            Some("git_push")
        );
        assert_eq!(
            snapshot.pending_question_prompt.as_deref(),
            Some("Proceed?")
        );
        assert_eq!(
            snapshot.pr_url.as_deref(),
            Some("https://example.invalid/pr/1")
        );
        assert_eq!(snapshot.updated_at, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn progress_renderer_default_matches_new() {
        let renderer = super::ProgressRenderer::new();
        let default_renderer = super::ProgressRenderer::default();

        assert_eq!(renderer.actions, default_renderer.actions);
        assert_eq!(renderer.total_actions, default_renderer.total_actions);
        assert_eq!(renderer.last, default_renderer.last);
        assert_eq!(
            renderer.warned_load_error,
            default_renderer.warned_load_error
        );
    }

    #[test]
    fn resumed_snapshot_with_existing_opencode_command_counts_as_changed() {
        let renderer = super::ProgressRenderer::new();
        let snapshot = ProgressSnapshot::from_state(&sample_state());

        let changed = match renderer.last.as_ref() {
            Some(last) => {
                last.opencode_dispatch_attempt != snapshot.opencode_dispatch_attempt
                    || last.opencode_last_command != snapshot.opencode_last_command
            }
            None => snapshot.opencode_last_command.is_some(),
        };

        assert!(changed);
    }
}
