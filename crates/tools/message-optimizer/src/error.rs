use thiserror::Error;

#[derive(Debug, Error)]
pub enum MessageOptimizerError {
    #[error("message must not be empty")]
    EmptyMessage,

    #[error("anthropic api error: {0}")]
    Anthropic(#[from] anthropic_async::AnthropicError),

    #[error("optimizer output contract violation: {0}")]
    OutputContract(String),
}
