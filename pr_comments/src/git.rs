use anyhow::{Context, Result};
use git2::Repository;
use url::Url;

pub struct GitInfo {
    pub owner: String,
    pub repo: String,
    pub current_branch: Option<String>,
}

pub fn get_git_info() -> Result<GitInfo> {
    let repo = Repository::discover(".").context("Not in a git repository")?;

    let remote = repo
        .find_remote("origin")
        .context("No 'origin' remote found")?;

    let url = remote.url().context("Remote 'origin' has no URL")?;

    let (owner, repo_name) = parse_github_url(url)?;

    let current_branch = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(String::from));

    Ok(GitInfo {
        owner,
        repo: repo_name,
        current_branch,
    })
}

pub fn parse_github_url(url: &str) -> Result<(String, String)> {
    // Handle both HTTPS and SSH URLs
    if url.starts_with("git@github.com:") {
        // SSH format: git@github.com:owner/repo.git
        let path = url.trim_start_matches("git@github.com:");
        parse_repo_path(path)
    } else if let Ok(parsed) = Url::parse(url) {
        // HTTPS format: https://github.com/owner/repo.git
        if parsed.host_str() == Some("github.com") {
            let path = parsed.path().trim_start_matches('/');
            parse_repo_path(path)
        } else {
            anyhow::bail!("Not a GitHub URL: {}", url)
        }
    } else {
        anyhow::bail!("Invalid git remote URL: {}", url)
    }
}

fn parse_repo_path(path: &str) -> Result<(String, String)> {
    let path = path.trim_end_matches(".git");
    let parts: Vec<&str> = path.split('/').collect();

    if parts.len() == 2 {
        Ok((parts[0].to_string(), parts[1].to_string()))
    } else {
        anyhow::bail!("Invalid GitHub repository path: {}", path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_urls() {
        let test_cases = vec![
            ("https://github.com/owner/repo.git", ("owner", "repo")),
            ("https://github.com/owner/repo", ("owner", "repo")),
            ("git@github.com:owner/repo.git", ("owner", "repo")),
            ("git@github.com:owner/repo", ("owner", "repo")),
        ];

        for (url, (expected_owner, expected_repo)) in test_cases {
            let (owner, repo) = parse_github_url(url).unwrap();
            assert_eq!(owner, expected_owner);
            assert_eq!(repo, expected_repo);
        }
    }
}
