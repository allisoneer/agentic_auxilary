use anyhow::{Result, anyhow};
use clap::Args;
use colored::Colorize;

use crate::config::RepoConfigManager;
use crate::git::utils::get_control_repo_root;
use crate::utils::paths;

#[derive(Args)]
pub struct MigrateArgs {
    /// Show what would be migrated without making changes
    #[arg(long)]
    pub dry_run: bool,

    /// Proceed with migration without confirmation
    #[arg(long)]
    pub yes: bool,
}

pub async fn execute(args: MigrateArgs) -> Result<()> {
    let repo_root = get_control_repo_root(&std::env::current_dir()?)?;
    let mgr = RepoConfigManager::new(repo_root.clone());

    // Explicitly check for existence to distinguish "no file" from "missing version"
    let config_path = paths::get_repo_config_path(&repo_root);
    if !config_path.exists() {
        anyhow::bail!("No repository configuration found. Run 'thoughts config create' first.");
    }

    // Treat None (no version field) and "1.0" as legacy v1; only early-exit if already v2
    match mgr.peek_config_version()? {
        Some(v) if v == "2.0" => {
            println!("Already on v2. No action taken.");
            return Ok(());
        }
        _ => {}
    }

    // Summarize migration
    let ds = mgr
        .load_desired_state()?
        .ok_or_else(|| anyhow!("Configuration was deleted during migration"))?;
    println!("This will migrate your config to v2:");
    println!("  - context mounts: {}", ds.context_mounts.len());
    println!("  - references: {}", ds.references.len());
    println!("  - rules will be dropped (backed up if present)");
    println!("  - a backup will be created only if non-empty");

    if args.dry_run {
        println!("\nDry-run: no changes written.");
        return Ok(());
    }

    if !args.yes {
        println!("\nUse --yes to proceed.");
        return Ok(());
    }

    let _ = mgr.ensure_v2_default()?; // performs migration + backup
    println!(
        "\n{} Migrated to v2. See MIGRATION_V1_TO_V2.md",
        "✓".green()
    );
    Ok(())
}
