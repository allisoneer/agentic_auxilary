#[cfg(not(unix))]
compile_error!("agentic-outer-dag only supports Unix-like platforms (Linux/macOS).");

use anyhow::Result;
use clap::Parser;
use serde_json::json;
use std::path::Path;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod cli;
mod dag;
mod github;
mod linear;
mod opencode;
mod preview;
mod state;
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
}

struct ResumeOptions<'a> {
    branch: Option<&'a str>,
    worktree_path: Option<&'a Path>,
    no_linear_handoff: bool,
    no_opencode_dispatch: bool,
    stop_after: Option<state::StageKind>,
    poll_interval_seconds: Option<u64>,
    coderabbit_timeout_seconds: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default)]
struct SettingsOverrides {
    linear_handoff_enabled: Option<bool>,
    opencode_dispatch_enabled: Option<bool>,
    poll_interval_seconds: Option<u64>,
    coderabbit_timeout_seconds: Option<u64>,
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
    let command = cli.command;

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
        } => {
            handle_resume(ResumeOptions {
                branch: branch.as_deref(),
                worktree_path: worktree.as_deref(),
                no_linear_handoff,
                no_opencode_dispatch,
                stop_after,
                poll_interval_seconds,
                coderabbit_timeout_seconds,
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

    if let Some(linear_handoff_enabled) = overrides.linear_handoff_enabled {
        settings.linear_handoff_enabled = linear_handoff_enabled;
    }

    if let Some(opencode_dispatch_enabled) = overrides.opencode_dispatch_enabled {
        settings.opencode_dispatch_enabled = opencode_dispatch_enabled;
    }

    Ok(overrides.poll_interval_seconds.is_some()
        || overrides.coderabbit_timeout_seconds.is_some()
        || overrides.linear_handoff_enabled.is_some()
        || overrides.opencode_dispatch_enabled.is_some())
}

async fn handle_start(ticket: &str, options: StartOptions<'_>) -> Result<()> {
    if options.dry_run {
        let plan = preview::build_dry_run_start_preview(
            ticket,
            options.branch,
            options.worktree_path,
            options.force,
        )?;
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }

    let target = worktree::resolve(options.branch, options.worktree_path, true)?;
    worktree::chdir_to(&target)?;

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
        },
    )?;
    state::store::ThoughtsStateStore::save(&state)?;

    let mut engine = dag::engine::DagEngine::for_current_dir()?;
    engine.run_until_stop(options.stop_after).await?;
    let state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("persisted state disappeared after start"))?;
    print_status(&state, false)
}

async fn handle_resume(options: ResumeOptions<'_>) -> Result<()> {
    let target = worktree::resolve(options.branch, options.worktree_path, false)?;
    worktree::chdir_to(&target)?;

    let mut state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("no persisted state found; run start first"))?;
    let settings_changed = apply_settings_overrides(
        &mut state.settings,
        SettingsOverrides {
            linear_handoff_enabled: options.no_linear_handoff.then_some(false),
            opencode_dispatch_enabled: options.no_opencode_dispatch.then_some(false),
            poll_interval_seconds: options.poll_interval_seconds,
            coderabbit_timeout_seconds: options.coderabbit_timeout_seconds,
        },
    )?;
    if settings_changed {
        state::store::ThoughtsStateStore::save(&state)?;
    }
    let mut engine = dag::engine::DagEngine::for_current_dir()?;
    engine.run_until_stop(options.stop_after).await?;
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
        "opencode_session_id": state.opencode.active_session_id,
        "opencode_last_command": state.opencode.last_command,
        "opencode_last_diagnostics": state.opencode.last_diagnostics,
        "ticket_to_pr_runs": state.counters.ticket_to_pr_runs,
        "resolve_comments_runs": state.counters.resolve_comments_runs,
        "opencode_dispatch_enabled": state.settings.opencode_dispatch_enabled,
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
    let mut state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("no persisted state found in the current worktree"))?;
    anyhow::ensure!(
        matches!(
            state.stage.kind,
            state::StageKind::StoppedPermissionRequired
        ),
        "current state is not waiting on a permission response"
    );
    let pending = state
        .opencode
        .pending_permission
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no pending permission payload found in state"))?;
    let session_id = state
        .opencode
        .active_session_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no active OpenCode session found in state"))?;

    let supervisor =
        opencode::supervisor::OpenCodeSupervisor::start(std::path::Path::new(".")).await?;
    supervisor
        .respond_permission(&session_id, &pending.request_id, allow)
        .await?;

    state.opencode.pending_permission = None;
    state.stage.kind = state
        .opencode
        .resume_stage
        .clone()
        .unwrap_or(state::StageKind::DispatchingTicketToPr);
    state.stage.details = None;
    state::store::ThoughtsStateStore::save(&state)?;

    let mut engine = dag::engine::DagEngine::for_current_dir()?;
    engine.run_until_stop(None).await?;
    let state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("persisted state disappeared after responding"))?;
    print_status(&state, false)
}

async fn handle_respond_question(answer: &str) -> Result<()> {
    let mut state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("no persisted state found in the current worktree"))?;
    anyhow::ensure!(
        matches!(state.stage.kind, state::StageKind::StoppedQuestionRequired),
        "current state is not waiting on a question response"
    );
    let pending = state
        .opencode
        .pending_question
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no pending question payload found in state"))?;
    let session_id = state
        .opencode
        .active_session_id
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no active OpenCode session found in state"))?;

    let supervisor =
        opencode::supervisor::OpenCodeSupervisor::start(std::path::Path::new(".")).await?;
    supervisor
        .respond_question(&session_id, &pending.request_id, answer)
        .await?;

    state.opencode.pending_question = None;
    state.stage.kind = state
        .opencode
        .resume_stage
        .clone()
        .unwrap_or(state::StageKind::DispatchingTicketToPr);
    state.stage.details = None;
    state::store::ThoughtsStateStore::save(&state)?;

    let mut engine = dag::engine::DagEngine::for_current_dir()?;
    engine.run_until_stop(None).await?;
    let state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("persisted state disappeared after responding"))?;
    print_status(&state, false)
}

async fn handle_handoff(message: Option<&str>) -> Result<()> {
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
    state::store::ThoughtsStateStore::delete()?;
    println!("state reset");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::SettingsOverrides;
    use super::apply_settings_overrides;
    use super::compact_status_payload;
    use super::state;
    use crate::worktree::TargetWorktree;
    use serde_json::Value;

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
        state.opencode.active_session_id = Some("session-123".to_string());
        state.opencode.last_command = Some("linear_ticket_2_pr".to_string());
        state.opencode.last_diagnostics = Some(state::OpenCodeDiagnostics {
            checked_at: "2026-01-01T00:00:00Z".to_string(),
            command_message_id: Some("msg-outer-dag-1".to_string()),
            final_assistant_message_id: Some("msg-assistant-1".to_string()),
            final_finish_reason: Some("stop".to_string()),
            guard_detected: true,
            final_tool_error: Some(state::OpenCodeToolErrorDiagnostics {
                tool: "read".to_string(),
                error: "nested guard tripped".to_string(),
            }),
        });
        state.counters.ticket_to_pr_runs = 1;
        state.counters.resolve_comments_runs = 0;
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
            "opencode_session_id",
            "opencode_last_command",
            "opencode_last_diagnostics",
            "ticket_to_pr_runs",
            "resolve_comments_runs",
            "opencode_dispatch_enabled",
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
            },
        )
        .expect("overrides should apply");

        assert!(changed);
        assert!(!state.settings.linear_handoff_enabled);
        assert!(!state.settings.opencode_dispatch_enabled);
        assert_eq!(state.settings.poll_interval_seconds, 3);
        assert_eq!(state.settings.coderabbit_timeout_seconds, 90);
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
}
