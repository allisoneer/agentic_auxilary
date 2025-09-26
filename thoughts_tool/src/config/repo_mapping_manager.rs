use super::types::{RepoLocation, RepoMapping};
use crate::utils::paths::{self, sanitize_dir_name};
use anyhow::{Context, Result, bail};
use atomicwrites::{AllowOverwrite, AtomicFile};
use std::io::Write;
use std::path::PathBuf;

pub struct RepoMappingManager {
    mapping_path: PathBuf,
}

impl RepoMappingManager {
    pub fn new() -> Result<Self> {
        let mapping_path = paths::get_repo_mapping_path()?;
        Ok(Self { mapping_path })
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

    /// Resolve a git URL to its local path
    pub fn resolve_url(&self, url: &str) -> Result<Option<PathBuf>> {
        let mapping = self.load()?;

        // Handle subdirectory URLs (git@github.com:user/repo.git:docs/api)
        let (base_url, subpath) = parse_url_and_subpath(url);

        if let Some(location) = mapping.mappings.get(&base_url) {
            let mut path = location.path.clone();
            if let Some(sub) = subpath {
                path = path.join(sub);
            }
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    /// Add a URL-to-path mapping
    pub fn add_mapping(&mut self, url: String, path: PathBuf, auto_managed: bool) -> Result<()> {
        let mut mapping = self.load()?;

        // Basic validation
        if !path.exists() {
            bail!("Path does not exist: {}", path.display());
        }

        if !path.is_dir() {
            bail!("Path is not a directory: {}", path.display());
        }

        let location = RepoLocation {
            path,
            auto_managed,
            last_sync: None,
        };

        mapping.mappings.insert(url, location);
        self.save(&mapping)?;
        Ok(())
    }

    /// Remove a URL mapping
    #[allow(dead_code)]
    // TODO(2): Add "thoughts mount unmap" command for cleanup
    pub fn remove_mapping(&mut self, url: &str) -> Result<()> {
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

    /// Get default clone path for a URL
    pub fn get_default_clone_path(url: &str) -> Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;

        let repo_name = sanitize_dir_name(&extract_repo_name_from_url(url)?);
        Ok(home.join(".thoughts").join("clones").join(repo_name))
    }

    /// Update last sync time
    pub fn update_sync_time(&mut self, url: &str) -> Result<()> {
        let mut mapping = self.load()?;

        if let Some(location) = mapping.mappings.get_mut(url) {
            location.last_sync = Some(chrono::Utc::now());
            self.save(&mapping)?;
        }

        Ok(())
    }
}

fn parse_url_and_subpath(url: &str) -> (String, Option<String>) {
    // Look for a subpath pattern: URL followed by :path
    // For SSH URLs like git@github.com:user/repo.git, the first colon is part of the URL
    // So we look for a pattern where we have a second colon after .git or end of repo name

    // First, check if this looks like it might have a subpath
    let parts: Vec<&str> = url.splitn(3, ':').collect();

    if parts.len() == 3 {
        // Potential subpath - verify it's not part of the URL
        let potential_subpath = parts[2];
        let potential_base = format!("{}:{}", parts[0], parts[1]);

        // Check if the base looks like a complete URL
        if (potential_base.contains('@') && potential_base.contains('/'))
            || potential_base.ends_with(".git")
        {
            // This looks like URL:subpath format
            return (potential_base, Some(potential_subpath.to_string()));
        }
    }

    (url.to_string(), None)
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

pub fn extract_org_repo_from_url(url: &str) -> anyhow::Result<(String, String)> {
    // Normalize
    let url = url.trim_end_matches(".git");
    // SSH: git@github.com:org/repo
    if let Some(at_pos) = url.find('@')
        && let Some(colon_pos) = url[at_pos..].find(':')
    {
        let path = &url[at_pos + colon_pos + 1..]; // org/repo
        let mut it = path.split('/');
        let org = it.next().ok_or_else(|| anyhow::anyhow!("No org"))?;
        let repo = it.next().ok_or_else(|| anyhow::anyhow!("No repo"))?;
        return Ok((org.into(), repo.into()));
    }
    // HTTPS: https://github.com/org/repo
    if let Some(host_pos) = url.find("://") {
        let path = &url[host_pos + 3..]; // host/org/repo
        let mut it = path.split('/');
        let _host = it.next().ok_or_else(|| anyhow::anyhow!("No host"))?;
        let org = it.next().ok_or_else(|| anyhow::anyhow!("No org"))?;
        let repo = it.next().ok_or_else(|| anyhow::anyhow!("No repo"))?;
        return Ok((org.into(), repo.into()));
    }
    anyhow::bail!("Unsupported URL: {url}")
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
}
