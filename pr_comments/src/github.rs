use crate::models::{AllComments, IssueComment, PrSummary, ReviewComment};
use anyhow::Result;
use octocrab::Octocrab;

pub struct GitHubClient {
    client: Octocrab,
    owner: String,
    repo: String,
}

impl GitHubClient {
    pub fn new(owner: String, repo: String, token: Option<String>) -> Result<Self> {
        let client = if let Some(token) = token {
            Octocrab::builder()
                .personal_token(token)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to create GitHub client with token: {:?}", e))?
        } else {
            Octocrab::default()
        };

        Ok(Self { client, owner, repo })
    }

    pub async fn get_pr_from_branch(&self, branch: &str) -> Result<Option<u64>> {
        // Search for open PRs with this head branch
        let pulls = self.client
            .pulls(&self.owner, &self.repo)
            .list()
            .state(octocrab::params::State::Open)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list open pull requests for {}/{}: {:?}", self.owner, self.repo, e))?;

        for pr in pulls {
            if pr.head.ref_field == branch {
                return Ok(Some(pr.number));
            }
        }

        Ok(None)
    }

    pub async fn get_all_comments(&self, pr_number: u64) -> Result<AllComments> {
        // Get PR details
        let pr = self.client
            .pulls(&self.owner, &self.repo)
            .get(pr_number)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get PR #{} details for {}/{}: {:?}", pr_number, self.owner, self.repo, e))?;

        // Get review comments (code comments)
        let review_comments = self.get_review_comments(pr_number).await?;

        // Get issue comments (discussion comments)
        let issue_comments = self.get_issue_comments(pr_number).await?;

        Ok(AllComments {
            pr_number,
            pr_title: pr.title.unwrap_or_default(),
            total_comments: review_comments.len() + issue_comments.len(),
            review_comments,
            issue_comments,
        })
    }

    pub async fn get_review_comments(&self, pr_number: u64) -> Result<Vec<ReviewComment>> {
        let mut comments = Vec::new();
        let mut page = 1u32;

        loop {
            let response = self.client
                .pulls(&self.owner, &self.repo)
                .list_comments(Some(pr_number))
                .page(page)
                .per_page(100)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch review comments for PR #{}: {:?}", pr_number, e))?;

            if response.items.is_empty() {
                break;
            }

            comments.extend(
                response.items
                    .into_iter()
                    .map(ReviewComment::from)
            );

            page += 1;
        }

        Ok(comments)
    }

    pub async fn get_issue_comments(&self, pr_number: u64) -> Result<Vec<IssueComment>> {
        let mut comments = Vec::new();
        let mut page = 1u32;

        loop {
            let response = self.client
                .issues(&self.owner, &self.repo)
                .list_comments(pr_number)
                .page(page)
                .per_page(100)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to fetch issue comments for PR #{}: {:?}", pr_number, e))?;

            if response.items.is_empty() {
                break;
            }

            comments.extend(
                response.items
                    .into_iter()
                    .map(IssueComment::from)
            );

            page += 1;
        }

        Ok(comments)
    }

    pub async fn list_prs(&self, state: Option<String>) -> Result<Vec<PrSummary>> {
        let state = match state.as_deref() {
            Some("open") => octocrab::params::State::Open,
            Some("closed") => octocrab::params::State::Closed,
            Some("all") => octocrab::params::State::All,
            None => octocrab::params::State::Open,
            _ => anyhow::bail!("Invalid state: {}. Use 'open', 'closed', or 'all'", state.unwrap()),
        };

        let pulls = self.client
            .pulls(&self.owner, &self.repo)
            .list()
            .state(state)
            .per_page(30)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to list pull requests for {}/{}: {:?}", self.owner, self.repo, e))?;

        Ok(pulls.items.into_iter().map(|pr| PrSummary {
            number: pr.number,
            title: pr.title.unwrap_or_default(),
            author: pr.user.map(|u| u.login).unwrap_or_default(),
            state: if pr.state == Some(octocrab::models::IssueState::Open) {
                "open".to_string()
            } else {
                "closed".to_string()
            },
            created_at: pr.created_at.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
            updated_at: pr.updated_at.map(|dt| dt.to_rfc3339()).unwrap_or_default(),
            comment_count: pr.comments.unwrap_or(0) as u32,
            review_comment_count: pr.review_comments.unwrap_or(0) as u32,
        }).collect())
    }
}