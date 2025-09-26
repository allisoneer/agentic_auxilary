use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod commands;
mod config;
mod error;
mod git;
mod mount;
mod platform;
mod utils;

use crate::config::SyncStrategy;

#[derive(Parser)]
#[command(name = "thoughts")]
#[command(about = "A flexible thought management tool with filesystem merging")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Increase logging verbosity
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize thoughts for current repository
    Init {
        /// Force re-initialization even if already initialized
        #[arg(short, long)]
        force: bool,
    },

    /// Sync git-backed mounts
    Sync {
        /// Specific mount to sync (syncs current repository's mounts if not specified)
        mount: Option<String>,

        /// Commit message for sync
        #[arg(short, long)]
        message: Option<String>,

        /// Sync all auto-sync mounts regardless of current directory
        #[arg(short, long)]
        all: bool,
    },

    /// Show mount status
    Status {
        /// Show detailed status
        #[arg(short, long)]
        detailed: bool,
    },

    /// Mount management commands
    Mount {
        #[command(subcommand)]
        command: MountCommands,
    },

    /// Configuration commands
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Subcommand)]
enum MountCommands {
    /// Add an existing local repository as a mount
    Add {
        /// Path to the local git repository
        path: std::path::PathBuf,

        /// Mount name (optional positional)
        mount_path: Option<String>,

        /// Sync strategy
        #[arg(long, value_parser = clap::value_parser!(SyncStrategy), default_value = "auto")]
        sync: SyncStrategy,

        /// Description
        #[arg(short, long)]
        description: Option<String>,
    },

    /// Remove a mount
    Remove {
        /// Name of the mount to remove
        mount_name: String,
    },

    /// List all mounts
    List {
        /// Show verbose information including paths
        #[arg(short, long)]
        verbose: bool,
    },

    /// Update active mounts to match configuration
    Update,

    /// Clone a repository for mounting
    Clone {
        /// Git URL to clone
        url: String,

        /// Optional path to clone to (defaults to ~/.thoughts/clones/<repo-name>)
        path: Option<std::path::PathBuf>,
    },

    /// Debug mount operations
    Debug {
        #[command(subcommand)]
        command: MountDebugCommands,
    },
}

#[derive(Subcommand)]
enum MountDebugCommands {
    /// Show detailed mount information
    Info {
        /// Mount name or target path
        target: String,
    },

    /// Show exact mount command for debugging
    Command {
        /// Mount name
        mount_name: String,
    },

    /// Force remount with current settings
    Remount {
        /// Mount name
        mount_name: String,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Create a new repository configuration
    Create,

    /// Show current configuration
    Show {
        /// Output as JSON
        #[arg(short, long)]
        json: bool,
    },

    /// Edit configuration in $EDITOR
    Edit {},

    /// Validate configuration
    Validate,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let log_level = match (cli.quiet, cli.verbose) {
        (true, _) => "error",
        (false, 0) => "info",
        (false, 1) => "debug",
        (false, _) => "trace",
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level)),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_thread_ids(false)
                .with_thread_names(false),
        )
        .init();

    info!("Starting thoughts v{}", env!("CARGO_PKG_VERSION"));

    // Execute command
    match cli.command {
        Commands::Init { force } => commands::init::execute(force).await,
        Commands::Sync {
            mount,
            message: _,
            all,
        } => commands::sync::execute(mount, all).await,
        Commands::Status { detailed } => commands::status::execute(detailed).await,
        Commands::Mount { command } => match command {
            MountCommands::Add {
                path,
                mount_path,
                sync,
                description,
            } => commands::mount::add::execute(path, mount_path, sync, description).await,
            MountCommands::Remove { mount_name } => {
                commands::mount::remove::execute(mount_name).await
            }
            MountCommands::List { verbose } => commands::mount::list::execute(verbose).await,
            MountCommands::Update => commands::mount::update::execute().await,
            MountCommands::Clone { url, path } => commands::mount::clone::execute(url, path).await,
            MountCommands::Debug { command } => match command {
                MountDebugCommands::Info { target } => {
                    commands::mount::debug::info::execute(target).await
                }
                MountDebugCommands::Command { mount_name } => {
                    commands::mount::debug::command::execute(mount_name).await
                }
                MountDebugCommands::Remount { mount_name } => {
                    commands::mount::debug::remount::execute(mount_name).await
                }
            },
        },
        Commands::Config { command } => match command {
            ConfigCommands::Create => commands::config::create::execute().await,
            ConfigCommands::Show { json } => commands::config::show::execute(json).await,
            ConfigCommands::Edit {} => commands::config::edit::execute().await,
            ConfigCommands::Validate => commands::config::validate::execute().await,
        },
    }
}
