use crate::config::{Mount, PersonalConfigManager, RepoConfigManager, SyncStrategy};
use crate::git::utils::get_remote_url;
use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct MountMerger {
    repo_root: PathBuf,
}

impl MountMerger {
    pub fn new(repo_root: PathBuf) -> Self {
        Self { repo_root }
    }

    pub async fn get_all_mounts(&self) -> Result<HashMap<String, (Mount, MountSource)>> {
        let mut mounts = HashMap::new();

        // 1. Load repository mounts from .thoughts/config.json
        let repo_manager = RepoConfigManager::new(self.repo_root.clone());
        if let Some(repo_config) = repo_manager.load()? {
            // Repository mounts come from 'requires' field
            for required in &repo_config.requires {
                let mount = Mount::Git {
                    url: required.remote.clone(),
                    sync: required.sync,
                    subpath: required.subpath.clone(),
                };
                let name = required.mount_path.clone();
                mounts.insert(name, (mount, MountSource::Repository));
            }
        }

        // 2. Get current repository URL for pattern matching
        let repo_url = get_remote_url(&self.repo_root)?;

        // 3. Load personal mounts for this specific repository
        // These are stored in ~/.thoughts/config.json under repository_mounts[repo_url]
        let personal_mounts = PersonalConfigManager::get_repository_mounts(&repo_url)?;
        for pm in personal_mounts {
            let mount = Mount::Git {
                url: pm.remote.clone(),
                sync: SyncStrategy::Auto,
                subpath: pm.subpath.clone(),
            };
            mounts.insert(pm.mount_path, (mount, MountSource::Personal));
        }

        // 4. Evaluate patterns to find matching mounts
        // Patterns like "git@github.com:mycompany/*" match repository URLs
        if let Some(personal_config) = PersonalConfigManager::load()? {
            for pattern in &personal_config.patterns {
                if pattern_matches(&pattern.match_remote, &repo_url) {
                    for pm in &pattern.personal_mounts {
                        let mount = Mount::Git {
                            url: pm.remote.clone(),
                            sync: SyncStrategy::Auto,
                            subpath: pm.subpath.clone(),
                        };
                        mounts.insert(pm.mount_path.clone(), (mount, MountSource::Pattern));
                    }
                }
            }
        }

        Ok(mounts)
    }
}

#[derive(Debug)]
pub enum MountSource {
    Repository, // From .thoughts/config.json in repository
    Personal,   // From ~/.thoughts/config.json direct mount for this repo
    Pattern,    // From ~/.thoughts/config.json pattern match
}

fn pattern_matches(pattern: &str, url: &str) -> bool {
    // Convert pattern to regex, supporting * wildcard
    let regex_pattern = pattern.replace(".", r"\.").replace("*", ".*");

    regex::Regex::new(&format!("^{regex_pattern}$"))
        .map(|re| re.is_match(url))
        .unwrap_or(false)
}
