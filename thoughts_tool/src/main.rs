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
    /// Add a new mount
    Add {
        /// Path to the git repository to mount
        path: std::path::PathBuf,

        /// Custom name for the mount (defaults to repository name)
        #[arg(short = 'n', long)]
        name: Option<String>,

        /// Add as a personal mount instead of repository mount
        #[arg(short, long)]
        personal: bool,
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
        /// Show personal config instead of repository config
        #[arg(short, long)]
        personal: bool,
    },

    /// Edit configuration in $EDITOR
    Edit {
        /// Edit personal config instead of repository config
        #[arg(short, long)]
        personal: bool,
    },

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
                name,
                personal,
            } => commands::mount::add::execute(path, name, personal).await,
            MountCommands::Remove { mount_name } => {
                commands::mount::remove::execute(mount_name).await
            }
            MountCommands::List { verbose } => commands::mount::list::execute(verbose).await,
            MountCommands::Update => commands::mount::update::execute().await,
            MountCommands::Clone { url, path } => commands::mount::clone::execute(url, path).await,
        },
        Commands::Config { command } => match command {
            ConfigCommands::Create => commands::config::create::execute().await,
            ConfigCommands::Show { json, personal } => commands::config::show::execute(json, personal).await,
            ConfigCommands::Edit { personal } => commands::config::edit::execute(personal).await,
            ConfigCommands::Validate => commands::config::validate::execute().await,
        },
    }
}
