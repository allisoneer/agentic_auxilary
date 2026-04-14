//! Unified MCP server for all agentic-tools.
//!
//! This binary exposes all 19+ tools from the various domain crates through a single
//! MCP stdio server, with optional allowlist filtering.

#[cfg(not(unix))]
compile_error!(
    "agentic-mcp only supports Unix-like platforms (Linux/macOS). Windows is not supported."
);

use agentic_config::loader::load_merged;
use agentic_tools_mcp::OutputMode;
use agentic_tools_mcp::RegistryServer;
use agentic_tools_mcp::ServiceExt;
use agentic_tools_mcp::stdio;
use agentic_tools_registry::AgenticTools;
use agentic_tools_registry::AgenticToolsConfig;
use clap::Parser;
use colored::Colorize;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(name = "agentic-mcp")]
#[command(about = "Unified MCP server for all agentic-tools", version)]
struct Args {
    /// Comma-separated allowlist (case-insensitive). Example: `cli_ls,cli_grep,ask_reasoning_model`
    #[arg(long, value_name = "NAMES")]
    allow: Option<String>,

    /// JSON config file path for server settings (allowlist/output)
    #[arg(long = "server-config", value_name = "PATH")]
    server_config: Option<String>,

    /// List available tools and exit
    #[arg(long)]
    list_tools: bool,

    /// Output mode: text | structured (default: text)
    #[arg(long, value_parser = ["text", "structured"])]
    output: Option<String>,

    // Convenience flags for individual tool filtering
    // TODO(3): Probably don't need these convenience flags. They are kinda archaic for the old
    // agentic-tools setup. We likely can remove them after ensuring no one else uses them.
    /// Enable `cli_ls` tool
    #[arg(long)]
    cli_ls: bool,

    /// Enable `ask_agent` tool
    #[arg(long)]
    ask_agent: bool,

    /// Enable `cli_grep` tool
    #[arg(long)]
    cli_grep: bool,

    /// Enable `cli_instant_grep` tool
    #[arg(long)]
    cli_instant_grep: bool,

    /// Enable `cli_glob` tool
    #[arg(long)]
    cli_glob: bool,

    /// Enable `cli_just_search` tool
    #[arg(long)]
    cli_just_search: bool,

    /// Enable `cli_just_execute` tool
    #[arg(long)]
    cli_just_execute: bool,
}

#[derive(Deserialize)]
struct FileConfig {
    allowlist: Option<HashSet<String>>,
    output: Option<String>,
}

fn parse_config(args: &Args) -> (AgenticToolsConfig, Option<String>) {
    // Parse server config if provided
    let mut allowlist: Option<HashSet<String>> = None;
    let mut file_output: Option<String> = None;
    if let Some(path) = args.server_config.as_deref() {
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
                eprintln!("Warning: Failed to read config file: {e}; ignoring");
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
    if args.cli_instant_grep {
        flag_set.insert("cli_instant_grep".to_string());
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
            ..Default::default()
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

    // Load agentic.toml for tool-specific config (subagents, reasoning)
    let cwd = std::env::current_dir()?;
    let loaded = load_merged(&cwd)?;

    // Print config warnings
    for w in &loaded.warnings {
        eprintln!("{} {}", "WARN".yellow(), w);
    }

    // Parse server config (allowlist, output mode)
    let (mut reg_cfg, file_output) = parse_config(&args);

    // Attach tool config sections from agentic.toml
    reg_cfg.subagents = loaded.config.subagents.clone();
    reg_cfg.reasoning = loaded.config.reasoning.clone();
    reg_cfg.web_retrieval = loaded.config.web_retrieval.clone();
    reg_cfg.cli_tools = loaded.config.cli_tools.clone();
    reg_cfg.exa = loaded.config.services.exa.clone();
    reg_cfg.anthropic = loaded.config.services.anthropic.clone();

    let reg = AgenticTools::new(reg_cfg);

    if args.list_tools {
        let mut names = reg.list_names();
        names.sort();
        eprintln!("Available tools ({}):", names.len());
        for n in names {
            eprintln!("  - {n}");
        }
        return Ok(());
    }

    let output_mode = match (args.output.as_deref(), file_output.as_deref()) {
        (Some("structured"), _) | (None, Some("structured")) => OutputMode::Structured,
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
