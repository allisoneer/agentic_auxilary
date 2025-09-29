pub mod client;
pub mod errors;
pub mod optimizer;
pub mod template;
pub mod token;

use serde::{Deserialize, Serialize};
use universal_tool_core::prelude::*;
use crate::{
    client::OrClient, errors::*, optimizer::{call_optimizer, parser::parse_optimizer_output},
    template::inject_files, token::enforce_limit
};
use async_openai::types::*;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileMeta {
    pub filename: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum PromptType {
    Reasoning,
    Plan,
}

#[derive(Clone, Default)]
pub struct Gpt5Reasoner;

#[universal_tool_router(
    cli(name = "gpt5_reasoner"), // we won't use generated CLI, but harmless
    mcp(name = "gpt5_reasoner", version = "0.1.0")
)]
impl Gpt5Reasoner {
    #[universal_tool(description = "Optimize a prompt using file metadata and execute with GPT-5")]
    pub async fn optimize_and_execute(
        &self,
        prompt: String,
        files: Vec<FileMeta>,
        prompt_type: PromptType,
    ) -> std::result::Result<String, ToolError> {
        gpt5_reasoner_impl(prompt, files, None, prompt_type).await
    }
}

pub async fn gpt5_reasoner_impl(
    prompt: String,
    files: Vec<FileMeta>,
    optimizer_model: Option<String>,
    prompt_type: PromptType,
) -> std::result::Result<String, ToolError> {
    // Load env OpenRouter key (CLI already optionally did dotenv)
    let client = OrClient::from_env().map_err(ToolError::from)?;

    // Step 1: optimize
    // Get optimizer model from parameter or environment, default to "openai/gpt-5"
    let opt_model = optimizer_model
        .or_else(|| std::env::var("OPTIMIZER_MODEL").ok())
        .unwrap_or_else(|| "openai/gpt-5".to_string());

    let raw = call_optimizer(&client, &opt_model, &prompt_type, &prompt, &files)
        .await.map_err(ToolError::from)?;

    // Debug: Print the raw optimizer output if RUST_LOG is set
    tracing::debug!("Raw optimizer output:\n{}", raw);

    let parsed = parse_optimizer_output(&raw).map_err(|e| {
        // On parse error, include the raw output for debugging
        tracing::error!("Failed to parse optimizer output:\n{}", raw);
        ToolError::from(e)
    })?;

    tracing::debug!("Parsed optimizer output: {} groups found", parsed.groups.file_groups.len());
    for group in &parsed.groups.file_groups {
        tracing::debug!("  Group '{}': {} files", group.name, group.files.len());
    }

    // Step 2: inject, token check, execute
    let mut final_prompt = inject_files(&parsed.xml_template, &parsed.groups)
        .await.map_err(ToolError::from)?;

    // Replace the {original_prompt} placeholder with the actual prompt
    final_prompt = final_prompt.replace("{original_prompt}", &prompt);

    let token_count = crate::token::count_tokens(&final_prompt).map_err(ToolError::from)?;
    tracing::debug!("Final prompt token count: {}", token_count);
    tracing::debug!("Final prompt after injection (first 500 chars):\n{}...",
        final_prompt.chars().take(500).collect::<String>());

    enforce_limit(&final_prompt).map_err(ToolError::from)?;

    // Execute GPT-5
    tracing::debug!("Executing final prompt with openai/gpt-5 at high reasoning effort");
    let req = CreateChatCompletionRequestArgs::default()
        .model("openai/gpt-5")
        .messages([
            ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessageArgs::default().content(final_prompt).build().unwrap()
            )
        ])
        .reasoning_effort(ReasoningEffort::High)
        .temperature(0.2)
        .build().map_err(|e| ToolError::from(ReasonerError::OpenAI(e)))?;

    let resp = client.client.chat().create(req).await.map_err(|e| ToolError::from(ReasonerError::from(e)))?;
    let content = resp.choices.first().and_then(|c| c.message.content.clone())
        .ok_or_else(|| ToolError::new(universal_tool_core::error::ErrorCode::ExecutionFailed, "GPT-5 returned empty content"))?;
    Ok(content)
}
