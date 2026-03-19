//! Shared orchestrator server state.
//!
//! Wraps `ManagedServer` + `Client` + cached model context limits + config.

use agentic_config::types::OrchestratorConfig;
use anyhow::Context;
use opencode_rs::Client;
use opencode_rs::server::ManagedServer;
use opencode_rs::server::ServerOptions;
use opencode_rs::types::message::Message;
use opencode_rs::types::message::Part;
use opencode_rs::types::provider::ProviderListResponse;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Environment variable name for the orchestrator-managed recursion guard.
pub const OPENCODE_ORCHESTRATOR_MANAGED_ENV: &str = "OPENCODE_ORCHESTRATOR_MANAGED";

/// User-facing message returned when orchestrator tools are invoked in a nested context.
pub const ORCHESTRATOR_MANAGED_GUARD_MESSAGE: &str = "ENV VAR OPENCODE_ORCHESTRATOR_MANAGED is set to 1. This most commonly happens when you're \
     in a nested orchestration session. Consult a human for assistance or try to accomplish your \
     task without the orchestration tools.";

/// Check if the orchestrator-managed env var is set (guard enabled).
pub fn managed_guard_enabled() -> bool {
    match std::env::var(OPENCODE_ORCHESTRATOR_MANAGED_ENV) {
        Ok(v) => v != "0" && !v.trim().is_empty(),
        Err(_) => false,
    }
}

/// Retry an async init operation once (2 total attempts) with tracing logs.
pub async fn init_with_retry<T, F, Fut>(mut f: F) -> anyhow::Result<T>
where
    F: FnMut(usize) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let mut last_err: Option<anyhow::Error> = None;

    for attempt in 1..=2 {
        tracing::info!(attempt, "orchestrator server lazy init attempt");
        match f(attempt).await {
            Ok(v) => {
                if attempt > 1 {
                    tracing::info!(
                        attempt,
                        "orchestrator server lazy init succeeded after retry"
                    );
                }
                return Ok(v);
            }
            Err(e) => {
                tracing::warn!(attempt, error = %e, "orchestrator server lazy init failed");
                last_err = Some(e);
            }
        }
    }

    tracing::error!("orchestrator server lazy init exhausted retries");
    // Safety: The loop always runs at least once and sets last_err on failure
    match last_err {
        Some(e) => Err(e),
        None => anyhow::bail!("init_with_retry: unexpected empty error state"),
    }
}

/// Key for looking up model context limits: (`provider_id`, `model_id`)
pub type ModelKey = (String, String);

/// Shared state wrapping the managed `OpenCode` server and HTTP client.
pub struct OrchestratorServer {
    /// Keep alive for lifecycle; Drop kills the opencode serve process.
    /// `None` when using an external client (e.g., wiremock tests).
    _managed: Option<ManagedServer>,
    /// HTTP client for `OpenCode` API
    client: Client,
    /// Cached model context limits from GET /provider
    model_context_limits: HashMap<ModelKey, u64>,
    /// Base URL of the managed server
    base_url: String,
    /// Orchestrator configuration (session timeouts, compaction threshold)
    config: OrchestratorConfig,
}

impl OrchestratorServer {
    /// Start a new managed `OpenCode` server and build the client.
    ///
    /// This is the eager initialization path that spawns the server immediately.
    /// Prefer `start_lazy()` for deferred initialization.
    ///
    /// # Errors
    ///
    /// Returns an error if the server fails to start or the client cannot be built.
    #[allow(clippy::allow_attributes, dead_code)]
    pub async fn start() -> anyhow::Result<Arc<Self>> {
        Ok(Arc::new(Self::start_impl().await?))
    }

    /// Lazy initialization path for `OnceCell` usage.
    ///
    /// Checks the recursion guard env var first, then uses retry logic.
    /// Returns `Self` (not `Arc<Self>`) for direct storage in `OnceCell`.
    ///
    /// # Errors
    ///
    /// Returns the guard message if `OPENCODE_ORCHESTRATOR_MANAGED` is set.
    /// Returns an error if the server fails to start after 2 attempts.
    pub async fn start_lazy() -> anyhow::Result<Self> {
        if managed_guard_enabled() {
            anyhow::bail!(ORCHESTRATOR_MANAGED_GUARD_MESSAGE);
        }

        init_with_retry(|_attempt| async { Self::start_impl().await }).await
    }

    /// Internal implementation that actually spawns the server.
    async fn start_impl() -> anyhow::Result<Self> {
        // Load configuration (best-effort, use defaults if unavailable)
        let cwd = std::env::current_dir().unwrap_or_default();
        let config = match agentic_config::loader::load_merged(&cwd) {
            Ok(loaded) => {
                for w in &loaded.warnings {
                    tracing::warn!("{w}");
                }
                loaded.config.orchestrator
            }
            Err(e) => {
                tracing::warn!("Failed to load config, using defaults: {e}");
                OrchestratorConfig::default()
            }
        };

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

        Ok(Self {
            _managed: Some(managed),
            client,
            model_context_limits,
            base_url,
            config,
        })
    }

    /// Get the HTTP client.
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Get the base URL of the managed server.
    #[allow(clippy::allow_attributes, dead_code)]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Look up context limit for a specific model.
    pub fn context_limit(&self, provider_id: &str, model_id: &str) -> Option<u64> {
        self.model_context_limits
            .get(&(provider_id.to_string(), model_id.to_string()))
            .copied()
    }

    /// Get the session deadline duration.
    pub fn session_deadline(&self) -> Duration {
        Duration::from_secs(self.config.session_deadline_secs)
    }

    /// Get the inactivity timeout duration.
    pub fn inactivity_timeout(&self) -> Duration {
        Duration::from_secs(self.config.inactivity_timeout_secs)
    }

    /// Get the compaction threshold (0.0 - 1.0).
    pub fn compaction_threshold(&self) -> f64 {
        self.config.compaction_threshold
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

/// Test support utilities (requires `test-support` feature).
///
/// These functions may appear unused when compiling non-test targets because
/// cargo's feature unification enables the feature for all targets when tests
/// are compiled. The `dead_code` warning is expected and suppressed.
#[cfg(feature = "test-support")]
#[allow(dead_code, clippy::allow_attributes)]
impl OrchestratorServer {
    /// Build an `OrchestratorServer` wrapper around an existing client.
    ///
    /// Does NOT manage an opencode process (intended for wiremock tests).
    /// Model context limits are not loaded and will return `None` for all lookups.
    pub fn from_client(client: Client, base_url: impl Into<String>) -> Arc<Self> {
        Arc::new(Self::from_client_unshared(client, base_url))
    }

    /// Build an `OrchestratorServer` wrapper returning `Self` (not `Arc<Self>`).
    ///
    /// Useful for tests that need to populate an `OnceCell` directly.
    pub fn from_client_unshared(client: Client, base_url: impl Into<String>) -> Self {
        Self {
            _managed: None,
            client,
            model_context_limits: HashMap::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            config: OrchestratorConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;

    /// Mutex to serialize env var tests (env vars are process-global).
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[tokio::test]
    async fn init_with_retry_succeeds_on_first_attempt() {
        let attempts = AtomicUsize::new(0);

        let result: u32 = init_with_retry(|_| {
            let n = attempts.fetch_add(1, Ordering::SeqCst);
            async move {
                // Always succeed
                assert_eq!(n, 0, "should only be called once on success");
                Ok(42)
            }
        })
        .await
        .unwrap();

        assert_eq!(result, 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn init_with_retry_retries_once_and_succeeds() {
        let attempts = AtomicUsize::new(0);

        let result: u32 = init_with_retry(|_| {
            let n = attempts.fetch_add(1, Ordering::SeqCst);
            async move {
                if n == 0 {
                    anyhow::bail!("fail first");
                }
                Ok(42)
            }
        })
        .await
        .unwrap();

        assert_eq!(result, 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn init_with_retry_fails_after_two_attempts() {
        let attempts = AtomicUsize::new(0);

        let err = init_with_retry::<(), _, _>(|_| {
            attempts.fetch_add(1, Ordering::SeqCst);
            async { anyhow::bail!("always fail") }
        })
        .await
        .unwrap_err();

        assert!(err.to_string().contains("always fail"));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn managed_guard_disabled_when_env_not_set() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        // Ensure the env var is not set
        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::remove_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV) };
        assert!(!managed_guard_enabled());
    }

    #[test]
    fn managed_guard_enabled_when_env_is_1() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV, "1") };
        assert!(managed_guard_enabled());
        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::remove_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV) };
    }

    #[test]
    fn managed_guard_disabled_when_env_is_0() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV, "0") };
        assert!(!managed_guard_enabled());
        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::remove_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV) };
    }

    #[test]
    fn managed_guard_disabled_when_env_is_empty() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV, "") };
        assert!(!managed_guard_enabled());
        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::remove_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV) };
    }

    #[test]
    fn managed_guard_disabled_when_env_is_whitespace() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV, "   ") };
        assert!(!managed_guard_enabled());
        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::remove_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV) };
    }

    #[test]
    fn managed_guard_enabled_when_env_is_truthy() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV, "true") };
        assert!(managed_guard_enabled());
        // SAFETY: Test runs with ENV_LOCK held, ensuring no concurrent env modification
        unsafe { std::env::remove_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV) };
    }
}
