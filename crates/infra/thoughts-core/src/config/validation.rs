use crate::git::ref_key::encode_ref_key;
use crate::repo_identity::RepoIdentity;
use crate::repo_identity::parse_url_and_subpath;
use anyhow::Result;
use anyhow::bail;
use std::borrow::Cow;

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
    let id = RepoIdentity::parse(&base).map_err(|e| {
        anyhow::anyhow!(
            "Unsupported URL (cannot parse host): {}\nDetails: {}",
            url,
            e
        )
    })?;
    Ok(id.host)
}

/// Validate that a reference URL is well-formed and points to org/repo (repo-level only)
pub fn validate_reference_url(url: &str) -> Result<()> {
    let url = url.trim();
    if url.contains('?') || url.contains('#') {
        bail!(
            "Reference URLs cannot contain '?' or '#' alternate ref encodings: {}",
            url
        );
    }
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
    // Ensure org/repo structure is parseable via RepoIdentity
    RepoIdentity::parse(&base).map_err(|e| {
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

/// Canonical key (host, org_path, repo) all lowercased, without .git
pub fn canonical_reference_key(url: &str) -> Result<(String, String, String)> {
    let (base, _) = parse_url_and_subpath(url);
    let key = RepoIdentity::parse(&base)?.canonical_key();
    Ok((key.host, key.org_path, key.repo))
}

/// Canonical key for a specific reference instance: repository identity plus optional ref key.
fn normalize_pinned_ref_name_for_identity(ref_name: &str) -> Cow<'_, str> {
    if let Some(rest) = ref_name.strip_prefix("refs/remotes/")
        && let Some((_remote, branch)) = rest.split_once('/')
        && !branch.is_empty()
    {
        return Cow::Owned(format!("refs/heads/{branch}"));
    }

    Cow::Borrowed(ref_name)
}

pub(crate) fn normalize_encoded_ref_key_for_identity(ref_key: &str) -> Cow<'_, str> {
    const REMOTES_PREFIX: &str = "r-refs~2fremotes~2f";
    const HEADS_PREFIX: &str = "r-refs~2fheads~2f";

    if let Some(rest) = ref_key.strip_prefix(REMOTES_PREFIX)
        && let Some((_remote_enc, branch_enc)) = rest.split_once("~2f")
        && !branch_enc.is_empty()
    {
        return Cow::Owned(format!("{HEADS_PREFIX}{branch_enc}"));
    }

    Cow::Borrowed(ref_key)
}

pub fn canonical_reference_instance_key(
    url: &str,
    ref_name: Option<&str>,
) -> Result<(String, String, String, Option<String>)> {
    let (host, org_path, repo) = canonical_reference_key(url)?;
    let ref_key = ref_name
        .map(normalize_pinned_ref_name_for_identity)
        .map(|name| encode_ref_key(name.as_ref()))
        .transpose()?;
    Ok((host, org_path, repo, ref_key))
}

pub fn validate_pinned_ref_full_name(ref_name: &str) -> Result<()> {
    let trimmed = ref_name.trim();
    if trimmed.is_empty() {
        bail!("ref cannot be empty");
    }
    if trimmed != ref_name {
        bail!("Pinned ref must not contain leading/trailing whitespace");
    }
    if trimmed.ends_with('/') {
        bail!("Pinned ref cannot end with '/'");
    }
    let ref_name = trimmed;

    if let Some(rest) = ref_name.strip_prefix("refs/heads/") {
        if rest.is_empty() {
            bail!("Pinned ref cannot be the bare prefix 'refs/heads/'");
        }
        return Ok(());
    }

    if let Some(rest) = ref_name.strip_prefix("refs/tags/") {
        if rest.is_empty() {
            bail!("Pinned ref cannot be the bare prefix 'refs/tags/'");
        }
        return Ok(());
    }

    if let Some(rest) = ref_name.strip_prefix("refs/remotes/") {
        let mut parts = rest.splitn(2, '/');
        let remote = parts.next().unwrap_or("");
        let branch = parts.next().unwrap_or("");
        if remote.is_empty() || branch.is_empty() {
            bail!(
                "Legacy pinned ref must be 'refs/remotes/<remote>/<branch>' (got '{}')",
                ref_name
            );
        }
        return Ok(());
    }

    bail!(
        "Pinned refs must be full ref names starting with 'refs/heads/', 'refs/tags/', or 'refs/remotes/' (got '{}')",
        ref_name
    );
}

pub fn validate_pinned_ref_full_name_new_input(ref_name: &str) -> Result<()> {
    let trimmed = ref_name.trim();
    if trimmed.is_empty() {
        bail!("ref cannot be empty");
    }
    if trimmed != ref_name {
        bail!("Pinned ref must not contain leading/trailing whitespace");
    }
    if trimmed.ends_with('/') {
        bail!("Pinned ref cannot end with '/'");
    }
    let ref_name = trimmed;

    if let Some(rest) = ref_name.strip_prefix("refs/heads/") {
        if rest.is_empty() {
            bail!("Pinned ref cannot be the bare prefix 'refs/heads/'");
        }
        return Ok(());
    }

    if let Some(rest) = ref_name.strip_prefix("refs/tags/") {
        if rest.is_empty() {
            bail!("Pinned ref cannot be the bare prefix 'refs/tags/'");
        }
        return Ok(());
    }

    bail!(
        "Pinned refs must be full ref names starting with 'refs/heads/' or 'refs/tags/' (got '{}')",
        ref_name
    );
}

// --- MCP HTTPS-only validation helpers ---

/// True if the URL uses SSH schemes we do not support in MCP
pub fn is_ssh_url(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("git@") || s.starts_with("ssh://")
}

/// True if URL starts with https://
pub fn is_https_url(s: &str) -> bool {
    s.trim_start().to_lowercase().starts_with("https://")
}

/// Validate MCP add_reference input:
/// - Reject SSH and http://
/// - Reject subpaths
/// - Accept GitHub web or clone URLs (https://github.com/org/repo[.git])
/// - Accept generic https://*.git clone URLs
pub fn validate_reference_url_https_only(url: &str) -> Result<()> {
    let url = url.trim();

    if url.contains('?') || url.contains('#') {
        bail!(
            "Reference URLs cannot contain '?' or '#' alternate ref encodings: {}",
            url
        );
    }

    // Reject subpaths (URL:subpath)
    let (base, subpath) = parse_url_and_subpath(url);
    if subpath.is_some() {
        bail!(
            "Cannot add URL with subpath as a reference: {}\n\nReferences are repo-level only.",
            url
        );
    }

    if is_ssh_url(&base) {
        bail!(
            "SSH URLs are not supported by the MCP add_reference tool: {}\n\n\
             Please provide an HTTPS URL, e.g.:\n  https://github.com/org/repo(.git)\n\n\
             If you must use SSH, run the CLI instead:\n  thoughts references add <git@... or ssh://...>",
            base
        );
    }
    if !is_https_url(&base) {
        bail!(
            "Only HTTPS URLs are supported by the MCP add_reference tool: {}\n\n\
             Please provide an HTTPS URL, e.g.:\n  https://github.com/org/repo(.git)",
            base
        );
    }

    // Parse as RepoIdentity to validate structure
    let id = RepoIdentity::parse(&base).map_err(|e| {
        anyhow::anyhow!(
            "Invalid repository URL (expected host/org/repo).\nDetails: {}",
            e
        )
    })?;

    // For non-GitHub hosts, require .git suffix
    if id.host != "github.com" && !base.ends_with(".git") {
        bail!(
            "For non-GitHub hosts, please provide an HTTPS clone URL ending with .git:\n  {}",
            base
        );
    }

    Ok(())
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

    #[test]
    fn test_canonical_reference_instance_key_distinguishes_refs() {
        let main = canonical_reference_instance_key(
            "https://github.com/user/repo",
            Some("refs/heads/main"),
        )
        .unwrap();
        let tag = canonical_reference_instance_key(
            "https://github.com/user/repo",
            Some("refs/tags/v1.0.0"),
        )
        .unwrap();
        let unpinned =
            canonical_reference_instance_key("https://github.com/user/repo", None).unwrap();

        assert_ne!(main, tag);
        assert_ne!(main, unpinned);
        assert_ne!(tag, unpinned);
    }

    #[test]
    fn test_canonical_reference_instance_key_normalizes_legacy_refs_remotes_to_heads() {
        let legacy = canonical_reference_instance_key(
            "https://github.com/org/repo",
            Some("refs/remotes/origin/main"),
        )
        .unwrap();
        let canonical = canonical_reference_instance_key(
            "https://github.com/org/repo",
            Some("refs/heads/main"),
        )
        .unwrap();

        assert_eq!(legacy, canonical);
    }

    #[test]
    fn test_normalize_encoded_ref_key_for_identity_collapses_legacy_remotes() {
        let legacy = encode_ref_key("refs/remotes/origin/main").unwrap();
        let canonical = encode_ref_key("refs/heads/main").unwrap();

        assert_eq!(
            normalize_encoded_ref_key_for_identity(&legacy).as_ref(),
            canonical
        );
    }

    #[test]
    fn test_validate_reference_url_rejects_query_and_fragment() {
        assert!(validate_reference_url("https://github.com/org/repo?ref=main").is_err());
        assert!(validate_reference_url("https://github.com/org/repo#main").is_err());
    }
}

#[cfg(test)]
mod mcp_https_validation_tests {
    use super::*;

    #[test]
    fn test_https_only_accepts_github_web_and_clone() {
        assert!(validate_reference_url_https_only("https://github.com/org/repo").is_ok());
        assert!(validate_reference_url_https_only("https://github.com/org/repo.git").is_ok());
    }

    #[test]
    fn test_https_only_accepts_generic_dot_git() {
        assert!(validate_reference_url_https_only("https://gitlab.com/group/proj.git").is_ok());
    }

    #[test]
    fn test_https_only_rejects_ssh_and_http_and_subpath() {
        assert!(validate_reference_url_https_only("git@github.com:org/repo.git").is_err());
        assert!(validate_reference_url_https_only("ssh://host/org/repo.git").is_err());
        assert!(validate_reference_url_https_only("http://github.com/org/repo.git").is_err());
        assert!(validate_reference_url_https_only("https://github.com/org/repo.git:docs").is_err());
    }

    #[test]
    fn test_is_ssh_url_helper() {
        assert!(is_ssh_url("git@github.com:org/repo.git"));
        assert!(is_ssh_url("ssh://user@host/repo.git"));
        assert!(!is_ssh_url("https://github.com/org/repo"));
        assert!(!is_ssh_url("http://github.com/org/repo"));
    }

    #[test]
    fn test_is_https_url_helper() {
        assert!(is_https_url("https://github.com/org/repo"));
        assert!(is_https_url("HTTPS://github.com/org/repo")); // case-insensitive
        assert!(!is_https_url("http://github.com/org/repo"));
        assert!(!is_https_url("git@github.com:org/repo"));
    }

    #[test]
    fn test_https_only_rejects_non_github_without_dot_git() {
        // Non-GitHub without .git suffix should be rejected
        assert!(validate_reference_url_https_only("https://gitlab.com/group/proj").is_err());
    }

    #[test]
    fn test_https_only_rejects_query_and_fragment() {
        assert!(validate_reference_url_https_only("https://github.com/org/repo?ref=main").is_err());
        assert!(validate_reference_url_https_only("https://github.com/org/repo#main").is_err());
    }
}

#[cfg(test)]
mod pinned_ref_name_tests {
    use super::validate_pinned_ref_full_name;

    #[test]
    fn accepts_allowed_full_refs() {
        assert!(validate_pinned_ref_full_name("refs/heads/main").is_ok());
        assert!(validate_pinned_ref_full_name("refs/tags/v1.0.0").is_ok());
        assert!(validate_pinned_ref_full_name("refs/remotes/origin/main").is_ok());
    }

    #[test]
    fn rejects_shorthand_and_other_namespaces() {
        assert!(validate_pinned_ref_full_name("main").is_err());
        assert!(validate_pinned_ref_full_name("v1.0.0").is_err());
        assert!(validate_pinned_ref_full_name("origin/main").is_err());
        assert!(validate_pinned_ref_full_name("refs/pull/123/head").is_err());
    }

    #[test]
    fn rejects_incomplete_prefixes() {
        assert!(validate_pinned_ref_full_name("refs/heads/").is_err());
        assert!(validate_pinned_ref_full_name("refs/tags/").is_err());
        assert!(validate_pinned_ref_full_name("refs/remotes/").is_err());
        assert!(validate_pinned_ref_full_name("refs/remotes/origin/").is_err());
    }

    #[test]
    fn rejects_leading_and_trailing_whitespace() {
        assert!(validate_pinned_ref_full_name(" refs/heads/main").is_err());
        assert!(validate_pinned_ref_full_name("refs/heads/main ").is_err());
        assert!(validate_pinned_ref_full_name(" refs/tags/v1.0.0 ").is_err());
    }

    #[test]
    fn rejects_trailing_slash_full_refs() {
        assert!(validate_pinned_ref_full_name("refs/heads/main/").is_err());
        assert!(validate_pinned_ref_full_name("refs/tags/v1.0.0/").is_err());
        assert!(validate_pinned_ref_full_name("refs/remotes/origin/main/").is_err());
    }
}

#[cfg(test)]
mod pinned_ref_name_new_input_tests {
    use super::validate_pinned_ref_full_name_new_input;

    #[test]
    fn accepts_heads_and_tags_only() {
        assert!(validate_pinned_ref_full_name_new_input("refs/heads/main").is_ok());
        assert!(validate_pinned_ref_full_name_new_input("refs/tags/v1.0.0").is_ok());
    }

    #[test]
    fn rejects_refs_remotes_and_shorthand() {
        assert!(validate_pinned_ref_full_name_new_input("refs/remotes/origin/main").is_err());
        assert!(validate_pinned_ref_full_name_new_input("main").is_err());
        assert!(validate_pinned_ref_full_name_new_input("refs/pull/123/head").is_err());
    }

    #[test]
    fn new_input_rejects_incomplete_prefixes() {
        assert!(validate_pinned_ref_full_name_new_input("refs/heads/").is_err());
        assert!(validate_pinned_ref_full_name_new_input("refs/tags/").is_err());
    }

    #[test]
    fn rejects_leading_and_trailing_whitespace() {
        assert!(validate_pinned_ref_full_name_new_input(" refs/heads/main").is_err());
        assert!(validate_pinned_ref_full_name_new_input("refs/heads/main ").is_err());
        assert!(validate_pinned_ref_full_name_new_input(" refs/tags/v1.0.0 ").is_err());
    }

    #[test]
    fn rejects_trailing_slash_full_refs() {
        assert!(validate_pinned_ref_full_name_new_input("refs/heads/main/").is_err());
        assert!(validate_pinned_ref_full_name_new_input("refs/tags/v1.0.0/").is_err());
    }
}
