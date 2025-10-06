use clap::{Parser, Subcommand};
use gpt5_reasoner::{DirectoryMeta, FileMeta, Gpt5Reasoner, PromptType, gpt5_reasoner_impl};
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use universal_tool_core::mcp::ServiceExt;

#[derive(Parser)]
#[command(
    name = "gpt5_reasoner",
    version,
    about = "GPT-5 prompt optimizer and executor"
)]
struct Args {
    #[command(subcommand)]
    command: Commands,

    /// Load .env from current directory
    #[arg(long = "dot-env", global = true)]
    dot_env: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the optimizer and executor
    Run {
        /// Prompt type: reasoning | plan
        #[arg(long)]
        prompt_type: PromptType,

        /// Path to text file containing the user prompt (exclusive with --prompt)
        #[arg(long, conflicts_with = "prompt")]
        prompt_file: Option<String>,

        /// Raw prompt string (exclusive with --prompt-file)
        #[arg(long, conflicts_with = "prompt_file")]
        prompt: Option<String>,

        /// Path to JSON file with array of {filename, description}
        #[arg(long)]
        files_json: String,

        /// Optional path to JSON file with array of DirectoryMeta
        /// Example: [{ "directory_path": "src", "description": "source", "extensions": ["rs"], "recursive": true, "include_hidden": false }]
        #[arg(long)]
        directories_json: Option<String>,
    },
    /// Run as MCP server
    Mcp,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gpt5_reasoner=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    // Handle .env loading globally
    if args.dot_env {
        let _ = dotenvy::dotenv(); // ignore errors
    }

    match args.command {
        Commands::Mcp => {
            // MCP mode
            let server = Gpt5ReasonerServer {
                tools: Arc::new(Gpt5Reasoner),
            };
            let transport = universal_tool_core::mcp::stdio();
            let svc = server.serve(transport).await?;
            svc.waiting().await?;
            Ok(())
        }
        Commands::Run {
            prompt_type,
            prompt_file,
            prompt,
            files_json,
            directories_json,
        } => {
            // CLI mode
            let prompt = match (&prompt, &prompt_file) {
                (Some(p), None) => p.clone(),
                (None, Some(path)) => std::fs::read_to_string(path)?,
                (None, None) => {
                    eprintln!("Error: Must provide either --prompt or --prompt-file");
                    eprintln!("\nFor more information, try '--help'");
                    std::process::exit(2);
                }
                (Some(_), Some(_)) => unreachable!("clap handles conflicts"),
            };

            let files: Vec<FileMeta> =
                serde_json::from_str(&std::fs::read_to_string(&files_json)?)?;

            let directories: Option<Vec<DirectoryMeta>> = match directories_json {
                Some(path) => {
                    let txt = std::fs::read_to_string(path)?;
                    let dirs: Vec<DirectoryMeta> = serde_json::from_str(&txt)?;
                    Some(dirs)
                }
                None => None,
            };

            let result = gpt5_reasoner_impl(prompt, files, directories, None, prompt_type).await;

            match result {
                Ok(out) => {
                    println!("{}", out);
                    Ok(())
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }
}

// MCP server wrapper
pub struct Gpt5ReasonerServer {
    tools: Arc<Gpt5Reasoner>,
}
universal_tool_core::implement_mcp_server!(Gpt5ReasonerServer, tools);
