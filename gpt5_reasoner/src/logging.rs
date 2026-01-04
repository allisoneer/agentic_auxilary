// gpt5_reasoner/src/logging.rs
use agentic_logging::TokenUsage;
use async_openai::types::CreateChatCompletionResponse;
use std::time::Duration;

/// Extract TokenUsage (including optional reasoning_tokens) from an OpenAI chat response.
pub fn extract_token_usage(resp: &CreateChatCompletionResponse) -> Option<TokenUsage> {
    let usage = resp.usage.as_ref()?;
    let reasoning_tokens = usage
        .completion_tokens_details
        .as_ref()
        .and_then(|d| d.reasoning_tokens);
    Some(TokenUsage {
        prompt: usage.prompt_tokens,
        completion: usage.completion_tokens,
        total: usage.total_tokens,
        reasoning_tokens,
    })
}

#[derive(Debug, Clone, Copy)]
pub enum EmptyContentKind {
    NoChoices,
    NoContent,
    EmptyString,
    WhitespaceOnly,
}

impl std::fmt::Display for EmptyContentKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmptyContentKind::NoChoices => write!(f, "no_choices"),
            EmptyContentKind::NoContent => write!(f, "none_content"),
            EmptyContentKind::EmptyString => write!(f, "empty_string"),
            EmptyContentKind::WhitespaceOnly => write!(f, "whitespace_only"),
        }
    }
}

/// Classify empty content scenarios using the first choice (typical single-choice usage).
pub fn classify_empty_chat_content(
    resp: &CreateChatCompletionResponse,
) -> Option<EmptyContentKind> {
    if resp.choices.is_empty() {
        return Some(EmptyContentKind::NoChoices);
    }
    let first = &resp.choices[0];
    match &first.message.content {
        None => Some(EmptyContentKind::NoContent),
        Some(s) if s.is_empty() => Some(EmptyContentKind::EmptyString),
        Some(s) if s.trim().is_empty() => Some(EmptyContentKind::WhitespaceOnly),
        _ => None,
    }
}

/// Log compact, structured response metadata for diagnostics. Returns any detected empty classification.
pub fn log_chat_response(
    phase: &str,
    resp: &CreateChatCompletionResponse,
    duration: Duration,
) -> Option<EmptyContentKind> {
    // Top-level
    let id = resp.id.clone();
    let model = resp.model.clone();
    let system_fingerprint = resp.system_fingerprint.clone().unwrap_or_default();

    // Choices and first choice details
    let choices_len = resp.choices.len();
    let first = resp.choices.first();

    // Safely collect optional details
    let finish_reason = first
        .and_then(|c| c.finish_reason)
        .map(|fr| format!("{fr:?}"))
        .unwrap_or_else(|| "n/a".into());

    let role = first
        .map(|c| format!("{:?}", c.message.role))
        .unwrap_or_else(|| "n/a".into());

    let content_len = first
        .and_then(|c| c.message.content.as_ref())
        .map(|s| s.len())
        .unwrap_or(0);

    let refusal_len = first
        .and_then(|c| c.message.refusal.as_ref())
        .map(|s| s.len())
        .unwrap_or(0);

    // Note: reasoning field not available in async-openai 0.29.3
    // Would be: c.message.reasoning for GPT-5 internal reasoning trace

    let tool_calls_count = first
        .and_then(|c| c.message.tool_calls.as_ref())
        .map(|v| v.len())
        .unwrap_or(0);

    // Usage tokens
    let usage = resp.usage.as_ref();
    let prompt_tokens = usage.map(|u| u.prompt_tokens).unwrap_or(0);
    let completion_tokens = usage.map(|u| u.completion_tokens).unwrap_or(0);
    let total_tokens = usage.map(|u| u.total_tokens).unwrap_or(0);

    // Reasoning tokens (GPT-5)
    let reasoning_tokens = usage
        .and_then(|u| u.completion_tokens_details.as_ref())
        .and_then(|d| d.reasoning_tokens);

    // Debug log
    tracing::debug!(
        "GPT response meta: phase={} id={} model={} system_fingerprint={} duration_ms={} \
         choices={} first_finish_reason={} role={} content_len={} refusal_len={} tool_calls={} \
         usage_prompt={} usage_completion={} usage_total={} reasoning_tokens={}",
        phase,
        id,
        model,
        system_fingerprint,
        duration.as_millis(),
        choices_len,
        finish_reason,
        role,
        content_len,
        refusal_len,
        tool_calls_count,
        prompt_tokens,
        completion_tokens,
        total_tokens,
        reasoning_tokens
            .map(|v| v.to_string())
            .unwrap_or_else(|| "n/a".into()),
    );

    // Empty classification (used by call sites for warn/error behavior)
    classify_empty_chat_content(resp)
}

/// Convenience warn logger for empty scenarios with helpful fields.
pub fn log_empty_warning(phase: &str, kind: EmptyContentKind, resp: &CreateChatCompletionResponse) {
    let first = resp.choices.first();
    let finish_reason = first
        .and_then(|c| c.finish_reason)
        .map(|fr| format!("{fr:?}"))
        .unwrap_or_else(|| "n/a".into());

    let tool_calls_count = first
        .and_then(|c| c.message.tool_calls.as_ref())
        .map(|v| v.len())
        .unwrap_or(0);

    let usage = resp.usage.as_ref();
    let prompt_tokens = usage.map(|u| u.prompt_tokens).unwrap_or(0);
    let completion_tokens = usage.map(|u| u.completion_tokens).unwrap_or(0);
    let reasoning_tokens = usage
        .and_then(|u| u.completion_tokens_details.as_ref())
        .and_then(|d| d.reasoning_tokens)
        .map(|v| v.to_string())
        .unwrap_or_else(|| "n/a".into());

    tracing::warn!(
        "Empty response content detected: phase={} kind={} finish_reason={} tool_calls={} \
         usage_prompt={} usage_completion={} reasoning_tokens={}",
        phase,
        kind,
        finish_reason,
        tool_calls_count,
        prompt_tokens,
        completion_tokens,
        reasoning_tokens
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_kind_display() {
        assert_eq!(EmptyContentKind::NoChoices.to_string(), "no_choices");
        assert_eq!(EmptyContentKind::NoContent.to_string(), "none_content");
        assert_eq!(EmptyContentKind::EmptyString.to_string(), "empty_string");
        assert_eq!(
            EmptyContentKind::WhitespaceOnly.to_string(),
            "whitespace_only"
        );
    }

    #[test]
    fn test_extract_token_usage_parses_all_fields() {
        let raw = serde_json::json!({
            "id": "chatcmpl-1",
            "object": "chat.completion",
            "created": 1234567890,
            "model": "openai/gpt-5",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "ok" },
                "logprobs": null,
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30,
                "completion_tokens_details": { "reasoning_tokens": 7 }
            }
        });
        let resp: async_openai::types::CreateChatCompletionResponse =
            serde_json::from_value(raw).unwrap();

        let usage = extract_token_usage(&resp).expect("usage present");
        assert_eq!(usage.prompt, 10);
        assert_eq!(usage.completion, 20);
        assert_eq!(usage.total, 30);
        assert_eq!(usage.reasoning_tokens, Some(7));
    }
}
