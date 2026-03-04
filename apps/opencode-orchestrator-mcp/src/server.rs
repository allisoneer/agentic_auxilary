//! Shared orchestrator server state.
//!
//! Wraps `ManagedServer` + `Client` + cached model context limits.

use anyhow::Context;
use opencode_rs::Client;
use opencode_rs::server::{ManagedServer, ServerOptions};
use opencode_rs::types::message::{Message, Part};
use opencode_rs::types::provider::ProviderListResponse;
use std::collections::HashMap;
use std::sync::Arc;

/// Key for looking up model context limits: (`provider_id`, `model_id`)
pub type ModelKey = (String, String);

/// Shared state wrapping the managed `OpenCode` server and HTTP client.
pub struct OrchestratorServer {
    /// Keep alive for lifecycle; Drop kills the opencode serve process
    _managed: ManagedServer,
    /// HTTP client for `OpenCode` API
    client: Client,
    /// Cached model context limits from GET /provider
    model_context_limits: HashMap<ModelKey, u64>,
    /// Base URL of the managed server
    base_url: String,
}

impl OrchestratorServer {
    /// Start a new managed `OpenCode` server and build the client.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to start or the client cannot be built.
    pub async fn start() -> anyhow::Result<Arc<Self>> {
        let managed = ManagedServer::start(ServerOptions::default())
            .await
            .context("Failed to start embedded `opencode serve`")?;

        // Avoid trailing slash to prevent `//event` formatting
        let base_url = managed.url().to_string().trim_end_matches('/').to_string();

        let directory = std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().to_string());

        let mut builder = Client::builder().base_url(&base_url);
        if let Some(dir) = &directory {
            builder = builder.directory(dir.clone());
        }

        let client = builder
            .build()
            .context("Failed to build opencode-rs HTTP client")?;

        // Load model context limits (best-effort, don't fail if unavailable)
        let model_context_limits = Self::load_model_limits(&client).await.unwrap_or_else(|e| {
            tracing::warn!("Failed to load model limits: {}", e);
            HashMap::new()
        });

        tracing::info!("Loaded {} model context limits", model_context_limits.len());

        Ok(Arc::new(Self {
            _managed: managed,
            client,
            model_context_limits,
            base_url,
        }))
    }

    /// Get the HTTP client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get the base URL of the managed server.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Look up context limit for a specific model.
    pub fn context_limit(&self, provider_id: &str, model_id: &str) -> Option<u64> {
        self.model_context_limits
            .get(&(provider_id.to_string(), model_id.to_string()))
            .copied()
    }

    /// Load model context limits from GET /provider.
    async fn load_model_limits(client: &Client) -> anyhow::Result<HashMap<ModelKey, u64>> {
        let resp: ProviderListResponse = client.providers().list().await?;
        let mut limits = HashMap::new();

        for provider in resp.all {
            for (model_id, model) in provider.models {
                if let Some(limit) = model.limit.as_ref().and_then(|l| l.context) {
                    limits.insert((provider.id.clone(), model_id), limit);
                }
            }
        }

        Ok(limits)
    }

    /// Extract text content from the last assistant message.
    pub fn extract_assistant_text(messages: &[Message]) -> Option<String> {
        // Find the last assistant message
        let assistant_msg = messages.iter().rev().find(|m| m.info.role == "assistant")?;

        // Join all text parts
        let text: String = assistant_msg
            .parts
            .iter()
            .filter_map(|p| {
                if let Part::Text { text, .. } = p {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        if text.trim().is_empty() {
            None
        } else {
            Some(text)
        }
    }
}
