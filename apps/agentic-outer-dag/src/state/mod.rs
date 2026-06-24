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
    pub max_review_cycles: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stage {
    pub kind: StageKind,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
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
    pub active_session_id: Option<String>,
    pub last_command: Option<String>,
    pub dispatch_attempt: u32,
    pub resume_stage: Option<StageKind>,
    pub pending_permission: Option<PendingPermission>,
    pub pending_question: Option<PendingQuestion>,
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
                poll_interval_seconds: 30,
                coderabbit_timeout_seconds: 3600,
                max_review_cycles: 5,
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
    }
}
