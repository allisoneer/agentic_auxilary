//! Configuration management commands.
//!
//! Provides init, show, schema, edit, and validate subcommands for
//! managing agentic.toml configuration files.

use agentic_config::loader::LoadedAgenticConfig;
use agentic_config::loader::global_config_path;
use agentic_config::loader::load_merged;
use agentic_config::loader::local_config_path;
use agentic_config::types::AgenticConfig;
use anyhow::Context;
use anyhow::Result;
use atomicwrites::AtomicFile;
use atomicwrites::OverwriteBehavior;
use clap::Subcommand;
use colored::Colorize;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

// =============================================================================
// Helper functions (DRY refactor)
// =============================================================================

/// Resolve optional --path argument to current directory if not provided.
fn resolve_dir(path: Option<PathBuf>) -> Result<PathBuf> {
    match path {
        None => std::env::current_dir().context("Failed to determine current directory"),
        Some(p) => {
            if !p.exists() {
                anyhow::bail!("--path does not exist: {}", p.display());
            }
            if !p.is_dir() {
                anyhow::bail!("--path is not a directory: {}", p.display());
            }
            Ok(p)
        }
    }
}

/// Ensure parent directory exists for a config file path.
fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }
    Ok(())
}

/// Create default config and serialize to pretty TOML.
fn default_config_toml_pretty() -> Result<String> {
    let cfg = AgenticConfig::default();
    toml::to_string_pretty(&cfg).context("Failed to serialize default config")
}

/// Write string contents to file atomically.
fn write_atomic_str(path: &Path, contents: &str) -> Result<()> {
    AtomicFile::new(path, OverwriteBehavior::AllowOverwrite)
        .write(|f| f.write_all(contents.as_bytes()))
        .with_context(|| format!("Failed to write config to {}", path.display()))?;
    Ok(())
}

/// Create config file with defaults if it doesn't exist.
fn ensure_config_exists_with_defaults(path: &Path) -> Result<()> {
    if !path.exists() {
        ensure_parent_dir(path)?;
        write_atomic_str(path, &default_config_toml_pretty()?)?;
    }
    Ok(())
}

/// Print warnings from loaded config.
fn print_load_feedback(loaded: &LoadedAgenticConfig) {
    for warning in &loaded.warnings {
        eprintln!("{} {}", "WARN".yellow(), warning);
    }
}

// =============================================================================
// CLI Subcommands
// =============================================================================

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

    /// Output the JSON Schema for agentic.toml
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
    if global {
        let path = global_config_path()?;
        ensure_parent_dir(&path)?;

        if path.exists() && !force {
            anyhow::bail!(
                "Config file already exists: {}\nUse --force to overwrite",
                path.display()
            );
        }

        write_atomic_str(&path, &default_config_toml_pretty()?)?;
        println!(
            "{} Created {}",
            "OK".green(),
            path.display().to_string().cyan()
        );
        return Ok(());
    }

    let dir = std::env::current_dir()?;
    let path = local_config_path(&dir);

    if path.exists() && !force {
        anyhow::bail!(
            "Config file already exists: {}\nUse --force to overwrite",
            path.display()
        );
    }

    write_atomic_str(&path, &default_config_toml_pretty()?)?;

    println!(
        "{} Created {}",
        "OK".green(),
        path.display().to_string().cyan()
    );
    Ok(())
}

fn cmd_show(json_output: bool, path: Option<PathBuf>) -> Result<()> {
    let dir = resolve_dir(path)?;
    let loaded = load_merged(&dir)?;

    print_load_feedback(&loaded);

    // Output the config
    if json_output {
        // JSON output for scripting
        println!("{}", serde_json::to_string(&loaded.config)?);
    } else {
        // TOML output (default)
        println!("{}", toml::to_string_pretty(&loaded.config)?);
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
        ensure_config_exists_with_defaults(&global)?;
        global
    } else {
        let local = local_config_path(&std::env::current_dir()?);
        ensure_config_exists_with_defaults(&local)?;
        local
    };

    let argv = agentic_tools_utils::editor_argv()?;

    let status = Command::new(&argv.program)
        .args(&argv.args)
        .arg(&path)
        .status()
        .with_context(|| format!("Failed to run editor: {}", argv.raw))?;

    if !status.success() {
        anyhow::bail!("Editor exited with non-zero status");
    }

    // Validate after edit
    let raw = std::fs::read_to_string(&path)?;
    let mut warnings = vec![];

    // Check for deprecated and unknown keys in raw TOML
    if let Ok(v) = toml::from_str::<toml::Value>(&raw) {
        warnings.extend(agentic_config::validation::detect_deprecated_keys_toml(&v));
        warnings.extend(agentic_config::validation::detect_unknown_top_level_keys_toml(&v));
    }

    match toml::from_str::<AgenticConfig>(&raw) {
        Ok(config) => {
            warnings.extend(agentic_config::validation::validate(&config));
            if warnings.is_empty() {
                println!("{} Configuration is valid", "OK".green());
            } else {
                println!("{} Configuration has warnings:", "WARN".yellow());
                for w in warnings {
                    println!("  - {w}");
                }
            }
        }
        Err(e) => {
            eprintln!("{} Configuration has errors: {}", "ERROR".red(), e);
            anyhow::bail!("Invalid TOML in configuration file");
        }
    }

    Ok(())
}

fn cmd_validate(path: Option<PathBuf>) -> Result<()> {
    let dir = resolve_dir(path)?;
    let loaded = load_merged(&dir)?;

    if loaded.warnings.is_empty() {
        println!("{} Configuration is valid", "OK".green());
    } else {
        println!(
            "{} Configuration has {} warning(s):",
            "WARN".yellow(),
            loaded.warnings.len()
        );
        for w in &loaded.warnings {
            println!("  - {w}");
        }
    }

    println!("\nConfig files:");
    println!("  Global: {}", loaded.paths.global.display());
    println!("  Local:  {}", loaded.paths.local.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    static CWD_LOCK: Mutex<()> = Mutex::new(());

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Self {
            let mut path = std::env::temp_dir();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            path.push(format!("{}{}-{}", prefix, std::process::id(), nanos));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    struct CwdGuard {
        prev: PathBuf,
    }

    impl CwdGuard {
        fn set(dir: &Path) -> Self {
            let prev = std::env::current_dir().unwrap();
            std::env::set_current_dir(dir).unwrap();
            Self { prev }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.prev);
        }
    }

    #[test]
    fn test_init_creates_config() {
        let _lock = CWD_LOCK.lock().unwrap();

        let temp = TestDir::new("agentic-init-");
        let _cwd = CwdGuard::set(&temp.path);

        cmd_init(false, false).unwrap();
        assert!(temp.path.join("agentic.toml").exists());
    }
}
