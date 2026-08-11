#[cfg(not(unix))]
compile_error!("agentic-outer-dag only supports Unix-like platforms (Linux/macOS).");

use anyhow::Result;
use clap::Parser;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use tracing::debug;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod cli;
mod dag;
mod github;
mod linear;
mod opencode;
mod owner;
mod preview;
mod progress;
mod state;
#[cfg(test)]
mod test_support;
mod worktree;

struct StartOptions<'a> {
    branch: Option<&'a str>,
    worktree_path: Option<&'a Path>,
    dry_run: bool,
    force: bool,
    no_linear_handoff: bool,
    no_opencode_dispatch: bool,
    stop_after: Option<state::StageKind>,
    poll_interval_seconds: Option<u64>,
    coderabbit_timeout_seconds: Option<u64>,
    opencode_session_deadline_seconds: Option<u64>,
    opencode_inactivity_timeout_seconds: Option<u64>,
    final_only: bool,
}

struct ResumeOptions<'a> {
    branch: Option<&'a str>,
    worktree_path: Option<&'a Path>,
    no_linear_handoff: bool,
    no_opencode_dispatch: bool,
    stop_after: Option<state::StageKind>,
    poll_interval_seconds: Option<u64>,
    coderabbit_timeout_seconds: Option<u64>,
    opencode_session_deadline_seconds: Option<u64>,
    opencode_inactivity_timeout_seconds: Option<u64>,
    final_only: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct SettingsOverrides {
    linear_handoff_enabled: Option<bool>,
    opencode_dispatch_enabled: Option<bool>,
    poll_interval_seconds: Option<u64>,
    coderabbit_timeout_seconds: Option<u64>,
    opencode_session_deadline_seconds: Option<u64>,
    opencode_inactivity_timeout_seconds: Option<u64>,
}

fn ensure_supported_dry_run_usage(dry_run: bool, command: &cli::Commands) -> Result<()> {
    if !dry_run {
        return Ok(());
    }

    match command {
        cli::Commands::Start { .. } | cli::Commands::Status { .. } => Ok(()),
        _ => anyhow::bail!(
            "--dry-run is only supported with `start` (preview); remove it for this command"
        ),
    }
}

#[cfg(test)]
fn require_actionable_resume_stage(state: &state::RunState) -> Result<state::StageKind> {
    let resume_stage = state.opencode.resume_stage.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "missing resume_stage in state; state may be corrupted; consider `agentic-outer-dag reset --yes`"
        )
    })?;

    anyhow::ensure!(
        crate::dag::stages::sequence_index(&resume_stage).is_some(),
        "invalid resume_stage in state ({resume_stage:?}); expected an actionable stage; consider `agentic-outer-dag reset --yes`"
    );

    Ok(resume_stage)
}

fn has_recovered_nonterminal_invocation(state: &state::RunState) -> bool {
    state
        .opencode
        .current_invocation
        .as_ref()
        .is_some_and(|invocation| {
            !matches!(invocation.phase, state::OpenCodeInvocationPhase::Terminal)
        })
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install the rustls CryptoProvider before any HTTP clients are created.
    // Required because Cargo's additive features cause both ring and aws-lc-rs
    // to be compiled in via transitive dependencies, and rustls 0.23+ panics
    // if it can't auto-select a single provider.
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("failed to install rustls aws-lc-rs CryptoProvider"))?;

    let cli = cli::Cli::parse();
    let dry_run = cli.dry_run;
    let final_only = cli.final_only;
    let command = cli.command;
    ensure_supported_dry_run_usage(dry_run, &command)?;

    let log_level = match (cli.quiet, cli.verbose) {
        (true, _) => "error",
        (false, 0) => "info",
        (false, 1) => "debug",
        (false, _) => "trace",
    };

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .init();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        "starting agentic-outer-dag"
    );

    match command {
        cli::Commands::Start {
            ticket,
            branch,
            worktree,
            force,
            no_linear_handoff,
            no_opencode_dispatch,
            stop_after,
            poll_interval_seconds,
            coderabbit_timeout_seconds,
            opencode_session_deadline_seconds,
            opencode_inactivity_timeout_seconds,
        } => {
            handle_start(
                &ticket,
                StartOptions {
                    branch: branch.as_deref(),
                    worktree_path: worktree.as_deref(),
                    dry_run,
                    force,
                    no_linear_handoff,
                    no_opencode_dispatch,
                    stop_after,
                    poll_interval_seconds,
                    coderabbit_timeout_seconds,
                    opencode_session_deadline_seconds,
                    opencode_inactivity_timeout_seconds,
                    final_only,
                },
            )
            .await
        }
        cli::Commands::Resume {
            branch,
            worktree,
            no_linear_handoff,
            no_opencode_dispatch,
            stop_after,
            poll_interval_seconds,
            coderabbit_timeout_seconds,
            opencode_session_deadline_seconds,
            opencode_inactivity_timeout_seconds,
        } => {
            handle_resume(ResumeOptions {
                branch: branch.as_deref(),
                worktree_path: worktree.as_deref(),
                no_linear_handoff,
                no_opencode_dispatch,
                stop_after,
                poll_interval_seconds,
                coderabbit_timeout_seconds,
                opencode_session_deadline_seconds,
                opencode_inactivity_timeout_seconds,
                final_only,
            })
            .await
        }
        cli::Commands::Status { json } => handle_status(json),
        cli::Commands::RespondPermission { allow, deny } => {
            handle_respond_permission(allow, deny).await
        }
        cli::Commands::RespondQuestion { answer } => handle_respond_question(&answer).await,
        cli::Commands::Handoff { message } => handle_handoff(message.as_deref()).await,
        cli::Commands::Reset { yes } => handle_reset(yes),
    }
}

async fn run_engine_until_stop_with_progress(
    engine: &mut dag::engine::DagEngine,
    stop_after: Option<state::StageKind>,
    final_only: bool,
) -> Result<()> {
    if final_only {
        return engine.run_until_stop(stop_after).await;
    }

    let stop = Arc::new(AtomicBool::new(false));
    let progress_stop = Arc::clone(&stop);
    let progress_task = tokio::spawn(async move {
        let mut renderer = crate::progress::ProgressRenderer::new();
        renderer.tick_best_effort();

        let mut ticker = tokio::time::interval(crate::progress::ProgressRenderer::poll_interval());
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        while !progress_stop.load(Ordering::Relaxed) {
            ticker.tick().await;
            renderer.tick_best_effort();
        }

        renderer.tick_best_effort();
    });

    let result = engine.run_until_stop(stop_after).await;
    stop.store(true, Ordering::Relaxed);
    if let Err(err) = progress_task.await {
        debug!(error = %err, "progress renderer task ended abnormally");
    }
    result
}

fn apply_settings_overrides(
    settings: &mut state::Settings,
    overrides: SettingsOverrides,
) -> Result<bool> {
    if let Some(poll_interval_seconds) = overrides.poll_interval_seconds {
        anyhow::ensure!(
            poll_interval_seconds > 0,
            "poll interval must be at least 1 second"
        );
        settings.poll_interval_seconds = poll_interval_seconds;
    }

    if let Some(coderabbit_timeout_seconds) = overrides.coderabbit_timeout_seconds {
        anyhow::ensure!(
            coderabbit_timeout_seconds > 0,
            "CodeRabbit timeout must be at least 1 second"
        );
        settings.coderabbit_timeout_seconds = coderabbit_timeout_seconds;
    }

    if let Some(opencode_session_deadline_seconds) = overrides.opencode_session_deadline_seconds {
        anyhow::ensure!(
            opencode_session_deadline_seconds > 0,
            "OpenCode session deadline must be at least 1 second"
        );
        settings.opencode_session_deadline_seconds = opencode_session_deadline_seconds;
    }

    if let Some(opencode_inactivity_timeout_seconds) = overrides.opencode_inactivity_timeout_seconds
    {
        anyhow::ensure!(
            opencode_inactivity_timeout_seconds > 0,
            "OpenCode inactivity timeout must be at least 1 second"
        );
        settings.opencode_inactivity_timeout_seconds = opencode_inactivity_timeout_seconds;
    }

    if let Some(linear_handoff_enabled) = overrides.linear_handoff_enabled {
        settings.linear_handoff_enabled = linear_handoff_enabled;
    }

    if let Some(opencode_dispatch_enabled) = overrides.opencode_dispatch_enabled {
        settings.opencode_dispatch_enabled = opencode_dispatch_enabled;
    }

    Ok(overrides.poll_interval_seconds.is_some()
        || overrides.coderabbit_timeout_seconds.is_some()
        || overrides.opencode_session_deadline_seconds.is_some()
        || overrides.opencode_inactivity_timeout_seconds.is_some()
        || overrides.linear_handoff_enabled.is_some()
        || overrides.opencode_dispatch_enabled.is_some())
}

async fn resolve_start_effective_branch(
    ticket: &str,
    branch: Option<&str>,
    worktree_path: Option<&Path>,
) -> Result<Option<String>> {
    if worktree_path.is_some() {
        return Ok(branch.map(str::to_string));
    }

    if let Some(branch) = branch {
        return Ok(Some(branch.to_string()));
    }

    Ok(Some(
        crate::linear::require_issue_branch_name_for_start(ticket).await?,
    ))
}

async fn handle_start(ticket: &str, options: StartOptions<'_>) -> Result<()> {
    let effective_branch =
        resolve_start_effective_branch(ticket, options.branch, options.worktree_path).await?;

    if options.dry_run {
        let plan = preview::build_dry_run_start_preview(
            ticket,
            effective_branch.as_deref(),
            options.worktree_path,
            options.force,
        )?;
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }

    let target = worktree::resolve(effective_branch.as_deref(), options.worktree_path, true)?;
    worktree::chdir_to(&target)?;
    let owner = Arc::new(owner::OwnerRuntime::acquire(Path::new("."))?);

    if state::store::ThoughtsStateStore::load()?.is_some() && !options.force {
        anyhow::bail!(
            "state file already exists for branch '{}'; rerun with --force to overwrite",
            target.branch
        );
    }

    let mut state = state::RunState::for_start(ticket, &target, options.dry_run)?;
    apply_settings_overrides(
        &mut state.settings,
        SettingsOverrides {
            linear_handoff_enabled: Some(!options.no_linear_handoff),
            opencode_dispatch_enabled: Some(!options.no_opencode_dispatch),
            poll_interval_seconds: options.poll_interval_seconds,
            coderabbit_timeout_seconds: options.coderabbit_timeout_seconds,
            opencode_session_deadline_seconds: options.opencode_session_deadline_seconds,
            opencode_inactivity_timeout_seconds: options.opencode_inactivity_timeout_seconds,
        },
    )?;
    state::store::ThoughtsStateStore::save(&state)?;

    let mut engine = dag::engine::DagEngine::for_current_dir_with_owner(Arc::clone(&owner))?;
    run_engine_until_stop_with_progress(&mut engine, options.stop_after, options.final_only)
        .await?;
    let state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("persisted state disappeared after start"))?;
    print_status(&state, false)
}

async fn handle_resume(options: ResumeOptions<'_>) -> Result<()> {
    let target = worktree::resolve(options.branch, options.worktree_path, false)?;
    worktree::chdir_to(&target)?;
    let owner = Arc::new(owner::OwnerRuntime::acquire(Path::new("."))?);

    let mut state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("no persisted state found; run start first"))?;
    if has_recovered_nonterminal_invocation(&state) {
        state.stage.kind = state::StageKind::StoppedManualHandoff;
        state.stage.details = Some(
            "recovered a nonterminal OpenCode invocation; automatic redispatch is disabled because prior execution disposition is uncertain"
                .to_string(),
        );
        state.opencode.last_resolve_workflow_outcome =
            Some(state::ResolveWorkflowOutcome::ManualHandoff);
        state::store::ThoughtsStateStore::save(&state)?;
        return print_status(&state, false);
    }
    let settings_changed = apply_settings_overrides(
        &mut state.settings,
        SettingsOverrides {
            linear_handoff_enabled: options.no_linear_handoff.then_some(false),
            opencode_dispatch_enabled: options.no_opencode_dispatch.then_some(false),
            poll_interval_seconds: options.poll_interval_seconds,
            coderabbit_timeout_seconds: options.coderabbit_timeout_seconds,
            opencode_session_deadline_seconds: options.opencode_session_deadline_seconds,
            opencode_inactivity_timeout_seconds: options.opencode_inactivity_timeout_seconds,
        },
    )?;
    if settings_changed {
        state::store::ThoughtsStateStore::save(&state)?;
    }
    let mut engine = dag::engine::DagEngine::for_current_dir_with_owner(Arc::clone(&owner))?;
    run_engine_until_stop_with_progress(&mut engine, options.stop_after, options.final_only)
        .await?;
    let state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("persisted state disappeared after resume"))?;
    print_status(&state, false)
}

fn handle_status(as_json: bool) -> Result<()> {
    let state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("no persisted state found in the current worktree"))?;
    print_status(&state, as_json)
}

fn print_status(state: &state::RunState, as_json: bool) -> Result<()> {
    if as_json {
        println!("{}", serde_json::to_string_pretty(state)?);
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&compact_status_payload(state))?
        );
    }

    Ok(())
}

fn compact_status_payload(state: &state::RunState) -> serde_json::Value {
    let worktree_exists = Path::new(&state.worktree.path).exists();
    let invocation = state.opencode.current_invocation.as_ref();

    json!({
        "ticket": state.ticket.linear_key,
        "branch": state.worktree.branch,
        "worktree": state.worktree.path,
        "stage": state.stage.kind,
        "state_file": format!("./thoughts/{}/artifacts/{}", state.worktree.branch, state::STATE_FILENAME),
        "stage_details": state.stage.details,
        "last_error": state.last_error,
        "worktree_exists": worktree_exists,
        "pr_number": state.pr.number,
        "pr_url": state.pr.url,
        "pr_is_draft": state.pr.is_draft,
        "pr_ready_for_review": state.pr.ready_for_review,
        "opencode_session_id": state.opencode.active_session_id,
        "opencode_last_command": state.opencode.last_command,
        "opencode_last_diagnostics": state.opencode.last_diagnostics,
        "opencode_invocation_phase": invocation.map(|invocation| &invocation.phase),
        "opencode_invocation_command": invocation.map(|invocation| &invocation.command),
        "opencode_invocation_id": invocation.map(|invocation| &invocation.invocation_id),
        "opencode_invocation_session_id": invocation.map(|invocation| &invocation.session_id),
        "opencode_invocation_message_id": invocation.map(|invocation| &invocation.command_message_id),
        "opencode_invocation_lifecycle_result": invocation.and_then(|invocation| invocation.lifecycle_result.as_ref()),
        "opencode_invocation_task_disposition": invocation.and_then(|invocation| invocation.task_disposition.as_ref()),
        "opencode_invocation_post_attempts": invocation.map(|invocation| invocation.literal_post_attempts),
        "opencode_resolve_start_retries": state.opencode.resolve_start_retries,
        "opencode_last_lifecycle_anomaly": state.opencode.last_lifecycle_anomaly,
        "opencode_last_resolve_workflow_outcome": state.opencode.last_resolve_workflow_outcome,
        "ticket_to_pr_runs": state.counters.ticket_to_pr,
        "resolve_comments_runs": state.counters.resolve_comments,
        "resolve_ci_runs": state.counters.resolve_ci,
        "ci": state.ci,
        "opencode_dispatch_enabled": state.settings.opencode_dispatch_enabled,
        "opencode_session_deadline_seconds": state.settings.opencode_session_deadline_seconds,
        "opencode_inactivity_timeout_seconds": state.settings.opencode_inactivity_timeout_seconds,
        "linear_handoff_enabled": state.settings.linear_handoff_enabled,
        "linear_handoff_posted": state.handoff.linear_comment_posted,
        "linear_handoff_posted_at": state.handoff.posted_at,
        "pr_lookup": state.pr.last_lookup,
        "run_id": state.run_id,
        "updated_at": state.updated_at,
    })
}

async fn handle_respond_permission(allow: bool, deny: bool) -> Result<()> {
    anyhow::ensure!(allow ^ deny, "exactly one of --allow or --deny is required");
    owner::send_response(
        Path::new("."),
        owner::InterruptionResponse::Permission { allow },
    )
    .await?;
    println!("permission response accepted by foreground owner");
    Ok(())
}

async fn handle_respond_question(answer: &str) -> Result<()> {
    owner::send_response(
        Path::new("."),
        owner::InterruptionResponse::Question {
            answers: vec![vec![answer.to_string()]],
        },
    )
    .await?;
    println!("question response accepted by foreground owner");
    Ok(())
}

async fn handle_handoff(message: Option<&str>) -> Result<()> {
    owner::OwnerRuntime::ensure_unlocked(Path::new("."))?;
    let mut state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("no persisted state found in the current worktree"))?;
    let body = message.unwrap_or("manual handoff requested from agentic-outer-dag");
    linear::post_handoff_once_forced(&mut state, body).await?;
    state.stage.kind = state::StageKind::StoppedManualHandoff;
    state.stage.details = Some(body.to_string());
    state::store::ThoughtsStateStore::save(&state)?;
    print_status(&state, false)
}

fn handle_reset(yes: bool) -> Result<()> {
    anyhow::ensure!(yes, "reset requires --yes");
    owner::OwnerRuntime::ensure_unlocked(Path::new("."))?;
    state::store::ThoughtsStateStore::delete()?;
    println!("state reset");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::SettingsOverrides;
    use super::StartOptions;
    use super::apply_settings_overrides;
    use super::compact_status_payload;
    use super::ensure_supported_dry_run_usage;
    use super::handle_start;
    use super::has_recovered_nonterminal_invocation;
    use super::require_actionable_resume_stage;
    use super::resolve_start_effective_branch;
    use super::state;
    use crate::cli;
    use crate::test_support::CwdGuard;
    use crate::test_support::EnvVarGuard;
    use crate::test_support::process_state_lock;
    use crate::test_support::run_git;
    use crate::worktree::TargetWorktree;
    use anyhow::Result;
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    fn sample_state() -> state::RunState {
        let mut state = state::RunState::for_start(
            "ENG-992",
            &TargetWorktree {
                path: std::env::current_dir().expect("cwd available for test"),
                branch: "feature/eng-992".to_string(),
                base_ref: "origin/main".to_string(),
            },
            false,
        )
        .expect("sample state builds");

        state.stage.kind = state::StageKind::StoppedFailed;
        state.stage.details = Some("detailed failure".to_string());
        state.last_error = Some("detailed failure".to_string());
        state.pr.number = Some(258);
        state.pr.url = Some("https://example.invalid/pr/258".to_string());
        state.pr.is_draft = Some(false);
        state.pr.ready_for_review.last_result = Some("already_ready:existing_pr_guard".to_string());
        state.opencode.active_session_id = Some("session-123".to_string());
        state.opencode.last_command = Some("linear_ticket_2_pr".to_string());
        state.opencode.last_diagnostics = Some(state::OpenCodeDiagnostics {
            checked_at: "2026-01-01T00:00:00Z".to_string(),
            literal_post_attempts: 1,
            command_message_id: Some("msg-outer-dag-1".to_string()),
            final_assistant_message_id: Some("msg-assistant-1".to_string()),
            final_finish_reason: Some("stop".to_string()),
            guard_detected: true,
            final_tool_error: Some(state::OpenCodeToolErrorDiagnostics {
                tool: "read".to_string(),
                error: "nested guard tripped".to_string(),
            }),
            command_transport_error: None,
        });
        state.opencode.current_invocation = Some(state::OpenCodeInvocationState {
            invocation_id: "invocation-1".to_string(),
            command: "resolve_pr_comments".to_string(),
            session_id: "session-123".to_string(),
            command_message_id: "msg-outer-dag-1".to_string(),
            phase: state::OpenCodeInvocationPhase::Terminal,
            literal_post_attempts: 2,
            lifecycle_result: Some(state::InvocationLifecycleResult::AcceptedButNotStarted),
            task_disposition: Some(state::TaskDisposition {
                server_abort: state::ServerAbortDisposition::Succeeded { aborted: true },
                local_task: state::LocalTaskDisposition::AbortedAndJoined,
            }),
            pending_interruption: Some(state::PendingInterruptionIdentity {
                kind: state::InterruptionKind::Permission,
                request_id: "secret-runtime-request".to_string(),
            }),
        });
        state.opencode.resolve_start_retries = 2;
        state.opencode.last_lifecycle_anomaly = Some(state::InvocationLifecycleAnomaly {
            invocation_id: "invocation-1".to_string(),
            observed_at: "2026-01-01T00:00:00Z".to_string(),
            kind: state::InvocationLifecycleAnomalyKind::AcceptedButNotStarted,
        });
        state.opencode.last_resolve_workflow_outcome =
            Some(state::ResolveWorkflowOutcome::RetriesExhausted);
        state.counters.ticket_to_pr = 1;
        state.counters.resolve_comments = 0;
        state.counters.resolve_ci = 2;
        state.ci.last_remediated_head_sha = Some("deadbeef".to_string());
        state.ci.last_remediated_fingerprint = Some("fingerprint-1".to_string());
        state.ci.grace_polls_remaining = 1;
        state.settings.opencode_dispatch_enabled = false;
        state.pr.last_lookup = Some(state::PrLookupDiagnostics {
            checked_at: "2026-01-01T00:00:00Z".to_string(),
            stage: state::StageKind::DispatchingTicketToPr,
            requested_branch: "feature/eng-992".to_string(),
            current_branch: Some("feature/eng-992".to_string()),
            repo_owner: "allisoneer".to_string(),
            repo_name: "agentic_auxilary".to_string(),
            token_source: Some("GH_TOKEN".to_string()),
            empty_result_reason: Some("no_open_pull_requests_matched_branch".to_string()),
            pr_number: None,
            pr_is_draft: None,
            outcome: "not_found".to_string(),
        });
        state.handoff.linear_comment_posted = false;
        state
    }

    #[test]
    fn compact_status_payload_preserves_existing_fields_and_adds_diagnostics() {
        let mut state = sample_state();
        state.worktree.path = std::env::temp_dir()
            .join(format!("missing-outer-dag-worktree-{}", std::process::id()))
            .display()
            .to_string();

        let payload = compact_status_payload(&state);

        for key in [
            "ticket",
            "branch",
            "worktree",
            "stage",
            "state_file",
            "stage_details",
            "last_error",
            "worktree_exists",
            "pr_number",
            "pr_url",
            "pr_is_draft",
            "pr_ready_for_review",
            "opencode_session_id",
            "opencode_last_command",
            "opencode_last_diagnostics",
            "opencode_invocation_phase",
            "opencode_invocation_command",
            "opencode_invocation_id",
            "opencode_invocation_session_id",
            "opencode_invocation_message_id",
            "opencode_invocation_lifecycle_result",
            "opencode_invocation_task_disposition",
            "opencode_invocation_post_attempts",
            "opencode_resolve_start_retries",
            "opencode_last_lifecycle_anomaly",
            "opencode_last_resolve_workflow_outcome",
            "ticket_to_pr_runs",
            "resolve_comments_runs",
            "resolve_ci_runs",
            "ci",
            "opencode_dispatch_enabled",
            "opencode_session_deadline_seconds",
            "opencode_inactivity_timeout_seconds",
            "linear_handoff_enabled",
            "linear_handoff_posted",
            "linear_handoff_posted_at",
            "pr_lookup",
            "run_id",
            "updated_at",
        ] {
            assert!(payload.get(key).is_some(), "missing key: {key}");
        }

        assert_eq!(
            payload.get("ticket"),
            Some(&Value::String("ENG-992".to_string()))
        );
        assert_eq!(
            payload.get("branch"),
            Some(&Value::String("feature/eng-992".to_string()))
        );
        assert_eq!(
            payload.get("state_file"),
            Some(&Value::String(format!(
                "./thoughts/{}/artifacts/{}",
                state.worktree.branch,
                state::STATE_FILENAME
            )))
        );
        assert_eq!(
            payload.get("stage"),
            Some(&Value::String("stopped_failed".to_string()))
        );
        assert_eq!(
            payload.get("stage_details"),
            Some(&Value::String("detailed failure".to_string()))
        );
        assert_eq!(
            payload.get("last_error"),
            Some(&Value::String("detailed failure".to_string()))
        );
        assert_eq!(payload.get("worktree_exists"), Some(&Value::Bool(false)));
        assert_eq!(
            payload.get("opencode_dispatch_enabled"),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            payload.get("linear_handoff_enabled"),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            payload.get("pr_number"),
            Some(&Value::Number(258_u64.into()))
        );
        assert_eq!(payload.get("pr_is_draft"), Some(&Value::Bool(false)));
        assert_eq!(
            payload.get("resolve_ci_runs"),
            Some(&Value::Number(2_u64.into()))
        );
        assert_eq!(
            payload
                .get("ci")
                .and_then(|ci| ci.get("last_remediated_head_sha")),
            Some(&Value::String("deadbeef".to_string()))
        );
        assert_eq!(
            payload
                .get("pr_lookup")
                .and_then(|lookup| lookup.get("repo_owner")),
            Some(&Value::String("allisoneer".to_string()))
        );
        assert_eq!(
            payload
                .get("opencode_last_diagnostics")
                .and_then(|diagnostics| diagnostics.get("guard_detected")),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            payload.get("opencode_invocation_phase"),
            Some(&Value::String("terminal".to_string()))
        );
        assert_eq!(
            payload.get("opencode_invocation_post_attempts"),
            Some(&Value::Number(2_u64.into()))
        );
        assert_eq!(
            payload.get("opencode_resolve_start_retries"),
            Some(&Value::Number(2_u64.into()))
        );
        assert!(payload.get("opencode_current_invocation").is_none());
        assert!(!payload.to_string().contains("secret-runtime-request"));
    }

    #[test]
    fn apply_settings_overrides_updates_resume_relevant_fields() {
        let mut state = sample_state();

        let changed = apply_settings_overrides(
            &mut state.settings,
            SettingsOverrides {
                linear_handoff_enabled: Some(false),
                opencode_dispatch_enabled: Some(false),
                poll_interval_seconds: Some(3),
                coderabbit_timeout_seconds: Some(90),
                opencode_session_deadline_seconds: Some(28_800),
                opencode_inactivity_timeout_seconds: Some(900),
            },
        )
        .expect("overrides should apply");

        assert!(changed);
        assert!(!state.settings.linear_handoff_enabled);
        assert!(!state.settings.opencode_dispatch_enabled);
        assert_eq!(state.settings.poll_interval_seconds, 3);
        assert_eq!(state.settings.coderabbit_timeout_seconds, 90);
        assert_eq!(state.settings.opencode_session_deadline_seconds, 28_800);
        assert_eq!(state.settings.opencode_inactivity_timeout_seconds, 900);
    }

    #[test]
    fn apply_settings_overrides_preserves_defaults_when_no_overrides_given() {
        let mut state = sample_state();
        let original = state.settings.clone();

        let changed = apply_settings_overrides(&mut state.settings, SettingsOverrides::default())
            .expect("empty overrides should succeed");

        assert!(!changed);
        assert_eq!(
            state.settings.poll_interval_seconds,
            original.poll_interval_seconds
        );
        assert_eq!(
            state.settings.coderabbit_timeout_seconds,
            original.coderabbit_timeout_seconds
        );
        assert_eq!(
            state.settings.opencode_session_deadline_seconds,
            original.opencode_session_deadline_seconds
        );
        assert_eq!(
            state.settings.opencode_inactivity_timeout_seconds,
            original.opencode_inactivity_timeout_seconds
        );
        assert_eq!(
            state.settings.linear_handoff_enabled,
            original.linear_handoff_enabled
        );
        assert_eq!(
            state.settings.opencode_dispatch_enabled,
            original.opencode_dispatch_enabled
        );
    }

    #[test]
    fn apply_settings_overrides_rejects_zero_poll_interval() {
        let mut state = sample_state();

        let err = apply_settings_overrides(
            &mut state.settings,
            SettingsOverrides {
                poll_interval_seconds: Some(0),
                ..SettingsOverrides::default()
            },
        )
        .expect_err("zero poll interval should fail");

        assert!(
            err.to_string()
                .contains("poll interval must be at least 1 second")
        );
    }

    #[test]
    fn apply_settings_overrides_rejects_zero_opencode_session_deadline() {
        let mut state = sample_state();

        let err = apply_settings_overrides(
            &mut state.settings,
            SettingsOverrides {
                opencode_session_deadline_seconds: Some(0),
                ..SettingsOverrides::default()
            },
        )
        .expect_err("zero deadline should fail");

        assert!(
            err.to_string()
                .contains("OpenCode session deadline must be at least 1 second")
        );
    }

    #[test]
    fn ensure_supported_dry_run_usage_rejects_mutating_non_start_commands() {
        for command in [
            cli::Commands::Resume {
                branch: None,
                worktree: None,
                no_linear_handoff: false,
                no_opencode_dispatch: false,
                stop_after: None,
                poll_interval_seconds: None,
                coderabbit_timeout_seconds: None,
                opencode_session_deadline_seconds: None,
                opencode_inactivity_timeout_seconds: None,
            },
            cli::Commands::RespondPermission {
                allow: true,
                deny: false,
            },
            cli::Commands::RespondQuestion {
                answer: "yes".to_string(),
            },
            cli::Commands::Handoff { message: None },
            cli::Commands::Reset { yes: true },
        ] {
            let err = ensure_supported_dry_run_usage(true, &command)
                .expect_err("mutating command should reject --dry-run");
            assert!(
                err.to_string()
                    .contains("--dry-run is only supported with `start`")
            );
        }
    }

    #[test]
    fn require_actionable_resume_stage_rejects_missing_and_terminal_stages() {
        let mut state = sample_state();
        state.opencode.resume_stage = None;
        let err =
            require_actionable_resume_stage(&state).expect_err("missing resume stage should fail");
        assert!(err.to_string().contains("missing resume_stage in state"));

        state.opencode.resume_stage = Some(state::StageKind::StoppedFailed);
        let err =
            require_actionable_resume_stage(&state).expect_err("terminal resume stage should fail");
        assert!(err.to_string().contains("invalid resume_stage in state"));
    }

    #[test]
    fn require_actionable_resume_stage_accepts_active_stage() {
        let mut state = sample_state();
        state.opencode.resume_stage = Some(state::StageKind::DispatchingResolvePrComments);
        assert_eq!(
            require_actionable_resume_stage(&state).unwrap(),
            state::StageKind::DispatchingResolvePrComments
        );
    }

    #[test]
    fn every_nonterminal_invocation_phase_requires_conservative_recovery() {
        for phase in [
            state::OpenCodeInvocationPhase::Prepared,
            state::OpenCodeInvocationPhase::PostAttempted,
            state::OpenCodeInvocationPhase::RunningAssistantStarted,
            state::OpenCodeInvocationPhase::PausedPermission,
            state::OpenCodeInvocationPhase::PausedQuestion,
        ] {
            let mut state = sample_state();
            state.opencode.current_invocation = Some(state::OpenCodeInvocationState {
                invocation_id: "inv-1".to_string(),
                command: "resolve_pr_comments".to_string(),
                session_id: "session-1".to_string(),
                command_message_id: "msg-1".to_string(),
                phase,
                literal_post_attempts: 1,
                lifecycle_result: None,
                task_disposition: None,
                pending_interruption: None,
            });
            assert!(has_recovered_nonterminal_invocation(&state));
        }

        let mut state = sample_state();
        state.opencode.current_invocation.as_mut().unwrap().phase =
            state::OpenCodeInvocationPhase::Terminal;
        assert!(!has_recovered_nonterminal_invocation(&state));
    }

    #[tokio::test]
    #[expect(
        clippy::await_holding_lock,
        reason = "tests intentionally serialize process-wide env/cwd mutation across async work"
    )]
    async fn resolve_start_effective_branch_prefers_explicit_branch_and_skips_linear() {
        let _guard = process_state_lock().lock().unwrap();
        let _linear_api_key = EnvVarGuard::remove("LINEAR_API_KEY");

        let branch = resolve_start_effective_branch("ENG-123", Some("feature/manual"), None)
            .await
            .unwrap();

        assert_eq!(branch, Some("feature/manual".to_string()));
    }

    #[tokio::test]
    #[expect(
        clippy::await_holding_lock,
        reason = "tests intentionally serialize process-wide env/cwd mutation across async work"
    )]
    async fn resolve_start_effective_branch_skips_linear_when_worktree_path_provided_without_branch()
     {
        let _guard = process_state_lock().lock().unwrap();
        let temp = TempDir::new().unwrap();
        let _linear_api_key = EnvVarGuard::remove("LINEAR_API_KEY");

        let branch = resolve_start_effective_branch("ENG-123", None, Some(temp.path()))
            .await
            .unwrap();

        assert_eq!(branch, None);
    }

    #[tokio::test]
    #[expect(
        clippy::await_holding_lock,
        reason = "tests intentionally serialize process-wide env/cwd mutation across async work"
    )]
    async fn start_dry_run_without_branch_requires_linear_and_fails_without_api_key() {
        let _guard = process_state_lock().lock().unwrap();
        let fixture = GitFixture::new().unwrap();
        let _cwd = CwdGuard::pushd(&fixture.repo).unwrap();
        let _linear_api_key = EnvVarGuard::remove("LINEAR_API_KEY");
        let _linear_url = EnvVarGuard::remove("LINEAR_GRAPHQL_URL");

        let err = handle_start(
            "ENG-123",
            StartOptions {
                branch: None,
                worktree_path: None,
                dry_run: true,
                force: false,
                no_linear_handoff: false,
                no_opencode_dispatch: false,
                stop_after: None,
                poll_interval_seconds: None,
                coderabbit_timeout_seconds: None,
                opencode_session_deadline_seconds: None,
                opencode_inactivity_timeout_seconds: None,
                final_only: false,
            },
        )
        .await
        .expect_err("branch-omitted dry-run should require Linear");

        let message = err.to_string();
        assert!(message.contains("--branch omitted"));
        assert!(message.contains("LINEAR_API_KEY"));
    }

    #[tokio::test]
    #[expect(
        clippy::await_holding_lock,
        reason = "tests intentionally serialize process-wide env/cwd mutation across async work"
    )]
    async fn start_dry_run_with_explicit_branch_does_not_require_linear() {
        let _guard = process_state_lock().lock().unwrap();
        let fixture = GitFixture::new().unwrap();
        let _cwd = CwdGuard::pushd(&fixture.repo).unwrap();
        let _linear_api_key = EnvVarGuard::remove("LINEAR_API_KEY");

        handle_start(
            "ENG-123",
            StartOptions {
                branch: Some("feature/manual"),
                worktree_path: None,
                dry_run: true,
                force: false,
                no_linear_handoff: false,
                no_opencode_dispatch: false,
                stop_after: None,
                poll_interval_seconds: None,
                coderabbit_timeout_seconds: None,
                opencode_session_deadline_seconds: None,
                opencode_inactivity_timeout_seconds: None,
                final_only: false,
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    #[expect(
        clippy::await_holding_lock,
        reason = "tests intentionally serialize process-wide env/cwd mutation across async work"
    )]
    async fn start_dry_run_with_worktree_path_does_not_require_linear() {
        let _guard = process_state_lock().lock().unwrap();
        let fixture = GitFixture::new().unwrap();
        let _linear_api_key = EnvVarGuard::remove("LINEAR_API_KEY");

        handle_start(
            "ENG-123",
            StartOptions {
                branch: None,
                worktree_path: Some(&fixture.repo),
                dry_run: true,
                force: false,
                no_linear_handoff: false,
                no_opencode_dispatch: false,
                stop_after: None,
                poll_interval_seconds: None,
                coderabbit_timeout_seconds: None,
                opencode_session_deadline_seconds: None,
                opencode_inactivity_timeout_seconds: None,
                final_only: false,
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    #[expect(
        clippy::await_holding_lock,
        reason = "tests intentionally serialize process-wide env/cwd mutation across async work"
    )]
    async fn resolve_start_effective_branch_auto_selects_from_linear_branch_name() {
        let _guard = process_state_lock().lock().unwrap();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issues": {
                        "nodes": [{
                            "id": "uuid-1",
                            "identifier": "ENG-123",
                            "branchName": "feature/eng-123"
                        }],
                        "pageInfo": {
                            "hasNextPage": false,
                            "endCursor": null
                        }
                    }
                }
            })))
            .mount(&server)
            .await;
        let _linear_api_key = EnvVarGuard::set("LINEAR_API_KEY", "test");
        let _linear_url =
            EnvVarGuard::set("LINEAR_GRAPHQL_URL", format!("{}/graphql", server.uri()));

        let branch = resolve_start_effective_branch("ENG-123", None, None)
            .await
            .unwrap();

        assert_eq!(branch, Some("feature/eng-123".to_string()));
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
    }

    #[tokio::test]
    #[expect(
        clippy::await_holding_lock,
        reason = "tests intentionally serialize process-wide env/cwd mutation across async work"
    )]
    async fn resolve_start_effective_branch_errors_on_main_or_master_from_linear() {
        let _guard = process_state_lock().lock().unwrap();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issues": {
                        "nodes": [{
                            "id": "uuid-1",
                            "identifier": "ENG-123",
                            "branchName": "main"
                        }],
                        "pageInfo": {
                            "hasNextPage": false,
                            "endCursor": null
                        }
                    }
                }
            })))
            .mount(&server)
            .await;
        let _linear_api_key = EnvVarGuard::set("LINEAR_API_KEY", "test");
        let _linear_url =
            EnvVarGuard::set("LINEAR_GRAPHQL_URL", format!("{}/graphql", server.uri()));

        let err = resolve_start_effective_branch("ENG-123", None, None)
            .await
            .expect_err("main branch names should be rejected");

        let message = err.to_string();
        assert!(message.contains("not allowed"));
        assert!(message.contains("pass --branch to override"));
    }

    #[tokio::test]
    #[expect(
        clippy::await_holding_lock,
        reason = "tests intentionally serialize process-wide env/cwd mutation across async work"
    )]
    async fn resolve_start_effective_branch_errors_when_linear_issue_not_found() {
        let _guard = process_state_lock().lock().unwrap();
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/graphql"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": {
                    "issues": {
                        "nodes": [],
                        "pageInfo": {
                            "hasNextPage": false,
                            "endCursor": null
                        }
                    }
                }
            })))
            .mount(&server)
            .await;
        let _linear_api_key = EnvVarGuard::set("LINEAR_API_KEY", "test");
        let _linear_url =
            EnvVarGuard::set("LINEAR_GRAPHQL_URL", format!("{}/graphql", server.uri()));

        let err = resolve_start_effective_branch("ENG-123", None, None)
            .await
            .expect_err("missing issue should error");

        let message = format!("{err:#}");
        assert!(message.contains("--branch omitted"));
        assert!(message.contains("not found"));
    }

    struct GitFixture {
        _temp: TempDir,
        repo: PathBuf,
    }

    impl GitFixture {
        fn new() -> Result<Self> {
            let temp = TempDir::new()?;
            let repo = temp.path().join("repo");

            run_git(temp.path(), ["init", repo.to_str().unwrap()])?;
            run_git(&repo, ["config", "user.name", "Test User"])?;
            run_git(&repo, ["config", "user.email", "test@example.com"])?;
            fs::write(repo.join("README.md"), "base\n")?;
            run_git(&repo, ["add", "README.md"])?;
            run_git(&repo, ["commit", "-m", "initial"])?;

            Ok(Self { _temp: temp, repo })
        }
    }
}
