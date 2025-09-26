use clap::Parser;
use gpt5_reasoner::{FileMeta, Gpt5Reasoner, PromptType, gpt5_reasoner_impl};
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
    /// Run in MCP server mode
    #[arg(long)]
    mcp: bool,

    /// Load .env from current directory
    #[arg(long = "dot-env")]
    dot_env: bool,

    /// Prompt type: reasoning | plan
    #[arg(long)]
    prompt_type: PromptType,

    /// Path to text file containing the user prompt (exclusive with --prompt)
    #[arg(long)]
    prompt_file: Option<String>,

    /// Raw prompt string (exclusive with --prompt-file)
    #[arg(long)]
    prompt: Option<String>,

    /// Path to JSON file with array of {filename, description}
    #[arg(long)]
    files_json: String,
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

    if args.mcp {
        // MCP mode
        let server = Gpt5ReasonerServer {
            tools: Arc::new(Gpt5Reasoner),
        };
        let transport = universal_tool_core::mcp::stdio();
        let svc = server.serve(transport).await?;
        svc.waiting().await?;
        return Ok(());
    }

    // CLI mode
    if args.dot_env {
        let _ = dotenvy::dotenv(); // ignore errors
    }

    let prompt = match (&args.prompt, &args.prompt_file) {
        (Some(p), None) => p.clone(),
        (None, Some(path)) => std::fs::read_to_string(path)?,
        _ => {
            eprintln!("Provide either --prompt or --prompt-file");
            std::process::exit(2);
        }
    };

    let files: Vec<FileMeta> = serde_json::from_str(&std::fs::read_to_string(&args.files_json)?)?;

    let result = gpt5_reasoner_impl(prompt, files, args.prompt_type).await;

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

// MCP server wrapper
pub struct Gpt5ReasonerServer {
    tools: Arc<Gpt5Reasoner>,
}
universal_tool_core::implement_mcp_server!(Gpt5ReasonerServer, tools);
