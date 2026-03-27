mod anthropic;
mod error;
mod prompts;
mod types;

pub use crate::error::MessageOptimizerError;
pub use crate::types::OptimizeMessageRequest;
pub use crate::types::OptimizedPrompt;

use crate::anthropic::AnthropicOptimizer;
use crate::anthropic::OptimizerBackend;
use crate::prompts::build_optimizer_user;
use crate::prompts::build_repair_user;
use crate::types::ModelOutput;

const MAX_ATTEMPTS: usize = 3;

pub async fn optimize_message(
    request: OptimizeMessageRequest,
) -> Result<OptimizedPrompt, MessageOptimizerError> {
    optimize_message_with_backend(&AnthropicOptimizer::new(), request).await
}

pub(crate) async fn optimize_message_with_backend<B: OptimizerBackend>(
    backend: &B,
    request: OptimizeMessageRequest,
) -> Result<OptimizedPrompt, MessageOptimizerError> {
    validate_request(&request)?;

    let mut optimizer_user = build_optimizer_user(&request);
    let mut attempt = 0;

    loop {
        match backend.optimize(optimizer_user.clone()).await {
            Ok(output) => return Ok(build_optimized_prompt(output)),
            Err(MessageOptimizerError::OutputContract(error)) if attempt + 1 < MAX_ATTEMPTS => {
                attempt += 1;
                tracing::warn!(attempt, error = %error, "optimizer output contract violation; retrying");
                optimizer_user = build_repair_user(&request, &error);
            }
            Err(error) => return Err(error),
        }
    }
}

fn validate_request(request: &OptimizeMessageRequest) -> Result<(), MessageOptimizerError> {
    if request.message.trim().is_empty() {
        return Err(MessageOptimizerError::EmptyMessage);
    }

    Ok(())
}

fn build_optimized_prompt(output: ModelOutput) -> OptimizedPrompt {
    let assembled_prompt = assemble(&output.system_prompt, &output.user_prompt);

    OptimizedPrompt {
        system_prompt: output.system_prompt,
        user_prompt: output.user_prompt,
        assembled_prompt,
    }
}

pub(crate) fn assemble(system_prompt: &str, user_prompt: &str) -> String {
    format!(
        "<system_prompt>\n{}\n</system_prompt>\n\n<user_prompt>\n{}\n</user_prompt>\n",
        system_prompt.trim_end(),
        user_prompt.trim_end(),
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use anthropic_async::types::content::ContentBlock;
    use anthropic_async::types::content::MessageRole;
    use anthropic_async::types::messages::MessagesCreateResponse;

    use super::*;
    use crate::anthropic::TOOL_NAME;
    use crate::anthropic::extract_model_output;
    use crate::anthropic::parse_model_output;

    struct StubBackend {
        responses: Mutex<Vec<Result<ModelOutput, MessageOptimizerError>>>,
        calls: Mutex<Vec<String>>,
    }

    impl StubBackend {
        fn new(responses: Vec<Result<ModelOutput, MessageOptimizerError>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl OptimizerBackend for StubBackend {
        async fn optimize(
            &self,
            user_prompt: String,
        ) -> Result<ModelOutput, MessageOptimizerError> {
            let mut calls = self.calls.lock().map_err(|_| {
                MessageOptimizerError::OutputContract("stub calls mutex poisoned".to_string())
            })?;
            calls.push(user_prompt);
            drop(calls);

            let mut responses = self.responses.lock().map_err(|_| {
                MessageOptimizerError::OutputContract("stub responses mutex poisoned".to_string())
            })?;

            if responses.is_empty() {
                return Err(MessageOptimizerError::OutputContract(
                    "stub backend exhausted responses".to_string(),
                ));
            }

            responses.remove(0)
        }
    }

    #[test]
    fn assemble_is_deterministic() {
        let assembled = assemble("system\n", "user\n\n");
        assert_eq!(
            assembled,
            "<system_prompt>\nsystem\n</system_prompt>\n\n<user_prompt>\nuser\n</user_prompt>\n"
        );
    }

    #[test]
    fn parse_model_output_rejects_unknown_keys() {
        let result = parse_model_output(&serde_json::json!({
            "system_prompt": "system",
            "user_prompt": "user",
            "extra": true,
        }));

        assert!(matches!(
            result,
            Err(MessageOptimizerError::OutputContract(message)) if message.contains("unknown field")
        ));
    }

    #[test]
    fn parse_model_output_rejects_missing_required_keys() {
        let result = parse_model_output(&serde_json::json!({
            "system_prompt": "system",
        }));

        assert!(matches!(
            result,
            Err(MessageOptimizerError::OutputContract(message)) if message.contains("missing field")
        ));
    }

    #[test]
    fn parse_model_output_rejects_empty_values() {
        let result = parse_model_output(&serde_json::json!({
            "system_prompt": "   ",
            "user_prompt": "user",
        }));

        assert_eq!(
            result.err().map(|error| error.to_string()),
            Some(
                "optimizer output contract violation: system_prompt must not be empty".to_string()
            )
        );
    }

    #[test]
    fn extract_model_output_rejects_missing_tool_call() {
        let response = response_with_content(vec![ContentBlock::Text {
            text: "hello".to_string(),
            citations: None,
        }]);

        assert_eq!(
            extract_model_output(&response)
                .err()
                .map(|error| error.to_string()),
            Some(
                "optimizer output contract violation: missing tool call in optimizer response"
                    .to_string()
            )
        );
    }

    #[test]
    fn extract_model_output_rejects_wrong_tool_name() {
        let response = response_with_content(vec![ContentBlock::ToolUse {
            id: "tool-1".to_string(),
            name: "wrong_tool".to_string(),
            input: serde_json::json!({
                "system_prompt": "system",
                "user_prompt": "user",
            }),
        }]);

        assert_eq!(
            extract_model_output(&response)
                .err()
                .map(|error| error.to_string()),
            Some(format!(
                "optimizer output contract violation: expected tool '{TOOL_NAME}', got 'wrong_tool'"
            ))
        );
    }

    #[tokio::test]
    async fn empty_message_is_rejected_before_backend_call() {
        let backend = StubBackend::new(vec![]);
        let request = OptimizeMessageRequest {
            message: "   ".to_string(),
            supplemental_context: None,
        };

        let result = optimize_message_with_backend(&backend, request).await;
        assert!(matches!(result, Err(MessageOptimizerError::EmptyMessage)));

        let calls = backend
            .calls
            .lock()
            .map_err(|_| ())
            .ok()
            .map(|guard| guard.len());
        assert_eq!(calls, Some(0));
    }

    #[tokio::test]
    async fn retry_flow_recovers_after_contract_violation() {
        let backend = StubBackend::new(vec![
            Err(MessageOptimizerError::OutputContract(
                "missing tool call in optimizer response".to_string(),
            )),
            Ok(ModelOutput {
                system_prompt: "system prompt".to_string(),
                user_prompt: "user prompt".to_string(),
            }),
        ]);

        let request = OptimizeMessageRequest {
            message: "Do the task".to_string(),
            supplemental_context: Some("Constraints: be concise".to_string()),
        };

        let result = optimize_message_with_backend(&backend, request).await;

        match result {
            Ok(prompt) => assert_eq!(
                prompt,
                OptimizedPrompt {
                    system_prompt: "system prompt".to_string(),
                    user_prompt: "user prompt".to_string(),
                    assembled_prompt:
                        "<system_prompt>\nsystem prompt\n</system_prompt>\n\n<user_prompt>\nuser prompt\n</user_prompt>\n"
                            .to_string(),
                }
            ),
            Err(error) => panic!("unexpected error: {error}"),
        }

        let calls = backend.calls.lock().map_err(|_| ()).ok();
        let calls = calls.as_deref().map_or(&[][..], |value| value.as_slice());
        assert_eq!(calls.len(), 2);
        assert!(calls[0].contains("<message>\nDo the task\n</message>"));
        assert!(calls[1].contains("<contract_error>"));
    }

    fn response_with_content(content: Vec<ContentBlock>) -> MessagesCreateResponse {
        MessagesCreateResponse {
            id: "msg_123".to_string(),
            kind: "message".to_string(),
            role: MessageRole::Assistant,
            content,
            model: "claude-sonnet-4-6".to_string(),
            stop_reason: Some("tool_use".to_string()),
            usage: None,
        }
    }
}
