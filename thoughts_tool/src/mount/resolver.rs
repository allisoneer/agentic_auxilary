use crate::config::{Mount, RepoMappingManager};
use crate::mount::utils::normalize_mount_path;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct MountResolver {
    repo_mapping: RepoMappingManager,
}

impl MountResolver {
    pub fn new() -> Result<Self> {
        Ok(Self {
            repo_mapping: RepoMappingManager::new()?,
        })
    }

    /// Resolve a mount to its local filesystem path
    pub fn resolve_mount(&self, mount: &Mount) -> Result<PathBuf> {
        match mount {
            Mount::Directory { path, .. } => {
                // Directory mounts need path normalization
                Ok(normalize_mount_path(path)?)
            }
            Mount::Git { url, subpath, .. } => {
                // Build full URL with subpath if present
                let full_url = if let Some(sub) = subpath {
                    format!("{url}:{sub}")
                } else {
                    url.clone()
                };

                // Resolve through mapping
                self.repo_mapping.resolve_url(&full_url)?.ok_or_else(|| {
                    use colored::Colorize;
                    anyhow::anyhow!(
                        "Repository not cloned: {}\n\n\
                             To fix this, run one of:\n  \
                             • {} (auto-managed)\n  \
                             • {} (custom location)",
                        url,
                        format!("thoughts mount clone {url}").cyan(),
                        format!("thoughts mount clone {url} /your/path").cyan()
                    )
                })
            }
        }
    }

    /// Check if a mount needs cloning
    pub fn needs_clone(&self, mount: &Mount) -> Result<bool> {
        match mount {
            Mount::Directory { .. } => Ok(false),
            Mount::Git { url, .. } => Ok(self.repo_mapping.resolve_url(url)?.is_none()),
        }
    }

    /// Get clone URL and suggested path for a mount
    #[allow(dead_code)]
    // TODO(2): Integrate into clone command for consistency
    pub fn get_clone_info(&self, mount: &Mount) -> Result<Option<(String, PathBuf)>> {
        match mount {
            Mount::Directory { .. } => Ok(None),
            Mount::Git { url, .. } => {
                if self.needs_clone(mount)? {
                    let clone_path = RepoMappingManager::get_default_clone_path(url)?;
                    Ok(Some((url.clone(), clone_path)))
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Resolve all mounts in a config
    #[allow(dead_code)]
    // TODO(2): Keep for future batch operations and diagnostics
    pub fn resolve_all(&self, mounts: &HashMap<String, Mount>) -> Vec<(String, Result<PathBuf>)> {
        mounts
            .iter()
            .map(|(name, mount)| (name.clone(), self.resolve_mount(mount)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SyncStrategy;

    #[test]
    fn test_directory_resolution() {
        let resolver = MountResolver::new().unwrap();
        let mount = Mount::Directory {
            path: PathBuf::from("/home/user/docs"),
            sync: SyncStrategy::None,
        };

        let resolved = resolver.resolve_mount(&mount).unwrap();
        assert_eq!(resolved, PathBuf::from("/home/user/docs"));
    }

    #[test]
    fn test_git_mount_detection() {
        let mount = Mount::Git {
            url: "git@github.com:test/repo.git".to_string(),
            sync: SyncStrategy::Auto,
            subpath: None,
        };

        // Test that we can detect git mounts
        assert!(mount.is_git());
    }
}
