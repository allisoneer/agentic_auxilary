// TODO(1): Add optional JS-rendering sidecar for dynamic pages (Playwright/etc.)

use agentic_tools_core::error::ToolError;
use chrono::Utc;

use crate::WebTools;
use crate::types::{WebFetchInput, WebFetchOutput};

/// Default maximum download size: 5 MB
const DEFAULT_MAX_BYTES: usize = 5 * 1024 * 1024;

/// Hard maximum allowed `max_bytes`: 20 MB
pub const HARD_MAX_BYTES: usize = 20 * 1024 * 1024;

/// Execute a web fetch: download URL, convert content, optionally summarize.
///
/// # Errors
/// Returns `ToolError` if the HTTP request fails, the content type is unsupported, or summarization fails.
pub async fn web_fetch(
    tools: &WebTools,
    input: WebFetchInput,
) -> Result<WebFetchOutput, ToolError> {
    let max_bytes = input.max_bytes.unwrap_or(DEFAULT_MAX_BYTES);

    if max_bytes > HARD_MAX_BYTES {
        return Err(ToolError::invalid_input(format!(
            "max_bytes must be <= {HARD_MAX_BYTES} (20MB)"
        )));
    }

    // Send GET request
    let mut response = tools
        .http
        .get(&input.url)
        .send()
        .await
        .map_err(|e| ToolError::external(format!("HTTP request failed: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(ToolError::external(format!(
            "HTTP request failed with status {status} for {}",
            response.url()
        )));
    }

    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // Download body with size cap (streaming)
    #[allow(clippy::cast_possible_truncation)]
    // max_bytes is already bounded by HARD_MAX_BYTES (20MB)
    let initial_capacity = response
        .content_length()
        .map_or(8 * 1024, |len| len.min(max_bytes as u64) as usize)
        .min(max_bytes);

    let mut bytes: Vec<u8> = Vec::with_capacity(initial_capacity);
    let mut truncated = false;

    loop {
        // Conservative: once we reach the cap, stop without attempting to read more
        if bytes.len() >= max_bytes {
            truncated = true;
            break;
        }

        let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| ToolError::external(format!("Failed to read response body: {e}")))?
        else {
            break;
        };

        let remaining = max_bytes - bytes.len();
        if chunk.len() > remaining {
            bytes.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }

        bytes.extend_from_slice(&chunk);
    }

    // Convert based on content-type
    let (title, content) = decode_and_convert(&bytes, &content_type)?;

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
    let lower = html.to_ascii_lowercase();
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

    #[test]
    fn test_extract_title_unicode_before_tag() {
        // Turkish İ (2→3 bytes under to_lowercase) would panic or corrupt with old code
        assert_eq!(
            extract_title("İ<title>Test Page</title>"),
            Some("Test Page".to_string())
        );
    }

    #[test]
    fn test_extract_title_mixed_case_tags() {
        // Verify ASCII case-insensitivity still works
        assert_eq!(
            extract_title("<TITLE>Upper</TITLE>"),
            Some("Upper".to_string())
        );
        assert_eq!(
            extract_title("<TiTlE>Mixed</TiTlE>"),
            Some("Mixed".to_string())
        );
    }

    mod integration {
        use super::*;
        use crate::WebTools;
        use crate::types::WebFetchInput;
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        #[tokio::test]
        async fn web_fetch_returns_error_on_404() {
            let mock_server = MockServer::start().await;

            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(404).set_body_string("Not Found"))
                .mount(&mock_server)
                .await;

            let http = reqwest::Client::new();
            let tools = WebTools::with_http_client(http);

            let input = WebFetchInput {
                url: mock_server.uri(),
                summarize: false,
                max_bytes: None,
            };

            let result = web_fetch(&tools, input).await;
            assert!(result.is_err(), "Expected error for 404 response");
            let err = result.unwrap_err();
            assert!(
                err.to_string().contains("404"),
                "Error message should mention 404 status"
            );
        }

        #[tokio::test]
        async fn web_fetch_returns_error_on_500() {
            let mock_server = MockServer::start().await;

            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
                .mount(&mock_server)
                .await;

            let http = reqwest::Client::new();
            let tools = WebTools::with_http_client(http);

            let input = WebFetchInput {
                url: mock_server.uri(),
                summarize: false,
                max_bytes: None,
            };

            let result = web_fetch(&tools, input).await;
            assert!(result.is_err(), "Expected error for 500 response");
            let err = result.unwrap_err();
            assert!(
                err.to_string().contains("500"),
                "Error message should mention 500 status"
            );
        }

        #[tokio::test]
        async fn web_fetch_succeeds_on_200() {
            let mock_server = MockServer::start().await;

            Mock::given(method("GET"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_string("Hello, world!")
                        .insert_header("Content-Type", "text/plain"),
                )
                .mount(&mock_server)
                .await;

            let http = reqwest::Client::new();
            let tools = WebTools::with_http_client(http);

            let input = WebFetchInput {
                url: mock_server.uri(),
                summarize: false,
                max_bytes: None,
            };

            let result = web_fetch(&tools, input).await;
            assert!(result.is_ok(), "Expected success for 200 response");
            let output = result.unwrap();
            assert_eq!(output.content, "Hello, world!");
        }

        #[tokio::test]
        async fn web_fetch_detects_html_without_content_type() {
            let mock_server = MockServer::start().await;

            let html = b"<!DOCTYPE html><html><head><title>Test Page</title></head><body><p>Hello</p></body></html>";

            // HTML response with NO Content-Type header (misconfigured server)
            // Use set_body_bytes to avoid wiremock's automatic text/plain Content-Type
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(html.as_slice()))
                .mount(&mock_server)
                .await;

            let http = reqwest::Client::new();
            let tools = WebTools::with_http_client(http);

            let input = WebFetchInput {
                url: mock_server.uri(),
                summarize: false,
                max_bytes: None,
            };

            let result = web_fetch(&tools, input).await;
            assert!(
                result.is_ok(),
                "Expected success for HTML without Content-Type"
            );
            let output = result.unwrap();

            // Verify content_type is empty (no header)
            assert!(
                output.content_type.is_empty(),
                "Content-Type should be empty, got: {}",
                output.content_type
            );

            // Verify HTML heuristic detected the content and converted to markdown
            assert_eq!(
                output.title.as_deref(),
                Some("Test Page"),
                "Should extract title via looks_like_html heuristic"
            );
            assert!(
                output.content.contains("Hello"),
                "Content should be converted"
            );
            assert!(
                !output.content.contains("<p>"),
                "HTML tags should be removed by markdown conversion"
            );
        }
    }
}
