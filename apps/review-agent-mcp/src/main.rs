//! MCP server for spawning lens-specific Opus code reviewers.
//!
//! This binary exposes a single tool:
//! - `spawn` - spawn a lens-specific Opus reviewer over `./review.diff`

use agentic_tools_mcp::{OutputMode, RegistryServer, ServiceExt, stdio};
use std::sync::Arc;

mod prompts;
mod tools;
mod types;
mod validation;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    let registry = tools::build_registry();

    eprintln!("review-agent-mcp started ({} tools)", registry.len(),);

    let server = RegistryServer::new(Arc::new(registry))
        .with_info("review-agent-mcp", env!("CARGO_PKG_VERSION"))
        .with_output_mode(OutputMode::Structured);

    let transport = stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    Ok(())
}
