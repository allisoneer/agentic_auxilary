//! Canonical repository identity normalization.
//!
//! This module provides `RepoIdentity` as the single source of truth for repository identity,
//! enabling consistent URL normalization across SSH, HTTPS, and various git hosting formats.

use anyhow::{Result, bail};

/// Maximum allowed subgroup nesting depth (GitLab supports up to 20 levels).
const MAX_SUBGROUP_DEPTH: usize = 20;

/// Canonical repository identity extracted from a git URL.
///
/// This struct normalizes various URL formats (SSH, HTTPS, with/without .git suffix)
/// into a consistent identity that can be used for deduplication and matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoIdentity {
    /// Host name (lowercased), e.g., "github.com"
    pub host: String,
    /// Organization path (may contain multiple segments for GitLab subgroups), e.g., "org" or "group/subgroup"
    pub org_path: String,
    /// Repository name (no .git suffix, no trailing slash)
    pub repo: String,
}

/// Canonical key for identity-based lookups and deduplication.
///
/// All fields are lowercased for case-insensitive matching.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoIdentityKey {
    pub host: String,
    pub org_path: String,
    pub repo: String,
}

impl RepoIdentity {
    /// Parse a git URL into a RepoIdentity.
    ///
    /// Supported formats:
    /// - SSH scp-like: `git@github.com:org/repo.git`
    /// - SSH with port: `ssh://git@host:2222/org/repo.git`
    /// - HTTPS: `https://github.com/org/repo` or `https://github.com/org/repo.git`
    /// - GitLab subgroups: `https://gitlab.com/a/b/c/repo.git`
    /// - Azure DevOps: `https://dev.azure.com/org/proj/_git/repo`
    ///
    /// # Errors
    /// Returns an error if the URL cannot be parsed or has invalid structure.
    pub fn parse(url: &str) -> Result<Self> {
        let url = url.trim();

        // Determine URL type and extract host + path
        let (host, path) = if url.starts_with("git@") {
            // SSH scp-like: git@host:path
            parse_scp_url(url)?
        } else if url.starts_with("ssh://") {
            // SSH with scheme: ssh://[user@]host[:port]/path
            parse_ssh_scheme_url(url)?
        } else if url.starts_with("https://") || url.starts_with("http://") {
            // HTTPS/HTTP: scheme://[user@]host[:port]/path
            parse_https_url(url)?
        } else {
            bail!("Unsupported URL format: {}", url);
        };

        // Normalize path: remove trailing slashes and .git suffix
        let path = path
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .trim_end_matches('/');

        // Split path into segments and validate
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        if segments.is_empty() {
            bail!("URL has no path segments: {}", url);
        }

        // Check for invalid segments
        for seg in &segments {
            if *seg == "." || *seg == ".." {
                bail!("Invalid path segment '{}' in URL: {}", seg, url);
            }
        }

        if segments.len() > MAX_SUBGROUP_DEPTH + 1 {
            bail!(
                "Path has too many segments ({}, max {}): {}",
                segments.len(),
                MAX_SUBGROUP_DEPTH + 1,
                url
            );
        }

        // Handle Azure DevOps special case: org/proj/_git/repo
        let (org_path, repo) = if let Some(git_idx) = segments.iter().position(|s| *s == "_git") {
            if git_idx + 1 >= segments.len() {
                bail!("Azure DevOps URL missing repo after _git: {}", url);
            }
            let org_segments = &segments[..git_idx];
            let repo = segments[git_idx + 1];
            (org_segments.join("/"), repo.to_string())
        } else if segments.len() == 1 {
            // Single segment: treat as repo with empty org (unusual but valid for some hosts)
            (String::new(), segments[0].to_string())
        } else {
            // Standard case: all but last segment is org_path, last is repo
            let org_segments = &segments[..segments.len() - 1];
            let repo = segments[segments.len() - 1];
            (org_segments.join("/"), repo.to_string())
        };

        Ok(Self {
            host: host.to_lowercase(),
            org_path,
            repo,
        })
    }

    /// Get the canonical key for identity-based lookups.
    ///
    /// All fields are lowercased for case-insensitive matching.
    pub fn canonical_key(&self) -> RepoIdentityKey {
        RepoIdentityKey {
            host: self.host.to_lowercase(),
            org_path: self.org_path.to_lowercase(),
            repo: self.repo.to_lowercase(),
        }
    }
}

/// Parse SSH scp-like URL: `git@host:path` or `user@host:path`
fn parse_scp_url(url: &str) -> Result<(String, String)> {
    // Format: [user@]host:path
    let without_user = url.find('@').map(|i| &url[i + 1..]).unwrap_or(url);

    let colon_pos = without_user
        .find(':')
        .ok_or_else(|| anyhow::anyhow!("Invalid scp-like URL (missing colon): {}", url))?;

    let host = &without_user[..colon_pos];
    let path = &without_user[colon_pos + 1..];

    if host.is_empty() {
        bail!("Empty host in URL: {}", url);
    }

    Ok((host.to_string(), path.to_string()))
}

/// Parse SSH scheme URL: `ssh://[user@]host[:port]/path`
fn parse_ssh_scheme_url(url: &str) -> Result<(String, String)> {
    let without_scheme = url
        .strip_prefix("ssh://")
        .ok_or_else(|| anyhow::anyhow!("Not an SSH URL: {}", url))?;

    // Strip userinfo if present
    let without_user = without_scheme
        .find('@')
        .map(|i| &without_scheme[i + 1..])
        .unwrap_or(without_scheme);

    // Find the first slash (separates host[:port] from path)
    let slash_pos = without_user
        .find('/')
        .ok_or_else(|| anyhow::anyhow!("SSH URL missing path: {}", url))?;

    let host_port = &without_user[..slash_pos];
    let path = &without_user[slash_pos + 1..];

    // Extract host (strip port if present)
    let host = host_port
        .split(':')
        .next()
        .ok_or_else(|| anyhow::anyhow!("Empty host in URL: {}", url))?;

    if host.is_empty() {
        bail!("Empty host in URL: {}", url);
    }

    Ok((host.to_string(), path.to_string()))
}

/// Parse HTTPS/HTTP URL: `scheme://[user@]host[:port]/path`
fn parse_https_url(url: &str) -> Result<(String, String)> {
    let scheme_end = url
        .find("://")
        .ok_or_else(|| anyhow::anyhow!("Invalid URL (missing ://): {}", url))?;

    let without_scheme = &url[scheme_end + 3..];

    // Strip userinfo if present
    let without_user = without_scheme
        .find('@')
        .map(|i| &without_scheme[i + 1..])
        .unwrap_or(without_scheme);

    // Find the first slash (separates host[:port] from path)
    let slash_pos = without_user
        .find('/')
        .ok_or_else(|| anyhow::anyhow!("URL missing path: {}", url))?;

    let host_port = &without_user[..slash_pos];
    let path = &without_user[slash_pos + 1..];

    // Extract host (strip port if present)
    let host = host_port
        .split(':')
        .next()
        .ok_or_else(|| anyhow::anyhow!("Empty host in URL: {}", url))?;

    if host.is_empty() {
        bail!("Empty host in URL: {}", url);
    }

    Ok((host.to_string(), path.to_string()))
}

/// Split `url` into (base_url, optional_subpath) using a last-colon heuristic.
///
/// Treats it as `URL:subpath` only if the base portion parses as a valid `RepoIdentity`.
/// This avoids confusing `host:port` for a subpath delimiter.
///
/// # Examples
/// ```ignore
/// // No subpath
/// parse_url_and_subpath("git@github.com:org/repo.git")
///   => ("git@github.com:org/repo.git", None)
///
/// // With subpath
/// parse_url_and_subpath("git@github.com:org/repo.git:docs/api")
///   => ("git@github.com:org/repo.git", Some("docs/api"))
///
/// // SSH with port (port is NOT a subpath)
/// parse_url_and_subpath("ssh://git@host:2222/org/repo.git")
///   => ("ssh://git@host:2222/org/repo.git", None)
///
/// // SSH with port AND subpath
/// parse_url_and_subpath("ssh://git@host:2222/org/repo.git:docs/api")
///   => ("ssh://git@host:2222/org/repo.git", Some("docs/api"))
/// ```
pub fn parse_url_and_subpath(url: &str) -> (String, Option<String>) {
    // Strategy: find the rightmost colon and check if the left side parses as a valid URL.
    // If it does, the right side is a subpath. If not, there's no subpath.

    // Handle scheme-based URLs: ssh://, https://, http://
    // For these, we need to be careful about host:port patterns

    let url = url.trim();

    // Try splitting from the right
    if let Some(colon_pos) = url.rfind(':') {
        let potential_base = &url[..colon_pos];
        let potential_subpath = &url[colon_pos + 1..];

        // Don't split if subpath is empty
        if potential_subpath.is_empty() {
            return (url.to_string(), None);
        }

        // Don't split if subpath looks like a port (all digits)
        if potential_subpath.chars().all(|c| c.is_ascii_digit()) {
            return (url.to_string(), None);
        }

        // Don't split if potential_base is empty or just a scheme
        if potential_base.is_empty() || potential_base.ends_with("//") {
            return (url.to_string(), None);
        }

        // Try parsing the base as a RepoIdentity
        if RepoIdentity::parse(potential_base).is_ok() {
            return (
                potential_base.to_string(),
                Some(potential_subpath.to_string()),
            );
        }
    }

    (url.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== RepoIdentity::parse tests =====

    #[test]
    fn test_parse_ssh_scp_basic() {
        let id = RepoIdentity::parse("git@github.com:org/repo.git").unwrap();
        assert_eq!(id.host, "github.com");
        assert_eq!(id.org_path, "org");
        assert_eq!(id.repo, "repo");
    }

    #[test]
    fn test_parse_ssh_scp_no_git_suffix() {
        let id = RepoIdentity::parse("git@github.com:org/repo").unwrap();
        assert_eq!(id.host, "github.com");
        assert_eq!(id.org_path, "org");
        assert_eq!(id.repo, "repo");
    }

    #[test]
    fn test_parse_https_basic() {
        let id = RepoIdentity::parse("https://github.com/org/repo").unwrap();
        assert_eq!(id.host, "github.com");
        assert_eq!(id.org_path, "org");
        assert_eq!(id.repo, "repo");
    }

    #[test]
    fn test_parse_https_with_git_suffix() {
        let id = RepoIdentity::parse("https://github.com/org/repo.git").unwrap();
        assert_eq!(id.host, "github.com");
        assert_eq!(id.org_path, "org");
        assert_eq!(id.repo, "repo");
    }

    #[test]
    fn test_parse_https_trailing_slash() {
        let id = RepoIdentity::parse("https://github.com/org/repo/").unwrap();
        assert_eq!(id.host, "github.com");
        assert_eq!(id.org_path, "org");
        assert_eq!(id.repo, "repo");
    }

    #[test]
    fn test_parse_ssh_with_port() {
        let id = RepoIdentity::parse("ssh://git@host.example.com:2222/org/repo.git").unwrap();
        assert_eq!(id.host, "host.example.com");
        assert_eq!(id.org_path, "org");
        assert_eq!(id.repo, "repo");
    }

    #[test]
    fn test_parse_gitlab_subgroups() {
        let id = RepoIdentity::parse("https://gitlab.com/group/subgroup/team/repo.git").unwrap();
        assert_eq!(id.host, "gitlab.com");
        assert_eq!(id.org_path, "group/subgroup/team");
        assert_eq!(id.repo, "repo");
    }

    #[test]
    fn test_parse_gitlab_deep_subgroups() {
        let id = RepoIdentity::parse("https://gitlab.com/a/b/c/d/e/repo.git").unwrap();
        assert_eq!(id.host, "gitlab.com");
        assert_eq!(id.org_path, "a/b/c/d/e");
        assert_eq!(id.repo, "repo");
    }

    #[test]
    fn test_parse_azure_devops() {
        let id = RepoIdentity::parse("https://dev.azure.com/myorg/myproj/_git/myrepo").unwrap();
        assert_eq!(id.host, "dev.azure.com");
        assert_eq!(id.org_path, "myorg/myproj");
        assert_eq!(id.repo, "myrepo");
    }

    #[test]
    fn test_parse_host_case_normalized() {
        let id = RepoIdentity::parse("https://GitHub.COM/Org/Repo").unwrap();
        assert_eq!(id.host, "github.com");
        // org_path and repo preserve case
        assert_eq!(id.org_path, "Org");
        assert_eq!(id.repo, "Repo");
    }

    #[test]
    fn test_parse_http_scheme() {
        let id = RepoIdentity::parse("http://github.com/org/repo").unwrap();
        assert_eq!(id.host, "github.com");
        assert_eq!(id.org_path, "org");
        assert_eq!(id.repo, "repo");
    }

    #[test]
    fn test_parse_rejects_invalid_segments() {
        assert!(RepoIdentity::parse("https://github.com/../repo").is_err());
        assert!(RepoIdentity::parse("https://github.com/./repo").is_err());
    }

    #[test]
    fn test_parse_rejects_unsupported_scheme() {
        assert!(RepoIdentity::parse("ftp://github.com/org/repo").is_err());
        assert!(RepoIdentity::parse("org/repo").is_err());
    }

    // ===== canonical_key tests =====

    #[test]
    fn test_canonical_key_equality_across_schemes() {
        let ssh = RepoIdentity::parse("git@github.com:User/Repo.git").unwrap();
        let https = RepoIdentity::parse("https://github.com/user/repo").unwrap();

        assert_eq!(ssh.canonical_key(), https.canonical_key());
    }

    #[test]
    fn test_canonical_key_different_repos() {
        let a = RepoIdentity::parse("git@github.com:org/repo-a.git").unwrap();
        let b = RepoIdentity::parse("git@github.com:org/repo-b.git").unwrap();

        assert_ne!(a.canonical_key(), b.canonical_key());
    }

    #[test]
    fn test_canonical_key_different_orgs() {
        let a = RepoIdentity::parse("git@github.com:alice/utils.git").unwrap();
        let b = RepoIdentity::parse("git@github.com:bob/utils.git").unwrap();

        assert_ne!(a.canonical_key(), b.canonical_key());
    }

    // ===== parse_url_and_subpath tests =====

    #[test]
    fn test_subpath_none_basic() {
        let (url, sub) = parse_url_and_subpath("git@github.com:user/repo.git");
        assert_eq!(url, "git@github.com:user/repo.git");
        assert_eq!(sub, None);
    }

    #[test]
    fn test_subpath_present() {
        let (url, sub) = parse_url_and_subpath("git@github.com:user/repo.git:docs/api");
        assert_eq!(url, "git@github.com:user/repo.git");
        assert_eq!(sub, Some("docs/api".to_string()));
    }

    #[test]
    fn test_subpath_https_none() {
        let (url, sub) = parse_url_and_subpath("https://github.com/user/repo");
        assert_eq!(url, "https://github.com/user/repo");
        assert_eq!(sub, None);
    }

    #[test]
    fn test_subpath_ssh_port_not_confused() {
        // Port should NOT be treated as subpath
        let (url, sub) = parse_url_and_subpath("ssh://git@host:2222/org/repo.git");
        assert_eq!(url, "ssh://git@host:2222/org/repo.git");
        assert_eq!(sub, None);
    }

    #[test]
    fn test_subpath_ssh_port_with_actual_subpath() {
        let (url, sub) = parse_url_and_subpath("ssh://git@host:2222/org/repo.git:docs/api");
        assert_eq!(url, "ssh://git@host:2222/org/repo.git");
        assert_eq!(sub, Some("docs/api".to_string()));
    }

    #[test]
    fn test_subpath_empty_subpath_ignored() {
        let (url, sub) = parse_url_and_subpath("git@github.com:user/repo.git:");
        assert_eq!(url, "git@github.com:user/repo.git:");
        assert_eq!(sub, None);
    }
}
