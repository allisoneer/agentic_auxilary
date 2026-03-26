use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OptimizeMessageRequest {
    pub message: String,
    pub supplemental_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct OptimizedPrompt {
    pub system_prompt: String,
    pub user_prompt: String,
    pub assembled_prompt: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ModelOutput {
    pub system_prompt: String,
    pub user_prompt: String,
}
