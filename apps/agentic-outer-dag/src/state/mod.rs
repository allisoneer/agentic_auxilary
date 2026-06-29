pub mod store;

use crate::worktree::TargetWorktree;
use anyhow::Result;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::process;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

pub const STATE_FILENAME: &str = "agentic-outer-dag-state.json";
pub const DEFAULT_POLL_INTERVAL_SECONDS: u64 = 30;
pub const DEFAULT_CODERABBIT_TIMEOUT_SECONDS: u64 = 3600;
pub const DEFAULT_OPENCODE_SESSION_DEADLINE_SECONDS: u64 = 8 * 60 * 60;
pub const DEFAULT_OPENCODE_INACTIVITY_TIMEOUT_SECONDS: u64 = 5 * 60;
const SCHEMA_VERSION: u32 = 1;
static RUN_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunState {
    pub schema_version: u32,
    pub run_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub ticket: TicketRef,
    pub worktree: WorktreeRef,
    pub settings: Settings,
    pub stage: Stage,
    pub opencode: OpenCodeState,
    pub pr: PrState,
    pub freshness: FreshnessState,
    pub coderabbit: CodeRabbitState,
    pub handoff: HandoffState,
    pub counters: Counters,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketRef {
    pub linear_key: String,
    pub linear_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeRef {
    pub path: String,
    pub branch: String,
    pub base_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub dry_run: bool,
    pub poll_interval_seconds: u64,
    pub coderabbit_timeout_seconds: u64,
    #[serde(default = "default_opencode_session_deadline_seconds")]
    pub opencode_session_deadline_seconds: u64,
    #[serde(default = "default_opencode_inactivity_timeout_seconds")]
    pub opencode_inactivity_timeout_seconds: u64,
    pub max_review_cycles: u32,
    #[serde(default = "default_linear_handoff_enabled")]
    pub linear_handoff_enabled: bool,
    #[serde(default = "default_opencode_dispatch_enabled")]
    pub opencode_dispatch_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    pub kind: StageKind,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
#[value(rename_all = "snake_case")]
pub enum StageKind {
    Init,
    FreshnessBeforeTicketToPr,
    DispatchingTicketToPr,
    DetectingPr,
    FreshnessBeforeCoderabbitWait,
    WaitingForCoderabbit,
    DispatchingResolvePrComments,
    StoppedPermissionRequired,
    StoppedQuestionRequired,
    StoppedDirtyTree,
    StoppedRebaseConflict,
    StoppedManualHandoff,
    StoppedReviewSkipped,
    StoppedTimedOut,
    StoppedReadyForHumanReview,
    StoppedFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpenCodeState {
    #[serde(default)]
    pub active_session_id: Option<String>,
    #[serde(default)]
    pub last_command: Option<String>,
    pub dispatch_attempt: u32,
    #[serde(default)]
    pub resume_stage: Option<StageKind>,
    #[serde(default)]
    pub pending_permission: Option<PendingPermission>,
    #[serde(default)]
    pub pending_question: Option<PendingQuestion>,
    #[serde(default)]
    pub last_diagnostics: Option<OpenCodeDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenCodeDiagnostics {
    pub checked_at: String,
    #[serde(default)]
    pub command_message_id: Option<String>,
    #[serde(default)]
    pub final_assistant_message_id: Option<String>,
    #[serde(default)]
    pub final_finish_reason: Option<String>,
    #[serde(default)]
    pub guard_detected: bool,
    #[serde(default)]
    pub final_tool_error: Option<OpenCodeToolErrorDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpenCodeToolErrorDiagnostics {
    pub tool: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPermission {
    pub request_id: String,
    pub permission_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingQuestion {
    pub request_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrState {
    pub number: Option<u64>,
    pub url: Option<String>,
    pub head_sha: Option<String>,
    pub last_observed_head_sha: Option<String>,
    #[serde(default)]
    pub last_lookup: Option<PrLookupDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrLookupDiagnostics {
    pub checked_at: String,
    pub stage: StageKind,
    pub requested_branch: String,
    pub current_branch: Option<String>,
    pub repo_owner: String,
    pub repo_name: String,
    #[serde(default)]
    pub token_source: Option<String>,
    #[serde(default)]
    pub empty_result_reason: Option<String>,
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FreshnessState {
    pub last_checked_at: Option<String>,
    pub last_result: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodeRabbitState {
    pub current_cycle: u32,
    pub cycles: Vec<CodeRabbitCycle>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRabbitCycle {
    pub cycle: u32,
    pub head_sha: String,
    pub started_at: String,
    pub status: String,
    pub observed: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HandoffState {
    pub linear_comment_posted: bool,
    pub linear_comment_body_sha256: Option<String>,
    pub posted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Counters {
    pub ticket_to_pr_runs: u32,
    pub resolve_comments_runs: u32,
}

impl RunState {
    pub fn for_start(ticket: &str, worktree: &TargetWorktree, dry_run: bool) -> Result<Self> {
        let now = Utc::now().to_rfc3339();
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            run_id: generate_run_id(),
            created_at: now.clone(),
            updated_at: now,
            ticket: TicketRef {
                linear_key: ticket.to_string(),
                linear_url: None,
            },
            worktree: WorktreeRef {
                path: worktree.path.canonicalize()?.display().to_string(),
                branch: worktree.branch.clone(),
                base_ref: worktree.base_ref.clone(),
            },
            settings: Settings {
                dry_run,
                poll_interval_seconds: DEFAULT_POLL_INTERVAL_SECONDS,
                coderabbit_timeout_seconds: DEFAULT_CODERABBIT_TIMEOUT_SECONDS,
                opencode_session_deadline_seconds: DEFAULT_OPENCODE_SESSION_DEADLINE_SECONDS,
                opencode_inactivity_timeout_seconds: DEFAULT_OPENCODE_INACTIVITY_TIMEOUT_SECONDS,
                max_review_cycles: 5,
                linear_handoff_enabled: default_linear_handoff_enabled(),
                opencode_dispatch_enabled: default_opencode_dispatch_enabled(),
            },
            stage: Stage {
                kind: StageKind::FreshnessBeforeTicketToPr,
                details: None,
            },
            opencode: OpenCodeState::default(),
            pr: PrState::default(),
            freshness: FreshnessState::default(),
            coderabbit: CodeRabbitState::default(),
            handoff: HandoffState::default(),
            counters: Counters::default(),
            last_error: None,
        })
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now().to_rfc3339();
    }
}

const fn default_linear_handoff_enabled() -> bool {
    true
}

const fn default_opencode_dispatch_enabled() -> bool {
    true
}

const fn default_opencode_session_deadline_seconds() -> u64 {
    DEFAULT_OPENCODE_SESSION_DEADLINE_SECONDS
}

const fn default_opencode_inactivity_timeout_seconds() -> u64 {
    DEFAULT_OPENCODE_INACTIVITY_TIMEOUT_SECONDS
}

fn generate_run_id() -> String {
    let tick = RUN_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "outer-dag-{}-{}-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        process::id(),
        tick
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage_kinds() -> Vec<StageKind> {
        vec![
            StageKind::Init,
            StageKind::FreshnessBeforeTicketToPr,
            StageKind::DispatchingTicketToPr,
            StageKind::DetectingPr,
            StageKind::FreshnessBeforeCoderabbitWait,
            StageKind::WaitingForCoderabbit,
            StageKind::DispatchingResolvePrComments,
            StageKind::StoppedPermissionRequired,
            StageKind::StoppedQuestionRequired,
            StageKind::StoppedDirtyTree,
            StageKind::StoppedRebaseConflict,
            StageKind::StoppedManualHandoff,
            StageKind::StoppedReviewSkipped,
            StageKind::StoppedTimedOut,
            StageKind::StoppedReadyForHumanReview,
            StageKind::StoppedFailed,
        ]
    }

    #[test]
    fn stage_kind_roundtrips_through_serde() {
        for kind in stage_kinds() {
            let json = serde_json::to_string(&kind).unwrap();
            let roundtrip: StageKind = serde_json::from_str(&json).unwrap();
            assert_eq!(roundtrip, kind);
        }
    }

    #[test]
    fn run_state_roundtrips_with_pending_interruptions() {
        let mut state = RunState::for_start(
            "ENG-992",
            &TargetWorktree {
                path: std::env::current_dir().unwrap(),
                branch: "feature/eng-992".to_string(),
                base_ref: "origin/main".to_string(),
            },
            true,
        )
        .unwrap();

        state.opencode.pending_permission = Some(PendingPermission {
            request_id: "perm-1".to_string(),
            permission_type: "file.write".to_string(),
        });
        state.opencode.pending_question = Some(PendingQuestion {
            request_id: "question-1".to_string(),
            prompt: "Continue?".to_string(),
        });
        state.opencode.last_diagnostics = Some(OpenCodeDiagnostics {
            checked_at: "2026-01-01T00:00:00Z".to_string(),
            command_message_id: Some("msg-outer-dag-1".to_string()),
            final_assistant_message_id: Some("msg-assistant-1".to_string()),
            final_finish_reason: Some("stop".to_string()),
            guard_detected: false,
            final_tool_error: Some(OpenCodeToolErrorDiagnostics {
                tool: "read".to_string(),
                error: "permission denied".to_string(),
            }),
        });
        state.stage.kind = StageKind::StoppedQuestionRequired;

        let json = serde_json::to_string_pretty(&state).unwrap();
        let roundtrip: RunState = serde_json::from_str(&json).unwrap();

        assert_eq!(roundtrip.stage.kind, StageKind::StoppedQuestionRequired);
        assert_eq!(
            roundtrip
                .opencode
                .pending_permission
                .as_ref()
                .unwrap()
                .permission_type,
            "file.write"
        );
        assert_eq!(
            roundtrip.opencode.pending_question.as_ref().unwrap().prompt,
            "Continue?"
        );
        assert_eq!(
            roundtrip
                .opencode
                .last_diagnostics
                .as_ref()
                .and_then(|diagnostics| diagnostics.final_tool_error.as_ref())
                .map(|diagnostics| diagnostics.tool.as_str()),
            Some("read")
        );
        assert!(roundtrip.settings.linear_handoff_enabled);
        assert!(roundtrip.settings.opencode_dispatch_enabled);
    }

    #[test]
    fn settings_default_safety_flags_enabled_for_legacy_state_files() {
        let value = serde_json::json!({
            "schema_version": 1,
            "run_id": "outer-dag-test",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z",
            "ticket": {
                "linear_key": "ENG-992",
                "linear_url": null
            },
            "worktree": {
                "path": "/tmp/worktree",
                "branch": "feature/eng-992",
                "base_ref": "origin/main"
            },
            "settings": {
                "dry_run": false,
                "poll_interval_seconds": 30,
                "coderabbit_timeout_seconds": 3600,
                "max_review_cycles": 5
            },
            "stage": {
                "kind": "freshness_before_ticket_to_pr",
                "details": null
            },
            "opencode": {
                "active_session_id": null,
                "last_command": null,
                "dispatch_attempt": 0,
                "resume_stage": null,
                "pending_permission": null,
                "pending_question": null,
                "last_diagnostics": null
            },
            "pr": {
                "number": null,
                "url": null,
                "head_sha": null,
                "last_observed_head_sha": null
            },
            "freshness": {
                "last_checked_at": null,
                "last_result": null
            },
            "coderabbit": {
                "current_cycle": 0,
                "cycles": []
            },
            "handoff": {
                "linear_comment_posted": false,
                "linear_comment_body_sha256": null,
                "posted_at": null
            },
            "counters": {
                "ticket_to_pr_runs": 0,
                "resolve_comments_runs": 0
            },
            "last_error": null
        });

        let roundtrip: RunState = serde_json::from_value(value).unwrap();

        assert!(roundtrip.settings.linear_handoff_enabled);
        assert!(roundtrip.settings.opencode_dispatch_enabled);
        assert_eq!(
            roundtrip.settings.opencode_session_deadline_seconds,
            DEFAULT_OPENCODE_SESSION_DEADLINE_SECONDS
        );
        assert_eq!(
            roundtrip.settings.opencode_inactivity_timeout_seconds,
            DEFAULT_OPENCODE_INACTIVITY_TIMEOUT_SECONDS
        );
        assert!(roundtrip.opencode.last_diagnostics.is_none());
    }

    #[test]
    fn opencode_diagnostics_default_new_fields_for_legacy_state() {
        let opencode: OpenCodeState = serde_json::from_value(serde_json::json!({
            "active_session_id": null,
            "last_command": null,
            "dispatch_attempt": 0,
            "resume_stage": null,
            "pending_permission": null,
            "pending_question": null
        }))
        .unwrap();

        assert!(opencode.last_diagnostics.is_none());
    }

    #[test]
    fn run_state_for_start_uses_default_timing_settings() {
        let state = RunState::for_start(
            "ENG-992",
            &TargetWorktree {
                path: std::env::current_dir().unwrap(),
                branch: "feature/eng-992".to_string(),
                base_ref: "origin/main".to_string(),
            },
            false,
        )
        .unwrap();

        assert_eq!(
            state.settings.poll_interval_seconds,
            DEFAULT_POLL_INTERVAL_SECONDS
        );
        assert_eq!(
            state.settings.coderabbit_timeout_seconds,
            DEFAULT_CODERABBIT_TIMEOUT_SECONDS
        );
        assert_eq!(
            state.settings.opencode_session_deadline_seconds,
            DEFAULT_OPENCODE_SESSION_DEADLINE_SECONDS
        );
        assert_eq!(
            state.settings.opencode_inactivity_timeout_seconds,
            DEFAULT_OPENCODE_INACTIVITY_TIMEOUT_SECONDS
        );
    }

    #[test]
    fn pr_lookup_diagnostics_default_new_fields_for_legacy_state() {
        let diagnostics: PrLookupDiagnostics = serde_json::from_value(serde_json::json!({
            "checked_at": "2026-01-01T00:00:00Z",
            "stage": "dispatching_ticket_to_pr",
            "requested_branch": "feature/eng-992",
            "current_branch": "feature/eng-992",
            "repo_owner": "allisoneer",
            "repo_name": "agentic_auxilary",
            "outcome": "not_found"
        }))
        .unwrap();

        assert_eq!(diagnostics.token_source, None);
        assert_eq!(diagnostics.empty_result_reason, None);
    }
}
