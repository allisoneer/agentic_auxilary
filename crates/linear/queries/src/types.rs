use crate::scalars::{DateTime, TimelessDate};
use linear_schema::linear as schema;

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct User {
    pub id: cynic::Id,
    pub name: String,
    #[cynic(rename = "displayName")]
    pub display_name: String,
    pub email: String,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct Team {
    pub id: cynic::Id,
    pub key: String,
    pub name: String,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct WorkflowState {
    pub id: cynic::Id,
    pub name: String,
    #[cynic(rename = "type")]
    pub state_type: String,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct Project {
    pub id: cynic::Id,
    pub name: String,
}

/// Minimal parent issue fragment to avoid recursive Issue expansion.
#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear", graphql_type = "Issue")]
pub struct ParentIssue {
    pub id: cynic::Id,
    pub identifier: String,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct Issue {
    pub id: cynic::Id,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: f64,
    #[cynic(rename = "priorityLabel")]
    pub priority_label: String,
    #[cynic(rename = "labelIds")]
    pub label_ids: Vec<String>,
    #[cynic(rename = "dueDate")]
    pub due_date: Option<TimelessDate>,
    pub url: String,
    #[cynic(rename = "createdAt")]
    pub created_at: DateTime,
    #[cynic(rename = "updatedAt")]
    pub updated_at: DateTime,

    // Details-only metadata (still fetched on Issue fragment)
    pub estimate: Option<f64>,
    pub parent: Option<ParentIssue>,
    #[cynic(rename = "startedAt")]
    pub started_at: Option<DateTime>,
    #[cynic(rename = "completedAt")]
    pub completed_at: Option<DateTime>,
    #[cynic(rename = "canceledAt")]
    pub canceled_at: Option<DateTime>,

    pub creator: Option<User>,
    pub team: Team,
    pub state: Option<WorkflowState>,
    pub assignee: Option<User>,
    pub project: Option<Project>,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct PageInfo {
    #[cynic(rename = "hasNextPage")]
    pub has_next_page: bool,
    #[cynic(rename = "endCursor")]
    pub end_cursor: Option<String>,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct IssueConnection {
    pub nodes: Vec<Issue>,
    #[cynic(rename = "pageInfo")]
    pub page_info: PageInfo,
}

/// IssueSearchResult from searchIssues query - has issue fields directly.
/// NOTE: Duplicates subset of Issue fields; keep in sync.
/// Nullability differs (e.g., state is non-null here).
#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct IssueSearchResult {
    pub id: cynic::Id,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: f64,
    #[cynic(rename = "priorityLabel")]
    pub priority_label: String,
    #[cynic(rename = "labelIds")]
    pub label_ids: Vec<String>,
    #[cynic(rename = "dueDate")]
    pub due_date: Option<TimelessDate>,
    pub url: String,
    #[cynic(rename = "createdAt")]
    pub created_at: DateTime,
    #[cynic(rename = "updatedAt")]
    pub updated_at: DateTime,
    pub creator: Option<User>,
    pub team: Team,
    pub state: WorkflowState,
    pub assignee: Option<User>,
    pub project: Option<Project>,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct IssueSearchPayload {
    pub nodes: Vec<IssueSearchResult>,
    #[cynic(rename = "pageInfo")]
    pub page_info: PageInfo,
}

// ============================================================================
// Metadata query types
// ============================================================================

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct IssueLabel {
    pub id: cynic::Id,
    pub name: String,
    pub team: Option<Team>,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct UserConnection {
    pub nodes: Vec<User>,
    #[cynic(rename = "pageInfo")]
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct TeamConnection {
    pub nodes: Vec<Team>,
    #[cynic(rename = "pageInfo")]
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct ProjectConnection {
    pub nodes: Vec<Project>,
    #[cynic(rename = "pageInfo")]
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct WorkflowStateConnection {
    pub nodes: Vec<WorkflowState>,
    #[cynic(rename = "pageInfo")]
    pub page_info: PageInfo,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct IssueLabelConnection {
    pub nodes: Vec<IssueLabel>,
    #[cynic(rename = "pageInfo")]
    pub page_info: PageInfo,
}
