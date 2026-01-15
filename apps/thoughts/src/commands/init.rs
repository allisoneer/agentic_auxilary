use crate::config::RepoConfigManager;
use crate::git::utils::{
    get_control_repo_root, get_current_repo, get_main_repo_for_worktree, is_worktree,
};
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

        // Idempotent check-and-act for worktree .thoughts-data symlink
        let mut had_errors = false;
        match ensure_symlink_abs_target(&worktree_thoughts_data, &main_thoughts_data, force) {
            Ok(SymlinkOutcome::Created) => {
                eprintln!(
                    "{}: Created .thoughts-data symlink to main repository",
                    "Success".green()
                );
            }
            Ok(SymlinkOutcome::AlreadyCorrect) => {
                eprintln!("{}: .thoughts-data symlink already correct", "Info".cyan());
            }
            Ok(SymlinkOutcome::Fixed) => {
                eprintln!(
                    "{}: Fixed .thoughts-data symlink to main repository",
                    "Success".green()
                );
            }
            Ok(SymlinkOutcome::NeedsForce { current_target }) => {
                had_errors = true;
                eprintln!(
                    "{}: .thoughts-data symlink points to {} but should point to {}",
                    "Error".red(),
                    current_target.display(),
                    main_thoughts_data.display()
                );
                eprintln!(
                    "Re-run with {} to update the symlink safely.",
                    "--force".cyan()
                );
            }
            Ok(SymlinkOutcome::NonSymlinkExists) => {
                had_errors = true;
                eprintln!(
                    "{}: .thoughts-data exists but is not a symlink",
                    "Error".red()
                );
                eprintln!(
                    "Please remove/rename it manually (not removed automatically to avoid data loss)."
                );
            }
            Err(e) => {
                had_errors = true;
                eprintln!(
                    "{}: Failed to ensure .thoughts-data symlink: {}",
                    "Error".red(),
                    e
                );
            }
        }

        // Get mount dirs from config to create workspace symlinks
        let repo_config_manager = RepoConfigManager::new(get_control_repo_root(&repo_root)?);
        let mount_dirs = if let Some(desired) = repo_config_manager.load_desired_state()? {
            desired.mount_dirs
        } else {
            // Use v1 defaults that will map to v2
            crate::config::MountDirsV2 {
                thoughts: "thoughts".into(),
                context: "context".into(),
                references: "references".into(),
            }
        };

        // Create the three workspace symlinks in the worktree
        let thoughts_link = repo_root.join(&mount_dirs.thoughts);
        let context_link = repo_root.join(&mount_dirs.context);
        let references_link = repo_root.join(&mount_dirs.references);

        let thoughts_relative = format!(".thoughts-data/{}", mount_dirs.thoughts);
        let context_relative = format!(".thoughts-data/{}", mount_dirs.context);
        let references_relative = format!(".thoughts-data/{}", mount_dirs.references);

        // Idempotent workspace symlinks (relative links to .thoughts-data/*)
        for (name, link, rel, abs_target) in [
            (
                &mount_dirs.thoughts,
                &thoughts_link,
                &thoughts_relative,
                main_thoughts_data.join(&mount_dirs.thoughts),
            ),
            (
                &mount_dirs.context,
                &context_link,
                &context_relative,
                main_thoughts_data.join(&mount_dirs.context),
            ),
            (
                &mount_dirs.references,
                &references_link,
                &references_relative,
                main_thoughts_data.join(&mount_dirs.references),
            ),
        ] {
            match ensure_symlink_rel_target(link, rel, &abs_target, force) {
                Ok(SymlinkOutcome::Created) => {
                    eprintln!("{}: Created {} -> {}", "Success".green(), name, rel);
                }
                Ok(SymlinkOutcome::AlreadyCorrect) => {
                    eprintln!("{}: {} symlink already correct", "Info".cyan(), name);
                }
                Ok(SymlinkOutcome::Fixed) => {
                    eprintln!("{}: Fixed {} symlink", "Success".green(), name);
                }
                Ok(SymlinkOutcome::NeedsForce { current_target }) => {
                    had_errors = true;
                    eprintln!(
                        "{}: {} symlink points to {} but should be {}",
                        "Error".red(),
                        name,
                        current_target.display(),
                        rel
                    );
                    eprintln!(
                        "Re-run with {} to update the symlink safely.",
                        "--force".cyan()
                    );
                }
                Ok(SymlinkOutcome::NonSymlinkExists) => {
                    had_errors = true;
                    eprintln!("{}: {} exists but is not a symlink", "Error".red(), name);
                    eprintln!(
                        "Please remove/rename it manually (not removed automatically to avoid data loss)."
                    );
                }
                Err(e) => {
                    had_errors = true;
                    eprintln!(
                        "{}: Failed to ensure {} symlink: {}",
                        "Error".red(),
                        name,
                        e
                    );
                }
            }
        }

        // Inject Claude Code permissions (worktree)
        {
            match crate::utils::claude_settings::inject_additional_directories(&repo_root) {
                Ok(summary) => {
                    if !summary.warn_conflicting_denies.is_empty() {
                        eprintln!(
                            "{}: Some allow rules may be shadowed by deny rules: {:?}",
                            "Warning".yellow(),
                            summary.warn_conflicting_denies
                        );
                    }
                    let new_items =
                        summary.added_additional_dirs.len() + summary.added_allow_rules.len();
                    if new_items > 0 {
                        eprintln!(
                            "{}: Updated Claude Code permissions ({} new item{})",
                            "Success".green(),
                            new_items,
                            if new_items == 1 { "" } else { "s" }
                        );
                        eprintln!("  {}", summary.settings_path.display());
                    } else {
                        eprintln!(
                            "{}: Claude Code permissions already present; no changes needed",
                            "Info".cyan()
                        );
                    }
                }
                Err(e) => {
                    eprintln!(
                        "{}: Failed to update Claude Code settings: {}",
                        "Warning".yellow(),
                        e
                    );
                    eprintln!("Proceeding without updating .claude/settings.local.json");
                }
            }
        }

        // Ensure gitignore has backup patterns
        let _ = ensure_gitignore_entry(
            &repo_root,
            "/.claude/settings.local.json.bak",
            Some("Claude settings backup (managed by thoughts)"),
        );
        let _ = ensure_gitignore_entry(
            &repo_root,
            "/.claude/settings.local.json.malformed.*.bak",
            Some("Claude settings quarantine backups (auto-pruned)"),
        );

        if had_errors {
            // Return a genuine error after best-effort injection/gitignore updates
            return Err(anyhow!(
                "One or more symlinks require --force or manual cleanup"
            ));
        }
        eprintln!("{}: Worktree initialization complete!", "Success".green());
        eprintln!("The worktree now shares mounts with the main repository.");
        eprintln!("\nCreated workspace symlinks:");
        eprintln!("  {} -> {}", mount_dirs.thoughts, thoughts_relative);
        eprintln!("  {} -> {}", mount_dirs.context, context_relative);
        eprintln!("  {} -> {}", mount_dirs.references, references_relative);

        return Ok(());
    }

    // Continue with normal initialization for main repository...
    println!(
        "Initializing thoughts for repository at: {}",
        repo_root.display()
    );

    // Load or create repository configuration
    let repo_config_manager = RepoConfigManager::new(repo_root.clone());
    let was_v1 = matches!(repo_config_manager.peek_config_version()?, Some(v) if v == "1.0");
    let cfg_v2 = repo_config_manager
        .ensure_v2_default()
        .context("Failed to create repository configuration")?;
    let mount_dirs = cfg_v2.mount_dirs.clone();

    if was_v1 {
        println!(
            "Upgraded to v2 config. A v1 backup was created if non-empty. See MIGRATION_V1_TO_V2.md"
        );
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

    // Resolve symlink targets and ensure idempotently
    let thoughts_link = repo_root.join(&mount_dirs.thoughts);
    let context_link = repo_root.join(&mount_dirs.context);
    let references_link = repo_root.join(&mount_dirs.references);

    let mut had_errors = false;

    // Create or validate symlinks with relative targets
    let thoughts_relative = format!(".thoughts-data/{}", mount_dirs.thoughts);
    let context_relative = format!(".thoughts-data/{}", mount_dirs.context);
    let references_relative = format!(".thoughts-data/{}", mount_dirs.references);

    for (name, link, rel, abs_target) in [
        (
            &mount_dirs.thoughts,
            &thoughts_link,
            &thoughts_relative,
            thoughts_target_dir.clone(),
        ),
        (
            &mount_dirs.context,
            &context_link,
            &context_relative,
            context_target_dir.clone(),
        ),
        (
            &mount_dirs.references,
            &references_link,
            &references_relative,
            references_target_dir.clone(),
        ),
    ] {
        match ensure_symlink_rel_target(link, rel, &abs_target, force) {
            Ok(SymlinkOutcome::Created) => {
                println!("{} Created {} -> {}", "✓".green(), name, rel);
            }
            Ok(SymlinkOutcome::AlreadyCorrect) => {
                println!("{} {} symlink already correct", "Info".cyan(), name);
            }
            Ok(SymlinkOutcome::Fixed) => {
                println!("{} Fixed {} symlink", "✓".green(), name);
            }
            Ok(SymlinkOutcome::NeedsForce { current_target }) => {
                had_errors = true;
                eprintln!(
                    "{}: {} symlink points to {} but should be {}",
                    "Error".red(),
                    name,
                    current_target.display(),
                    rel
                );
                eprintln!(
                    "Re-run with {} to update the symlink safely.",
                    "--force".cyan()
                );
            }
            Ok(SymlinkOutcome::NonSymlinkExists) => {
                had_errors = true;
                eprintln!("{}: {} exists but is not a symlink", "Error".red(), name);
                eprintln!(
                    "Please remove/rename it manually (not removed automatically to avoid data loss)."
                );
            }
            Err(e) => {
                had_errors = true;
                eprintln!(
                    "{}: Failed to ensure {} symlink: {}",
                    "Error".red(),
                    name,
                    e
                );
            }
        }
    }

    // Add to .gitignore - only the data directory needs to be ignored!
    // The symlinks themselves can be tracked by git
    ensure_gitignore_entry(
        &repo_root,
        "/.thoughts-data",
        Some("Thoughts data directory (created by thoughts init)"),
    )?;
    // Also ensure Claude backup patterns (best-effort; ignore errors)
    let _ = ensure_gitignore_entry(
        &repo_root,
        "/.claude/settings.local.json.bak",
        Some("Claude settings backup (managed by thoughts)"),
    );
    let _ = ensure_gitignore_entry(
        &repo_root,
        "/.claude/settings.local.json.malformed.*.bak",
        Some("Claude settings quarantine backups (auto-pruned)"),
    );

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

    // Inject Claude Code permissions (regular repo)
    {
        match crate::utils::claude_settings::inject_additional_directories(&repo_root) {
            Ok(summary) => {
                if !summary.warn_conflicting_denies.is_empty() {
                    println!(
                        "{}: Some allow rules may be shadowed by deny rules: {:?}",
                        "Warning".yellow(),
                        summary.warn_conflicting_denies
                    );
                }
                let new_items =
                    summary.added_additional_dirs.len() + summary.added_allow_rules.len();
                if new_items > 0 {
                    println!(
                        "{} Updated Claude Code permissions ({} new item{})",
                        "✓".green(),
                        new_items,
                        if new_items == 1 { "" } else { "s" }
                    );
                    println!("  {}", summary.settings_path.display());
                } else {
                    println!(
                        "{} Claude Code permissions already present; no changes needed",
                        "Info".cyan()
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "{}: Failed to update Claude Code settings: {}",
                    "Warning".yellow(),
                    e
                );
                eprintln!("Proceeding without updating .claude/settings.local.json");
            }
        }
    }

    if had_errors {
        return Err(anyhow!(
            "One or more symlinks require --force or manual cleanup"
        ));
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
    std::os::unix::fs::symlink(target, link)
        .with_context(|| format!("Failed to create symlink {link:?} -> {target}"))?;
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

#[derive(Debug)]
enum SymlinkOutcome {
    Created,
    AlreadyCorrect,
    Fixed,
    NeedsForce { current_target: PathBuf },
    NonSymlinkExists,
}

/// Ensure that `link` is a symlink pointing to the absolute `abs_target`.
/// If incorrect and `force` is true, fix it. If force is false, return NeedsForce.
/// Never delete non-symlinks.
fn ensure_symlink_abs_target(
    link: &Path,
    abs_target: &Path,
    force: bool,
) -> Result<SymlinkOutcome> {
    if !link.exists() {
        create_symlink(&abs_target.to_string_lossy(), link)?;
        return Ok(SymlinkOutcome::Created);
    }
    let meta = fs::symlink_metadata(link)?;
    if !meta.file_type().is_symlink() {
        return Ok(SymlinkOutcome::NonSymlinkExists);
    }
    // Compare resolved targets; fall back to literal compare on failure
    let current = fs::read_link(link).unwrap_or_default();
    let resolved_link = fs::canonicalize(link);
    let resolved_expected = fs::canonicalize(abs_target);
    let is_correct = match (resolved_link, resolved_expected) {
        (Ok(a), Ok(b)) => a == b,
        _ => current == abs_target, // fallback
    };
    if is_correct {
        return Ok(SymlinkOutcome::AlreadyCorrect);
    }
    if force {
        fs::remove_file(link)?;
        create_symlink(&abs_target.to_string_lossy(), link)?;
        Ok(SymlinkOutcome::Fixed)
    } else {
        Ok(SymlinkOutcome::NeedsForce {
            current_target: current,
        })
    }
}

/// Ensure that `link` points to the relative target string `rel_target`.
/// For correctness check, we prefer resolved path equality against `abs_target`,
/// falling back to literal link text equality with `rel_target`.
fn ensure_symlink_rel_target(
    link: &Path,
    rel_target: &str,
    abs_target: &Path,
    force: bool,
) -> Result<SymlinkOutcome> {
    if !link.exists() {
        create_symlink(rel_target, link)?;
        return Ok(SymlinkOutcome::Created);
    }
    let meta = fs::symlink_metadata(link)?;
    if !meta.file_type().is_symlink() {
        return Ok(SymlinkOutcome::NonSymlinkExists);
    }
    let current = fs::read_link(link).unwrap_or_default();

    let resolved_link = fs::canonicalize(link);
    let resolved_expected = fs::canonicalize(abs_target);
    let is_correct = match (resolved_link, resolved_expected) {
        (Ok(a), Ok(b)) => a == b,
        _ => current == Path::new(rel_target),
    };

    if is_correct {
        return Ok(SymlinkOutcome::AlreadyCorrect);
    }
    if force {
        fs::remove_file(link)?;
        create_symlink(rel_target, link)?;
        Ok(SymlinkOutcome::Fixed)
    } else {
        Ok(SymlinkOutcome::NeedsForce {
            current_target: current,
        })
    }
}
