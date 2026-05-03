use crate::errors::ReasonerError;
use crate::errors::Result;
use async_openai::Client;
use async_openai::config::OpenAIConfig;

pub struct OrClient {
    pub client: Client<OpenAIConfig>,
}

impl OrClient {
    pub fn from_env(api_base_url: Option<&str>) -> Result<Self> {
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .map_err(|_| ReasonerError::MissingEnv("OPENROUTER_API_KEY".into()))?;

        let base = api_base_url
            .unwrap_or("https://openrouter.ai/api/v1")
            .trim_end_matches('/');

        let config = OpenAIConfig::new()
            .with_api_base(base)
            .with_api_key(api_key);

        Ok(Self {
            client: Client::with_config(config),
        })
    }
}
