use linear_schema::linear as schema;
use serde::{Deserialize, Serialize};

/// Wrapper for Linear DateTime scalar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateTime(pub String);

impl cynic::schema::IsScalar<schema::DateTime> for DateTime {
    type SchemaType = schema::DateTime;
}

impl cynic::coercions::CoercesTo<schema::DateTime> for DateTime {}

/// Wrapper for Linear DateTimeOrDuration scalar (used in date comparators)
/// Accepts ISO 8601 date strings or duration strings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateTimeOrDuration(pub String);

impl cynic::schema::IsScalar<schema::DateTimeOrDuration> for DateTimeOrDuration {
    type SchemaType = schema::DateTimeOrDuration;
}

impl cynic::coercions::CoercesTo<schema::DateTimeOrDuration> for DateTimeOrDuration {}

/// Wrapper for Linear TimelessDate scalar (YYYY-MM-DD format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelessDate(pub String);

impl cynic::schema::IsScalar<schema::TimelessDate> for TimelessDate {
    type SchemaType = schema::TimelessDate;
}

impl cynic::coercions::CoercesTo<schema::TimelessDate> for TimelessDate {}
