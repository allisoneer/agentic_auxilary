use crate::models::{
    AllComments, GraphQLResponse, IssueComment, PrSummary, PullRequestData, ReviewComment,
};
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

        // Get all review comments (include resolved, include replies)
        let review_comments = self
            .get_review_comments(pr_number, Some(true), Some(true), None, None, None)
            .await?;

        // Get all issue comments
        let issue_comments = self.get_issue_comments(pr_number, None, None, None).await?;

        Ok(AllComments {
            pr_number,
            pr_title: pr.title.unwrap_or_default(),
            total_comments: review_comments.len() + issue_comments.len(),
            review_comments,
            issue_comments,
        })
    }

    pub async fn get_review_comments(
        &self,
        pr_number: u64,
        include_resolved: Option<bool>,
        include_replies: Option<bool>,
        author: Option<&str>,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<Vec<ReviewComment>> {
        // Quick return for limit=0
        if matches!(limit, Some(0)) {
            return Ok(vec![]);
        }

        let include_resolved = include_resolved.unwrap_or(false);
        let include_replies = include_replies.unwrap_or(true);

        // Preload resolution map only if filtering resolved
        let resolution_map = if !include_resolved {
            Some(self.get_review_thread_resolution_status(pr_number).await?)
        } else {
            None
        };

        use std::collections::HashSet;

        let mut results: Vec<ReviewComment> = Vec::new();
        let mut passing_parents: HashSet<u64> = HashSet::new();
        let mut skip_remaining = offset.unwrap_or(0);
        let take_limit = limit.unwrap_or(usize::MAX);

        // Track thread context for page-local completion
        let mut current_thread_parent: Option<u64> = None;
        let mut finish_thread_on_page: Option<u64> = None;

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

            for raw in response.items {
                let c = ReviewComment::from(raw);

                // Filter 1: Resolution
                if let Some(map) = resolution_map.as_ref()
                    && let Some(&is_resolved) = map.get(&c.id)
                    && is_resolved
                {
                    continue;
                }

                let is_reply = c.in_reply_to_id.is_some();
                let parent_id = c.in_reply_to_id;

                // Filter 2: Replies
                if !include_replies && is_reply {
                    continue;
                }

                // Filter 3: Author (thread-scoped)
                let mut keep = true;
                if let Some(author_login) = author {
                    if !is_reply {
                        // Top-level must match author
                        keep = c.user == author_login;
                        if keep {
                            passing_parents.insert(c.id);
                        }
                    } else {
                        // Reply: include iff parent passed filter
                        keep = parent_id
                            .map(|pid| passing_parents.contains(&pid))
                            .unwrap_or(false);
                    }
                }

                if !keep {
                    if !is_reply && author.is_some() {
                        current_thread_parent = None;
                    }
                    continue;
                }

                // Track thread context
                if !is_reply {
                    current_thread_parent = Some(c.id);
                }

                // Filter 4: Offset/limit with counters
                if skip_remaining > 0 {
                    skip_remaining -= 1;
                    continue;
                }

                if results.len() < take_limit {
                    results.push(c);
                } else if let (Some(pid), Some(finish_parent)) = (parent_id, finish_thread_on_page)
                {
                    // Finishing replies for specific parent on this page
                    if pid == finish_parent {
                        results.push(c);
                    }
                } else if let (Some(cur_parent), true) = (current_thread_parent, include_replies) {
                    // Hit limit; start page-local thread completion
                    if finish_thread_on_page.is_none() {
                        finish_thread_on_page = Some(cur_parent);
                    }
                    if is_reply && parent_id == Some(cur_parent) {
                        results.push(c);
                    }
                }
            }

            // Stop after this page if finishing thread or hit limit
            if finish_thread_on_page.is_some() || results.len() >= take_limit {
                break;
            }

            page += 1;
        }

        Ok(results)
    }

    pub async fn get_issue_comments(
        &self,
        pr_number: u64,
        author: Option<&str>,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<Vec<IssueComment>> {
        if matches!(limit, Some(0)) {
            return Ok(vec![]);
        }

        let mut results = Vec::new();
        let mut skip_remaining = offset.unwrap_or(0);
        let take_limit = limit.unwrap_or(usize::MAX);

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

            for raw in response.items {
                let c = IssueComment::from(raw);

                // Author filter
                if let Some(a) = author
                    && c.user != a
                {
                    continue;
                }

                // Offset/limit
                if skip_remaining > 0 {
                    skip_remaining -= 1;
                    continue;
                }

                if results.len() < take_limit {
                    results.push(c);
                }
            }

            if results.len() >= take_limit {
                break;
            }
            page += 1;
        }

        Ok(results)
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

    async fn get_review_thread_resolution_status(
        &self,
        pr_number: u64,
    ) -> Result<HashMap<u64, bool>> {
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
                && !errors.is_empty()
            {
                return Err(anyhow::anyhow!(
                    "GraphQL errors: {}",
                    errors
                        .iter()
                        .map(|e| e.message.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            let data = response
                .data
                .ok_or_else(|| anyhow::anyhow!("No data in GraphQL response"))?;
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
