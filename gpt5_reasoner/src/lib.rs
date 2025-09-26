pub mod client;
pub mod errors;
pub mod optimizer;
pub mod template;
pub mod token;

use serde::{Deserialize, Serialize};
use universal_tool_core::prelude::*;

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
    ) -> Result<String, ToolError> {
        gpt5_reasoner_impl(prompt, files, prompt_type).await
    }
}

// Public API used by CLI as well
pub async fn gpt5_reasoner_impl(
    prompt: String,
    files: Vec<FileMeta>,
    prompt_type: PromptType,
) -> Result<String, ToolError> {
    // orchestrate step1 → parse → injection → tokens → step2
    // implemented in later phases
    unimplemented!()
}
