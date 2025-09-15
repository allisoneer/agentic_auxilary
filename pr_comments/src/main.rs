use anyhow::Result;
use clap::{Parser, Subcommand};
use pr_comments::{PrComments, PrCommentsServer};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use universal_tool_core::mcp::ServiceExt;

#[derive(Parser)]
#[command(name = "pr-comments")]
#[command(about = "Fetch GitHub PR comments via CLI or MCP", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Run as MCP server
    #[arg(long)]
    mcp: bool,

    /// Repository in owner/repo format (auto-detected if not provided)
    #[arg(long, value_name = "OWNER/REPO")]
    repo: Option<String>,

    /// CLI subcommand and arguments
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run as MCP server (same as --mcp)
    Mcp,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pr_comments=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    // Determine if we should run MCP mode
    let run_mcp = args.mcp || matches!(args.command, Some(Commands::Mcp));

    if run_mcp {
        run_mcp_server(args.repo).await
    } else {
        // Build CLI args to forward to UTF
        let cli_args: Vec<String> = if args.args.is_empty() {
            // No trailing args captured - use full env args
            std::env::args().collect()
        } else {
            // Trailing args captured - prepend program name
            let mut cli_args = vec![env!("CARGO_PKG_NAME").to_string()];
            cli_args.extend(args.args);
            cli_args
        };

        run_cli(args.repo, cli_args).await
    }
}

async fn run_cli(repo: Option<String>, cli_args: Vec<String>) -> Result<()> {
    // Create the tool instance
    let tool = if let Some(repo_spec) = repo {
        let parts: Vec<&str> = repo_spec.split('/').collect();
        if parts.len() != 2 {
            anyhow::bail!("Repository must be in owner/repo format");
        }
        PrComments::with_repo(parts[0].to_string(), parts[1].to_string())
    } else {
        PrComments::new()?
    };

    // Create CLI app
    let app = tool.create_cli_command()
        .about("Fetch GitHub PR comments")
        .version(env!("CARGO_PKG_VERSION"))
        .arg_required_else_help(true);

    // Parse and execute with forwarded args
    let matches = app.try_get_matches_from(cli_args)
        .unwrap_or_else(|e| e.exit());

    match tool.execute_cli(matches).await {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn run_mcp_server(repo: Option<String>) -> Result<()> {
    eprintln!("🚀 Starting PR Comments MCP Server");

    // Create the tool instance
    let tool = if let Some(repo_spec) = repo {
        let parts: Vec<&str> = repo_spec.split('/').collect();
        if parts.len() != 2 {
            anyhow::bail!("Repository must be in owner/repo format");
        }
        PrComments::with_repo(parts[0].to_string(), parts[1].to_string())
    } else {
        match PrComments::new() {
            Ok(tool) => tool,
            Err(_) => {
                eprintln!("Warning: Not in a git repository. Repository must be specified with --repo");
                eprintln!("MCP clients will need to provide repository information");
                // Create a dummy instance for MCP - clients will need to specify repo
                PrComments::with_repo("".to_string(), "".to_string())
            }
        }
    };

    let server = PrCommentsServer::new(Arc::new(tool));
    let transport = universal_tool_core::mcp::stdio();

    // The serve method will run until the client disconnects
    let _running = server.serve(transport).await.map_err(|e| {
        eprintln!("MCP server error: {}", e);
        anyhow::anyhow!("MCP server failed: {}", e)
    })?;

    // The server has stopped (client disconnected)
    eprintln!("MCP server stopped");
    Ok(())
}