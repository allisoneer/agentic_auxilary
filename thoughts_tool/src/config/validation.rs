use crate::config::repo_mapping_manager::{extract_org_repo_from_url, parse_url_and_subpath};
use anyhow::{Result, bail};

/// Sanitize a mount name for use as directory name
pub fn sanitize_mount_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect()
}

/// Return true if string looks like a git URL we support
pub fn is_git_url(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("git@")
        || s.starts_with("https://")
        || s.starts_with("http://")
        || s.starts_with("ssh://")
}

/// Extract host from SSH/HTTPS URLs
pub fn get_host_from_url(url: &str) -> Result<String> {
    let (base, _) = parse_url_and_subpath(url);
    let base = base.trim_end_matches(".git");

    if let Some(at) = base.find('@')
        && let Some(colon) = base[at..].find(':')
    {
        let host = &base[at + 1..at + colon];
        return Ok(host.to_lowercase());
    }
    if let Some(scheme) = base.find("://") {
        let rest = &base[scheme + 3..];
        let host = rest
            .split('/')
            .next()
            .ok_or_else(|| anyhow::anyhow!("No host"))?;
        // Strip userinfo and port if present (e.g., user@host:2222)
        let host = host.split('@').next_back().unwrap_or(host);
        let host = host.split(':').next().unwrap_or(host);
        return Ok(host.to_lowercase());
    }
    bail!("Unsupported URL (cannot parse host): {}", url)
}

/// Validate that a reference URL is well-formed and points to org/repo (repo-level only)
pub fn validate_reference_url(url: &str) -> Result<()> {
    let url = url.trim();
    let (base, subpath) = parse_url_and_subpath(url);
    if subpath.is_some() {
        bail!(
            "Cannot add URL with subpath as a reference: {}\n\n\
             References are repo-level only.\n\
             Try one of:\n\
               - Add the repository URL without a subpath\n\
               - Use 'thoughts mount add <local-subdir>' for subdirectory mounts",
            url
        );
    }
    if !is_git_url(&base) {
        bail!(
            "Invalid reference value: {}\n\n\
             Must be a git URL using one of:\n  - git@host:org/repo(.git)\n  - https://host/org/repo(.git)\n  - ssh://user@host[:port]/org/repo(.git)\n",
            url
        );
    }
    // Ensure org/repo structure is parseable
    extract_org_repo_from_url(&base).map_err(|e| {
        anyhow::anyhow!(
            "Invalid repository URL: {}\n\n\
             Expected a URL with an org and repo (e.g., github.com/org/repo).\n\
             Details: {}",
            url,
            e
        )
    })?;
    Ok(())
}

/// Canonical key (host, org, repo) all lowercased, without .git
pub fn canonical_reference_key(url: &str) -> Result<(String, String, String)> {
    let (base, _) = parse_url_and_subpath(url);
    let (org, repo) = extract_org_repo_from_url(&base)?;
    let host = get_host_from_url(&base)?;
    Ok((host.to_lowercase(), org.to_lowercase(), repo.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_mount_name() {
        assert_eq!(sanitize_mount_name("valid-name_123"), "valid-name_123");
        assert_eq!(sanitize_mount_name("bad name!@#"), "bad_name___");
        assert_eq!(sanitize_mount_name("CamelCase"), "CamelCase");
    }
}

#[cfg(test)]
mod ref_validation_tests {
    use super::*;

    #[test]
    fn test_is_git_url() {
        assert!(is_git_url("git@github.com:org/repo.git"));
        assert!(is_git_url("https://github.com/org/repo"));
        assert!(is_git_url("ssh://user@host:22/org/repo"));
        assert!(is_git_url("http://gitlab.com/org/repo"));
        assert!(!is_git_url("org/repo"));
        assert!(!is_git_url("/local/path"));
    }

    #[test]
    fn test_validate_reference_url_accepts_valid() {
        assert!(validate_reference_url("git@github.com:org/repo.git").is_ok());
        assert!(validate_reference_url("https://github.com/org/repo").is_ok());
    }

    #[test]
    fn test_validate_reference_url_rejects_subpath() {
        assert!(validate_reference_url("git@github.com:org/repo.git:docs").is_err());
    }

    #[test]
    fn test_canonical_reference_key_normalizes() {
        let a = canonical_reference_key("git@github.com:User/Repo.git").unwrap();
        let b = canonical_reference_key("https://github.com/user/repo").unwrap();
        assert_eq!(a, b);
        assert_eq!(a, ("github.com".into(), "user".into(), "repo".into()));
    }
}
