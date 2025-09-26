use anyhow::Result;
use clap::{Parser, Subcommand};
use pr_comments::{PrComments, PrCommentsServer};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use universal_tool_core::mcp::ServiceExt;

#[derive(Parser)]
#[command(name = "pr-comments")]
#[command(about = "Fetch GitHub PR comments via CLI or MCP", long_about = None)]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Commands,

    /// Repository in owner/repo format (auto-detected if not provided)
    #[arg(long, value_name = "OWNER/REPO", global = true)]
    repo: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Get all comments for a PR
    All {
        /// PR number (auto-detected if not provided)
        #[arg(long)]
        pr: Option<u64>,
    },
    /// Get review comments (code comments) for a PR
    ReviewComments {
        /// PR number (auto-detected if not provided)
        #[arg(long)]
        pr: Option<u64>,
        /// Include resolved review comments (defaults to false)
        #[arg(long)]
        include_resolved: bool,
    },
    /// Get issue comments (discussion) for a PR
    IssueComments {
        /// PR number (auto-detected if not provided)
        #[arg(long)]
        pr: Option<u64>,
    },
    /// List pull requests in the repository
    ListPrs {
        /// PR state filter: open, closed, or all
        #[arg(long, default_value = "open")]
        state: String,
    },
    /// Run as MCP server
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

    match args.command {
        Commands::Mcp => run_mcp_server(args.repo).await,
        _ => run_cli(args).await,
    }
}

async fn run_cli(args: Args) -> Result<()> {
    // Create the tool instance
    let tool = if let Some(repo_spec) = args.repo {
        let parts: Vec<&str> = repo_spec.split('/').collect();
        if parts.len() != 2 {
            anyhow::bail!("Repository must be in owner/repo format");
        }
        PrComments::with_repo(parts[0].to_string(), parts[1].to_string())
    } else {
        PrComments::new()?
    };

    // Execute the appropriate command
    match args.command {
        Commands::All { pr } => match tool.get_all_comments(pr).await {
            Ok(comments) => println!("{}", serde_json::to_string_pretty(&comments)?),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
        Commands::ReviewComments {
            pr,
            include_resolved,
        } => match tool.get_review_comments(pr, Some(include_resolved)).await {
            Ok(comments) => println!("{}", serde_json::to_string_pretty(&comments)?),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
        Commands::IssueComments { pr } => match tool.get_issue_comments(pr).await {
            Ok(comments) => println!("{}", serde_json::to_string_pretty(&comments)?),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
        Commands::ListPrs { state } => match tool.list_prs(Some(state)).await {
            Ok(prs) => println!("{}", serde_json::to_string_pretty(&prs)?),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
        Commands::Mcp => unreachable!("MCP command should be handled in main"),
    }

    Ok(())
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
                eprintln!(
                    "Warning: Not in a git repository. Repository must be specified with --repo"
                );
                eprintln!("MCP clients will need to provide repository information");
                // Create a dummy instance for MCP - clients will need to specify repo
                PrComments::with_repo("".to_string(), "".to_string())
            }
        }
    };

    let server = PrCommentsServer::new(Arc::new(tool));
    let transport = universal_tool_core::mcp::stdio();

    // The serve method will run until the client disconnects
    let service = match server.serve(transport).await {
        Ok(service) => service,
        Err(e) => {
            eprintln!("MCP server error: {}", e);

            // Add targeted hints for common handshake issues
            let msg = format!("{e}");
            if msg.contains("ExpectedInitializeRequest")
                || msg.contains("expect initialized request")
            {
                eprintln!("Hint: Client must send 'initialize' request first.");
            }
            if msg.contains("ExpectedInitializedNotification")
                || msg.contains("initialize notification")
            {
                eprintln!(
                    "Hint: Client must send 'notifications/initialized' after receiving InitializeResult."
                );
            }
            return Err(anyhow::anyhow!("MCP server failed: {}", e));
        }
    };

    // Critical: Wait for the service to complete
    service.waiting().await?;

    // The server has stopped (client disconnected)
    eprintln!("MCP server stopped");
    Ok(())
}
