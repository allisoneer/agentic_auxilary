use linear_schema::linear as schema;
use serde::{Deserialize, Serialize};

/// Wrapper for Linear DateTime scalar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateTime(pub String);

impl cynic::schema::IsScalar<schema::DateTime> for DateTime {
    type SchemaType = schema::DateTime;
}

impl cynic::coercions::CoercesTo<schema::DateTime> for DateTime {}
