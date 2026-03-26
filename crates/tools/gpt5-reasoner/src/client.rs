use crate::errors::ReasonerError;
use crate::errors::Result;
use async_openai::Client;
use async_openai::config::OpenAIConfig;

pub struct OrClient {
    pub client: Client<OpenAIConfig>,
}

impl OrClient {
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .map_err(|_| ReasonerError::MissingEnv("OPENROUTER_API_KEY".into()))?;

        let config = OpenAIConfig::new()
            .with_api_base("https://openrouter.ai/api/v1")
            .with_api_key(api_key);

        Ok(Self {
            client: Client::with_config(config),
        })
    }
}
