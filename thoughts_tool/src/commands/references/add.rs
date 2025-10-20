use crate::config::validation::{canonical_reference_key, is_git_url, validate_reference_url};
use crate::config::{ReferenceEntry, RepoConfigManager, RepoMappingManager};
use crate::git::utils::{find_repo_root, get_control_repo_root, get_remote_url, is_git_repo};
use crate::utils::paths::expand_path;
use anyhow::{Context, Result, bail};
use colored::Colorize;
use std::path::PathBuf;

pub async fn execute(input: String) -> Result<()> {
    let repo_root = get_control_repo_root(&std::env::current_dir()?)?;
    let mgr = RepoConfigManager::new(repo_root);
    let mut cfg = mgr.ensure_v2_default()?;

    // Build set of canonical keys for duplicate detection across variants
    let mut existing_keys = std::collections::HashSet::new();
    for e in &cfg.references {
        let url = match e {
            ReferenceEntry::Simple(s) => s.as_str(),
            ReferenceEntry::WithMetadata(rm) => rm.remote.as_str(),
        };
        if let Ok(key) = canonical_reference_key(url) {
            existing_keys.insert(key);
        }
    }

    // Determine if input is a URL or a local path
    let (final_url, local_path_for_mapping) = if is_git_url(&input) {
        // Validate URL
        validate_reference_url(&input)?;

        // Check duplicates via canonical key
        let key = canonical_reference_key(&input)?;
        if existing_keys.contains(&key) {
            println!(
                "{}\nReference already exists (detected by normalized host/org/repo):\n  {}",
                "Note:".yellow(),
                input
            );
            return Ok(());
        }

        (input.clone(), None)
    } else {
        // Treat as local path
        let path = PathBuf::from(&input);
        let expanded = expand_path(&path)?;
        if !expanded.exists() {
            bail!(
                "Path does not exist: {}\n\nTo add a remote repository by URL:\n  thoughts references add <git-url>",
                expanded.display()
            );
        }
        if !expanded.is_dir() {
            bail!(
                "Path is not a directory: {}\nPlease provide a directory path (the repository root).",
                expanded.display()
            );
        }

        // Git repo or subdirectory?
        if is_git_repo(&expanded) {
            let url = get_remote_url(&expanded).context(
                "Git repository has no 'origin' remote.\nAdd a remote first:\n  git remote add origin <git-url>",
            )?;

            validate_reference_url(&url)?;

            // Duplicate check via canonical key
            let key = canonical_reference_key(&url)?;
            if existing_keys.contains(&key) {
                println!(
                    "{}\nReference already exists for repository:\n  {}\nLocal path was resolved to the same origin URL.",
                    "Note:".yellow(),
                    url
                );
                return Ok(());
            }

            // Register mapping so sync can find and git pull this location
            let mut repo_mapping = RepoMappingManager::new()?;
            repo_mapping.add_mapping(url.clone(), expanded.clone(), false)?;

            (url, Some(expanded))
        } else if let Ok(repo_root) = find_repo_root(&expanded) {
            bail!(
                "Cannot add subdirectory as a reference:\n  {}\n\nReferences are repo-level only.\nDetected repository root:\n  {}\n\nTry one of:\n  1) Add the repository root as a reference:\n     thoughts references add {}\n  2) If you need a subdirectory mount, use:\n     thoughts mount add {}",
                expanded.display(),
                repo_root.display(),
                repo_root.display(),
                expanded.display(),
            );
        } else {
            bail!(
                "Path is not a git repository: {}\n\nInitialize and add a remote first:\n  git init\n  git remote add origin <git-url>\n  thoughts references add <repo-root>",
                expanded.display()
            );
        }
    };

    // Append URL to config after passing all validation
    cfg.references
        .push(ReferenceEntry::Simple(final_url.clone()));
    mgr.save_v2(&cfg)?;

    println!("{} Added reference: {}", "✓".green(), final_url);
    if let Some(lp) = local_path_for_mapping {
        println!(
            "Local repository mapped for sync:\n  {} -> {}",
            final_url,
            lp.display()
        );
    }
    println!("Run 'thoughts references sync' to clone/update and mount it.");
    Ok(())
}
