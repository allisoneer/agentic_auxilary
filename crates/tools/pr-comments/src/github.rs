use crate::models::{
    CommentSourceType, GraphQLResponse, PrSummary, PullRequestData, ReviewComment, Thread,
};
use anyhow::Result;
use octocrab::Octocrab;
use std::collections::HashMap;
use std::time::Duration;

pub struct GitHubClient {
    client: Octocrab,
    owner: String,
    repo: String,
}

impl GitHubClient {
    pub fn new(owner: String, repo: String, token: Option<String>) -> Result<Self> {
        let builder = Octocrab::builder()
            .set_connect_timeout(Some(Duration::from_secs(10)))
            .set_read_timeout(Some(Duration::from_secs(30)))
            .set_write_timeout(Some(Duration::from_secs(30)));

        let builder = if let Some(token) = token {
            builder.personal_token(token)
        } else {
            builder
        };

        let client = builder
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create GitHub client: {:?}", e))?;

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
        let mut passing_parents: HashSet<u64> = HashSet::new(); // author-filter passing top-levels

        // NEW: Track parents that were offset-skipped vs. actually included.
        // - skipped_parents: Parents skipped by offset; their replies must not leak through.
        // - included_parents: Parents actually added to results; only these can have replies shown.
        let mut skipped_parents: HashSet<u64> = HashSet::new();
        let mut included_parents: HashSet<u64> = HashSet::new();

        let mut skip_remaining = offset.unwrap_or(0);
        let take_limit = limit.unwrap_or(usize::MAX);

        // Intentional page-local thread completion: finish replies on current page only.
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

                // Filter 1: Resolution (unchanged)
                if let Some(map) = resolution_map.as_ref()
                    && let Some(&is_resolved) = map.get(&c.id)
                    && is_resolved
                {
                    continue;
                }

                let is_reply = c.in_reply_to_id.is_some();
                let parent_id = c.in_reply_to_id;

                // Filter 2: Replies flag (unchanged)
                if !include_replies && is_reply {
                    continue;
                }

                // Filter 3: Author (thread-scoped) - unchanged semantics
                let mut keep = true;
                if let Some(author_login) = author {
                    if !is_reply {
                        keep = c.user == author_login;
                        if keep {
                            // NOTE: This records that the parent passed author filter.
                            // It does NOT mean it was included in results.
                            passing_parents.insert(c.id);
                        }
                    } else {
                        // Replies pass author filter iff their parent passed.
                        keep = parent_id
                            .map(|pid| passing_parents.contains(&pid))
                            .unwrap_or(false);
                    }
                }
                if !keep {
                    // No state updates for excluded items beyond author semantics.
                    continue;
                }

                // NEW: Reply gating BEFORE offset/limit
                // Ensures replies only appear if their parent is actually included in results,
                // and replies to offset-skipped parents do NOT count toward the offset.
                if is_reply {
                    if let Some(pid) = parent_id {
                        // If parent was offset-skipped, silently drop this reply.
                        if skipped_parents.contains(&pid) {
                            continue;
                        }
                        // If parent hasn't been included yet (and we're not finishing it), drop reply.
                        if !included_parents.contains(&pid) && finish_thread_on_page != Some(pid) {
                            continue;
                        }
                    } else {
                        // Defensive: replies must have a parent
                        continue;
                    }
                }

                // Filter 4: Offset handling
                if skip_remaining > 0 {
                    skip_remaining -= 1;
                    if !is_reply {
                        // Track this parent so its replies cannot leak through the offset.
                        skipped_parents.insert(c.id);
                    }
                    continue;
                }

                // Limit handling + results insertion
                if results.len() < take_limit {
                    // We can still take items; insert and update included-parent state.
                    results.push(c.clone());
                    if !is_reply {
                        included_parents.insert(c.id);
                    }
                    continue;
                }

                // Over limit: page-local thread completion for replies only,
                // and only if the parent was actually included in results.
                // This is intentional: we do NOT fetch additional pages to complete threads.
                if include_replies
                    && is_reply
                    && let Some(pid) = parent_id
                    && included_parents.contains(&pid)
                {
                    if finish_thread_on_page.is_none() {
                        // Activate completion ONLY for a parent already in results.
                        finish_thread_on_page = Some(pid);
                    }
                    if finish_thread_on_page == Some(pid) {
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

    pub async fn get_review_thread_resolution_status(
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

    /// Fetch all review comments for a PR without complex filtering.
    /// Returns all comments in API order.
    pub async fn fetch_review_comments(&self, pr_number: u64) -> Result<Vec<ReviewComment>> {
        let mut results = Vec::new();
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
                results.push(ReviewComment::from(raw));
            }

            page += 1;
        }

        Ok(results)
    }

    /// Build threads from a flat list of comments.
    /// Groups comments by parent ID; top-level comments become thread parents.
    pub fn build_threads(
        &self,
        comments: Vec<ReviewComment>,
        resolution_map: &HashMap<u64, bool>,
    ) -> Vec<Thread> {
        // Separate parents from replies
        let mut parents: Vec<ReviewComment> = Vec::new();
        let mut replies_by_parent: HashMap<u64, Vec<ReviewComment>> = HashMap::new();

        for c in comments {
            if let Some(parent_id) = c.in_reply_to_id {
                replies_by_parent.entry(parent_id).or_default().push(c);
            } else {
                parents.push(c);
            }
        }

        // Build threads
        parents
            .into_iter()
            .map(|parent| {
                let is_resolved = resolution_map.get(&parent.id).copied().unwrap_or(false);
                let replies = replies_by_parent.remove(&parent.id).unwrap_or_default();
                Thread {
                    parent,
                    replies,
                    is_resolved,
                }
            })
            .collect()
    }

    /// Filter threads by resolution status and comment source type.
    pub fn filter_threads(
        &self,
        threads: Vec<Thread>,
        src: CommentSourceType,
        include_resolved: bool,
    ) -> Vec<Thread> {
        threads
            .into_iter()
            .filter(|thread| {
                // Filter by resolution
                if !include_resolved && thread.is_resolved {
                    return false;
                }

                // Filter by comment source type (based on parent's is_bot)
                match src {
                    CommentSourceType::Robot => thread.parent.is_bot,
                    CommentSourceType::Human => !thread.parent.is_bot,
                    CommentSourceType::All => true,
                }
            })
            .collect()
    }

    /// Reply to an existing review comment on a PR.
    /// Returns the created comment.
    pub async fn reply_to_comment(
        &self,
        pr_number: u64,
        comment_id: u64,
        body: &str,
    ) -> Result<ReviewComment> {
        let comment = self
            .client
            .pulls(&self.owner, &self.repo)
            .reply_to_comment(pr_number, octocrab::models::CommentId(comment_id), body)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to reply to comment {} on PR #{}: {:?}",
                    comment_id,
                    pr_number,
                    e
                )
            })?;

        Ok(ReviewComment::from_review_comment(comment))
    }
}

// Test helper module - public for integration tests
pub mod test_helpers {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[derive(Debug, Clone)]
    pub struct FilterParams<'a> {
        pub include_resolved: bool,
        pub include_replies: bool,
        pub author: Option<&'a str>,
        pub offset: Option<usize>,
        pub limit: Option<usize>,
        // For tests that want to simulate resolution filtering:
        pub resolved_ids: HashMap<u64, bool>, // id -> is_resolved
    }

    // Pure in-memory pipeline that mirrors get_review_comments logic.
    pub fn apply_filters(mut comments: Vec<ReviewComment>, p: FilterParams) -> Vec<ReviewComment> {
        let mut results: Vec<ReviewComment> = Vec::new();
        let mut passing_parents: HashSet<u64> = HashSet::new();
        let mut skipped_parents: HashSet<u64> = HashSet::new();
        let mut included_parents: HashSet<u64> = HashSet::new();
        let mut finish_thread_on_page: Option<u64> = None;

        let mut skip_remaining = p.offset.unwrap_or(0);
        let take_limit = p.limit.unwrap_or(usize::MAX);

        for c in comments.drain(..) {
            // Filter 1: Resolution
            if !p.include_resolved
                && let Some(&is_resolved) = p.resolved_ids.get(&c.id)
                && is_resolved
            {
                continue;
            }

            let is_reply = c.in_reply_to_id.is_some();
            let parent_id = c.in_reply_to_id;

            // Filter 2: Replies flag
            if !p.include_replies && is_reply {
                continue;
            }

            // Filter 3: Author (thread-scoped)
            let mut keep = true;
            if let Some(login) = p.author {
                if !is_reply {
                    keep = c.user == login;
                    if keep {
                        passing_parents.insert(c.id);
                    }
                } else {
                    keep = parent_id
                        .map(|pid| passing_parents.contains(&pid))
                        .unwrap_or(false);
                }
            }
            if !keep {
                continue;
            }

            // Reply gating before offset/limit
            if is_reply {
                if let Some(pid) = parent_id {
                    if skipped_parents.contains(&pid) {
                        continue;
                    }
                    if !included_parents.contains(&pid) && finish_thread_on_page != Some(pid) {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            // Offset
            if skip_remaining > 0 {
                skip_remaining -= 1;
                if !is_reply {
                    skipped_parents.insert(c.id);
                }
                continue;
            }

            // Limit + insertion
            if results.len() < take_limit {
                if !is_reply {
                    included_parents.insert(c.id);
                }
                results.push(c);
                continue;
            }

            // Over limit: page-local thread completion
            if p.include_replies
                && is_reply
                && let Some(pid) = parent_id
                && included_parents.contains(&pid)
            {
                if finish_thread_on_page.is_none() {
                    finish_thread_on_page = Some(pid);
                }
                if finish_thread_on_page == Some(pid) {
                    results.push(c);
                }
            }
        }

        results
    }
}
