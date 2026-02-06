use super::types::{RepoLocation, RepoMapping};
use crate::repo_identity::{
    RepoIdentity, RepoIdentityKey, parse_url_and_subpath as identity_parse_url_and_subpath,
};
use crate::utils::locks::FileLock;
use crate::utils::paths::{self, sanitize_dir_name};
use anyhow::{Context, Result, bail};
use atomicwrites::{AllowOverwrite, AtomicFile};
use std::io::Write;
use std::path::PathBuf;

/// Indicates how a URL was resolved to a mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlResolutionKind {
    /// The URL matched exactly as stored in repos.json
    Exact,
    /// The URL matched via canonical identity comparison (different scheme/format)
    CanonicalFallback,
}

/// Details about a resolved URL mapping.
#[derive(Debug, Clone)]
pub struct ResolvedUrl {
    /// The key in repos.json that matched
    pub matched_url: String,
    /// How the match was found
    pub resolution: UrlResolutionKind,
    /// The location details (cloned)
    pub location: RepoLocation,
}

pub struct RepoMappingManager {
    mapping_path: PathBuf,
}

impl RepoMappingManager {
    pub fn new() -> Result<Self> {
        let mapping_path = paths::get_repo_mapping_path()?;
        Ok(Self { mapping_path })
    }

    /// Get the lock file path for repos.json RMW operations.
    fn lock_path(&self) -> PathBuf {
        let name = self
            .mapping_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        self.mapping_path.with_file_name(format!("{name}.lock"))
    }

    pub fn load(&self) -> Result<RepoMapping> {
        if !self.mapping_path.exists() {
            // First time - create empty mapping
            let default = RepoMapping::default();
            self.save(&default)?;
            return Ok(default);
        }

        let contents = std::fs::read_to_string(&self.mapping_path)
            .context("Failed to read repository mapping file")?;
        let mapping: RepoMapping =
            serde_json::from_str(&contents).context("Failed to parse repository mapping")?;
        Ok(mapping)
    }

    pub fn save(&self, mapping: &RepoMapping) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = self.mapping_path.parent() {
            paths::ensure_dir(parent)?;
        }

        // Atomic write for safety
        let json = serde_json::to_string_pretty(mapping)?;
        let af = AtomicFile::new(&self.mapping_path, AllowOverwrite);
        af.write(|f| f.write_all(json.as_bytes()))?;

        Ok(())
    }

    /// Resolve a git URL with detailed resolution information.
    ///
    /// Returns the matched URL key, resolution kind, location, and optional subpath.
    pub fn resolve_url_with_details(
        &self,
        url: &str,
    ) -> Result<Option<(ResolvedUrl, Option<String>)>> {
        let mapping = self.load()?; // read-only; atomic writes make this safe
        let (base_url, subpath) = parse_url_and_subpath(url);

        // Try exact match first
        if let Some(loc) = mapping.mappings.get(&base_url) {
            return Ok(Some((
                ResolvedUrl {
                    matched_url: base_url,
                    resolution: UrlResolutionKind::Exact,
                    location: loc.clone(),
                },
                subpath,
            )));
        }

        // Canonical fallback: parse target URL and find a matching key
        let target_key = match RepoIdentity::parse(&base_url) {
            Ok(id) => id.canonical_key(),
            Err(_) => return Ok(None),
        };

        let mut matches: Vec<(String, RepoLocation)> = mapping
            .mappings
            .iter()
            .filter_map(|(k, v)| {
                let (k_base, _) = parse_url_and_subpath(k);
                let key = RepoIdentity::parse(&k_base).ok()?.canonical_key();
                (key == target_key).then(|| (k.clone(), v.clone()))
            })
            .collect();

        // Sort for deterministic selection
        matches.sort_by(|a, b| a.0.cmp(&b.0));

        if let Some((matched_url, location)) = matches.into_iter().next() {
            return Ok(Some((
                ResolvedUrl {
                    matched_url,
                    resolution: UrlResolutionKind::CanonicalFallback,
                    location,
                },
                subpath,
            )));
        }

        Ok(None)
    }

    /// Resolve a git URL to its local path.
    ///
    /// Uses exact match first, then falls back to canonical identity matching
    /// to handle URL scheme variants (SSH vs HTTPS).
    pub fn resolve_url(&self, url: &str) -> Result<Option<PathBuf>> {
        if let Some((resolved, subpath)) = self.resolve_url_with_details(url)? {
            let mut p = resolved.location.path.clone();
            if let Some(sub) = subpath {
                p = p.join(sub);
            }
            return Ok(Some(p));
        }
        Ok(None)
    }

    /// Add a URL-to-path mapping with identity-based upsert.
    ///
    /// If a mapping with the same canonical identity already exists,
    /// it will be replaced (preserving any existing last_sync time).
    /// This prevents duplicate entries for SSH vs HTTPS variants.
    pub fn add_mapping(&mut self, url: String, path: PathBuf, auto_managed: bool) -> Result<()> {
        let _lock = FileLock::lock_exclusive(self.lock_path())?;
        let mut mapping = self.load()?; // safe under lock for RMW

        // Basic validation
        if !path.exists() {
            bail!("Path does not exist: {}", path.display());
        }

        if !path.is_dir() {
            bail!("Path is not a directory: {}", path.display());
        }

        let (base_url, _) = parse_url_and_subpath(&url);
        let new_key = RepoIdentity::parse(&base_url)?.canonical_key();

        // Find all existing entries with the same canonical identity
        let matching_urls: Vec<String> = mapping
            .mappings
            .keys()
            .filter_map(|k| {
                let (k_base, _) = parse_url_and_subpath(k);
                let key = RepoIdentity::parse(&k_base).ok()?.canonical_key();
                (key == new_key).then(|| k.clone())
            })
            .collect();

        // Preserve last_sync from any existing entry
        let preserved_last_sync = matching_urls
            .iter()
            .filter_map(|k| mapping.mappings.get(k).and_then(|loc| loc.last_sync))
            .max();

        // Remove all matching entries
        for k in matching_urls {
            mapping.mappings.remove(&k);
        }

        // Insert the new mapping
        mapping.mappings.insert(
            base_url,
            RepoLocation {
                path,
                auto_managed,
                last_sync: preserved_last_sync,
            },
        );

        self.save(&mapping)?;
        Ok(())
    }

    /// Remove a URL mapping
    #[allow(dead_code)]
    // TODO(2): Add "thoughts mount unmap" command for cleanup
    pub fn remove_mapping(&mut self, url: &str) -> Result<()> {
        let _lock = FileLock::lock_exclusive(self.lock_path())?;
        let mut mapping = self.load()?;
        mapping.mappings.remove(url);
        self.save(&mapping)?;
        Ok(())
    }

    /// Check if a URL is auto-managed
    pub fn is_auto_managed(&self, url: &str) -> Result<bool> {
        let mapping = self.load()?;
        Ok(mapping
            .mappings
            .get(url)
            .map(|loc| loc.auto_managed)
            .unwrap_or(false))
    }

    /// Get default clone path for a URL using hierarchical layout.
    ///
    /// Returns `~/.thoughts/clones/{host}/{org_path}/{repo}` with sanitized directory names.
    pub fn get_default_clone_path(url: &str) -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

        let (base_url, _sub) = parse_url_and_subpath(url);
        let id = RepoIdentity::parse(&base_url)?;
        let key = id.canonical_key(); // use canonical for stable paths across case/scheme

        let mut p = home
            .join(".thoughts")
            .join("clones")
            .join(sanitize_dir_name(&key.host));
        for seg in key.org_path.split('/') {
            if !seg.is_empty() {
                p = p.join(sanitize_dir_name(seg));
            }
        }
        p = p.join(sanitize_dir_name(&key.repo));
        Ok(p)
    }

    /// Update last sync time for a URL.
    ///
    /// Uses canonical fallback to update the correct entry even if the URL
    /// scheme differs from what's stored.
    pub fn update_sync_time(&mut self, url: &str) -> Result<()> {
        let _lock = FileLock::lock_exclusive(self.lock_path())?;
        let mut mapping = self.load()?;
        let now = chrono::Utc::now();

        // Prefer exact base_url key
        let (base_url, _) = parse_url_and_subpath(url);
        if let Some(loc) = mapping.mappings.get_mut(&base_url) {
            loc.last_sync = Some(now);
            self.save(&mapping)?;
            return Ok(());
        }

        // Fall back to canonical match
        // Note: We need to find the matching key without holding a mutable borrow
        let target_key = match RepoIdentity::parse(&base_url) {
            Ok(id) => id.canonical_key(),
            Err(_) => return Ok(()),
        };

        let matched_key: Option<String> = mapping
            .mappings
            .keys()
            .filter_map(|k| {
                let (k_base, _) = parse_url_and_subpath(k);
                let key = RepoIdentity::parse(&k_base).ok()?.canonical_key();
                (key == target_key).then(|| k.clone())
            })
            .next();

        if let Some(key) = matched_key
            && let Some(loc) = mapping.mappings.get_mut(&key)
        {
            loc.last_sync = Some(now);
            self.save(&mapping)?;
        }

        Ok(())
    }

    /// Get the canonical identity key for a URL, if parseable.
    pub fn get_canonical_key(url: &str) -> Option<RepoIdentityKey> {
        let (base, _) = parse_url_and_subpath(url);
        RepoIdentity::parse(&base).ok().map(|id| id.canonical_key())
    }
}

/// Parse a URL into (base_url, optional_subpath).
///
/// Delegates to the repo_identity module for robust port-aware parsing.
pub fn parse_url_and_subpath(url: &str) -> (String, Option<String>) {
    identity_parse_url_and_subpath(url)
}

pub fn extract_repo_name_from_url(url: &str) -> Result<String> {
    let url = url.trim_end_matches(".git");

    // Handle different URL formats
    if let Some(pos) = url.rfind('/') {
        Ok(url[pos + 1..].to_string())
    } else if let Some(pos) = url.rfind(':') {
        // SSH format like git@github.com:user/repo
        if let Some(slash_pos) = url[pos + 1..].rfind('/') {
            Ok(url[pos + 1 + slash_pos + 1..].to_string())
        } else {
            Ok(url[pos + 1..].to_string())
        }
    } else {
        bail!("Cannot extract repository name from URL: {}", url)
    }
}

/// Extract org_path and repo from a URL.
///
/// Delegates to RepoIdentity for robust parsing that handles:
/// - SSH with ports: `ssh://git@host:2222/org/repo.git`
/// - GitLab subgroups: `https://gitlab.com/a/b/c/repo.git`
/// - Azure DevOps: `https://dev.azure.com/org/proj/_git/repo`
pub fn extract_org_repo_from_url(url: &str) -> anyhow::Result<(String, String)> {
    let (base, _) = parse_url_and_subpath(url);
    let id = RepoIdentity::parse(&base)?;
    Ok((id.org_path, id.repo))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_url_and_subpath() {
        let (url, sub) = parse_url_and_subpath("git@github.com:user/repo.git");
        assert_eq!(url, "git@github.com:user/repo.git");
        assert_eq!(sub, None);

        let (url, sub) = parse_url_and_subpath("git@github.com:user/repo.git:docs/api");
        assert_eq!(url, "git@github.com:user/repo.git");
        assert_eq!(sub, Some("docs/api".to_string()));

        let (url, sub) = parse_url_and_subpath("https://github.com/user/repo");
        assert_eq!(url, "https://github.com/user/repo");
        assert_eq!(sub, None);
    }

    #[test]
    fn test_extract_repo_name() {
        assert_eq!(
            extract_repo_name_from_url("git@github.com:user/repo.git").unwrap(),
            "repo"
        );
        assert_eq!(
            extract_repo_name_from_url("https://github.com/user/repo").unwrap(),
            "repo"
        );
        assert_eq!(
            extract_repo_name_from_url("git@github.com:user/repo").unwrap(),
            "repo"
        );
    }

    #[test]
    fn test_extract_org_repo() {
        assert_eq!(
            extract_org_repo_from_url("git@github.com:user/repo.git").unwrap(),
            ("user".to_string(), "repo".to_string())
        );
        assert_eq!(
            extract_org_repo_from_url("https://github.com/user/repo").unwrap(),
            ("user".to_string(), "repo".to_string())
        );
        assert_eq!(
            extract_org_repo_from_url("git@github.com:user/repo").unwrap(),
            ("user".to_string(), "repo".to_string())
        );
        assert_eq!(
            extract_org_repo_from_url("https://github.com/modelcontextprotocol/rust-sdk.git")
                .unwrap(),
            ("modelcontextprotocol".to_string(), "rust-sdk".to_string())
        );
    }

    #[test]
    fn test_default_clone_path_hierarchical() {
        // Test hierarchical path: ~/.thoughts/clones/{host}/{org}/{repo}
        let p =
            RepoMappingManager::get_default_clone_path("git@github.com:org/repo.git:docs").unwrap();
        assert!(p.ends_with(std::path::Path::new(".thoughts/clones/github.com/org/repo")));
    }

    #[test]
    fn test_default_clone_path_gitlab_subgroups() {
        let p = RepoMappingManager::get_default_clone_path(
            "https://gitlab.com/group/subgroup/team/repo.git",
        )
        .unwrap();
        assert!(p.ends_with(std::path::Path::new(
            ".thoughts/clones/gitlab.com/group/subgroup/team/repo"
        )));
    }

    #[test]
    fn test_default_clone_path_ssh_port() {
        let p = RepoMappingManager::get_default_clone_path(
            "ssh://git@myhost.example.com:2222/org/repo.git",
        )
        .unwrap();
        assert!(p.ends_with(std::path::Path::new(
            ".thoughts/clones/myhost.example.com/org/repo"
        )));
    }

    #[test]
    fn test_canonical_key_consistency() {
        let ssh_key = RepoMappingManager::get_canonical_key("git@github.com:Org/Repo.git").unwrap();
        let https_key =
            RepoMappingManager::get_canonical_key("https://github.com/org/repo").unwrap();
        assert_eq!(
            ssh_key, https_key,
            "SSH and HTTPS should have same canonical key"
        );
    }
}
