use anyhow::Result;
use git2::Repository;
use pr_comments::PrComments;
use pr_comments::models::PrRef;

pub struct GitHubPrClient {
    pr_comments: PrComments,
    repo_context: RepoContext,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedPrLookup {
    pub requested_branch: String,
    pub current_branch: Option<String>,
    pub repo_owner: String,
    pub repo_name: String,
    pub pr: Option<PrRef>,
}

#[derive(Debug, Clone)]
struct RepoContext {
    owner: String,
    repo: String,
    current_branch: Option<String>,
}

impl GitHubPrClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            pr_comments: PrComments::new()?,
            repo_context: RepoContext::from_current_dir()?,
        })
    }

    pub async fn detect_open_pr_from_branch(&self, branch: &str) -> Result<DetectedPrLookup> {
        Ok(DetectedPrLookup {
            requested_branch: branch.to_string(),
            current_branch: self.repo_context.current_branch.clone(),
            repo_owner: self.repo_context.owner.clone(),
            repo_name: self.repo_context.repo.clone(),
            pr: self.pr_comments.get_open_pr_ref_from_branch(branch).await?,
        })
    }
}

impl RepoContext {
    fn from_current_dir() -> Result<Self> {
        let repo = Repository::discover(".")?;
        let remote = repo.find_remote("origin")?;
        let url = remote
            .url()
            .ok_or_else(|| anyhow::anyhow!("Remote 'origin' has no URL"))?;
        let (owner, repo_name) = parse_github_remote(url)?;
        let current_branch = repo
            .head()
            .ok()
            .and_then(|head| head.shorthand().map(String::from));

        Ok(Self {
            owner,
            repo: repo_name,
            current_branch,
        })
    }
}

fn parse_github_remote(url: &str) -> Result<(String, String)> {
    if let Some(path) = url.strip_prefix("git@github.com:") {
        return parse_repo_path(path);
    }

    if let Some(path) = url.strip_prefix("https://github.com/") {
        return parse_repo_path(path);
    }

    anyhow::bail!("Not a supported GitHub remote URL: {url}");
}

fn parse_repo_path(path: &str) -> Result<(String, String)> {
    let trimmed = path.trim_end_matches(".git");
    let mut parts = trimmed.split('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(owner), Some(repo), None) => Ok((owner.to_string(), repo.to_string())),
        _ => anyhow::bail!("Invalid GitHub repository path: {trimmed}"),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_github_remote;

    #[test]
    fn parse_github_remote_supports_https_and_ssh() {
        assert_eq!(
            parse_github_remote("https://github.com/owner/repo.git")
                .expect("https remote should parse"),
            ("owner".to_string(), "repo".to_string())
        );
        assert_eq!(
            parse_github_remote("git@github.com:owner/repo").expect("ssh remote should parse"),
            ("owner".to_string(), "repo".to_string())
        );
    }
}
