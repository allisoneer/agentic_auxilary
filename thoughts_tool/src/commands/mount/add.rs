use crate::config::{Mount, SyncStrategy, RepoMappingManager, RepoConfigManager, PersonalConfigManager};
use crate::config::{RepoConfig, PersonalMount, RequiredMount, MountDirs};
use crate::git::utils::{find_repo_root, get_remote_url, is_git_repo};
use crate::utils::paths::expand_path;
use anyhow::{Result, Context, bail};
use colored::*;
use std::path::PathBuf;

pub async fn execute(
    path: PathBuf,
    mount_path: Option<String>,
    sync: SyncStrategy,
    optional: bool,
    override_rules: Option<bool>,
    personal: bool,
    description: Option<String>,
) -> Result<()> {
    println!("{} mount configuration...", "Adding".green());
    
    let mut repo_mapping = RepoMappingManager::new()?;
    
    // Detect if user provided a Git URL and check if it's been cloned
    let path_str = path.to_string_lossy();
    let resolved_path = if path_str.starts_with("git@") || 
       path_str.starts_with("https://") || 
       path_str.starts_with("http://") ||
       path_str.starts_with("ssh://") {
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
                    path_str, path_str
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
        
        println!("Found remote URL: {}", url);
        
        // Add mapping (auto_managed=false for existing repos)
        repo_mapping.add_mapping(url.clone(), expanded.clone(), false)?;
        
        let mount_name = mount_path.unwrap_or_else(|| {
            expanded.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unnamed".to_string())
        });
        
        (mount_name, url, None)
    } else if let Ok(repo_root) = find_repo_root(&expanded) {
        // Subdirectory of a git repository
        println!("Path is within git repository: {}", repo_root.display());
        
        let url = get_remote_url(&repo_root)
            .context("Parent repository has no remote URL")?;
        
        // Add mapping for repo root (auto_managed=false for existing repos)
        repo_mapping.add_mapping(url.clone(), repo_root.clone(), false)?;
        
        // Calculate subpath
        let subpath = expanded.strip_prefix(&repo_root)?
            .to_string_lossy()
            .to_string();
        
        let mount_name = mount_path.unwrap_or_else(|| {
            expanded.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
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
    
    if personal {
        // Add to personal config at ~/.thoughts/config.json
        let current_repo_url = get_remote_url(&find_repo_root(&std::env::current_dir()?)?)?
            .to_string();
        
        let personal_mount = PersonalMount {
            remote: url.clone(),
            mount_path: mount_name.clone(),
            subpath: subpath.clone(),
            description: description.unwrap_or_else(|| format!("Personal mount for {}", mount_name)),
        };
        
        PersonalConfigManager::add_repository_mount(&current_repo_url, personal_mount)?;
        println!("✓ Added personal mount '{}'", mount_name);
    } else {
        // Add to repository config at .thoughts/config.json
        let repo_root = find_repo_root(&std::env::current_dir()?)?
            .to_path_buf();
        let repo_manager = RepoConfigManager::new(repo_root);
        let mut config = repo_manager.load()?.unwrap_or_else(|| RepoConfig {
            version: "1.0".to_string(),
            mount_dirs: MountDirs::default(),
            requires: vec![],
            rules: vec![],
        });
        
        // Check for duplicates in requires
        if config.requires.iter().any(|r| r.mount_path == mount_name) {
            bail!("Mount '{}' already exists", mount_name);
        }
        
        let required_mount = RequiredMount {
            remote: url,
            mount_path: mount_name.clone(),
            subpath,
            description: description.unwrap_or_else(|| format!("Repository mount for {}", mount_name)),
            optional,
            override_rules,
            sync,
        };
        config.requires.push(required_mount);
        repo_manager.save(&config)?;
        println!("✓ Added repository mount '{}'", mount_name);
    }
    
    // Automatically update active mounts
    crate::mount::auto_mount::update_active_mounts().await?;
    
    Ok(())
}