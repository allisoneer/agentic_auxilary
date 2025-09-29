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
    let user_prompt = build_user_prompt(pt, prompt, files);

    // Build the request with optional reasoning_effort for GPT-5/o-series models
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
                    .content(user_prompt)
                    .build()
                    .map_err(ReasonerError::OpenAI)?,
            ),
        ])
        .temperature(0.2);

    // If the optimizer model is GPT-5 or gpt-oss, set reasoning_effort=High
    let using_reasoning = optimizer_model.contains("gpt-5") || optimizer_model.contains("gpt-oss");
    if using_reasoning {
        tracing::debug!("Using high reasoning effort for optimizer model: {}", optimizer_model);
        req_builder.reasoning_effort(ReasoningEffort::High);
    } else {
        tracing::debug!("Using standard mode (no reasoning effort) for optimizer model: {}", optimizer_model);
    }

    let req = req_builder.build().map_err(ReasonerError::OpenAI)?;

    tracing::debug!("Calling optimizer with model: {}", optimizer_model);
    let resp = client.client.chat().create(req).await?;
    let content = resp
        .choices
        .first()
        .and_then(|c| c.message.content.clone())
        .ok_or_else(|| ReasonerError::Template("Optimizer returned empty content".into()))?;

    tracing::debug!("Optimizer response received, length: {} chars", content.len());
    Ok(content)
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
