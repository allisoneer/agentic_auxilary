use anyhow::Result;
use clap::{Parser, Subcommand};
use coding_agent_tools::{CodingAgentTools, CodingAgentToolsServer};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use universal_tool_core::mcp::ServiceExt;

#[derive(Parser)]
#[command(name = "coding-agent-tools")]
#[command(about = "Coding agent tools via CLI or MCP", long_about = None)]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List files and directories
    Ls {
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        depth: Option<u8>,
        #[arg(long, value_parser = ["all", "files", "dirs"])]
        show: Option<String>,
        #[arg(long)]
        ignore: Vec<String>,
        #[arg(long)]
        hidden: bool,
    },
    /// Run as MCP server
    Mcp,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "coding_agent_tools=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    match args.command {
        Commands::Mcp => run_mcp_server().await,
        Commands::Ls {
            path,
            depth,
            show,
            ignore,
            hidden,
        } => run_cli_ls(path, depth, show, ignore, hidden).await,
    }
}

async fn run_cli_ls(
    path: Option<String>,
    depth: Option<u8>,
    show: Option<String>,
    ignore: Vec<String>,
    hidden: bool,
) -> Result<()> {
    use coding_agent_tools::types::{Depth, Show};

    // Fresh instance each CLI invocation = no pagination state
    let tools = CodingAgentTools::new();

    let depth = depth.and_then(|d| Depth::new(d).ok());
    let show = show.and_then(|s| s.parse::<Show>().ok());
    let ignore = if ignore.is_empty() {
        None
    } else {
        Some(ignore)
    };

    let out = tools.ls(path, depth, show, ignore, Some(hidden)).await?;

    // CLI prints JSON
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

async fn run_mcp_server() -> Result<()> {
    eprintln!("Starting coding_agent_tools MCP Server");

    // Same instance for all MCP calls = pagination state persists
    let server = CodingAgentToolsServer::new(Arc::new(CodingAgentTools::new()));
    let transport = universal_tool_core::mcp::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;

    eprintln!("MCP server stopped");
    Ok(())
}
