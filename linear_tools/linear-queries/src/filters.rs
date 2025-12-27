use crate::scalars::DateTimeOrDuration;
use linear_schema::linear as schema;

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct StringComparator {
    pub eq: Option<String>,
    pub contains: Option<String>,
    #[cynic(rename = "containsIgnoreCase")]
    pub contains_ignore_case: Option<String>,
}

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct NullableStringComparator {
    pub eq: Option<String>,
    pub contains: Option<String>,
    #[cynic(rename = "containsIgnoreCase")]
    pub contains_ignore_case: Option<String>,
}

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct NullableNumberComparator {
    pub eq: Option<f64>,
    pub gte: Option<f64>,
    pub lte: Option<f64>,
}

/// ID comparator for filtering by entity IDs
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear", graphql_type = "IDComparator")]
pub struct IdComparator {
    pub eq: Option<cynic::Id>,
}

/// Date comparator for filtering by date ranges
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct DateComparator {
    pub eq: Option<DateTimeOrDuration>,
    pub gte: Option<DateTimeOrDuration>,
    pub lte: Option<DateTimeOrDuration>,
}

/// Filter for workflow state (by ID)
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct WorkflowStateFilter {
    pub id: Option<IdComparator>,
}

/// Filter for nullable user fields (by ID)
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct NullableUserFilter {
    pub id: Option<IdComparator>,
}

/// Filter for team (by ID)
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct TeamFilter {
    pub id: Option<IdComparator>,
}

/// Filter for nullable project fields (by ID)
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct NullableProjectFilter {
    pub id: Option<IdComparator>,
}

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct IssueFilter {
    pub title: Option<StringComparator>,
    pub description: Option<NullableStringComparator>,
    pub priority: Option<NullableNumberComparator>,
    pub state: Option<WorkflowStateFilter>,
    pub assignee: Option<NullableUserFilter>,
    pub team: Option<TeamFilter>,
    pub project: Option<NullableProjectFilter>,
    #[cynic(rename = "createdAt")]
    pub created_at: Option<DateComparator>,
    #[cynic(rename = "updatedAt")]
    pub updated_at: Option<DateComparator>,
}
