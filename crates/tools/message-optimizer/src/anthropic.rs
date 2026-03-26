use anthropic_async::AnthropicConfig;
use anthropic_async::BetaFeature;
use anthropic_async::Client;
use anthropic_async::types::content::ContentBlock;
use anthropic_async::types::content::MessageParam;
use anthropic_async::types::content::MessageRole;
use anthropic_async::types::content::SystemParam;
use anthropic_async::types::messages::MessagesCreateRequest;
use anthropic_async::types::messages::MessagesCreateResponse;
use anthropic_async::types::tools::Tool;
use anthropic_async::types::tools::ToolChoice;

use crate::error::MessageOptimizerError;
use crate::prompts::OPTIMIZER_SYSTEM;
use crate::types::ModelOutput;

pub const OPTIMIZER_MODEL: &str = "claude-sonnet-4-6";
pub const TOOL_NAME: &str = "emit_optimized_prompt";
const MAX_TOKENS: u32 = 4_096;

pub(crate) trait OptimizerBackend {
    async fn optimize(&self, user_prompt: String) -> Result<ModelOutput, MessageOptimizerError>;
}

#[derive(Debug, Clone)]
pub(crate) struct AnthropicOptimizer {
    client: Client<AnthropicConfig>,
}

impl AnthropicOptimizer {
    #[must_use]
    pub fn new() -> Self {
        let config =
            AnthropicConfig::new().with_beta_features([BetaFeature::StructuredOutputsLatest]);
        Self {
            client: Client::with_config(config),
        }
    }
}

impl Default for AnthropicOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl OptimizerBackend for AnthropicOptimizer {
    async fn optimize(&self, user_prompt: String) -> Result<ModelOutput, MessageOptimizerError> {
        let request = MessagesCreateRequest {
            model: OPTIMIZER_MODEL.to_string(),
            max_tokens: MAX_TOKENS,
            system: Some(SystemParam::from(OPTIMIZER_SYSTEM)),
            messages: vec![MessageParam {
                role: MessageRole::User,
                content: user_prompt.into(),
            }],
            tools: Some(vec![optimizer_tool()]),
            tool_choice: Some(ToolChoice::Tool {
                name: TOOL_NAME.to_string(),
                disable_parallel_tool_use: None,
            }),
            ..Default::default()
        };

        let response = self.client.messages().create(request).await?;
        extract_model_output(&response)
    }
}

#[must_use]
pub(crate) fn optimizer_tool() -> Tool {
    Tool {
        name: TOOL_NAME.to_string(),
        description: Some("Return the optimized prompts as strict JSON".to_string()),
        input_schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["system_prompt", "user_prompt"],
            "properties": {
                "system_prompt": { "type": "string" },
                "user_prompt": { "type": "string" }
            }
        }),
        cache_control: None,
        strict: Some(true),
    }
}

pub(crate) fn extract_model_output(
    response: &MessagesCreateResponse,
) -> Result<ModelOutput, MessageOptimizerError> {
    let mut tool_uses = response.content.iter().filter_map(|block| match block {
        ContentBlock::ToolUse { name, input, .. } => Some((name, input)),
        _ => None,
    });

    let (tool_name, input) = tool_uses.next().ok_or_else(|| {
        MessageOptimizerError::OutputContract("missing tool call in optimizer response".to_string())
    })?;

    if tool_uses.next().is_some() {
        return Err(MessageOptimizerError::OutputContract(
            "expected exactly one tool call in optimizer response".to_string(),
        ));
    }

    if tool_name != TOOL_NAME {
        return Err(MessageOptimizerError::OutputContract(format!(
            "expected tool '{TOOL_NAME}', got '{tool_name}'"
        )));
    }

    parse_model_output(input)
}

pub(crate) fn parse_model_output(
    input: &serde_json::Value,
) -> Result<ModelOutput, MessageOptimizerError> {
    let output: ModelOutput = serde_json::from_value(input.clone()).map_err(|error| {
        MessageOptimizerError::OutputContract(format!("invalid tool payload: {error}"))
    })?;

    if output.system_prompt.trim().is_empty() {
        return Err(MessageOptimizerError::OutputContract(
            "system_prompt must not be empty".to_string(),
        ));
    }

    if output.user_prompt.trim().is_empty() {
        return Err(MessageOptimizerError::OutputContract(
            "user_prompt must not be empty".to_string(),
        ));
    }

    Ok(output)
}
