//! Input/output types for web tools.

use std::fmt::Write;

use agentic_tools_core::fmt::{TextFormat, TextOptions};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================================
// web_fetch types
// ============================================================================

/// Input for the `web_fetch` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WebFetchInput {
    /// The URL to fetch
    pub url: String,
    /// Whether to generate a Haiku summary (default: false).
    /// Requires Anthropic credentials when enabled.
    #[serde(default)]
    pub summarize: bool,
    /// Maximum bytes to download (default: 5MB, hard limit: 20MB)
    #[serde(default)]
    pub max_bytes: Option<usize>,
}

/// Output from the `web_fetch` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WebFetchOutput {
    /// The final URL after redirects
    pub final_url: String,
    /// Page title (extracted from HTML if available)
    pub title: Option<String>,
    /// Content-Type header value
    pub content_type: String,
    /// Approximate word count of the content
    pub word_count: usize,
    /// Whether the content was truncated due to size limits
    pub truncated: bool,
    /// When the page was retrieved
    pub retrieved_at: DateTime<Utc>,
    /// The converted content (markdown for HTML, raw for text, pretty-printed for JSON)
    pub content: String,
    /// Optional Haiku summary (only present when summarize=true)
    pub summary: Option<String>,
}

impl TextFormat for WebFetchOutput {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "URL: {}", self.final_url);
        if let Some(title) = &self.title {
            let _ = writeln!(out, "Title: {title}");
        }
        let _ = write!(
            out,
            "Retrieved: {} | Words: {}",
            self.retrieved_at.format("%Y-%m-%d %H:%M UTC"),
            self.word_count
        );
        if self.truncated {
            out.push_str(" | TRUNCATED");
        }
        out.push('\n');
        if let Some(summary) = &self.summary {
            out.push_str("\n--- Summary ---\n");
            out.push_str(summary);
            out.push('\n');
        }
        out.push_str("\n--- Content ---\n");
        out.push_str(&self.content);
        out
    }
}

// ============================================================================
// web_search types
// ============================================================================

/// Input for the `web_search` tool.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct WebSearchInput {
    /// Search query. Use a natural-language question or description;
    /// Exa is semantic/neural search — do NOT use keyword-stuffed queries.
    pub query: String,
    /// Number of results to return (default: 8, max: 20)
    #[serde(default)]
    pub num_results: Option<u32>,
}

/// Output from the `web_search` tool.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WebSearchOutput {
    /// The original search query
    pub query: String,
    /// When the search was performed
    pub retrieved_at: DateTime<Utc>,
    /// Trimmed orientation context from Exa (if available)
    pub context: Option<String>,
    /// Compact, citable result cards
    pub results: Vec<WebSearchResultCard>,
}

/// A single result card from web search.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct WebSearchResultCard {
    /// URL of the result
    pub url: String,
    /// Domain extracted from URL
    pub domain: String,
    /// Page title
    pub title: Option<String>,
    /// Published date (if available)
    pub published_date: Option<String>,
    /// Author (if available)
    pub author: Option<String>,
    /// Relevance score (0-100)
    pub score: Option<u32>,
    /// Short snippet (up to 300 chars) from highlights or summary
    pub snippet: Option<String>,
}

impl TextFormat for WebSearchOutput {
    fn fmt_text(&self, _opts: &TextOptions) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "Query: {}", self.query);
        let _ = writeln!(
            out,
            "Retrieved: {}",
            self.retrieved_at.format("%Y-%m-%d %H:%M UTC")
        );

        if let Some(ctx) = &self.context {
            let _ = write!(out, "\n--- Context ---\n{ctx}\n");
        }

        let _ = write!(out, "\n--- Results ({}) ---\n", self.results.len());
        for (i, card) in self.results.iter().enumerate() {
            let _ = write!(
                out,
                "\n{}. {} ({})\n   {}\n",
                i + 1,
                card.title.as_deref().unwrap_or("(untitled)"),
                card.domain,
                card.url,
            );
            if let Some(date) = &card.published_date {
                let _ = write!(out, "   Date: {date}");
                if let Some(author) = &card.author {
                    let _ = write!(out, " | Author: {author}");
                }
                out.push('\n');
            }
            if let Some(score) = card.score {
                let _ = writeln!(out, "   Score: {score}/100");
            }
            if let Some(snippet) = &card.snippet {
                let _ = writeln!(out, "   {snippet}");
            }
        }
        out
    }
}
