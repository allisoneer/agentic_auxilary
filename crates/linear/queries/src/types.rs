use crate::scalars::DateTime;
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

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct Issue {
    pub id: cynic::Id,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: f64,
    pub url: String,
    #[cynic(rename = "createdAt")]
    pub created_at: DateTime,
    #[cynic(rename = "updatedAt")]
    pub updated_at: DateTime,
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

/// IssueSearchResult from searchIssues query - has issue fields directly
#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct IssueSearchResult {
    pub id: cynic::Id,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: f64,
    pub url: String,
    #[cynic(rename = "createdAt")]
    pub created_at: DateTime,
    #[cynic(rename = "updatedAt")]
    pub updated_at: DateTime,
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
