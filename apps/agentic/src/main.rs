//! Agentic unified CLI.
//!
//! The `agentic` command provides configuration management for the
//! agentic tools ecosystem.

use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "agentic")]
#[command(about = "Agentic unified CLI for configuration management")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,
}

#[derive(Subcommand)]
enum Commands {
    /// Configuration management commands
    Config {
        #[command(subcommand)]
        command: commands::config::ConfigCommands,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing based on verbosity
    let level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .init();

    match cli.command {
        Commands::Config { command } => commands::config::execute(command),
    }
}
