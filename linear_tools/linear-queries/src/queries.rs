use crate::filters::IssueFilter;
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
