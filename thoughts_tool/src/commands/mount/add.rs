use crate::config::{Mount, SyncStrategy, RepoMappingManager, RepoConfigManager, PersonalConfigManager};
use crate::config::{RepoConfig, PersonalMount, RequiredMount, MountDirs};
use crate::git::utils::{find_repo_root, get_remote_url, is_git_repo};
use crate::utils::paths::expand_path;
use anyhow::{Result, Context, bail};
use colored::*;
use std::path::PathBuf;

pub async fn execute(path: PathBuf, mount_name: Option<String>, personal: bool) -> Result<()> {
    println!("{} mount configuration...", "Adding".green());

    let mut repo_mapping = RepoMappingManager::new()?;
    
    // Convert path to string for URL detection
    let path_str = path.to_string_lossy();
    
    // Create mount based on input type
    let (mount_name, mount) = if is_git_url(&path_str) {
        handle_git_url(&path_str, mount_name, &mut repo_mapping)?
    } else {
        handle_local_path(&path, mount_name, &mut repo_mapping)?
    };
    
    // Extract URL and subpath from mount for saving
    let (url, subpath) = match &mount {
        Mount::Git { url, subpath, .. } => (url.clone(), subpath.clone()),
        Mount::Directory { path, .. } => {
            bail!("Directory mounts are not supported in the new configuration system. Use git repositories instead.");
        }
    };
    
    if personal {
        // Add to personal config at ~/.thoughts/config.json
        let current_repo_url = get_remote_url(&find_repo_root(&std::env::current_dir()?)?)?
            .to_string();
        
        let personal_mount = PersonalMount {
            remote: url.clone(),
            mount_path: mount_name.clone(),
            subpath: subpath.clone(),
            description: format!("Personal mount for {}", mount_name),
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
            description: format!("Repository mount for {}", mount_name),
            optional: false,
            override_rules: None,
            sync: SyncStrategy::Auto,
        };
        config.requires.push(required_mount);
        repo_manager.save(&config)?;
        println!("✓ Added repository mount '{}'", mount_name);
    }
    
    // Automatically update active mounts
    crate::mount::auto_mount::update_active_mounts().await?;
    
    Ok(())
}

fn is_git_url(path: &str) -> bool {
    path.starts_with("git@") || 
    path.starts_with("https://") || 
    path.starts_with("http://") || 
    path.starts_with("ssh://")
}

fn handle_git_url(
    url: &str,
    mount_name: Option<String>,
    repo_mapping: &mut RepoMappingManager
) -> Result<(String, Mount)> {
    println!("Detected git URL: {}", url);
    
    // Parse URL and potential subpath
    let (base_url, subpath) = parse_git_url(url)?;
    
    // Check if already mapped
    if let Some(local_path) = repo_mapping.resolve_url(&base_url)? {
        println!("Found existing clone at: {}", local_path.display());
    } else {
        // Will be cloned on first use
        let default_path = RepoMappingManager::get_default_clone_path(&base_url)?;
        println!("{}: Repository will be cloned to {} on first mount", 
            "Info".yellow(), 
            default_path.display()
        );
        
        // Add mapping for auto-clone
        repo_mapping.add_mapping(base_url.clone(), default_path, true)?;
    }
    
    // Generate mount name if not provided
    let mount_name = mount_name.unwrap_or_else(|| {
        if let Some(sub) = &subpath {
            // Use last component of subpath
            sub.split('/').last().unwrap_or("unnamed").to_string()
        } else {
            // Extract from URL
            crate::config::repo_mapping_manager::extract_repo_name_from_url(&base_url)
                .unwrap_or_else(|_| "unnamed".to_string())
        }
    });
    
    let mount = Mount::Git {
        url: base_url,
        sync: SyncStrategy::Auto,
        subpath,
    };
    
    Ok((mount_name, mount))
}

fn handle_local_path(
    path: &PathBuf,
    mount_name: Option<String>,
    repo_mapping: &mut RepoMappingManager
) -> Result<(String, Mount)> {
    let expanded = expand_path(path)?;
    
    if !expanded.exists() {
        bail!("Path does not exist: {}", expanded.display());
    }
    
    if !expanded.is_dir() {
        bail!("Path is not a directory: {}", expanded.display());
    }
    
    // Check if it's a git repository
    if is_git_repo(&expanded) {
        // Git repository - extract URL
        println!("Detected git repository at: {}", expanded.display());
        
        let url = get_remote_url(&expanded)
            .context("Git repository has no remote URL. Add a remote first.")?;
        
        println!("Found remote URL: {}", url);
        
        // Add mapping
        repo_mapping.add_mapping(url.clone(), expanded.clone(), false)?;
        
        let mount_name = mount_name.unwrap_or_else(|| {
            expanded.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unnamed".to_string())
        });
        
        let mount = Mount::Git {
            url,
            sync: SyncStrategy::Auto,
            subpath: None,
        };
        
        Ok((mount_name, mount))
    } else if let Ok(repo_root) = find_repo_root(&expanded) {
        // Subdirectory of a git repository
        println!("Path is within git repository: {}", repo_root.display());
        
        let url = get_remote_url(&repo_root)
            .context("Parent repository has no remote URL")?;
        
        // Add mapping for repo root
        repo_mapping.add_mapping(url.clone(), repo_root.clone(), false)?;
        
        // Calculate subpath
        let subpath = expanded.strip_prefix(&repo_root)?
            .to_string_lossy()
            .to_string();
        
        let mount_name = mount_name.unwrap_or_else(|| {
            expanded.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unnamed".to_string())
        });
        
        let mount = Mount::Git {
            url,
            sync: SyncStrategy::Auto,
            subpath: Some(subpath),
        };
        
        Ok((mount_name, mount))
    } else {
        // Regular directory
        println!("Regular directory mount");
        
        let mount_name = mount_name.unwrap_or_else(|| {
            expanded.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "unnamed".to_string())
        });
        
        let mount = Mount::Directory {
            path: expanded,
            sync: SyncStrategy::None,
        };
        
        Ok((mount_name, mount))
    }
}

fn parse_git_url(url: &str) -> Result<(String, Option<String>)> {
    // Handle subdirectory syntax: git@github.com:user/repo.git:docs/api
    if let Some(colon_pos) = url.rfind(':') {
        let before = &url[..colon_pos];
        
        // Make sure this isn't the scheme colon (https://)
        if !before.contains("://") && colon_pos > url.find('@').unwrap_or(0) {
            // This is a subpath separator
            let base = before.to_string();
            let sub = url[colon_pos + 1..].to_string();
            
            // Validate the subpath doesn't look like a URL component
            if !sub.contains('/') || sub.contains('@') || sub.contains("://") {
                // This is probably part of the URL, not a subpath
                Ok((url.to_string(), None))
            } else {
                Ok((base, Some(sub)))
            }
        } else {
            Ok((url.to_string(), None))
        }
    } else {
        Ok((url.to_string(), None))
    }
}