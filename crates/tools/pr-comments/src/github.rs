use crate::OpenPrRefLookupResult;
use crate::models::CheckSuiteSummary;
use crate::models::CommentSourceType;
use crate::models::GraphQLResponse;
use crate::models::IssueCommentSummary;
use crate::models::OpenPrRefData;
use crate::models::PrRef;
use crate::models::PrSummary;
use crate::models::PullRequestData;
use crate::models::PullRequestReviewSummary;
use crate::models::ReviewComment;
use crate::models::Thread;
use anyhow::Result;
use octocrab::Octocrab;
use reqwest::header::ACCEPT;
use reqwest::header::AUTHORIZATION;
use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;
use reqwest::header::USER_AGENT;
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Duration;

const REST_PER_PAGE: usize = 100;

pub struct GitHubClient {
    client: Octocrab,
    http: reqwest::Client,
    owner: String,
    repo: String,
    rest_base_url: String,
}

impl GitHubClient {
    pub fn new(owner: String, repo: String, token: Option<String>) -> Result<Self> {
        let header_token = token.clone();
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
            .map_err(|e| anyhow::anyhow!("Failed to create GitHub client: {e:?}"))?;

        let mut headers = HeaderMap::new();
        headers.insert(USER_AGENT, HeaderValue::from_static("pr_comments"));
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        if let Some(token) = header_token.as_deref() {
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|e| anyhow::anyhow!("Invalid GitHub token header: {e}"))?;
            headers.insert(AUTHORIZATION, value);
        }
        let http = reqwest::Client::builder()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create GitHub REST client: {e}"))?;

        Ok(Self {
            client,
            http,
            owner,
            repo,
            rest_base_url: "https://api.github.com".to_string(),
        })
    }

    #[cfg(test)]
    fn with_rest_base_url(mut self, rest_base_url: String) -> Self {
        self.rest_base_url = rest_base_url;
        self
    }

    pub async fn get_pr_from_branch(&self, branch: &str) -> Result<Option<u64>> {
        Ok(self
            .get_open_pr_ref_from_branch(branch)
            .await?
            .map(|pr| pr.number))
    }

    pub async fn get_open_pr_ref_from_branch(&self, branch: &str) -> Result<Option<PrRef>> {
        Ok(self.get_open_pr_ref_from_branch_detailed(branch).await?.pr)
    }

    pub async fn get_open_pr_ref_from_branch_detailed(
        &self,
        branch: &str,
    ) -> Result<OpenPrRefLookupResult> {
        let query = r"
            query($owner: String!, $repo: String!, $branch: String!) {
                repository(owner: $owner, name: $repo) {
                    pullRequests(states: OPEN, headRefName: $branch, first: 1) {
                        nodes {
                            number
                            url
                            headRefOid
                        }
                    }
                }
            }
        ";
        let variables = serde_json::json!({
            "owner": self.owner,
            "repo": self.repo,
            "branch": branch,
        });
        let response: GraphQLResponse<OpenPrRefData> = self
            .client
            .graphql(&serde_json::json!({
                "query": query,
                "variables": variables,
            }))
            .await
            .map_err(|e| anyhow::anyhow!("GraphQL query failed: {e}"))?;

        parse_open_pr_ref_lookup_response(response)
    }
}

fn parse_open_pr_ref_lookup_response(
    response: GraphQLResponse<OpenPrRefData>,
) -> Result<OpenPrRefLookupResult> {
    if let Some(errors) = response.errors
        && !errors.is_empty()
    {
        let messages = errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::bail!("GraphQL errors: {messages}");
    }

    let Some(data) = response.data else {
        return Ok(OpenPrRefLookupResult {
            pr: None,
            empty_result_reason: Some("graphql_response_missing_data".to_string()),
        });
    };

    let nodes = data.repository.pull_requests.nodes;
    let pr = nodes.into_iter().next().map(|node| PrRef {
        number: node.number,
        url: node.url,
        head_sha: node.head_ref_oid,
    });

    Ok(OpenPrRefLookupResult {
        empty_result_reason: Some("no_open_pull_requests_matched_branch".to_string())
            .filter(|_| pr.is_none()),
        pr,
    })
}

impl GitHubClient {
    pub async fn list_check_suites_for_ref(&self, sha: &str) -> Result<Vec<CheckSuiteSummary>> {
        let base_path = format!(
            "/repos/{}/{}/commits/{sha}/check-suites",
            self.owner, self.repo
        );
        let mut suites = Vec::new();
        let mut page = 1u32;
        loop {
            let value = self
                .rest_get(&format!("{base_path}?page={page}&per_page={REST_PER_PAGE}"))
                .await?;
            let page_suites = parse_check_suites_response(value)?;
            let page_len = page_suites.len();
            suites.extend(page_suites);
            if page_len < REST_PER_PAGE {
                break;
            }
            page += 1;
        }
        Ok(suites)
    }

    pub async fn list_pull_request_reviews(
        &self,
        pr_number: u64,
    ) -> Result<Vec<PullRequestReviewSummary>> {
        let base_path = format!(
            "/repos/{}/{}/pulls/{pr_number}/reviews",
            self.owner, self.repo
        );
        let mut reviews = Vec::new();
        let mut page = 1u32;
        loop {
            let value = self
                .rest_get(&format!("{base_path}?page={page}&per_page={REST_PER_PAGE}"))
                .await?;
            let page_reviews = parse_reviews_response(value)?;
            let page_len = page_reviews.len();
            reviews.extend(page_reviews);
            if page_len < REST_PER_PAGE {
                break;
            }
            page += 1;
        }
        Ok(reviews)
    }

    pub async fn list_issue_comments(&self, issue_number: u64) -> Result<Vec<IssueCommentSummary>> {
        let base_path = format!(
            "/repos/{}/{}/issues/{issue_number}/comments",
            self.owner, self.repo
        );
        let mut comments = Vec::new();
        let mut page = 1u32;
        loop {
            let value = self
                .rest_get(&format!("{base_path}?page={page}&per_page={REST_PER_PAGE}"))
                .await?;
            let page_comments = parse_issue_comments_response(value)?;
            let page_len = page_comments.len();
            comments.extend(page_comments);
            if page_len < REST_PER_PAGE {
                break;
            }
            page += 1;
        }
        Ok(comments)
    }

    async fn rest_get(&self, path: &str) -> Result<serde_json::Value> {
        let url = format!("{}{path}", self.rest_base_url);
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("GitHub REST request failed: {e}"))?
            .error_for_status()
            .map_err(|e| anyhow::anyhow!("GitHub REST request failed: {e}"))?;
        response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("GitHub REST JSON parse failed: {e}"))
    }

    pub fn parse_check_suites_fixture(value: serde_json::Value) -> Result<Vec<CheckSuiteSummary>> {
        parse_check_suites_response(value)
    }

    pub fn parse_reviews_fixture(
        value: serde_json::Value,
    ) -> Result<Vec<PullRequestReviewSummary>> {
        parse_reviews_response(value)
    }

    pub fn parse_issue_comments_fixture(
        value: serde_json::Value,
    ) -> Result<Vec<IssueCommentSummary>> {
        parse_issue_comments_response(value)
    }

    pub async fn get_review_comments(
        // Search for open PRs with this head branch
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
        let resolution_map = if include_resolved {
            None
        } else {
            Some(self.get_review_thread_resolution_status(pr_number).await?)
        };

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
                    anyhow::anyhow!("Failed to fetch review comments for PR #{pr_number}: {e:?}")
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
                    if is_reply {
                        // Replies pass author filter iff their parent passed.
                        keep = parent_id.is_some_and(|pid| passing_parents.contains(&pid));
                    } else {
                        keep = c.user == author_login;
                        if keep {
                            // NOTE: This records that the parent passed author filter.
                            // It does NOT mean it was included in results.
                            passing_parents.insert(c.id);
                        }
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
            Some("closed") => octocrab::params::State::Closed,
            Some("all") => octocrab::params::State::All,
            Some("open") | None => octocrab::params::State::Open,
            Some(other) => anyhow::bail!("Invalid state: {other}. Use 'open', 'closed', or 'all'"),
        };

        let mut prs = Vec::new();
        let mut page = 1u32;

        loop {
            let pulls = self
                .client
                .pulls(&self.owner, &self.repo)
                .list()
                .state(state)
                .page(page)
                .per_page(100)
                .send()
                .await
                .map_err(|e| {
                    let owner = self.owner.as_str();
                    let repo = self.repo.as_str();
                    anyhow::anyhow!("Failed to list pull requests for {owner}/{repo}: {e:?}")
                })?;

            if pulls.items.is_empty() {
                break;
            }

            prs.extend(pulls.items.into_iter().map(|pr| PrSummary {
                number: pr.number,
                title: pr.title.unwrap_or_default(),
                author: pr.user.map_or_else(String::new, |u| u.login),
                state: if pr.state == Some(octocrab::models::IssueState::Open) {
                    "open".to_string()
                } else {
                    "closed".to_string()
                },
                created_at: pr.created_at.map_or_else(String::new, |dt| dt.to_rfc3339()),
                updated_at: pr.updated_at.map_or_else(String::new, |dt| dt.to_rfc3339()),
                comment_count: pr.comments.unwrap_or(0) as u32,
                review_comment_count: pr.review_comments.unwrap_or(0) as u32,
            }));

            page += 1;
        }

        Ok(prs)
    }

    pub async fn get_review_thread_resolution_status(
        &self,
        pr_number: u64,
    ) -> Result<HashMap<u64, bool>> {
        let query = r"
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
        ";

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
                .map_err(|e| anyhow::anyhow!("GraphQL query failed: {e}"))?;

            if let Some(errors) = response.errors
                && !errors.is_empty()
            {
                let messages = errors
                    .iter()
                    .map(|e| e.message.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(anyhow::anyhow!("GraphQL errors: {messages}"));
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

            cursor.clone_from(&threads.page_info.end_cursor);
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
                    anyhow::anyhow!("Failed to fetch review comments for PR #{pr_number}: {e:?}")
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
                anyhow::anyhow!("Failed to reply to comment {comment_id} on PR #{pr_number}: {e:?}")
            })?;

        Ok(ReviewComment::from_review_comment(comment))
    }
}

#[derive(serde::Deserialize)]
struct CheckSuitesEnvelope {
    check_suites: Vec<CheckSuiteEntry>,
}

#[derive(serde::Deserialize)]
struct CheckSuiteEntry {
    id: u64,
    status: String,
    conclusion: Option<String>,
    app: Option<CheckSuiteApp>,
    updated_at: String,
}

#[derive(serde::Deserialize)]
struct CheckSuiteApp {
    slug: Option<String>,
}

#[derive(serde::Deserialize)]
struct ReviewEntry {
    id: u64,
    state: String,
    submitted_at: Option<String>,
    user: ReviewUser,
}

#[derive(serde::Deserialize)]
struct ReviewUser {
    login: String,
    #[serde(rename = "type")]
    user_type: Option<String>,
}

#[derive(serde::Deserialize)]
struct IssueCommentEntry {
    id: u64,
    body: String,
    created_at: String,
    user: ReviewUser,
}

fn parse_check_suites_response(value: serde_json::Value) -> Result<Vec<CheckSuiteSummary>> {
    let envelope: CheckSuitesEnvelope = serde_json::from_value(value)
        .map_err(|e| anyhow::anyhow!("Failed to parse check suites response: {e}"))?;
    Ok(envelope
        .check_suites
        .into_iter()
        .map(|suite| CheckSuiteSummary {
            id: suite.id,
            status: suite.status,
            conclusion: suite.conclusion,
            app_slug: suite.app.and_then(|app| app.slug),
            updated_at: suite.updated_at,
        })
        .collect())
}

fn parse_reviews_response(value: serde_json::Value) -> Result<Vec<PullRequestReviewSummary>> {
    let entries: Vec<ReviewEntry> = serde_json::from_value(value)
        .map_err(|e| anyhow::anyhow!("Failed to parse reviews response: {e}"))?;
    Ok(entries
        .into_iter()
        .map(|entry| PullRequestReviewSummary {
            id: entry.id,
            user_login: entry.user.login,
            user_type: entry.user.user_type,
            state: entry.state,
            submitted_at: entry.submitted_at,
        })
        .collect())
}

fn parse_issue_comments_response(value: serde_json::Value) -> Result<Vec<IssueCommentSummary>> {
    let entries: Vec<IssueCommentEntry> = serde_json::from_value(value)
        .map_err(|e| anyhow::anyhow!("Failed to parse issue comments response: {e}"))?;
    Ok(entries
        .into_iter()
        .map(|entry| IssueCommentSummary {
            id: entry.id,
            user_login: entry.user.login,
            user_type: entry.user.user_type,
            body: entry.body,
            created_at: entry.created_at,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::GitHubClient;
    use super::REST_PER_PAGE;
    use super::parse_open_pr_ref_lookup_response;
    use crate::models::GraphQLResponse;
    use crate::models::OpenPrRefData;
    use mockito::Matcher;
    use serde_json::json;
    use std::sync::Once;

    fn install_rustls_provider() {
        static INSTALL: Once = Once::new();
        INSTALL.call_once(|| {
            rustls::crypto::aws_lc_rs::default_provider()
                .install_default()
                .expect("rustls provider should install once");
        });
    }

    fn client(base_url: String) -> GitHubClient {
        install_rustls_provider();
        GitHubClient::new("owner".to_string(), "repo".to_string(), None)
            .expect("client should build")
            .with_rest_base_url(base_url)
    }

    fn check_suites_response(count: usize) -> serde_json::Value {
        json!({
            "check_suites": (0..count)
                .map(|index| json!({
                    "id": index + 1,
                    "status": "completed",
                    "conclusion": "success",
                    "app": { "slug": "coderabbitai" },
                    "updated_at": "2026-01-01T00:00:00Z"
                }))
                .collect::<Vec<_>>()
        })
    }

    fn reviews_response(count: usize) -> serde_json::Value {
        json!(
            (0..count)
                .map(|index| json!({
                    "id": index + 1,
                    "state": "COMMENTED",
                    "submitted_at": "2026-01-01T00:00:00Z",
                    "user": {
                        "login": format!("coderabbit-{index}"),
                        "type": "Bot"
                    }
                }))
                .collect::<Vec<_>>()
        )
    }

    fn issue_comments_response(count: usize) -> serde_json::Value {
        json!(
            (0..count)
                .map(|index| json!({
                    "id": index + 1,
                    "body": format!("comment {index}"),
                    "created_at": "2026-01-01T00:00:00Z",
                    "user": {
                        "login": format!("coderabbit-{index}"),
                        "type": "Bot"
                    }
                }))
                .collect::<Vec<_>>()
        )
    }

    #[tokio::test]
    async fn list_check_suites_for_ref_aggregates_multiple_pages() {
        let mut server = mockito::Server::new_async().await;
        let page1 = server
            .mock("GET", "/repos/owner/repo/commits/abc/check-suites")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("page".into(), "1".into()),
                Matcher::UrlEncoded("per_page".into(), REST_PER_PAGE.to_string()),
            ]))
            .with_status(200)
            .with_body(check_suites_response(REST_PER_PAGE).to_string())
            .create_async()
            .await;
        let page2 = server
            .mock("GET", "/repos/owner/repo/commits/abc/check-suites")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("page".into(), "2".into()),
                Matcher::UrlEncoded("per_page".into(), REST_PER_PAGE.to_string()),
            ]))
            .with_status(200)
            .with_body(check_suites_response(1).to_string())
            .create_async()
            .await;

        let suites = client(server.url())
            .list_check_suites_for_ref("abc")
            .await
            .expect("pagination should succeed");

        assert_eq!(suites.len(), REST_PER_PAGE + 1);
        page1.assert_async().await;
        page2.assert_async().await;
    }

    #[tokio::test]
    async fn list_pull_request_reviews_aggregates_multiple_pages() {
        let mut server = mockito::Server::new_async().await;
        let page1 = server
            .mock("GET", "/repos/owner/repo/pulls/7/reviews")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("page".into(), "1".into()),
                Matcher::UrlEncoded("per_page".into(), REST_PER_PAGE.to_string()),
            ]))
            .with_status(200)
            .with_body(reviews_response(REST_PER_PAGE).to_string())
            .create_async()
            .await;
        let page2 = server
            .mock("GET", "/repos/owner/repo/pulls/7/reviews")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("page".into(), "2".into()),
                Matcher::UrlEncoded("per_page".into(), REST_PER_PAGE.to_string()),
            ]))
            .with_status(200)
            .with_body(reviews_response(1).to_string())
            .create_async()
            .await;

        let reviews = client(server.url())
            .list_pull_request_reviews(7)
            .await
            .expect("pagination should succeed");

        assert_eq!(reviews.len(), REST_PER_PAGE + 1);
        page1.assert_async().await;
        page2.assert_async().await;
    }

    #[tokio::test]
    async fn list_issue_comments_aggregates_multiple_pages() {
        let mut server = mockito::Server::new_async().await;
        let page1 = server
            .mock("GET", "/repos/owner/repo/issues/9/comments")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("page".into(), "1".into()),
                Matcher::UrlEncoded("per_page".into(), REST_PER_PAGE.to_string()),
            ]))
            .with_status(200)
            .with_body(issue_comments_response(REST_PER_PAGE).to_string())
            .create_async()
            .await;
        let page2 = server
            .mock("GET", "/repos/owner/repo/issues/9/comments")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("page".into(), "2".into()),
                Matcher::UrlEncoded("per_page".into(), REST_PER_PAGE.to_string()),
            ]))
            .with_status(200)
            .with_body(issue_comments_response(1).to_string())
            .create_async()
            .await;

        let comments = client(server.url())
            .list_issue_comments(9)
            .await
            .expect("pagination should succeed");

        assert_eq!(comments.len(), REST_PER_PAGE + 1);
        page1.assert_async().await;
        page2.assert_async().await;
    }

    #[test]
    fn parse_open_pr_ref_lookup_response_reports_missing_data() {
        let response: GraphQLResponse<OpenPrRefData> = serde_json::from_value(json!({
            "data": null,
            "errors": null
        }))
        .expect("response should deserialize");

        let lookup = parse_open_pr_ref_lookup_response(response).expect("parse should succeed");

        assert!(lookup.pr.is_none());
        assert_eq!(
            lookup.empty_result_reason.as_deref(),
            Some("graphql_response_missing_data")
        );
    }

    #[test]
    fn parse_open_pr_ref_lookup_response_reports_empty_nodes() {
        let response: GraphQLResponse<OpenPrRefData> = serde_json::from_value(json!({
            "data": {
                "repository": {
                    "pullRequests": {
                        "nodes": []
                    }
                }
            },
            "errors": null
        }))
        .expect("response should deserialize");

        let lookup = parse_open_pr_ref_lookup_response(response).expect("parse should succeed");

        assert!(lookup.pr.is_none());
        assert_eq!(
            lookup.empty_result_reason.as_deref(),
            Some("no_open_pull_requests_matched_branch")
        );
    }
}

// Test helper module - public for integration tests
pub mod test_helpers {
    use super::ReviewComment;
    use std::collections::HashMap;
    use std::collections::HashSet;

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
    pub fn apply_filters(comments: Vec<ReviewComment>, p: &FilterParams) -> Vec<ReviewComment> {
        let mut results: Vec<ReviewComment> = Vec::new();
        let mut passing_parents: HashSet<u64> = HashSet::new();
        let mut skipped_parents: HashSet<u64> = HashSet::new();
        let mut included_parents: HashSet<u64> = HashSet::new();
        let mut finish_thread_on_page: Option<u64> = None;

        let mut skip_remaining = p.offset.unwrap_or(0);
        let take_limit = p.limit.unwrap_or(usize::MAX);

        for c in comments {
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
                if is_reply {
                    keep = parent_id.is_some_and(|pid| passing_parents.contains(&pid));
                } else {
                    keep = c.user == login;
                    if keep {
                        passing_parents.insert(c.id);
                    }
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
