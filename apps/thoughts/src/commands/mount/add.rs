use crate::config::validation::sanitize_mount_name;
use crate::config::{ContextMount, RepoConfigManager, RepoMappingManager, SyncStrategy};
use crate::git::utils::{find_repo_root, get_control_repo_root, get_remote_url, is_git_repo};
use crate::utils::paths::expand_path;
use anyhow::{Context, Result, bail};
use colored::*;
use std::path::PathBuf;

pub async fn execute(
    path: PathBuf,
    mount_path: Option<String>,
    sync: SyncStrategy,
    _description: Option<String>,
) -> Result<()> {
    println!("{} mount configuration...", "Adding".green());

    let mut repo_mapping = RepoMappingManager::new()?;

    // Detect if user provided a Git URL and check if it's been cloned
    let path_str = path.to_string_lossy();
    let resolved_path = if path_str.starts_with("git@")
        || path_str.starts_with("https://")
        || path_str.starts_with("http://")
        || path_str.starts_with("ssh://")
    {
        // Check if this URL has been cloned
        match repo_mapping.resolve_url(&path_str)? {
            Some(local_path) => {
                println!("Found cloned repository at: {}", local_path.display());
                local_path
            }
            None => {
                // Provide helpful error message
                bail!(
                    "Cannot add remote URL directly. Repository must be cloned first.\n\n\
                     To add this repository, use one of:\n\n\
                     1. Clone and manage automatically:\n\
                        thoughts mount clone {}\n\n\
                     2. Clone manually then add:\n\
                        git clone {} /path/to/local\n\
                        thoughts mount add /path/to/local",
                    path_str,
                    path_str
                );
            }
        }
    } else {
        // Use the provided local path
        path
    };

    // Continue with local path processing
    let expanded = expand_path(&resolved_path)?;

    if !expanded.exists() {
        bail!("Path does not exist: {}", expanded.display());
    }

    if !expanded.is_dir() {
        bail!("Path is not a directory: {}", expanded.display());
    }

    // Check if it's a git repository
    let (mount_name, url, subpath) = if is_git_repo(&expanded) {
        // Git repository - extract URL
        println!("Detected git repository at: {}", expanded.display());

        let url = get_remote_url(&expanded)
            .context("Git repository has no remote URL. Add a remote first.")?;

        println!("Found remote URL: {url}");

        // Add mapping (auto_managed=false for existing repos)
        repo_mapping.add_mapping(url.clone(), expanded.clone(), false)?;

        let mount_name = mount_path.unwrap_or_else(|| {
            expanded
                .file_name()
                .and_then(|n| n.to_str())
                .map(sanitize_mount_name)
                .unwrap_or_else(|| "unnamed".to_string())
        });

        (mount_name, url, None)
    } else if let Ok(repo_root) = find_repo_root(&expanded) {
        // Subdirectory of a git repository
        println!("Path is within git repository: {}", repo_root.display());

        let url = get_remote_url(&repo_root).context("Parent repository has no remote URL")?;

        // Add mapping for repo root (auto_managed=false for existing repos)
        repo_mapping.add_mapping(url.clone(), repo_root.clone(), false)?;

        // Calculate subpath
        let subpath = expanded
            .strip_prefix(&repo_root)?
            .to_string_lossy()
            .to_string();

        let mount_name = mount_path.unwrap_or_else(|| {
            expanded
                .file_name()
                .and_then(|n| n.to_str())
                .map(sanitize_mount_name)
                .unwrap_or_else(|| "unnamed".to_string())
        });

        (mount_name, url, Some(subpath))
    } else {
        // Not a git repository
        bail!(
            "Path '{}' is not a git repository.\n\n\
             The mount add command requires a git repository.\n\
             If you want to mount a regular directory, please initialize it as a git repository first:\n\
                cd {}\n\
                git init\n\
                git remote add origin <URL>",
            expanded.display(),
            expanded.display()
        );
    };

    // Add to repository config at .thoughts/config.json
    let repo_root = get_control_repo_root(&std::env::current_dir()?)?.to_path_buf();
    let mgr = RepoConfigManager::new(repo_root);
    let was_v1 = matches!(mgr.peek_config_version()?, Some(v) if v == "1.0");
    let mut cfg = mgr.ensure_v2_default()?; // may migrate

    // Check duplicates
    if cfg
        .context_mounts
        .iter()
        .any(|m| m.mount_path == mount_name)
    {
        bail!("Mount '{}' already exists", mount_name);
    }

    // Add mount
    cfg.context_mounts.push(ContextMount {
        remote: url,
        subpath,
        mount_path: mount_name.clone(),
        sync,
    });

    // Validate and save
    let warnings = mgr.save_v2_validated(&cfg)?;
    for w in warnings {
        eprintln!("Warning: {}", w);
    }

    println!("✓ Added repository mount '{}'", mount_name);
    if was_v1 {
        eprintln!("Upgraded to v2 config. See MIGRATION_V1_TO_V2.md");
    }

    // Automatically update active mounts
    crate::mount::auto_mount::update_active_mounts().await?;

    Ok(())
}
