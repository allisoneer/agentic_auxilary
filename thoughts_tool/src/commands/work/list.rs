use crate::config::{Mount, RepoConfigManager};
use crate::git::utils::{find_repo_root, get_control_repo_root};
use crate::mount::MountResolver;
use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;

pub async fn execute(recent: Option<usize>) -> Result<()> {
    let _code_root = find_repo_root(&std::env::current_dir()?)?;

    let mgr = RepoConfigManager::new(get_control_repo_root(&std::env::current_dir()?)?);
    let ds = mgr.load_desired_state()?.ok_or_else(|| {
        anyhow::anyhow!("No repository configuration found. Run 'thoughts init'.")
    })?;

    let tm = ds
        .thoughts_mount
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No thoughts_mount configured"))?;

    // Resolve the thoughts mount
    let resolver = MountResolver::new()?;
    let mount = Mount::Git {
        url: tm.remote.clone(),
        subpath: tm.subpath.clone(),
        sync: tm.sync,
    };

    let thoughts_root = resolver
        .resolve_mount(&mount)
        .context("Thoughts mount not cloned")?;

    let active_dir = thoughts_root.join("active");
    let completed_dir = thoughts_root.join("completed");

    // List active work
    println!("{}", "Active Work:".bold());
    if active_dir.exists() {
        let mut entries: Vec<_> = fs::read_dir(&active_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        entries.sort_by_key(|e| e.file_name());

        if entries.is_empty() {
            println!("  {}", "No active work".dimmed());
        } else {
            for entry in entries {
                let name = entry.file_name();
                let manifest_path = entry.path().join("manifest.json");

                // Try to read and display started_at if manifest exists
                if manifest_path.exists()
                    && let Ok(content) = fs::read_to_string(&manifest_path)
                    && let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&content)
                    && let Some(started_at) = manifest.get("started_at").and_then(|v| v.as_str())
                {
                    println!(
                        "  - {} (started: {})",
                        name.to_string_lossy().green(),
                        started_at
                    );
                    continue;
                }

                // Fallback if manifest doesn't exist or can't be read
                println!("  - {}", name.to_string_lossy().green());
            }
        }
    } else {
        println!("  {}", "No active directory".dimmed());
    }

    println!();
    println!("{}", "Completed Work:".bold());
    if completed_dir.exists() {
        let mut entries: Vec<_> = fs::read_dir(&completed_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        // Sort by modification time (newest first)
        entries.sort_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()));
        entries.reverse();

        // Limit if requested
        if let Some(limit) = recent {
            entries.truncate(limit);
        }

        if entries.is_empty() {
            println!("  {}", "No completed work".dimmed());
        } else {
            for entry in entries {
                let name = entry.file_name();
                println!("  - {}", name.to_string_lossy());
            }

            if recent.is_some() && recent.unwrap() < fs::read_dir(&completed_dir)?.count() {
                println!("  {} (use --recent <n> to show more)", "...".dimmed());
            }
        }
    } else {
        println!("  {}", "No completed directory".dimmed());
    }

    Ok(())
}
