use crate::config::RepoConfigManager;
use crate::git::utils::{get_current_repo, get_main_repo_for_worktree, is_worktree};
use crate::utils::git::ensure_gitignore_entry;
use crate::utils::paths::ensure_dir;
use anyhow::{Context, Result, anyhow};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

pub async fn execute(force: bool) -> Result<()> {
    // Ensure we're in a git repository
    let repo_root = get_current_repo().context("Not in a git repository. Run 'git init' first.")?;

    // Check if we're in a worktree
    if is_worktree(&repo_root)? {
        eprintln!("{}: Detected git worktree", "Info".cyan());

        // Get the main repository path
        let main_repo = get_main_repo_for_worktree(&repo_root)?;
        eprintln!("Main repository: {}", main_repo.display());

        let main_thoughts_data = main_repo.join(".thoughts-data");
        let worktree_thoughts_data = repo_root.join(".thoughts-data");

        // Ensure main repository is initialized first
        if !main_thoughts_data.exists() {
            eprintln!("{}: Main repository is not initialized", "Error".red());
            eprintln!("Please run 'thoughts init' in the main repository first:");
            eprintln!("  cd {}", main_repo.display());
            eprintln!("  thoughts init");
            return Err(anyhow!("Main repository must be initialized first"));
        }

        // Check if already initialized
        if worktree_thoughts_data.exists() && !force && worktree_thoughts_data.is_symlink() {
            eprintln!("{}: Worktree already initialized", "Info".cyan());
            let target = fs::read_link(&worktree_thoughts_data)
                .unwrap_or_else(|_| PathBuf::from("<invalid>"));
            eprintln!("  .thoughts-data -> {}", target.display());
            return Ok(());
        }

        // Clean up if forcing
        if force && worktree_thoughts_data.exists() {
            fs::remove_file(&worktree_thoughts_data).with_context(|| {
                format!("Failed to remove existing symlink: {worktree_thoughts_data:?}")
            })?;
        }

        // Create the symlink: .thoughts-data -> main repo's .thoughts-data
        create_symlink(
            &main_thoughts_data.to_string_lossy(),
            &worktree_thoughts_data,
        )?;
        eprintln!(
            "{}: Created .thoughts-data symlink to main repository",
            "Success".green()
        );
        eprintln!("{}: Worktree initialization complete!", "Success".green());
        eprintln!("The worktree now shares mounts with the main repository.");

        return Ok(());
    }

    // Continue with normal initialization for main repository...
    println!(
        "Initializing thoughts for repository at: {}",
        repo_root.display()
    );

    // Load or create repository configuration
    let repo_config_manager = RepoConfigManager::new(repo_root.clone());
    let config = repo_config_manager
        .ensure_default()
        .context("Failed to create repository configuration")?;

    // Get v2 mount dirs - either from desired state or from v1 config defaults
    let mount_dirs = if let Some(desired) = repo_config_manager.load_desired_state()? {
        desired.mount_dirs
    } else {
        // Use v1 defaults that will map to v2
        crate::config::MountDirsV2 {
            thoughts: "thoughts".into(),
            context: config.mount_dirs.repository.clone(),
            references: "references".into(),
        }
    };

    // Check for existing symlinks
    let thoughts_link = repo_root.join(&mount_dirs.thoughts);
    let context_link = repo_root.join(&mount_dirs.context);
    let references_link = repo_root.join(&mount_dirs.references);

    if !force {
        let mut existing = vec![];
        if thoughts_link.exists() {
            existing.push((&mount_dirs.thoughts, &thoughts_link));
        }
        if context_link.exists() {
            existing.push((&mount_dirs.context, &context_link));
        }
        if references_link.exists() {
            existing.push((&mount_dirs.references, &references_link));
        }

        if !existing.is_empty() {
            eprintln!(
                "{}: Repository already has thoughts directories:",
                "Error".red()
            );
            for (name, path) in existing {
                if path.is_symlink() {
                    let target = fs::read_link(path).unwrap_or_else(|_| PathBuf::from("<invalid>"));
                    eprintln!("  {} -> {}", name, target.display());
                } else {
                    eprintln!("  {} (not a symlink)", name);
                }
            }
            eprintln!("\nUse {} to reinitialize.", "--force".cyan());
            std::process::exit(1);
        }
    }

    // Validate that paths are not regular files/directories
    for (name, path) in [
        (&mount_dirs.thoughts, &thoughts_link),
        (&mount_dirs.context, &context_link),
        (&mount_dirs.references, &references_link),
    ] {
        if path.exists() && !path.is_symlink() {
            eprintln!("{}: {} exists but is not a symlink", "Error".red(), name);
            eprintln!("Please remove it manually or use {}", "--force".cyan());
            std::process::exit(1);
        }
    }

    // Create the actual thoughts directory structure
    let thoughts_dir = repo_root.join(".thoughts-data");
    ensure_dir(&thoughts_dir)?;

    let thoughts_target_dir = thoughts_dir.join(&mount_dirs.thoughts);
    let context_target_dir = thoughts_dir.join(&mount_dirs.context);
    let references_target_dir = thoughts_dir.join(&mount_dirs.references);

    ensure_dir(&thoughts_target_dir)?;
    ensure_dir(&context_target_dir)?;
    ensure_dir(&references_target_dir)?;

    // Remove existing symlinks if forcing
    if force {
        for path in [&thoughts_link, &context_link, &references_link] {
            if path.exists() && path.is_symlink() {
                fs::remove_file(path)
                    .with_context(|| format!("Failed to remove existing symlink: {path:?}"))?;
            }
        }
    }

    // Create symlinks with relative paths
    let thoughts_relative = format!(".thoughts-data/{}", mount_dirs.thoughts);
    let context_relative = format!(".thoughts-data/{}", mount_dirs.context);
    let references_relative = format!(".thoughts-data/{}", mount_dirs.references);

    create_symlink(&thoughts_relative, &thoughts_link)?;
    create_symlink(&context_relative, &context_link)?;
    create_symlink(&references_relative, &references_link)?;

    // Add to .gitignore - only the data directory needs to be ignored!
    // The symlinks themselves can be tracked by git
    ensure_gitignore_entry(
        &repo_root,
        "/.thoughts-data",
        Some("Thoughts data directory (created by thoughts init)"),
    )?;

    // Create README files if directories are empty
    create_readme_if_empty(
        &thoughts_target_dir,
        "Thoughts Workspace",
        "This is your unified thoughts workspace.\n\n\
         When configured, your personal thoughts repository will be mounted here.\n\n\
         ## Usage\n\n\
         - Configure thoughts mount: Add `thoughts_mount` to your config\n\
         - This provides a single workspace for all your notes across projects\n\
         - Changes here sync to your personal thoughts repository\n",
    )?;

    create_readme_if_empty(
        &context_target_dir,
        "Context Mounts",
        "This directory contains project-specific context and documentation.\n\n\
         These mounts are shared with your team through the repository config.\n\n\
         ## Suggested Mounts\n\n\
         - `docs` - Project documentation\n\
         - `architecture` - System design documents\n\
         - `decisions` - Architectural decision records\n\
         - `planning` - Feature planning and specs\n",
    )?;

    create_readme_if_empty(
        &references_target_dir,
        "Reference Repositories",
        "This directory contains read-only reference repositories.\n\n\
         References are organized by organization/repository.\n\n\
         ## Usage\n\n\
         - Add references: `thoughts references add <url>`\n\
         - Browse code from other repositories\n\
         - All mounts here are read-only for safety\n",
    )?;

    // Note: The actual mounting happens in a separate sync step
    // This init command only sets up the directory structure and configuration

    // Auto-mount all configured mounts
    println!("\n{} mounts...", "Setting up".green());
    match crate::mount::auto_mount::update_active_mounts().await {
        Ok(_) => {}
        Err(e) => {
            eprintln!("{}: Failed to set up mounts: {}", "Warning".yellow(), e);
            eprintln!(
                "Run {} manually to set up mounts",
                "thoughts mount update".cyan()
            );
        }
    }

    // Success message
    println!("\n{} Successfully initialized thoughts!", "✓".green());
    println!("\nCreated directory structure:");
    println!(
        "  {} -> {} (personal workspace)",
        mount_dirs.thoughts.cyan(),
        thoughts_target_dir
            .strip_prefix(&repo_root)
            .unwrap_or(&thoughts_target_dir)
            .display()
    );
    println!(
        "  {} -> {} (team-shared context)",
        mount_dirs.context.cyan(),
        context_target_dir
            .strip_prefix(&repo_root)
            .unwrap_or(&context_target_dir)
            .display()
    );
    println!(
        "  {} -> {} (reference repos)",
        mount_dirs.references.cyan(),
        references_target_dir
            .strip_prefix(&repo_root)
            .unwrap_or(&references_target_dir)
            .display()
    );
    println!(
        "\nConfiguration saved to: {}",
        ".thoughts/config.json".cyan()
    );
    println!("\nNext steps:");
    println!(
        "  - {} to add a context mount",
        "thoughts mount add <path>".cyan()
    );
    println!(
        "  - {} to add a reference",
        "thoughts references add <url>".cyan()
    );

    Ok(())
}

fn create_symlink(target: &str, link: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
            .with_context(|| format!("Failed to create symlink {link:?} -> {target}"))?;
    }

    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(target, link)
            .with_context(|| format!("Failed to create symlink {:?} -> {}", link, target))?;
    }

    Ok(())
}

fn create_readme_if_empty(dir: &Path, title: &str, content: &str) -> Result<()> {
    let readme_path = dir.join("README.md");

    if !readme_path.exists() {
        let full_content = format!("# {title}\n\n{content}");
        fs::write(&readme_path, full_content)
            .with_context(|| format!("Failed to create README at {readme_path:?}"))?;
    }

    Ok(())
}
