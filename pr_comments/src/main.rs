use anyhow::Result;
use clap::{Parser, Subcommand};
use pr_comments::{PrComments, PrCommentsServer, models::CommentSourceType};
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
    /// Get review comments (thread-paginated)
    Comments {
        /// PR number (auto-detected if not provided)
        #[arg(long)]
        pr: Option<u64>,
        /// Filter by comment source: robot, human, or all
        #[arg(long, value_parser = ["robot", "human", "all"])]
        comment_source_type: Option<String>,
        /// Include resolved review comments (defaults to false)
        #[arg(long)]
        include_resolved: bool,
    },
    /// Reply to a review comment
    Reply {
        /// PR number (auto-detected if not provided)
        #[arg(long)]
        pr: Option<u64>,
        /// ID of the comment to reply to
        #[arg(long)]
        comment_id: u64,
        /// Reply message body
        #[arg(long)]
        body: String,
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
    let args = Args::parse();

    // Detect MCP mode before initializing tracing
    let is_mcp = matches!(args.command, Commands::Mcp);

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "pr_comments=info".into());

    let fmt_layer = tracing_subscriber::fmt::layer();

    if is_mcp {
        // Route all tracing output to stderr in MCP mode to keep stdout clean for JSON-RPC
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer.with_writer(std::io::stderr))
            .init();
    } else {
        // Preserve existing behavior for CLI
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    }

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
        Commands::Comments {
            pr,
            comment_source_type,
            include_resolved,
        } => {
            // Parse comment_source_type string to enum
            let source_type = match comment_source_type.as_deref() {
                Some("robot") => Some(CommentSourceType::Robot),
                Some("human") => Some(CommentSourceType::Human),
                Some("all") => Some(CommentSourceType::All),
                None => None,
                Some(other) => {
                    eprintln!(
                        "Error: Invalid comment_source_type '{}'. Must be robot, human, or all.",
                        other
                    );
                    std::process::exit(1);
                }
            };
            match tool
                .get_comments(pr, source_type, Some(include_resolved))
                .await
            {
                Ok(list) => println!("{}", serde_json::to_string_pretty(&list.comments)?),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Reply {
            pr,
            comment_id,
            body,
        } => match tool.add_comment_reply(pr, comment_id, body).await {
            Ok(comment) => println!("{}", serde_json::to_string_pretty(&comment)?),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
        Commands::ListPrs { state } => match tool.list_prs(Some(state)).await {
            Ok(list) => println!("{}", serde_json::to_string_pretty(&list.prs)?),
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
