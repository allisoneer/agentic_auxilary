// TODO(2): Tool descriptions are static; consider framework support for injecting runtime context
// (e.g., today's date) into tool descriptions.

//! Haiku summarization via Anthropic API.

use agentic_tools_core::error::ToolError;
use anthropic_async::types::{ContentBlock, MessageParam, MessageRole, MessagesCreateRequest};
use tracing::debug;

use crate::WebTools;

/// Summarize markdown content using Claude Haiku.
///
/// Lazy-initializes the Anthropic client on first call.
/// Errors are NOT cached in the `OnceCell`, allowing retries.
///
/// # Errors
/// Returns `ToolError` if the Anthropic client cannot be initialized or the API call fails.
pub async fn summarize_markdown(tools: &WebTools, markdown: &str) -> Result<String, ToolError> {
    let client = tools
        .anthropic
        .get_or_try_init(|| async { init_anthropic_client().await })
        .await
        .map_err(|e| ToolError::external(format!("Failed to initialize Anthropic client: {e}")))?;

    let req = MessagesCreateRequest {
        model: "claude-3-5-haiku-latest".into(),
        max_tokens: 300,
        messages: vec![MessageParam {
            role: MessageRole::User,
            content: format!(
                "Summarize the following web page content in 5-8 concise bullet points. \
                 Focus on the key facts and takeaways.\n\n{markdown}"
            )
            .into(),
        }],
        system: None,
        temperature: Some(0.2),
        stop_sequences: None,
        top_p: None,
        top_k: None,
        metadata: None,
        tools: None,
        tool_choice: None,
        stream: None,
        output_format: None,
    };

    let resp = client
        .messages()
        .create(req)
        .await
        .map_err(|e| ToolError::external(format!("Haiku API call failed: {e}")))?;

    // Extract text from content blocks
    let text = resp
        .content
        .iter()
        .filter_map(|block| {
            if let ContentBlock::Text { text } = block {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    Ok(text)
}

/// Initialize the Anthropic client.
///
/// Attempts to find an API key from:
/// 1. `ANTHROPIC_API_KEY` environment variable
/// 2. `OpenCode` provider discovery (fallback)
async fn init_anthropic_client()
-> Result<anthropic_async::Client<anthropic_async::AnthropicConfig>, ToolError> {
    // Try env var first
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        debug!("Using ANTHROPIC_API_KEY from environment");
        let config = anthropic_async::AnthropicConfig::new().with_api_key(key);
        return Ok(anthropic_async::Client::with_config(config));
    }

    // Try OpenCode provider discovery
    match get_anthropic_key_from_opencode().await {
        Ok(key) => {
            debug!("Using Anthropic key from OpenCode provider");
            let config = anthropic_async::AnthropicConfig::new().with_api_key(key);
            Ok(anthropic_async::Client::with_config(config))
        }
        Err(e) => Err(ToolError::external(format!(
            "No Anthropic credentials available. Set ANTHROPIC_API_KEY or ensure OpenCode is running. Error: {e}"
        ))),
    }
}

/// Try to get an Anthropic API key from `OpenCode`'s provider endpoint.
async fn get_anthropic_key_from_opencode() -> Result<String, String> {
    let client = opencode_rs::Client::builder()
        .build()
        .map_err(|e| format!("Failed to build OpenCode client: {e}"))?;

    let providers = client
        .providers()
        .list()
        .await
        .map_err(|e| format!("Failed to list OpenCode providers: {e}"))?;

    for provider in providers.all {
        if provider.id == "anthropic"
            && let Some(key) = provider.key
            && !key.is_empty()
        {
            return Ok(key);
        }
    }

    Err("No Anthropic provider found in OpenCode".into())
}
