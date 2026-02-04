//! Types for the Exa `/contents` endpoint

use serde::{Deserialize, Serialize};

use super::common::{ContentsOptions, LivecrawlOption, SearchResult};

/// Request body for `POST /contents`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentsRequest {
    /// URLs to retrieve content for
    pub urls: Vec<String>,

    /// What content to include
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contents: Option<ContentsOptions>,

    /// Livecrawl option
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livecrawl: Option<LivecrawlOption>,

    /// Filter out results with empty content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_empty_results: Option<bool>,
}

impl ContentsRequest {
    /// Create a new contents request for the given URLs
    #[must_use]
    pub const fn new(urls: Vec<String>) -> Self {
        Self {
            urls,
            contents: None,
            livecrawl: None,
            filter_empty_results: None,
        }
    }
}

/// Response from `POST /contents`
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentsResponse {
    /// Content results
    pub results: Vec<SearchResult>,

    /// Cost in dollars
    #[serde(default)]
    pub cost_dollars: Option<super::search::CostDollars>,
}
