pub mod parser;
pub mod prompts;

use crate::{FileMeta, PromptType, client::OrClient, errors::*};
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
    ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs, ReasoningEffort,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct FileMetaSerializable<'a> {
    filename: &'a str,
    description: &'a str,
}

// Build the user prompt by filling placeholders
pub fn build_user_prompt(pt: &PromptType, prompt: &str, files: &[FileMeta]) -> String {
    let files_array = serde_json::to_string_pretty(
        &files
            .iter()
            .map(|f| FileMetaSerializable {
                filename: &f.filename,
                description: &f.description,
            })
            .collect::<Vec<_>>(),
    )
    .unwrap_or("[]".into());

    let template = match pt {
        PromptType::Reasoning => prompts::USER_OPTIMIZE_REASONING,
        PromptType::Plan => prompts::USER_OPTIMIZE_PLAN,
    };

    template
        .replace("{FILES_ARRAY}", &files_array)
        .replace("{USER_PROMPT}", prompt)
}

pub async fn call_optimizer(
    client: &OrClient,
    optimizer_model: &str,
    pt: &PromptType,
    prompt: &str,
    files: &[FileMeta],
) -> Result<String> {
    // Prepare the user prompt once; clone per attempt when building request
    let user_prompt = build_user_prompt(pt, prompt, files);

    // Application-level retry policy: 2 retries (3 attempts total), 500ms fixed delay
    const RETRIES: usize = 2;
    const DELAY: std::time::Duration = std::time::Duration::from_millis(500);

    for attempt in 0..=RETRIES {
        if attempt > 0 {
            tracing::warn!("Optimizer API attempt {} of {}", attempt + 1, RETRIES + 1);
            tokio::time::sleep(DELAY).await;
        }

        // Build the request inside the loop (cheap compared to network call)
        let mut req_builder = CreateChatCompletionRequestArgs::default();
        req_builder
            .model(optimizer_model)
            .messages([
                ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(prompts::SYSTEM_OPTIMIZER)
                        .build()
                        .map_err(ReasonerError::OpenAI)?,
                ),
                ChatCompletionRequestMessage::User(
                    ChatCompletionRequestUserMessageArgs::default()
                        .content(user_prompt.clone())
                        .build()
                        .map_err(ReasonerError::OpenAI)?,
                ),
            ])
            .temperature(0.2);

        // Set reasoning_effort for reasoning models
        let using_reasoning =
            optimizer_model.contains("gpt-5") || optimizer_model.contains("gpt-oss");
        if using_reasoning {
            tracing::debug!(
                "Using high reasoning effort for optimizer model: {}",
                optimizer_model
            );
            req_builder.reasoning_effort(ReasoningEffort::High);
        } else {
            tracing::debug!(
                "Using standard mode (no reasoning effort) for optimizer model: {}",
                optimizer_model
            );
        }

        let req = req_builder.build().map_err(ReasonerError::OpenAI)?;

        tracing::debug!("Calling optimizer with model: {}", optimizer_model);
        let start = std::time::Instant::now();

        match client.client.chat().create(req).await {
            Ok(resp) => {
                let duration = start.elapsed();
                tracing::debug!("Optimizer API succeeded in {:?}", duration);

                // NEW: log response metadata + classify emptiness
                let empty_kind = crate::logging::log_chat_response("optimizer", &resp, duration);
                if let Some(kind) = empty_kind {
                    // Optimizer: warn, but do NOT error (preserve downstream template retry)
                    crate::logging::log_empty_warning("optimizer", kind, &resp);
                }

                // Existing content extraction
                let content_opt = resp.choices.first().and_then(|c| c.message.content.clone());

                match content_opt {
                    None => {
                        // Maintain existing behavior: Template error triggers validation retry loops upstream
                        return Err(ReasonerError::Template(
                            "Optimizer returned empty content (None)".into(),
                        ));
                    }
                    Some(content) => {
                        // NEW: warn if empty or whitespace-only, but still return Ok(content)
                        if content.trim().is_empty() {
                            tracing::warn!(
                                "Optimizer returned empty/whitespace content (len={})",
                                content.len()
                            );
                        } else {
                            tracing::debug!("Optimizer response length: {} chars", content.len());
                        }
                        return Ok(content);
                    }
                }
            }
            Err(e) => {
                let retryable = crate::errors::is_retryable_app_level(&e);
                if attempt < RETRIES && retryable {
                    tracing::warn!("Optimizer call failed with retryable error: {e}; retrying...");
                    continue;
                }

                // Not retryable or retries exhausted
                if retryable {
                    tracing::error!(
                        "Optimizer call failed after {} attempts with retryable error: {}",
                        attempt + 1,
                        e
                    );
                } else {
                    tracing::error!("Optimizer call failed with non-retryable error: {}", e);
                }
                return Err(ReasonerError::OpenAI(e));
            }
        }
    }

    unreachable!("Optimizer retry loop should return on success or error")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_user_prompt() {
        let files = vec![
            FileMeta {
                filename: "src/main.rs".to_string(),
                description: "Main entry point".to_string(),
            },
            FileMeta {
                filename: "src/lib.rs".to_string(),
                description: "Library root".to_string(),
            },
        ];

        let prompt = build_user_prompt(&PromptType::Reasoning, "Update the main function", &files);

        // Check that placeholders were replaced
        assert!(!prompt.contains("{FILES_ARRAY}"));
        assert!(!prompt.contains("{USER_PROMPT}"));
        assert!(prompt.contains("Update the main function"));
        assert!(prompt.contains("src/main.rs"));
        assert!(prompt.contains("Main entry point"));
    }
}
