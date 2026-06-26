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
        } => {
            handle_start(
                &ticket,
                branch.as_deref(),
                worktree.as_deref(),
                dry_run,
                force,
            )
            .await
        }
        cli::Commands::Resume { branch, worktree } => {
            handle_resume(branch.as_deref(), worktree.as_deref()).await
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

async fn handle_start(
    ticket: &str,
    branch: Option<&str>,
    worktree_path: Option<&Path>,
    dry_run: bool,
    force: bool,
) -> Result<()> {
    if dry_run {
        let plan = preview::build_dry_run_start_preview(ticket, branch, worktree_path, force)?;
        println!("{}", serde_json::to_string_pretty(&plan)?);
        return Ok(());
    }

    let target = worktree::resolve(branch, worktree_path, true)?;
    worktree::chdir_to(&target)?;

    if state::store::ThoughtsStateStore::load()?.is_some() && !force {
        anyhow::bail!(
            "state file already exists for branch '{}'; rerun with --force to overwrite",
            target.branch
        );
    }

    let state = state::RunState::for_start(ticket, &target, dry_run)?;
    state::store::ThoughtsStateStore::save(&state)?;

    let engine = dag::engine::DagEngine::for_current_dir().await?;
    engine.run_until_stop().await?;
    let state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("persisted state disappeared after start"))?;
    print_status(&state, false)
}

async fn handle_resume(branch: Option<&str>, worktree_path: Option<&Path>) -> Result<()> {
    let target = worktree::resolve(branch, worktree_path, false)?;
    worktree::chdir_to(&target)?;

    state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("no persisted state found; run start first"))?;
    let engine = dag::engine::DagEngine::for_current_dir().await?;
    engine.run_until_stop().await?;
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
            serde_json::to_string_pretty(&json!({
                "ticket": state.ticket.linear_key,
                "branch": state.worktree.branch,
                "worktree": state.worktree.path,
                "stage": state.stage.kind,
                "state_file": format!("./thoughts/{}/artifacts/{}", state.worktree.branch, state::STATE_FILENAME),
            }))?
        );
    }

    Ok(())
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

    let engine = dag::engine::DagEngine::for_current_dir().await?;
    engine.run_until_stop().await?;
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

    let engine = dag::engine::DagEngine::for_current_dir().await?;
    engine.run_until_stop().await?;
    let state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("persisted state disappeared after responding"))?;
    print_status(&state, false)
}

async fn handle_handoff(message: Option<&str>) -> Result<()> {
    let mut state = state::store::ThoughtsStateStore::load()?
        .ok_or_else(|| anyhow::anyhow!("no persisted state found in the current worktree"))?;
    let body = message.unwrap_or("manual handoff requested from agentic-outer-dag");
    linear::post_handoff_once(&mut state, body).await?;
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
