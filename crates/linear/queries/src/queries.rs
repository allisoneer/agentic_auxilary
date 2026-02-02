use crate::filters::*;
use crate::types::*;
use linear_schema::linear as schema;

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct IssuesArguments {
    pub first: Option<i32>,
    pub after: Option<String>,
    pub filter: Option<IssueFilter>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Query",
    schema = "linear",
    variables = "IssuesArguments"
)]
pub struct IssuesQuery {
    #[arguments(first: $first, after: $after, filter: $filter)]
    pub issues: IssueConnection,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct IssueByIdArguments {
    pub id: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Query",
    schema = "linear",
    variables = "IssueByIdArguments"
)]
pub struct IssueByIdQuery {
    #[arguments(id: $id)]
    pub issue: Option<Issue>,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct SearchIssuesArguments {
    pub term: String,
    #[cynic(rename = "includeComments")]
    pub include_comments: Option<bool>,
    pub first: Option<i32>,
    pub after: Option<String>,
    pub filter: Option<IssueFilter>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Query",
    schema = "linear",
    variables = "SearchIssuesArguments"
)]
pub struct SearchIssuesQuery {
    #[arguments(term: $term, includeComments: $include_comments, first: $first, after: $after, filter: $filter)]
    #[cynic(rename = "searchIssues")]
    pub search_issues: IssueSearchPayload,
}

// ============================================================================
// Metadata queries
// ============================================================================

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct UsersArguments {
    pub first: Option<i32>,
    pub after: Option<String>,
    pub filter: Option<UserFilter>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Query",
    schema = "linear",
    variables = "UsersArguments"
)]
pub struct UsersQuery {
    #[arguments(first: $first, after: $after, filter: $filter)]
    pub users: UserConnection,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct TeamsArguments {
    pub first: Option<i32>,
    pub after: Option<String>,
    pub filter: Option<TeamFilter>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Query",
    schema = "linear",
    variables = "TeamsArguments"
)]
pub struct TeamsQuery {
    #[arguments(first: $first, after: $after, filter: $filter)]
    pub teams: TeamConnection,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct ProjectsArguments {
    pub first: Option<i32>,
    pub after: Option<String>,
    pub filter: Option<ProjectFilter>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Query",
    schema = "linear",
    variables = "ProjectsArguments"
)]
pub struct ProjectsQuery {
    #[arguments(first: $first, after: $after, filter: $filter)]
    pub projects: ProjectConnection,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct WorkflowStatesArguments {
    pub first: Option<i32>,
    pub after: Option<String>,
    pub filter: Option<WorkflowStateFilter>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Query",
    schema = "linear",
    variables = "WorkflowStatesArguments"
)]
pub struct WorkflowStatesQuery {
    #[arguments(first: $first, after: $after, filter: $filter)]
    #[cynic(rename = "workflowStates")]
    pub workflow_states: WorkflowStateConnection,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct IssueLabelsArguments {
    pub first: Option<i32>,
    pub after: Option<String>,
    pub filter: Option<IssueLabelFilter>,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Query",
    schema = "linear",
    variables = "IssueLabelsArguments"
)]
pub struct IssueLabelsQuery {
    #[arguments(first: $first, after: $after, filter: $filter)]
    #[cynic(rename = "issueLabels")]
    pub issue_labels: IssueLabelConnection,
}
