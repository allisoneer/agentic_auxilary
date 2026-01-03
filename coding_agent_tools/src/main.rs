use anyhow::Result;
use clap::{Parser, Subcommand};
use coding_agent_tools::{CodingAgentTools, CodingAgentToolsServer};
use std::collections::HashSet;
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
        #[arg(long, value_parser = clap::value_parser!(u8).range(0..=10))]
        depth: Option<u8>,
        #[arg(long, value_parser = ["all", "files", "dirs"])]
        show: Option<String>,
        #[arg(long)]
        ignore: Vec<String>,
        #[arg(long)]
        hidden: bool,
    },
    /// Run as MCP server
    Mcp {
        /// Expose the 'ls' tool (if any tool flags set, only flagged tools are exposed)
        #[arg(long)]
        ls: bool,
        /// Expose the 'spawn_agent' tool
        #[arg(long)]
        spawn_agent: bool,
        /// Expose the 'search_grep' tool
        #[arg(long)]
        search_grep: bool,
        /// Expose the 'search_glob' tool
        #[arg(long)]
        search_glob: bool,
        /// Expose the 'just_search' (just recipes) tool
        #[arg(long)]
        just_search: bool,
        /// Expose the 'just_execute' (just recipes) tool
        #[arg(long)]
        just_execute: bool,
    },
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
        Commands::Mcp {
            ls,
            spawn_agent,
            search_grep,
            search_glob,
            just_search,
            just_execute,
        } => {
            run_mcp_server(
                ls,
                spawn_agent,
                search_grep,
                search_glob,
                just_search,
                just_execute,
            )
            .await
        }
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

    let depth = depth
        .map(Depth::new)
        .transpose()
        .map_err(anyhow::Error::msg)?;
    let show = show
        .map(|s| s.parse::<Show>())
        .transpose()
        .map_err(anyhow::Error::msg)?;
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

async fn run_mcp_server(
    ls: bool,
    spawn_agent: bool,
    search_grep: bool,
    search_glob: bool,
    just_search: bool,
    just_execute: bool,
) -> Result<()> {
    eprintln!("Starting coding_agent_tools MCP Server");

    // Backwards-compat: no flags => expose all tools (allowlist = None)
    let allowlist =
        if ls || spawn_agent || search_grep || search_glob || just_search || just_execute {
            let mut set = HashSet::new();
            if ls {
                set.insert("ls".to_string());
            }
            if spawn_agent {
                set.insert("spawn_agent".to_string());
            }
            if search_grep {
                set.insert("search_grep".to_string());
            }
            if search_glob {
                set.insert("search_glob".to_string());
            }
            if just_search {
                set.insert("just_search".to_string());
            }
            if just_execute {
                set.insert("just_execute".to_string());
            }
            Some(set)
        } else {
            None
        };

    // Same instance for all MCP calls = pagination state persists
    let server =
        CodingAgentToolsServer::with_allowlist(Arc::new(CodingAgentTools::new()), allowlist);
    let transport = universal_tool_core::mcp::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;

    eprintln!("MCP server stopped");
    Ok(())
}
