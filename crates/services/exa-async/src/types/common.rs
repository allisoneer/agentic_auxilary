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

/// Represents the cost breakdown related to search.
/// Fields are optional because only non-zero costs are included by the API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostDollarsSearch {
    /// The cost in dollars for neural search.
    #[serde(default)]
    pub neural: Option<f64>,
    /// The cost in dollars for keyword search.
    #[serde(default)]
    pub keyword: Option<f64>,
}

/// Represents the cost breakdown related to contents retrieval.
/// Fields are optional because only non-zero costs are included by the API.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostDollarsContents {
    /// The cost in dollars for retrieving text.
    #[serde(default)]
    pub text: Option<f64>,
    /// The cost in dollars for retrieving highlights.
    #[serde(default)]
    pub highlights: Option<f64>,
    /// The cost in dollars for retrieving summary.
    #[serde(default)]
    pub summary: Option<f64>,
}

/// Represents the total cost breakdown for a request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CostDollars {
    /// Total cost
    #[serde(default)]
    pub total: Option<f64>,
    /// Search cost component (nested breakdown)
    #[serde(default)]
    pub search: Option<CostDollarsSearch>,
    /// Contents cost component (nested breakdown)
    #[serde(default)]
    pub contents: Option<CostDollarsContents>,
}

#[cfg(test)]
mod cost_dollars_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserializes_full_breakdown() {
        let v = json!({
            "total": 0.005,
            "search": { "neural": 0.003 },
            "contents": { "text": 0.001, "highlights": 0.0005, "summary": 0.0005 }
        });

        let cost: CostDollars = serde_json::from_value(v).unwrap();

        assert!((cost.total.unwrap() - 0.005).abs() < 1e-12);
        assert!((cost.search.as_ref().unwrap().neural.unwrap() - 0.003).abs() < 1e-12);
        assert!(cost.search.as_ref().unwrap().keyword.is_none());
        assert!((cost.contents.as_ref().unwrap().text.unwrap() - 0.001).abs() < 1e-12);
        assert!((cost.contents.as_ref().unwrap().highlights.unwrap() - 0.0005).abs() < 1e-12);
        assert!((cost.contents.as_ref().unwrap().summary.unwrap() - 0.0005).abs() < 1e-12);
    }

    #[test]
    fn deserializes_with_missing_optional_fields() {
        let v = json!({
            "total": 0.003,
            "search": { "neural": 0.003 }
        });

        let cost: CostDollars = serde_json::from_value(v).unwrap();

        assert!(cost.contents.is_none());
        let search = cost.search.unwrap();
        assert!((search.neural.unwrap() - 0.003).abs() < 1e-12);
        assert!(search.keyword.is_none());
    }

    #[test]
    fn deserializes_with_empty_nested_objects() {
        let v = json!({
            "total": 0.005,
            "search": {},
            "contents": {}
        });

        let cost: CostDollars = serde_json::from_value(v).unwrap();

        assert!(cost.search.is_some());
        assert!(cost.search.as_ref().unwrap().neural.is_none());
        assert!(cost.contents.is_some());
        assert!(cost.contents.as_ref().unwrap().text.is_none());
        assert!(cost.contents.as_ref().unwrap().highlights.is_none());
        assert!(cost.contents.as_ref().unwrap().summary.is_none());
    }

    #[test]
    fn deserializes_with_null_nested_fields() {
        let v = json!({
            "total": 0.005,
            "search": null,
            "contents": { "text": null, "highlights": 0.0005 }
        });

        let cost: CostDollars = serde_json::from_value(v).unwrap();

        assert!(cost.search.is_none());

        let contents = cost.contents.unwrap();
        assert!(contents.text.is_none());
        assert!((contents.highlights.unwrap() - 0.0005).abs() < 1e-12);
        assert!(contents.summary.is_none());
    }

    #[test]
    fn ignores_unknown_fields_for_forward_compatibility() {
        let v = json!({
            "total": 0.005,
            "search": { "neural": 0.003, "fast": 0.001 },
            "contents": { "text": 0.002, "images": 0.123 },
            "someFutureTopLevelField": "ignored"
        });

        let cost: CostDollars = serde_json::from_value(v).unwrap();

        assert!((cost.search.as_ref().unwrap().neural.unwrap() - 0.003).abs() < 1e-12);
        assert!(cost.search.as_ref().unwrap().keyword.is_none());
        assert!((cost.contents.as_ref().unwrap().text.unwrap() - 0.002).abs() < 1e-12);
    }
}
