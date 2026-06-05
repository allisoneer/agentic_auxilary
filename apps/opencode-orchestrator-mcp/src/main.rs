//! MCP server for orchestrator-style agents to spawn and manage `OpenCode` sessions.
//!
//! This binary exposes seven tools for orchestrator agents:
//! - `run` - start or resume an `OpenCode` session
//! - `list_sessions` - list existing sessions
//! - `get_session_state` - inspect a session's status, tool calls, and pending work
//! - `list_agents` - discover available visible `OpenCode` agents
//! - `list_commands` - discover available `OpenCode` commands
//! - `respond_permission` - reply to permission requests
//! - `respond_question` - reply to question requests

use agentic_tools_mcp::OutputMode;
use agentic_tools_mcp::RegistryServer;
use agentic_tools_mcp::ServiceExt;
use agentic_tools_mcp::stdio;
use std::sync::Arc;

mod config;
mod error;
mod logging;
mod server;
mod token_tracker;
mod tools;
mod types;
mod version;

use server::OrchestratorServerHandle;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(feature = "test-support")]
    {
        let _ = OrchestratorServerHandle::from_server_unshared;
        std::mem::drop(
            OrchestratorServerHandle::new().acquire_or_recover_with(|| async {
                anyhow::bail!("test-support lint reference only")
            }),
        );
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    // Install the rustls CryptoProvider before any HTTP clients are created.
    // Required because Cargo's additive features cause both ring and aws-lc-rs
    // to be compiled in via transitive dependencies, and rustls 0.23+ panics
    // if it can't auto-select a single provider.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    // Lazy initialization: server is spawned on first tool call, not at startup.
    // This saves resources when orchestrator tools are never invoked (~90% of cases).
    let orchestrator = Arc::new(OrchestratorServerHandle::new());
    let registry = tools::build_registry(&orchestrator);

    eprintln!(
        "opencode-orchestrator-mcp started ({} tools; embedded server is lazy-initialized on first tool call)",
        registry.len(),
    );

    let server = RegistryServer::new(Arc::new(registry))
        .with_info("opencode-orchestrator-mcp", env!("CARGO_PKG_VERSION"))
        .with_output_mode(OutputMode::Structured);

    let transport = stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}
