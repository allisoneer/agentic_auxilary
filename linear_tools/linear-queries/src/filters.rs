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

#[derive(cynic::InputObject, Clone, Debug, Default)]
#[cynic(schema = "linear")]
pub struct IssueFilter {
    pub title: Option<StringComparator>,
    pub description: Option<NullableStringComparator>,
    pub priority: Option<NullableNumberComparator>,
}
