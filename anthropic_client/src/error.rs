use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnthropicError {
    #[error("HTTP error: {0}")]
    Reqwest(#[from] reqwest::Error),

    #[error("API error: {0:?}")]
    Api(ApiErrorObject),

    #[error("Invalid configuration: {0}")]
    Config(String),

    #[error("Serialization error: {0}")]
    Serde(String),
}

#[derive(Debug, Clone)]
pub struct ApiErrorObject {
    pub r#type: Option<String>,
    pub message: String,
    pub request_id: Option<String>,
    pub code: Option<String>,
}
