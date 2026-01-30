//! Unified MCP server for all agentic-tools.
//!
//! This binary exposes all 19+ tools from the various domain crates through a single
//! MCP stdio server, with optional allowlist filtering.

use agentic_tools_mcp::{OutputMode, RegistryServer, ServiceExt, stdio};
use agentic_tools_registry::{AgenticTools, AgenticToolsConfig};
use clap::Parser;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "agentic-mcp")]
#[command(about = "Unified MCP server for all agentic-tools", version)]
struct Args {
    /// Comma-separated allowlist (case-insensitive). Example: cli_ls,cli_grep,ask_reasoning_model
    #[arg(long, value_name = "NAMES")]
    allow: Option<String>,

    /// JSON config file path (supports { "allowlist": ["cli_ls", "cli_grep"] })
    #[arg(long, value_name = "PATH")]
    config: Option<String>,

    /// List available tools and exit
    #[arg(long)]
    list_tools: bool,

    /// Output mode: text | structured (default: text)
    #[arg(long, value_parser = ["text", "structured"])]
    output: Option<String>,

    // Convenience flags for individual tool filtering
    // TODO(3): Probably don't need these convenience flags. They are kinda archaic for the old
    // agentic-tools setup. We likely can remove them after ensuring no one else uses them.
    /// Enable cli_ls tool
    #[arg(long)]
    cli_ls: bool,

    /// Enable ask_agent tool
    #[arg(long)]
    ask_agent: bool,

    /// Enable cli_grep tool
    #[arg(long)]
    cli_grep: bool,

    /// Enable cli_glob tool
    #[arg(long)]
    cli_glob: bool,

    /// Enable cli_just_search tool
    #[arg(long)]
    cli_just_search: bool,

    /// Enable cli_just_execute tool
    #[arg(long)]
    cli_just_execute: bool,
}

#[derive(Deserialize)]
struct FileConfig {
    allowlist: Option<HashSet<String>>,
    output: Option<String>,
}

fn parse_config(args: &Args) -> (AgenticToolsConfig, Option<String>) {
    // Parse --config if provided
    let mut allowlist: Option<HashSet<String>> = None;
    let mut file_output: Option<String> = None;
    if let Some(path) = &args.config {
        match fs::read_to_string(path) {
            Ok(s) => {
                if let Ok(fc) = serde_json::from_str::<FileConfig>(&s) {
                    allowlist = fc.allowlist;
                    file_output = fc.output;
                } else {
                    eprintln!("Warning: Failed to parse config JSON; ignoring");
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to read config file: {}; ignoring", e);
            }
        }
    }

    // Parse --allow if provided (wins over config file)
    if let Some(ref s) = args.allow {
        let set: HashSet<String> = s
            .split(',')
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .collect();
        if !set.is_empty() {
            allowlist = Some(set);
        }
    }

    // Merge convenience flags into allowlist
    let mut flag_set: HashSet<String> = HashSet::new();
    if args.cli_ls {
        flag_set.insert("cli_ls".to_string());
    }
    if args.ask_agent {
        flag_set.insert("ask_agent".to_string());
    }
    if args.cli_grep {
        flag_set.insert("cli_grep".to_string());
    }
    if args.cli_glob {
        flag_set.insert("cli_glob".to_string());
    }
    if args.cli_just_search {
        flag_set.insert("cli_just_search".to_string());
    }
    if args.cli_just_execute {
        flag_set.insert("cli_just_execute".to_string());
    }

    if !flag_set.is_empty() {
        allowlist.get_or_insert_with(HashSet::new).extend(flag_set);
    }

    (
        AgenticToolsConfig {
            allowlist,
            extras: serde_json::json!({}),
        },
        file_output,
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    // Install the rustls CryptoProvider before any HTTP clients are created.
    // Required because Cargo's additive features cause both ring and aws-lc-rs
    // to be compiled in via transitive dependencies (async-openai, jsonwebtoken, etc.),
    // and rustls 0.23+ panics if it can't auto-select a single provider.
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

    let args = Args::parse();

    let (cfg, file_output) = parse_config(&args);
    let reg = AgenticTools::new(cfg);

    if args.list_tools {
        let mut names = reg.list_names();
        names.sort();
        eprintln!("Available tools ({}):", names.len());
        for n in names {
            eprintln!("  - {}", n);
        }
        return Ok(());
    }

    let output_mode = match (args.output.as_deref(), file_output.as_deref()) {
        (Some("structured"), _) => OutputMode::Structured,
        (Some("text"), _) => OutputMode::Text,
        (None, Some("structured")) => OutputMode::Structured,
        (None, Some("text")) => OutputMode::Text,
        _ => OutputMode::Text, // default
    };

    eprintln!(
        "Starting agentic-mcp ({} tools) with output mode: {:?}",
        reg.len(),
        output_mode
    );

    let server = RegistryServer::new(Arc::new(reg))
        .with_info("agentic-mcp", env!("CARGO_PKG_VERSION"))
        .with_output_mode(output_mode);
    let transport = stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;

    Ok(())
}
