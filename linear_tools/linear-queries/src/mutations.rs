use crate::scalars::DateTime;
use crate::types::Issue;
use linear_schema::linear as schema;

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct IssueCreateInput {
    #[cynic(rename = "teamId")]
    pub team_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i32>,
    #[cynic(rename = "assigneeId")]
    pub assignee_id: Option<String>,
    #[cynic(rename = "stateId")]
    pub state_id: Option<String>,
    #[cynic(rename = "labelIds")]
    pub label_ids: Option<Vec<String>>,
    #[cynic(rename = "projectId")]
    pub project_id: Option<String>,
    #[cynic(rename = "parentId")]
    pub parent_id: Option<String>,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct IssuePayload {
    pub success: bool,
    pub issue: Option<Issue>,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct IssueCreateArguments {
    pub input: IssueCreateInput,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Mutation",
    schema = "linear",
    variables = "IssueCreateArguments"
)]
pub struct IssueCreateMutation {
    #[arguments(input: $input)]
    #[cynic(rename = "issueCreate")]
    pub issue_create: IssuePayload,
}

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct CommentCreateInput {
    #[cynic(rename = "issueId")]
    pub issue_id: String,
    pub body: Option<String>,
    #[cynic(rename = "parentId")]
    pub parent_id: Option<String>,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct Comment {
    pub id: cynic::Id,
    pub body: String,
    #[cynic(rename = "createdAt")]
    pub created_at: DateTime,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct CommentPayload {
    pub success: bool,
    pub comment: Option<Comment>,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct CommentCreateArguments {
    pub input: CommentCreateInput,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Mutation",
    schema = "linear",
    variables = "CommentCreateArguments"
)]
pub struct CommentCreateMutation {
    #[arguments(input: $input)]
    #[cynic(rename = "commentCreate")]
    pub comment_create: CommentPayload,
}
