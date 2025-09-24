use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ReviewComment {
    pub id: u64,
    pub user: String,
    pub body: String,
    pub path: String,
    pub line: Option<u64>,
    pub side: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub html_url: String,
    pub pull_request_review_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IssueComment {
    pub id: u64,
    pub user: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: Option<String>,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AllComments {
    pub pr_number: u64,
    pub pr_title: String,
    pub review_comments: Vec<ReviewComment>,
    pub issue_comments: Vec<IssueComment>,
    pub total_comments: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PrSummary {
    pub number: u64,
    pub title: String,
    pub author: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
    pub comment_count: u32,
    pub review_comment_count: u32,
}

impl From<octocrab::models::pulls::Comment> for ReviewComment {
    fn from(comment: octocrab::models::pulls::Comment) -> Self {
        Self {
            id: comment.id.0,
            user: comment.user.map(|u| u.login).unwrap_or_default(),
            body: comment.body,
            path: comment.path,
            line: comment.line,
            side: comment.side,
            created_at: comment.created_at.to_rfc3339(),
            updated_at: comment.updated_at.to_rfc3339(),
            html_url: comment.html_url,
            pull_request_review_id: comment.pull_request_review_id.map(|id| id.0),
        }
    }
}

impl From<octocrab::models::issues::Comment> for IssueComment {
    fn from(comment: octocrab::models::issues::Comment) -> Self {
        Self {
            id: comment.id.0,
            user: comment.user.login,
            body: comment.body.unwrap_or_default(),
            created_at: comment.created_at.to_rfc3339(),
            updated_at: comment.updated_at.map(|dt| dt.to_rfc3339()),
            html_url: comment.html_url.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLResponse<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLError {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestData {
    pub repository: Repository,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Repository {
    #[serde(rename = "pullRequest")]
    pub pull_request: PullRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    #[serde(rename = "reviewThreads")]
    pub review_threads: ReviewThreadConnection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewThreadConnection {
    pub nodes: Vec<ReviewThread>,
    #[serde(rename = "pageInfo")]
    pub page_info: PageInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewThread {
    pub id: String,
    #[serde(rename = "isResolved")]
    pub is_resolved: bool,
    pub comments: ReviewThreadCommentConnection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewThreadCommentConnection {
    pub nodes: Vec<ReviewThreadComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewThreadComment {
    pub id: String,
    #[serde(rename = "databaseId")]
    pub database_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageInfo {
    #[serde(rename = "hasNextPage")]
    pub has_next_page: bool,
    #[serde(rename = "endCursor")]
    pub end_cursor: Option<String>,
}
