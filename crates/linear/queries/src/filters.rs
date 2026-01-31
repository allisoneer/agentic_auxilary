use crate::scalars::DateTimeOrDuration;
use linear_schema::linear as schema;

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct StringComparator {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub eq: Option<String>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
    #[cynic(rename = "containsIgnoreCase", skip_serializing_if = "Option::is_none")]
    pub contains_ignore_case: Option<String>,
}

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct NullableStringComparator {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub eq: Option<String>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
    #[cynic(rename = "containsIgnoreCase", skip_serializing_if = "Option::is_none")]
    pub contains_ignore_case: Option<String>,
}

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct NullableNumberComparator {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub eq: Option<f64>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub gte: Option<f64>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub lte: Option<f64>,
}

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct NumberComparator {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub eq: Option<f64>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub gte: Option<f64>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub lte: Option<f64>,
}

/// ID comparator for filtering by entity IDs
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear", graphql_type = "IDComparator")]
pub struct IdComparator {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub eq: Option<cynic::Id>,
}

/// Date comparator for filtering by date ranges
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct DateComparator {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub eq: Option<DateTimeOrDuration>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub gte: Option<DateTimeOrDuration>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub lte: Option<DateTimeOrDuration>,
}

/// Filter for workflow state (by ID)
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct WorkflowStateFilter {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub id: Option<IdComparator>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub name: Option<StringComparator>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub team: Option<TeamFilter>,
}

/// Filter for nullable user fields (by ID)
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct NullableUserFilter {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub id: Option<IdComparator>,
}

/// Filter for team (by ID or key)
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct TeamFilter {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub id: Option<IdComparator>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub key: Option<StringComparator>,
}

/// Filter for nullable project fields (by ID)
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct NullableProjectFilter {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub id: Option<IdComparator>,
}

// ============================================================================
// Metadata query filters
// ============================================================================

/// User filtering options (uses displayName for name search per schema).
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct UserFilter {
    #[cynic(rename = "displayName", skip_serializing_if = "Option::is_none")]
    pub display_name: Option<StringComparator>,
}

/// Project filtering options.
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct ProjectFilter {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub name: Option<StringComparator>,
}

/// Nullable team filter (for IssueLabelFilter.team which uses NullableTeamFilter in schema)
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct NullableTeamFilter {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub id: Option<IdComparator>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub key: Option<StringComparator>,
}

/// Issue label filtering options.
#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct IssueLabelFilter {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub name: Option<StringComparator>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub team: Option<NullableTeamFilter>,
}

// ============================================================================
// Issue filter
// ============================================================================

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct IssueFilter {
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub title: Option<StringComparator>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub description: Option<NullableStringComparator>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub priority: Option<NullableNumberComparator>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub state: Option<WorkflowStateFilter>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub assignee: Option<NullableUserFilter>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub team: Option<TeamFilter>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub project: Option<NullableProjectFilter>,
    #[cynic(rename = "createdAt", skip_serializing_if = "Option::is_none")]
    pub created_at: Option<DateComparator>,
    #[cynic(rename = "updatedAt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateComparator>,
    #[cynic(skip_serializing_if = "Option::is_none")]
    pub number: Option<NumberComparator>,
}
