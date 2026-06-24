use anyhow::Result;
use pr_comments::PrComments;
use pr_comments::models::PrRef;

pub struct GitHubPrClient {
    pr_comments: PrComments,
}

impl GitHubPrClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            pr_comments: PrComments::new()?,
        })
    }

    pub async fn detect_open_pr_from_branch(&self, branch: &str) -> Result<Option<PrRef>> {
        self.pr_comments.get_open_pr_ref_from_branch(branch).await
    }
}
