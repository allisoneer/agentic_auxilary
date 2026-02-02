// TODO(1): Add optional JS-rendering sidecar for dynamic pages (Playwright/etc.)

use agentic_tools_core::error::ToolError;
use chrono::Utc;

use crate::WebTools;
use crate::types::{WebFetchInput, WebFetchOutput};

/// Default maximum download size: 5 MB
const DEFAULT_MAX_BYTES: usize = 5 * 1024 * 1024;

/// Execute a web fetch: download URL, convert content, optionally summarize.
///
/// # Errors
/// Returns `ToolError` if the HTTP request fails, the content type is unsupported, or summarization fails.
pub async fn web_fetch(
    tools: &WebTools,
    input: WebFetchInput,
) -> Result<WebFetchOutput, ToolError> {
    let max_bytes = input.max_bytes.unwrap_or(DEFAULT_MAX_BYTES);

    // Send GET request
    let response = tools
        .http
        .get(&input.url)
        .send()
        .await
        .map_err(|e| ToolError::external(format!("HTTP request failed: {e}")))?;

    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("text/plain")
        .to_string();

    // Download body with size cap
    let bytes = response
        .bytes()
        .await
        .map_err(|e| ToolError::external(format!("Failed to read response body: {e}")))?;

    let truncated = bytes.len() > max_bytes;
    let raw_bytes = if truncated {
        &bytes[..max_bytes]
    } else {
        &bytes
    };

    // Convert based on content-type
    let (title, content) = decode_and_convert(raw_bytes, &content_type)?;

    let word_count = content.split_whitespace().count();

    // Optional summarization
    let summary = if input.summarize {
        Some(
            crate::haiku::summarize_markdown(tools, &content)
                .await
                .map_err(|e| ToolError::external(format!("Summarization failed: {e}")))?,
        )
    } else {
        None
    };

    Ok(WebFetchOutput {
        final_url,
        title,
        content_type,
        word_count,
        truncated,
        retrieved_at: Utc::now(),
        content,
        summary,
    })
}

/// Decode bytes and convert to a useful text format based on content-type.
///
/// # Errors
/// Returns `ToolError` if the content type is unsupported or HTML conversion fails.
pub fn decode_and_convert(
    bytes: &[u8],
    content_type: &str,
) -> Result<(Option<String>, String), ToolError> {
    let ct_lower = content_type.to_lowercase();

    // Try to decode as UTF-8
    let text = String::from_utf8_lossy(bytes);

    if ct_lower.contains("text/html") || (ct_lower.is_empty() && looks_like_html(&text)) {
        let title = extract_title(&text);
        let md = htmd::convert(&text)
            .map_err(|e| ToolError::internal(format!("HTML conversion failed: {e}")))?;
        Ok((title, md))
    } else if ct_lower.contains("application/json") || ct_lower.contains("+json") {
        // Pretty-print JSON
        match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(val) => {
                let pretty =
                    serde_json::to_string_pretty(&val).unwrap_or_else(|_| text.into_owned());
                Ok((None, pretty))
            }
            Err(_) => Ok((None, text.into_owned())),
        }
    } else if ct_lower.starts_with("text/") || ct_lower.is_empty() {
        Ok((None, text.into_owned()))
    } else {
        // Binary or unsupported content type
        Err(ToolError::invalid_input(format!(
            "Unsupported content type: {content_type}. Only HTML, text, and JSON are supported."
        )))
    }
}

/// Best-effort `<title>` extraction from HTML.
#[must_use]
pub fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let after_tag = lower[start..].find('>')?;
    let title_start = start + after_tag + 1;
    let title_end = lower[title_start..].find("</title>")?;
    let title = html[title_start..title_start + title_end].trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

/// Simple heuristic to detect HTML content.
fn looks_like_html(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("<!DOCTYPE")
        || trimmed.starts_with("<!doctype")
        || trimmed.starts_with("<html")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_html() {
        let html = b"<html><head><title>Test Page</title></head><body><h1>Hello</h1><p>World</p></body></html>";
        let (title, content) = decode_and_convert(html, "text/html").unwrap();
        assert_eq!(title.as_deref(), Some("Test Page"));
        assert!(content.contains("Hello"));
        assert!(content.contains("World"));
    }

    #[test]
    fn test_decode_json() {
        let json = br#"{"key":"value","num":42}"#;
        let (title, content) = decode_and_convert(json, "application/json").unwrap();
        assert!(title.is_none());
        assert!(content.contains("\"key\": \"value\""));
    }

    #[test]
    fn test_decode_plain_text() {
        let text = b"Hello, world!";
        let (title, content) = decode_and_convert(text, "text/plain").unwrap();
        assert!(title.is_none());
        assert_eq!(content, "Hello, world!");
    }

    #[test]
    fn test_decode_binary_errors() {
        let bytes = b"\x00\x01\x02";
        let result = decode_and_convert(bytes, "application/octet-stream");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_title() {
        assert_eq!(
            extract_title("<html><head><title>My Page</title></head></html>"),
            Some("My Page".into())
        );
        assert_eq!(extract_title("<html><head></head></html>"), None);
        assert_eq!(extract_title("<title></title>"), None);
    }

    #[test]
    fn test_looks_like_html() {
        assert!(looks_like_html("<!DOCTYPE html><html>"));
        assert!(looks_like_html("  <html>"));
        assert!(!looks_like_html("Hello, world!"));
    }
}
