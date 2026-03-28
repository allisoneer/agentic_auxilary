use crate::scalars::DateTime;
use crate::scalars::TimelessDate;
use crate::types::Issue;
use linear_schema::linear as schema;

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct IssueCreateInput {
    #[cynic(rename = "teamId")]
    pub team_id: String,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    #[cynic(rename = "assigneeId", skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    #[cynic(rename = "stateId", skip_serializing_if = "Option::is_none")]
    pub state_id: Option<String>,
    #[cynic(rename = "labelIds", skip_serializing_if = "Option::is_none")]
    pub label_ids: Option<Vec<String>>,
    #[cynic(rename = "projectId", skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[cynic(rename = "parentId", skip_serializing_if = "Option::is_none")]
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
pub struct IssueUpdateInput {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    #[cynic(rename = "assigneeId", skip_serializing_if = "Option::is_none")]
    pub assignee_id: Option<String>,
    #[cynic(rename = "stateId", skip_serializing_if = "Option::is_none")]
    pub state_id: Option<String>,
    #[cynic(rename = "projectId", skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[cynic(rename = "parentId", skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[cynic(rename = "labelIds", skip_serializing_if = "Option::is_none")]
    pub label_ids: Option<Vec<String>>,
    #[cynic(rename = "addedLabelIds", skip_serializing_if = "Option::is_none")]
    pub added_label_ids: Option<Vec<String>>,
    #[cynic(rename = "removedLabelIds", skip_serializing_if = "Option::is_none")]
    pub removed_label_ids: Option<Vec<String>>,
    #[cynic(rename = "dueDate", skip_serializing_if = "Option::is_none")]
    pub due_date: Option<TimelessDate>,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct IssueUpdateArguments {
    pub id: String,
    pub input: IssueUpdateInput,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Mutation",
    schema = "linear",
    variables = "IssueUpdateArguments"
)]
pub struct IssueUpdateMutation {
    #[arguments(id: $id, input: $input)]
    #[cynic(rename = "issueUpdate")]
    pub issue_update: IssuePayload,
}

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct CommentCreateInput {
    #[cynic(rename = "issueId")]
    pub issue_id: String,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[cynic(rename = "parentId", skip_serializing_if = "Option::is_none")]
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

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct IssueArchiveArguments {
    pub id: String,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct IssueArchivePayload {
    pub success: bool,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Mutation",
    schema = "linear",
    variables = "IssueArchiveArguments"
)]
pub struct IssueArchiveMutation {
    #[arguments(id: $id)]
    #[cynic(rename = "issueArchive")]
    pub issue_archive: IssueArchivePayload,
}

// ============================================================================
// Issue relation mutations
// ============================================================================

#[derive(cynic::Enum, Clone, Copy, Debug)]
#[cynic(schema = "linear")]
pub enum IssueRelationType {
    #[cynic(rename = "blocks")]
    Blocks,
    #[cynic(rename = "duplicate")]
    Duplicate,
    #[cynic(rename = "related")]
    Related,
    #[cynic(rename = "similar")]
    Similar,
}

#[derive(cynic::InputObject, Clone, Debug)]
#[cynic(schema = "linear")]
pub struct IssueRelationCreateInput {
    #[cynic(rename = "issueId")]
    pub issue_id: String,
    #[cynic(rename = "relatedIssueId")]
    pub related_issue_id: String,
    #[cynic(rename = "type")]
    pub relation_type: IssueRelationType,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct IssueRelationPayload {
    pub success: bool,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct IssueRelationCreateArguments {
    pub input: IssueRelationCreateInput,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Mutation",
    schema = "linear",
    variables = "IssueRelationCreateArguments"
)]
pub struct IssueRelationCreateMutation {
    #[arguments(input: $input)]
    #[cynic(rename = "issueRelationCreate")]
    pub issue_relation_create: IssueRelationPayload,
}

#[derive(cynic::QueryFragment, Debug, Clone)]
#[cynic(schema = "linear")]
pub struct DeletePayload {
    pub success: bool,
}

#[derive(cynic::QueryVariables, Debug, Clone)]
pub struct IssueRelationDeleteArguments {
    pub id: String,
}

#[derive(cynic::QueryFragment, Debug)]
#[cynic(
    graphql_type = "Mutation",
    schema = "linear",
    variables = "IssueRelationDeleteArguments"
)]
pub struct IssueRelationDeleteMutation {
    #[arguments(id: $id)]
    #[cynic(rename = "issueRelationDelete")]
    pub issue_relation_delete: DeletePayload,
}
