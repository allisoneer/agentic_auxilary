use crate::config::{RepoConfigManager, RepoConfig, MountDirs};
use crate::git::utils::{get_current_repo, is_worktree, get_main_repo_for_worktree};
use crate::utils::paths::ensure_dir;
use crate::utils::git::ensure_gitignore_entry;
use anyhow::{Result, Context, anyhow};
use std::path::{Path, PathBuf};
use std::fs;
use colored::Colorize;

pub async fn execute(force: bool) -> Result<()> {
    // Ensure we're in a git repository
    let repo_root = get_current_repo()
        .context("Not in a git repository. Run 'git init' first.")?;
    
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
        if worktree_thoughts_data.exists() && !force {
            if worktree_thoughts_data.is_symlink() {
                eprintln!("{}: Worktree already initialized", "Info".cyan());
                let target = fs::read_link(&worktree_thoughts_data).unwrap_or_else(|_| PathBuf::from("<invalid>"));
                eprintln!("  .thoughts-data -> {}", target.display());
                return Ok(());
            }
        }
        
        // Clean up if forcing
        if force && worktree_thoughts_data.exists() {
            fs::remove_file(&worktree_thoughts_data)
                .with_context(|| format!("Failed to remove existing symlink: {:?}", worktree_thoughts_data))?;
        }
        
        // Create the symlink: .thoughts-data -> main repo's .thoughts-data
        create_symlink(&main_thoughts_data.to_string_lossy(), &worktree_thoughts_data)?;
        eprintln!("{}: Created .thoughts-data symlink to main repository", "Success".green());
        eprintln!("{}: Worktree initialization complete!", "Success".green());
        eprintln!("The worktree now shares mounts with the main repository.");
        
        return Ok(());
    }
    
    // Continue with normal initialization for main repository...
    println!("Initializing thoughts for repository at: {}", repo_root.display());
    
    // Load or create repository configuration
    let repo_config_manager = RepoConfigManager::new(repo_root.clone());
    let config = repo_config_manager.ensure_default()
        .context("Failed to create repository configuration")?;
    
    // Check for existing symlinks
    let context_link = repo_root.join(&config.mount_dirs.repository);
    let personal_link = repo_root.join(&config.mount_dirs.personal);
    
    if !force {
        let mut existing = vec![];
        if context_link.exists() {
            existing.push((&config.mount_dirs.repository, &context_link));
        }
        if personal_link.exists() {
            existing.push((&config.mount_dirs.personal, &personal_link));
        }
        
        if !existing.is_empty() {
            eprintln!("{}: Repository already has thoughts directories:", "Error".red());
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
    for (name, path) in [(&config.mount_dirs.repository, &context_link), 
                         (&config.mount_dirs.personal, &personal_link)] {
        if path.exists() && !path.is_symlink() {
            eprintln!("{}: {} exists but is not a symlink", "Error".red(), name);
            eprintln!("Please remove it manually or use {}", "--force".cyan());
            std::process::exit(1);
        }
    }
    
    // Create the actual thoughts directory structure
    let thoughts_dir = repo_root.join(".thoughts-data");
    ensure_dir(&thoughts_dir)?;
    
    let context_dir = thoughts_dir.join(&config.mount_dirs.repository);
    let personal_dir = thoughts_dir.join(&config.mount_dirs.personal);
    
    ensure_dir(&context_dir)?;
    ensure_dir(&personal_dir)?;
    
    // Remove existing symlinks if forcing
    if force {
        for path in [&context_link, &personal_link] {
            if path.exists() && path.is_symlink() {
                fs::remove_file(path)
                    .with_context(|| format!("Failed to remove existing symlink: {:?}", path))?;
            }
        }
    }
    
    // Create symlinks with relative paths
    let context_relative = format!(".thoughts-data/{}", config.mount_dirs.repository);
    let personal_relative = format!(".thoughts-data/{}", config.mount_dirs.personal);
    create_symlink(&context_relative, &context_link)?;
    create_symlink(&personal_relative, &personal_link)?;
    
    // Add to .gitignore - only the data directory needs to be ignored!
    // The symlinks themselves can be tracked by git
    ensure_gitignore_entry(&repo_root, "/.thoughts-data",
                          Some("Thoughts data directory (created by thoughts init)"))?;
    
    // Create README files if directories are empty
    create_readme_if_empty(&context_dir, "Repository Thoughts", 
        "This directory contains repository-specific thoughts and documentation.\n\n\
         These files are shared with your team through git.\n\n\
         ## Suggested Structure\n\n\
         - `architecture/` - System design and architecture documents\n\
         - `decisions/` - Architectural decision records (ADRs)\n\
         - `planning/` - Feature planning and specifications\n\
         - `research/` - Technical research and investigations\n")?;
    
    create_readme_if_empty(&personal_dir, "Personal Thoughts",
        "This directory contains your personal thoughts about this repository.\n\n\
         These files are private to you and not shared with the team.\n\n\
         ## Suggested Structure\n\n\
         - `todo.md` - Personal task list and notes\n\
         - `ideas/` - Feature ideas and brainstorming\n\
         - `learning/` - Notes while learning the codebase\n\
         - `debugging/` - Debugging notes and investigations\n")?;
    
    // Note: The actual mounting of personal mounts happens in a separate sync step
    // This init command only sets up the directory structure and configuration
    
    // Auto-mount all configured mounts
    println!("\n{} mounts...", "Setting up".green());
    match crate::mount::auto_mount::update_active_mounts().await {
        Ok(_) => {},
        Err(e) => {
            eprintln!("{}: Failed to set up mounts: {}", "Warning".yellow(), e);
            eprintln!("Run {} manually to set up mounts", "thoughts mount update".cyan());
        }
    }
    
    // Success message
    println!("\n{} Successfully initialized thoughts!", "✓".green());
    println!("\nCreated directory structure:");
    println!("  {} -> {} (team-shared thoughts)", 
             config.mount_dirs.repository.cyan(), 
             context_dir.strip_prefix(&repo_root).unwrap_or(&context_dir).display());
    println!("  {} -> {} (personal thoughts)", 
             config.mount_dirs.personal.cyan(),
             personal_dir.strip_prefix(&repo_root).unwrap_or(&personal_dir).display());
    println!("\nConfiguration saved to: {}", ".thoughts/config.json".cyan());
    println!("\nYour configured mounts are now available in the thoughts/ directory.");
    println!("\nNext steps:");
    println!("  - {} to add a repository mount", "thoughts mount add <url>".cyan());
    println!("  - {} to add a personal mount", "thoughts mount add <url> --personal".cyan());
    
    Ok(())
}

fn create_symlink(target: &str, link: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
            .with_context(|| format!("Failed to create symlink {:?} -> {}", link, target))?;
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
        let full_content = format!("# {}\n\n{}", title, content);
        fs::write(&readme_path, full_content)
            .with_context(|| format!("Failed to create README at {:?}", readme_path))?;
    }
    
    Ok(())
}