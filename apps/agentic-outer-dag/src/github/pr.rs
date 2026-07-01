use anyhow::Result;
use pr_comments::PrComments;
use pr_comments::git;
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
    pub token_source: Option<String>,
    pub empty_result_reason: Option<String>,
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
        let repo_context = RepoContext::from_current_dir()?;
        Ok(Self {
            pr_comments: PrComments::with_repo(
                repo_context.owner.clone(),
                repo_context.repo.clone(),
            ),
            repo_context,
        })
    }

    pub async fn detect_open_pr_from_branch(&self, branch: &str) -> Result<DetectedPrLookup> {
        let lookup = self
            .pr_comments
            .get_open_pr_ref_from_branch_detailed(branch)
            .await?;

        Ok(DetectedPrLookup {
            requested_branch: branch.to_string(),
            current_branch: self.repo_context.current_branch.clone(),
            repo_owner: self.repo_context.owner.clone(),
            repo_name: self.repo_context.repo.clone(),
            token_source: self.pr_comments.token_source_label().map(str::to_string),
            empty_result_reason: lookup.empty_result_reason,
            pr: lookup.pr,
        })
    }

    pub async fn mark_ready_for_review(&self, pr: &PrRef) -> Result<PrRef> {
        self.pr_comments.mark_pr_ready_for_review(&pr.node_id).await
    }
}

impl RepoContext {
    fn from_current_dir() -> Result<Self> {
        let git_info = git::get_git_info()?;

        Ok(Self {
            owner: git_info.owner,
            repo: git_info.repo,
            current_branch: git_info.current_branch,
        })
    }
}
