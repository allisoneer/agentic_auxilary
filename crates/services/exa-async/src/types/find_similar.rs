//! Types for the Exa `/findSimilar` endpoint

use serde::{Deserialize, Serialize};

use super::common::{ContentsOptions, SearchResult};

/// Request body for `POST /findSimilar`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindSimilarRequest {
    /// URL to find similar pages for
    pub url: String,

    /// Number of results to return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_results: Option<u32>,

    /// Exclude results from the same domain as the source URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_source_domain: Option<bool>,

    /// What content to include in results
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<ContentsOptions>,

    /// Include domains filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_domains: Option<Vec<String>>,

    /// Exclude domains filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_domains: Option<Vec<String>>,

    /// Start date filter (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_published_date: Option<String>,

    /// End date filter (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_published_date: Option<String>,

    /// Category filter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

impl FindSimilarRequest {
    /// Create a new find-similar request for the given URL
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            num_results: None,
            exclude_source_domain: None,
            contents: None,
            include_domains: None,
            exclude_domains: None,
            start_published_date: None,
            end_published_date: None,
            category: None,
        }
    }

    /// Set the number of results
    #[must_use]
    pub const fn with_num_results(mut self, n: u32) -> Self {
        self.num_results = Some(n);
        self
    }

    /// Exclude source domain from results
    #[must_use]
    pub const fn with_exclude_source_domain(mut self, exclude: bool) -> Self {
        self.exclude_source_domain = Some(exclude);
        self
    }
}

/// Response from `POST /findSimilar`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindSimilarResponse {
    /// Similar page results
    pub results: Vec<SearchResult>,

    /// Cost in dollars
    #[serde(default)]
    pub cost_dollars: Option<super::search::CostDollars>,
}
