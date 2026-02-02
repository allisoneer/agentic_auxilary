//! Shared types used across Exa API endpoints

use serde::{Deserialize, Serialize};

/// Search type for Exa queries
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SearchType {
    /// Automatic selection
    Auto,
    /// Neural/semantic search (default)
    #[default]
    Neural,
    /// Keyword-based search
    Keyword,
    /// Hybrid neural + keyword
    Hybrid,
}

/// Livecrawl option for content retrieval
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LivecrawlOption {
    /// Always livecrawl
    Always,
    /// Livecrawl if needed (fallback)
    Fallback,
    /// Never livecrawl
    Never,
    /// Automatically decide
    Auto,
}

/// Options for what content to retrieve with search results
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentsOptions {
    /// Include full text content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<TextContentsOptions>,
    /// Include highlights/snippets
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlights: Option<HighlightsContentsOptions>,
    /// Include summary
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<SummaryContentsOptions>,
}

/// Options for text content retrieval
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContentsOptions {
    /// Maximum number of characters to return
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_characters: Option<u32>,
    /// Include HTML tags
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_html_tags: Option<bool>,
}

/// Options for highlight content retrieval
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightsContentsOptions {
    /// Number of highlights per result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub num_sentences: Option<u32>,
    /// Number of highlights per result
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlights_per_url: Option<u32>,
    /// Query for highlights
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

/// Options for summary content retrieval
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryContentsOptions {
    /// Custom query for summary generation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
}

/// A single search result from the Exa API
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    /// URL of the result
    pub url: String,
    /// Unique ID for the result
    #[serde(default)]
    pub id: Option<String>,
    /// Title of the page
    #[serde(default)]
    pub title: Option<String>,
    /// Relevance score
    #[serde(default)]
    pub score: Option<f64>,
    /// Date the page was published
    #[serde(default)]
    pub published_date: Option<String>,
    /// Author of the page
    #[serde(default)]
    pub author: Option<String>,
    /// Full text content (if requested)
    #[serde(default)]
    pub text: Option<String>,
    /// Summary (if requested)
    #[serde(default)]
    pub summary: Option<String>,
    /// Highlights (if requested)
    #[serde(default)]
    pub highlights: Option<Vec<String>>,
    /// Highlight scores
    #[serde(default)]
    pub highlight_scores: Option<Vec<f64>>,
}
