//! Configuration management commands.
//!
//! Provides init, show, schema, edit, and validate subcommands for
//! managing agentic.json configuration files.

use agentic_config::{
    loader::{global_config_path, load_merged, local_config_path},
    types::AgenticConfig,
};
use anyhow::{Context, Result};
use atomicwrites::{AtomicFile, OverwriteBehavior};
use clap::Subcommand;
use colored::Colorize;
use std::{io::Write, path::PathBuf, process::Command};

#[derive(Subcommand)]
pub enum ConfigCommands {
    /// Initialize a new configuration file
    Init {
        /// Create global config instead of local
        #[arg(long)]
        global: bool,

        /// Overwrite existing config file
        #[arg(long)]
        force: bool,
    },

    /// Show the merged configuration
    Show {
        /// Output as raw JSON (no formatting)
        #[arg(long)]
        json: bool,

        /// Path to use as local directory (defaults to current dir)
        #[arg(long)]
        path: Option<PathBuf>,
    },

    /// Output the JSON Schema for agentic.json
    Schema,

    /// Open configuration in $EDITOR
    Edit {
        /// Edit global config instead of local
        #[arg(long)]
        global: bool,
    },

    /// Validate configuration and show warnings
    Validate {
        /// Path to use as local directory (defaults to current dir)
        #[arg(long)]
        path: Option<PathBuf>,
    },
}

pub fn execute(cmd: ConfigCommands) -> Result<()> {
    match cmd {
        ConfigCommands::Init { global, force } => cmd_init(global, force),
        ConfigCommands::Show { json, path } => cmd_show(json, path),
        ConfigCommands::Schema => cmd_schema(),
        ConfigCommands::Edit { global } => cmd_edit(global),
        ConfigCommands::Validate { path } => cmd_validate(path),
    }
}

fn cmd_init(global: bool, force: bool) -> Result<()> {
    let path = if global {
        let global = global_config_path()?;
        // Ensure parent directory exists
        if let Some(parent) = global.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory {}", parent.display()))?;
        }
        global
    } else {
        local_config_path(&std::env::current_dir()?)
    };

    if path.exists() && !force {
        anyhow::bail!(
            "Config file already exists: {}\nUse --force to overwrite",
            path.display()
        );
    }

    let config = AgenticConfig::default();
    let json = serde_json::to_string_pretty(&config)?;

    AtomicFile::new(&path, OverwriteBehavior::AllowOverwrite)
        .write(|f| f.write_all(json.as_bytes()))
        .with_context(|| format!("Failed to write config to {}", path.display()))?;

    println!(
        "{} Created {}",
        "OK".green(),
        path.display().to_string().cyan()
    );
    Ok(())
}

fn cmd_show(json_output: bool, path: Option<PathBuf>) -> Result<()> {
    let dir = path.unwrap_or(std::env::current_dir()?);
    let loaded = load_merged(&dir)?;

    // Show migration events
    for event in &loaded.events {
        match event {
            agentic_config::loader::LoadEvent::MigratedThoughtsV2 { from, to } => {
                eprintln!(
                    "{} Migrated config from {} to {}",
                    "INFO".blue(),
                    from.display(),
                    to.display()
                );
            }
        }
    }

    // Show warnings
    for warning in &loaded.warnings {
        eprintln!("{} {}", "WARN".yellow(), warning);
    }

    // Output the config
    if json_output {
        println!("{}", serde_json::to_string(&loaded.config)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&loaded.config)?);
    }

    Ok(())
}

fn cmd_schema() -> Result<()> {
    println!("{}", agentic_config::schema_json_pretty()?);
    Ok(())
}

fn cmd_edit(global: bool) -> Result<()> {
    let path = if global {
        let global = global_config_path()?;
        // Ensure the file exists with defaults if not
        if !global.exists() {
            if let Some(parent) = global.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let config = AgenticConfig::default();
            let json = serde_json::to_string_pretty(&config)?;
            AtomicFile::new(&global, OverwriteBehavior::AllowOverwrite)
                .write(|f| f.write_all(json.as_bytes()))?;
        }
        global
    } else {
        let local = local_config_path(&std::env::current_dir()?);
        // Create with defaults if not exists
        if !local.exists() {
            let config = AgenticConfig::default();
            let json = serde_json::to_string_pretty(&config)?;
            AtomicFile::new(&local, OverwriteBehavior::AllowOverwrite)
                .write(|f| f.write_all(json.as_bytes()))?;
        }
        local
    };

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    let status = Command::new(&editor)
        .arg(&path)
        .status()
        .with_context(|| format!("Failed to run editor: {}", editor))?;

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status");
    }

    // Validate after edit
    let raw = std::fs::read_to_string(&path)?;
    match serde_json::from_str::<AgenticConfig>(&raw) {
        Ok(config) => {
            let warnings = agentic_config::validation::validate(&config);
            if warnings.is_empty() {
                println!("{} Configuration is valid", "OK".green());
            } else {
                println!("{} Configuration has warnings:", "WARN".yellow());
                for w in warnings {
                    println!("  - {}", w);
                }
            }
        }
        Err(e) => {
            eprintln!("{} Configuration has errors: {}", "ERROR".red(), e);
            anyhow::bail!("Invalid JSON in configuration file");
        }
    }

    Ok(())
}

fn cmd_validate(path: Option<PathBuf>) -> Result<()> {
    let dir = path.unwrap_or(std::env::current_dir()?);
    let loaded = load_merged(&dir)?;

    if loaded.warnings.is_empty() {
        println!("{} Configuration is valid", "OK".green());
        println!("\nConfig files:");
        println!("  Global: {}", loaded.paths.global.display());
        println!("  Local:  {}", loaded.paths.local.display());
    } else {
        println!(
            "{} Configuration has {} warning(s):",
            "WARN".yellow(),
            loaded.warnings.len()
        );
        for w in &loaded.warnings {
            println!("  - {}", w);
        }
        println!("\nConfig files:");
        println!("  Global: {}", loaded.paths.global.display());
        println!("  Local:  {}", loaded.paths.local.display());
    }

    Ok(())
}
