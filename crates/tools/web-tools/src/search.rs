//! Web search implementation using Exa API.

use agentic_tools_core::error::ToolError;
use chrono::Utc;
use url::Url;

use crate::WebTools;
use crate::types::{WebSearchInput, WebSearchOutput, WebSearchResultCard};

/// Maximum number of results we'll request from Exa
const MAX_RESULTS: u32 = 20;
/// Default number of results
const DEFAULT_RESULTS: u32 = 8;
/// Max chars for context trimming
const MAX_CONTEXT_CHARS: usize = 1500;
/// Max chars for snippet trimming
const MAX_SNIPPET_CHARS: usize = 300;

/// Execute a web search via Exa's semantic search API.
///
/// # Errors
/// Returns `ToolError` if the Exa API call fails.
pub async fn web_search(
    tools: &WebTools,
    input: WebSearchInput,
) -> Result<WebSearchOutput, ToolError> {
    let num_results = input
        .num_results
        .unwrap_or(DEFAULT_RESULTS)
        .min(MAX_RESULTS);

    let req = exa_async::types::search::SearchRequest::new(&input.query)
        .with_num_results(num_results)
        .with_search_type(exa_async::types::common::SearchType::Neural)
        .with_contents(exa_async::types::common::ContentsOptions {
            text: Some(exa_async::types::common::TextContentsOptions {
                max_characters: Some(500),
                ..Default::default()
            }),
            highlights: Some(exa_async::types::common::HighlightsContentsOptions {
                num_sentences: Some(2),
                highlights_per_url: Some(2),
                ..Default::default()
            }),
            summary: Some(exa_async::types::common::SummaryContentsOptions::default()),
        });

    let resp = tools
        .exa
        .search()
        .create(req)
        .await
        .map_err(|e| ToolError::external(format!("Exa search failed: {e}")))?;

    // Trim context
    let context = resp
        .autoprompt_string
        .map(|s| trim_chars(&s, MAX_CONTEXT_CHARS));

    // Map results to cards
    let results: Vec<WebSearchResultCard> = resp
        .results
        .into_iter()
        .map(|r| {
            let domain = extract_domain(&r.url);
            let score = r.score.map(scale_score);
            let snippet = pick_snippet(&r);

            WebSearchResultCard {
                url: r.url,
                domain,
                title: r.title,
                published_date: r.published_date,
                author: r.author,
                score,
                snippet,
            }
        })
        .collect();

    Ok(WebSearchOutput {
        query: input.query,
        retrieved_at: Utc::now(),
        context,
        results,
    })
}

/// Extract domain from a URL, falling back to the raw URL on parse failure.
fn extract_domain(url_str: &str) -> String {
    Url::parse(url_str)
        .ok()
        .and_then(|u| u.host_str().map(String::from))
        .unwrap_or_else(|| url_str.to_string())
}

/// Scale Exa score (0.0-1.0 float) to 0-100 integer.
fn scale_score(score: f64) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let scaled = (score * 100.0).round() as u32;
    scaled.min(100)
}

/// Pick the best snippet from highlights or summary, trimmed.
fn pick_snippet(result: &exa_async::types::common::SearchResult) -> Option<String> {
    // Prefer highlights
    if let Some(highlights) = &result.highlights
        && let Some(first) = highlights.first()
        && !first.is_empty()
    {
        return Some(trim_chars(first, MAX_SNIPPET_CHARS));
    }
    // Fall back to summary
    if let Some(summary) = &result.summary
        && !summary.is_empty()
    {
        return Some(trim_chars(summary, MAX_SNIPPET_CHARS));
    }
    None
}

/// Trim a string to `max` characters, appending an ellipsis if truncated.
fn trim_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => format!("{}...", &s[..idx]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://example.com/page"), "example.com");
        assert_eq!(
            extract_domain("https://sub.example.co.uk/path"),
            "sub.example.co.uk"
        );
        assert_eq!(extract_domain("not-a-url"), "not-a-url");
    }

    #[test]
    fn test_scale_score() {
        assert_eq!(scale_score(0.95), 95);
        assert_eq!(scale_score(1.0), 100);
        assert_eq!(scale_score(0.0), 0);
        assert_eq!(scale_score(0.5), 50);
        assert_eq!(scale_score(1.5), 100); // capped
    }

    #[test]
    fn test_trim_chars() {
        assert_eq!(trim_chars("hello", 10), "hello");
        assert_eq!(trim_chars("hello world", 5), "hello...");
    }

    #[test]
    fn test_trim_chars_multibyte() {
        // Chinese: 4 chars, 12 bytes
        assert_eq!(trim_chars("你好世界", 2), "你好...");
        assert_eq!(trim_chars("你好世界", 4), "你好世界");
        assert_eq!(trim_chars("你好世界", 10), "你好世界");

        // Emoji: 3 chars, 12 bytes
        assert_eq!(trim_chars("🎉🎉🎉", 2), "🎉🎉...");
    }

    #[test]
    fn test_pick_snippet_prefers_highlights() {
        let result = exa_async::types::common::SearchResult {
            highlights: Some(vec!["highlight text".into()]),
            summary: Some("summary text".into()),
            ..Default::default()
        };
        assert_eq!(pick_snippet(&result), Some("highlight text".into()));
    }

    #[test]
    fn test_pick_snippet_falls_back_to_summary() {
        let result = exa_async::types::common::SearchResult {
            highlights: None,
            summary: Some("summary text".into()),
            ..Default::default()
        };
        assert_eq!(pick_snippet(&result), Some("summary text".into()));
    }

    #[test]
    fn test_pick_snippet_none() {
        let result = exa_async::types::common::SearchResult::default();
        assert_eq!(pick_snippet(&result), None);
    }
}
