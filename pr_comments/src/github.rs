use crate::models::{AllComments, IssueComment, PrSummary, ReviewComment, GraphQLResponse, PullRequestData};
use anyhow::Result;
use octocrab::Octocrab;
use std::collections::HashMap;

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
                .map_err(|e| {
                    anyhow::anyhow!("Failed to create GitHub client with token: {:?}", e)
                })?
        } else {
            Octocrab::default()
        };

        Ok(Self {
            client,
            owner,
            repo,
        })
    }

    pub async fn get_pr_from_branch(&self, branch: &str) -> Result<Option<u64>> {
        // Search for open PRs with this head branch
        let pulls = self
            .client
            .pulls(&self.owner, &self.repo)
            .list()
            .state(octocrab::params::State::Open)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to list open pull requests for {}/{}: {:?}",
                    self.owner,
                    self.repo,
                    e
                )
            })?;

        for pr in pulls {
            if pr.head.ref_field == branch {
                return Ok(Some(pr.number));
            }
        }

        Ok(None)
    }

    pub async fn get_all_comments(&self, pr_number: u64) -> Result<AllComments> {
        // Get PR details
        let pr = self
            .client
            .pulls(&self.owner, &self.repo)
            .get(pr_number)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to get PR #{} details for {}/{}: {:?}",
                    pr_number,
                    self.owner,
                    self.repo,
                    e
                )
            })?;

        // Get review comments (code comments) - always include all for this method
        let review_comments = self.get_review_comments(pr_number, Some(true)).await?;

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

    pub async fn get_review_comments(&self, pr_number: u64, include_resolved: Option<bool>) -> Result<Vec<ReviewComment>> {
        // First, fetch all review comments using REST API
        let mut all_comments = Vec::new();
        let mut page = 1u32;

        loop {
            let response = self
                .client
                .pulls(&self.owner, &self.repo)
                .list_comments(Some(pr_number))
                .page(page)
                .per_page(100)
                .send()
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to fetch review comments for PR #{}: {:?}",
                        pr_number,
                        e
                    )
                })?;

            if response.items.is_empty() {
                break;
            }

            all_comments.extend(response.items.into_iter().map(ReviewComment::from));
            page += 1;
        }

        // If include_resolved is None or false, filter out resolved comments
        let include_resolved = include_resolved.unwrap_or(false);
        if !include_resolved && !all_comments.is_empty() {
            // Fetch resolution status via GraphQL
            let resolution_map = self.get_review_thread_resolution_status(pr_number).await?;

            // Filter out resolved comments
            all_comments.retain(|comment| {
                // If we don't have resolution info for a comment, include it by default
                // If we do have info and it's resolved, exclude it
                resolution_map.get(&comment.id).map(|&is_resolved| !is_resolved).unwrap_or(true)
            });
        }

        Ok(all_comments)
    }

    pub async fn get_issue_comments(&self, pr_number: u64) -> Result<Vec<IssueComment>> {
        let mut comments = Vec::new();
        let mut page = 1u32;

        loop {
            let response = self
                .client
                .issues(&self.owner, &self.repo)
                .list_comments(pr_number)
                .page(page)
                .per_page(100)
                .send()
                .await
                .map_err(|e| {
                    anyhow::anyhow!(
                        "Failed to fetch issue comments for PR #{}: {:?}",
                        pr_number,
                        e
                    )
                })?;

            if response.items.is_empty() {
                break;
            }

            comments.extend(response.items.into_iter().map(IssueComment::from));

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
            _ => anyhow::bail!(
                "Invalid state: {}. Use 'open', 'closed', or 'all'",
                state.unwrap()
            ),
        };

        let pulls = self
            .client
            .pulls(&self.owner, &self.repo)
            .list()
            .state(state)
            .per_page(30)
            .send()
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to list pull requests for {}/{}: {:?}",
                    self.owner,
                    self.repo,
                    e
                )
            })?;

        Ok(pulls
            .items
            .into_iter()
            .map(|pr| PrSummary {
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
            })
            .collect())
    }

    async fn get_review_thread_resolution_status(&self, pr_number: u64) -> Result<HashMap<u64, bool>> {
        let query = r#"
            query($owner: String!, $repo: String!, $number: Int!, $cursor: String) {
                repository(owner: $owner, name: $repo) {
                    pullRequest(number: $number) {
                        reviewThreads(first: 100, after: $cursor) {
                            nodes {
                                id
                                isResolved
                                comments(first: 50) {
                                    nodes {
                                        id
                                        databaseId
                                    }
                                }
                            }
                            pageInfo {
                                hasNextPage
                                endCursor
                            }
                        }
                    }
                }
            }
        "#;

        let mut comment_resolution_map = HashMap::new();
        let mut cursor: Option<String> = None;

        loop {
            let variables = serde_json::json!({
                "owner": self.owner,
                "repo": self.repo,
                "number": pr_number as i32,
                "cursor": cursor,
            });

            let response: GraphQLResponse<PullRequestData> = self
                .client
                .graphql(&serde_json::json!({
                    "query": query,
                    "variables": variables,
                }))
                .await
                .map_err(|e| anyhow::anyhow!("GraphQL query failed: {}", e))?;

            if let Some(errors) = response.errors
                && !errors.is_empty() {
                    return Err(anyhow::anyhow!(
                        "GraphQL errors: {}",
                        errors.iter().map(|e| e.message.as_str()).collect::<Vec<_>>().join(", ")
                    ));
                }

            let data = response.data.ok_or_else(|| anyhow::anyhow!("No data in GraphQL response"))?;
            let threads = &data.repository.pull_request.review_threads;

            // Build map of comment ID -> resolution status
            for thread in &threads.nodes {
                for comment in &thread.comments.nodes {
                    if let Some(db_id) = comment.database_id {
                        comment_resolution_map.insert(db_id, thread.is_resolved);
                    }
                }
            }

            if !threads.page_info.has_next_page {
                break;
            }

            cursor = threads.page_info.end_cursor.clone();
        }

        Ok(comment_resolution_map)
    }
}
