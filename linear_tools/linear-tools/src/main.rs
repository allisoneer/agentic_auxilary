use anyhow::Result;
use clap::{Parser, Subcommand};
use linear_tools::LinearTools;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use universal_tool_core::mcp::ServiceExt;

#[derive(Parser)]
#[command(name = "linear-tools")]
#[command(about = "Linear issue management tools via CLI or MCP")]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Search issues
    Search {
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        priority: Option<i32>,
        #[arg(long)]
        state_id: Option<String>,
        #[arg(long)]
        assignee_id: Option<String>,
        #[arg(long)]
        team_id: Option<String>,
        #[arg(long)]
        project_id: Option<String>,
        #[arg(long)]
        created_after: Option<String>,
        #[arg(long)]
        created_before: Option<String>,
        #[arg(long)]
        updated_after: Option<String>,
        #[arg(long)]
        updated_before: Option<String>,
        #[arg(long)]
        first: Option<i32>,
        #[arg(long)]
        after: Option<String>,
    },
    /// Read a single issue
    Read {
        #[arg(long)]
        issue: String,
    },
    /// Create a new issue
    Create {
        #[arg(long)]
        team_id: String,
        #[arg(long)]
        title: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long)]
        priority: Option<i32>,
        #[arg(long)]
        assignee_id: Option<String>,
        #[arg(long)]
        project_id: Option<String>,
        #[arg(long)]
        state_id: Option<String>,
        #[arg(long)]
        parent_id: Option<String>,
        #[arg(long = "label-id")]
        label_ids: Vec<String>,
    },
    /// Add a comment to an issue
    Comment {
        #[arg(long)]
        issue: String,
        #[arg(long)]
        body: String,
        #[arg(long)]
        parent_id: Option<String>,
    },
    /// Start MCP server
    Mcp,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let is_mcp = matches!(args.command, Commands::Mcp);

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "linear_tools=info".into());
    let fmt_layer = tracing_subscriber::fmt::layer();

    if is_mcp {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer.with_writer(std::io::stderr))
            .init();
    } else {
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
    }

    match args.command {
        Commands::Mcp => run_mcp_server().await,
        _ => run_cli(args).await,
    }
}

async fn run_cli(args: Args) -> Result<()> {
    let tool = LinearTools::new();
    match args.command {
        Commands::Search {
            query,
            priority,
            state_id,
            assignee_id,
            team_id,
            project_id,
            created_after,
            created_before,
            updated_after,
            updated_before,
            first,
            after,
        } => {
            match tool
                .search_issues(
                    query,
                    priority,
                    state_id,
                    assignee_id,
                    team_id,
                    project_id,
                    created_after,
                    created_before,
                    updated_after,
                    updated_before,
                    first,
                    after,
                )
                .await
            {
                Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Read { issue } => match tool.read_issue(issue).await {
            Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
        Commands::Create {
            team_id,
            title,
            description,
            priority,
            assignee_id,
            project_id,
            state_id,
            parent_id,
            label_ids,
        } => {
            match tool
                .create_issue(
                    team_id,
                    title,
                    description,
                    priority,
                    assignee_id,
                    project_id,
                    state_id,
                    parent_id,
                    label_ids,
                )
                .await
            {
                Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Comment {
            issue,
            body,
            parent_id,
        } => match tool.add_comment(issue, body, parent_id).await {
            Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        },
        Commands::Mcp => unreachable!(),
    }
    Ok(())
}

async fn run_mcp_server() -> Result<()> {
    eprintln!("Starting Linear Tools MCP Server...");
    let server = linear_tools::LinearToolsServer::new(Arc::new(LinearTools::new()));
    let transport = universal_tool_core::mcp::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;
    eprintln!("MCP server stopped");
    Ok(())
}
