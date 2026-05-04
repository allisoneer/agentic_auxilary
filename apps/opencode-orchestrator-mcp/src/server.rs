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
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::RwLock;

use crate::version;

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

#[derive(Debug, Clone, PartialEq, Eq)]
enum ServerEntryState {
    Healthy,
    NeedsRecovery { reason: String },
}

const TOOL_ENTRY_HEALTH_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Shared recoverable handle for the process-global orchestrator server snapshot.
pub struct OrchestratorServerHandle {
    cached: AsyncMutex<Option<Arc<OrchestratorServer>>>,
}

impl Default for OrchestratorServerHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl OrchestratorServerHandle {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cached: AsyncMutex::new(None),
        }
    }

    /// Acquire a live orchestrator server snapshot for a tool entry.
    ///
    /// Existing callers keep their previously acquired `Arc<OrchestratorServer>`
    /// even if this handle later replaces the cached snapshot during recovery.
    pub async fn acquire(&self) -> anyhow::Result<Arc<OrchestratorServer>> {
        self.get_or_recover_with(OrchestratorServer::start_lazy)
            .await
    }

    async fn get_or_recover_with<F, Fut>(
        &self,
        mut start: F,
    ) -> anyhow::Result<Arc<OrchestratorServer>>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<OrchestratorServer>>,
    {
        loop {
            let snapshot = {
                let mut cached = self.cached.lock().await;

                if let Some(snapshot) = cached.as_ref() {
                    Arc::clone(snapshot)
                } else {
                    tracing::info!(
                        "orchestrator server missing cached snapshot; starting embedded server"
                    );

                    let rebuilt = Arc::new(start().await?);
                    // Full rebuild intentionally replaces the entire cached snapshot, which
                    // also resets `spawned_sessions` so `launched_by_you` only describes
                    // sessions owned by the current cached server instance.
                    *cached = Some(Arc::clone(&rebuilt));
                    return Ok(rebuilt);
                }
            };

            let state = snapshot.validate_for_tool_entry().await?;

            let mut cached = self.cached.lock().await;
            let Some(current) = cached.as_ref() else {
                continue;
            };

            if !Arc::ptr_eq(current, &snapshot) {
                continue;
            }

            match state {
                ServerEntryState::Healthy => return Ok(snapshot),
                ServerEntryState::NeedsRecovery { reason } => {
                    tracing::warn!(reason = %reason, "cached orchestrator server failed liveness check; rebuilding");

                    let rebuilt = Arc::new(start().await?);
                    // Full rebuild intentionally replaces the entire cached snapshot, which
                    // also resets `spawned_sessions` so `launched_by_you` only describes
                    // sessions owned by the current cached server instance.
                    *cached = Some(Arc::clone(&rebuilt));
                    return Ok(rebuilt);
                }
            }
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    #[must_use]
    pub fn from_server_unshared(server: OrchestratorServer) -> Self {
        Self {
            cached: AsyncMutex::new(Some(Arc::new(server))),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub async fn acquire_or_recover_with<F, Fut>(
        &self,
        start: F,
    ) -> anyhow::Result<Arc<OrchestratorServer>>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<OrchestratorServer>>,
    {
        self.get_or_recover_with(start).await
    }
}

/// Shared state wrapping the managed `OpenCode` server and HTTP client.
pub struct OrchestratorServer {
    /// Keep alive for lifecycle; Drop kills the opencode serve process.
    /// `None` when using an external client (e.g., wiremock tests).
    managed_server: StdMutex<Option<ManagedServer>>,
    /// HTTP client for `OpenCode` API
    client: Client,
    /// Cached model context limits from GET /provider
    model_context_limits: HashMap<ModelKey, u64>,
    /// Base URL of the managed server
    base_url: String,
    /// Orchestrator configuration (session timeouts, compaction threshold)
    config: OrchestratorConfig,
    /// Session IDs created by this orchestrator instance.
    spawned_sessions: Arc<RwLock<HashSet<String>>>,
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
        Self::start_lazy_with_config(None).await
    }

    /// Start the orchestrator server lazily with optional config injection.
    ///
    /// # Arguments
    ///
    /// * `config_json` - Optional JSON config to inject via `OPENCODE_CONFIG_CONTENT`
    ///
    /// # Errors
    ///
    /// Returns the guard message if `OPENCODE_ORCHESTRATOR_MANAGED` is set.
    /// Returns an error if the server fails to start after 2 attempts.
    pub async fn start_lazy_with_config(config_json: Option<String>) -> anyhow::Result<Self> {
        if managed_guard_enabled() {
            anyhow::bail!(ORCHESTRATOR_MANAGED_GUARD_MESSAGE);
        }

        init_with_retry(|_attempt| {
            let cfg = config_json.clone();
            async move { Self::start_impl_with_config(cfg).await }
        })
        .await
    }

    /// Internal implementation that actually spawns the server.
    async fn start_impl() -> anyhow::Result<Self> {
        let cwd = std::env::current_dir().context("Failed to resolve current directory")?;

        // Load configuration (best-effort, use defaults if unavailable)
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

        let launcher_config = version::resolve_launcher_config(&cwd)
            .context("Failed to resolve OpenCode launcher configuration")?;

        tracing::info!(
            binary = %launcher_config.binary,
            launcher_args = ?launcher_config.launcher_args,
            expected_version = %version::PINNED_OPENCODE_VERSION,
            "starting embedded opencode serve (pinned stable)"
        );

        let opts = ServerOptions::default()
            .binary(&launcher_config.binary)
            .launcher_args(launcher_config.launcher_args)
            .directory(cwd.clone());

        let managed = ManagedServer::start(opts)
            .await
            .context("Failed to start embedded `opencode serve`")?;

        // Avoid trailing slash to prevent `//event` formatting
        let base_url = managed.url().to_string().trim_end_matches('/').to_string();

        let client = Client::builder()
            .base_url(&base_url)
            .directory(cwd.to_string_lossy().to_string())
            .build()
            .context("Failed to build opencode-rs HTTP client")?;

        let health = client
            .misc()
            .health()
            .await
            .context("Failed to fetch /global/health for version validation")?;

        version::validate_exact_version(health.version.as_deref()).with_context(|| {
            format!(
                "Embedded OpenCode server did not match pinned stable v{} (binary={})",
                version::PINNED_OPENCODE_VERSION,
                launcher_config.binary
            )
        })?;

        // Load model context limits (best-effort, don't fail if unavailable)
        let model_context_limits = Self::load_model_limits(&client).await.unwrap_or_else(|e| {
            tracing::warn!("Failed to load model limits: {}", e);
            HashMap::new()
        });

        tracing::info!("Loaded {} model context limits", model_context_limits.len());

        Ok(Self {
            managed_server: StdMutex::new(Some(managed)),
            client,
            model_context_limits,
            base_url,
            config,
            spawned_sessions: Arc::new(RwLock::new(HashSet::new())),
        })
    }

    /// Internal implementation with optional config injection.
    async fn start_impl_with_config(config_json: Option<String>) -> anyhow::Result<Self> {
        let cwd = std::env::current_dir().context("Failed to resolve current directory")?;

        // Load configuration (best-effort, use defaults if unavailable)
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

        let launcher_config = version::resolve_launcher_config(&cwd)
            .context("Failed to resolve OpenCode launcher configuration")?;

        tracing::info!(
            binary = %launcher_config.binary,
            launcher_args = ?launcher_config.launcher_args,
            expected_version = %version::PINNED_OPENCODE_VERSION,
            config_injected = config_json.is_some(),
            "starting embedded opencode serve (pinned stable)"
        );

        let mut opts = ServerOptions::default()
            .binary(&launcher_config.binary)
            .launcher_args(launcher_config.launcher_args)
            .directory(cwd.clone());

        // Inject config if provided
        if let Some(cfg) = config_json {
            opts = opts.config_json(cfg);
        }

        let managed = ManagedServer::start(opts)
            .await
            .context("Failed to start embedded `opencode serve`")?;

        // Avoid trailing slash to prevent `//event` formatting
        let base_url = managed.url().to_string().trim_end_matches('/').to_string();

        let client = Client::builder()
            .base_url(&base_url)
            .directory(cwd.to_string_lossy().to_string())
            .build()
            .context("Failed to build opencode-rs HTTP client")?;

        let health = client
            .misc()
            .health()
            .await
            .context("Failed to fetch /global/health for version validation")?;

        version::validate_exact_version(health.version.as_deref()).with_context(|| {
            format!(
                "Embedded OpenCode server did not match pinned stable v{} (binary={})",
                version::PINNED_OPENCODE_VERSION,
                launcher_config.binary
            )
        })?;

        // Load model context limits (best-effort, don't fail if unavailable)
        let model_context_limits = Self::load_model_limits(&client).await.unwrap_or_else(|e| {
            tracing::warn!("Failed to load model limits: {}", e);
            HashMap::new()
        });

        tracing::info!("Loaded {} model context limits", model_context_limits.len());

        Ok(Self {
            managed_server: StdMutex::new(Some(managed)),
            client,
            model_context_limits,
            base_url,
            config,
            spawned_sessions: Arc::new(RwLock::new(HashSet::new())),
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

    /// Returns session IDs created by this orchestrator instance.
    pub fn spawned_sessions(&self) -> &Arc<RwLock<HashSet<String>>> {
        &self.spawned_sessions
    }

    fn managed_server_lock(&self) -> std::sync::MutexGuard<'_, Option<ManagedServer>> {
        self.managed_server
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn is_managed(&self) -> bool {
        self.managed_server_lock().is_some()
    }

    async fn validate_for_tool_entry(&self) -> anyhow::Result<ServerEntryState> {
        self.validate_for_tool_entry_with_timeout(TOOL_ENTRY_HEALTH_PROBE_TIMEOUT)
            .await
    }

    async fn validate_for_tool_entry_with_timeout(
        &self,
        health_probe_timeout: Duration,
    ) -> anyhow::Result<ServerEntryState> {
        if self.is_managed() {
            let is_running = {
                let mut managed = self.managed_server_lock();
                managed
                    .as_mut()
                    .is_some_and(opencode_rs::server::ManagedServer::is_running)
            };

            if !is_running {
                return Ok(ServerEntryState::NeedsRecovery {
                    reason: "managed child is no longer running".to_string(),
                });
            }
        }

        match tokio::time::timeout(health_probe_timeout, self.client.misc().health()).await {
            Ok(Ok(health)) if health.healthy => Ok(ServerEntryState::Healthy),
            Ok(Ok(_health)) => Ok(ServerEntryState::NeedsRecovery {
                reason: "/global/health reported unhealthy".to_string(),
            }),
            Ok(Err(error)) => Ok(ServerEntryState::NeedsRecovery {
                reason: format!("/global/health probe failed: {error}"),
            }),
            Err(_elapsed) => Ok(ServerEntryState::NeedsRecovery {
                reason: format!("/global/health probe timed out after {health_probe_timeout:?}"),
            }),
        }
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
#[cfg(any(test, feature = "test-support"))]
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
    /// Useful for tests that need to preseed an `OrchestratorServerHandle` directly.
    pub fn from_client_unshared(client: Client, base_url: impl Into<String>) -> Self {
        Self {
            managed_server: StdMutex::new(None),
            client,
            model_context_limits: HashMap::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            config: OrchestratorConfig::default(),
            spawned_sessions: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub fn from_managed_for_testing(
        managed: ManagedServer,
        client: Client,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            managed_server: StdMutex::new(Some(managed)),
            client,
            model_context_limits: HashMap::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            config: OrchestratorConfig::default(),
            spawned_sessions: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub async fn stop_managed_for_testing(&self) -> anyhow::Result<()> {
        let managed = {
            let mut guard = self.managed_server_lock();
            guard.take()
        };

        match managed {
            Some(managed) => managed.stop().await.map_err(Into::into),
            None => anyhow::bail!("no managed server is attached to this snapshot"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use std::time::Instant;
    use tokio::io::AsyncReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;
    use tokio::process::Command;
    use tokio::sync::Notify;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    struct ManagedEnvGuard {
        previous: Option<std::ffi::OsString>,
    }

    impl ManagedEnvGuard {
        fn new() -> Self {
            Self {
                previous: std::env::var_os(OPENCODE_ORCHESTRATOR_MANAGED_ENV),
            }
        }
    }

    impl Drop for ManagedEnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
                Some(value) => unsafe {
                    std::env::set_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV, value);
                },
                // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
                None => unsafe {
                    std::env::remove_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV);
                },
            }
        }
    }

    async fn health_mock_server() -> MockServer {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/global/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "healthy": true,
                "version": version::PINNED_OPENCODE_VERSION,
            })))
            .mount(&mock)
            .await;
        mock
    }

    fn test_client(base_url: &str) -> Client {
        opencode_rs::ClientBuilder::new()
            .base_url(base_url)
            .timeout_secs(5)
            .build()
            .unwrap()
    }

    fn external_server(base_url: &str) -> OrchestratorServer {
        OrchestratorServer::from_client_unshared(test_client(base_url), base_url)
    }

    async fn exited_child() -> tokio::process::Child {
        let mut child = Command::new("sh").arg("-c").arg("exit 0").spawn().unwrap();
        let _status = child.wait().await.unwrap();
        child
    }

    async fn managed_server_with_exited_child(base_url: &str) -> OrchestratorServer {
        let managed = ManagedServer::from_child_for_testing(exited_child().await, base_url, 9);
        OrchestratorServer::from_managed_for_testing(managed, test_client(base_url), base_url)
    }

    struct BlockingHealthServer {
        base_url: String,
        started_requests: Arc<AtomicUsize>,
        started_notify: Arc<Notify>,
        released: Arc<AtomicBool>,
        release_notify: Arc<Notify>,
        task: tokio::task::JoinHandle<()>,
    }

    impl BlockingHealthServer {
        async fn start(expected_requests: usize) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let started_requests = Arc::new(AtomicUsize::new(0));
            let started_notify = Arc::new(Notify::new());
            let released = Arc::new(AtomicBool::new(false));
            let release_notify = Arc::new(Notify::new());
            let body = format!(
                r#"{{"healthy":true,"version":"{}"}}"#,
                version::PINNED_OPENCODE_VERSION
            );
            let response = Arc::new(format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            ));

            let task = tokio::spawn({
                let started_requests = Arc::clone(&started_requests);
                let started_notify = Arc::clone(&started_notify);
                let released = Arc::clone(&released);
                let release_notify = Arc::clone(&release_notify);
                let response = Arc::clone(&response);

                async move {
                    let mut connections = Vec::with_capacity(expected_requests);

                    for _ in 0..expected_requests {
                        let (mut stream, _addr) = listener.accept().await.unwrap();
                        let started_requests = Arc::clone(&started_requests);
                        let started_notify = Arc::clone(&started_notify);
                        let released = Arc::clone(&released);
                        let release_notify = Arc::clone(&release_notify);
                        let response = Arc::clone(&response);

                        connections.push(tokio::spawn(async move {
                            let mut request = [0_u8; 1024];
                            let _read = stream.read(&mut request).await.unwrap();
                            started_requests.fetch_add(1, Ordering::SeqCst);
                            started_notify.notify_waiters();

                            while !released.load(Ordering::SeqCst) {
                                release_notify.notified().await;
                            }

                            stream.write_all(response.as_bytes()).await.unwrap();
                            stream.shutdown().await.unwrap();
                        }));
                    }

                    for connection in connections {
                        connection.await.unwrap();
                    }
                }
            });

            Self {
                base_url: format!("http://{addr}"),
                started_requests,
                started_notify,
                released,
                release_notify,
                task,
            }
        }

        async fn wait_for_requests(&self, expected_requests: usize) {
            tokio::time::timeout(Duration::from_secs(1), async {
                while self.started_requests.load(Ordering::SeqCst) < expected_requests {
                    self.started_notify.notified().await;
                }
            })
            .await
            .unwrap();
        }

        fn release(&self) {
            self.released.store(true, Ordering::SeqCst);
            self.release_notify.notify_waiters();
        }
    }

    impl Drop for BlockingHealthServer {
        fn drop(&mut self) {
            self.release();
            self.task.abort();
        }
    }

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

    #[tokio::test]
    async fn handle_serializes_initialization_and_reuses_snapshot() {
        let mock = health_mock_server().await;
        let base_url = mock.uri();
        let handle = Arc::new(OrchestratorServerHandle::new());
        let starts = Arc::new(AtomicUsize::new(0));

        let first = {
            let handle = Arc::clone(&handle);
            let starts = Arc::clone(&starts);
            let base_url = base_url.clone();
            tokio::spawn(async move {
                handle
                    .get_or_recover_with(|| {
                        let starts = Arc::clone(&starts);
                        let base_url = base_url.clone();
                        async move {
                            starts.fetch_add(1, Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            Ok(external_server(&base_url))
                        }
                    })
                    .await
            })
        };

        let second = {
            let handle = Arc::clone(&handle);
            let starts = Arc::clone(&starts);
            let base_url = base_url.clone();
            tokio::spawn(async move {
                handle
                    .get_or_recover_with(|| {
                        let starts = Arc::clone(&starts);
                        let base_url = base_url.clone();
                        async move {
                            starts.fetch_add(1, Ordering::SeqCst);
                            Ok(external_server(&base_url))
                        }
                    })
                    .await
            })
        };

        let first = first.await.unwrap().unwrap();
        let second = second.await.unwrap().unwrap();

        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[tokio::test]
    async fn validate_for_tool_entry_uses_health_for_external_server() {
        let mock = health_mock_server().await;
        let server = external_server(&mock.uri());

        let state = server.validate_for_tool_entry().await.unwrap();

        assert_eq!(state, ServerEntryState::Healthy);
        let requests = mock.received_requests().await.unwrap();
        assert!(
            requests
                .iter()
                .any(|request| request.url.path() == "/global/health"),
            "expected /global/health request"
        );
    }

    #[tokio::test]
    async fn validate_for_tool_entry_times_out_health_probe() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/global/health"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_secs(30))
                    .set_body_json(serde_json::json!({
                        "healthy": true,
                        "version": version::PINNED_OPENCODE_VERSION,
                    })),
            )
            .mount(&mock)
            .await;
        let server = external_server(&mock.uri());

        let state = server
            .validate_for_tool_entry_with_timeout(Duration::from_millis(25))
            .await
            .unwrap();

        assert_eq!(
            state,
            ServerEntryState::NeedsRecovery {
                reason: "/global/health probe timed out after 25ms".to_string(),
            }
        );
    }

    #[tokio::test]
    async fn validate_for_tool_entry_short_circuits_dead_managed_server() {
        let server = managed_server_with_exited_child("http://127.0.0.1:9").await;

        let state = server.validate_for_tool_entry().await.unwrap();

        assert_eq!(
            state,
            ServerEntryState::NeedsRecovery {
                reason: "managed child is no longer running".to_string(),
            }
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn handle_allows_concurrent_healthy_acquires_without_serializing_validation() {
        let health = BlockingHealthServer::start(3).await;
        let handle = Arc::new(OrchestratorServerHandle::from_server_unshared(
            external_server(&health.base_url),
        ));

        let started_at = Instant::now();
        let tasks = (0..3)
            .map(|_| {
                let handle = Arc::clone(&handle);
                tokio::spawn(async move { handle.acquire().await })
            })
            .collect::<Vec<_>>();

        health.wait_for_requests(3).await;
        tokio::time::sleep(Duration::from_millis(75)).await;
        health.release();

        let mut snapshots = Vec::with_capacity(tasks.len());
        for task in tasks {
            snapshots.push(task.await.unwrap().unwrap());
        }

        assert!(
            started_at.elapsed() < Duration::from_millis(250),
            "healthy acquires should overlap rather than serialize"
        );
        assert!(Arc::ptr_eq(&snapshots[0], &snapshots[1]));
        assert!(Arc::ptr_eq(&snapshots[1], &snapshots[2]));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn handle_single_flights_concurrent_stale_acquires() {
        let stale = Arc::new(managed_server_with_exited_child("http://127.0.0.1:9").await);
        let handle = Arc::new(OrchestratorServerHandle {
            cached: AsyncMutex::new(Some(Arc::clone(&stale))),
        });
        let mock = health_mock_server().await;
        let base_url = mock.uri();
        let starts = Arc::new(AtomicUsize::new(0));

        let tasks = (0..3)
            .map(|_| {
                let handle = Arc::clone(&handle);
                let starts = Arc::clone(&starts);
                let base_url = base_url.clone();
                tokio::spawn(async move {
                    handle
                        .get_or_recover_with(|| {
                            let starts = Arc::clone(&starts);
                            let base_url = base_url.clone();
                            async move {
                                starts.fetch_add(1, Ordering::SeqCst);
                                tokio::time::sleep(Duration::from_millis(50)).await;
                                Ok(external_server(&base_url))
                            }
                        })
                        .await
                })
            })
            .collect::<Vec<_>>();

        let mut snapshots = Vec::with_capacity(tasks.len());
        for task in tasks {
            snapshots.push(task.await.unwrap().unwrap());
        }

        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert!(!Arc::ptr_eq(&stale, &snapshots[0]));
        assert!(Arc::ptr_eq(&snapshots[0], &snapshots[1]));
        assert!(Arc::ptr_eq(&snapshots[1], &snapshots[2]));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn handle_retries_if_cache_changes_while_validating() {
        let old_health = BlockingHealthServer::start(1).await;
        let original = Arc::new(external_server(&old_health.base_url));
        let handle = Arc::new(OrchestratorServerHandle {
            cached: AsyncMutex::new(Some(Arc::clone(&original))),
        });
        let replacement_mock = health_mock_server().await;
        let replacement = Arc::new(external_server(&replacement_mock.uri()));

        let acquire = {
            let handle = Arc::clone(&handle);
            tokio::spawn(async move {
                handle
                    .acquire_or_recover_with(|| async { anyhow::bail!("should not rebuild") })
                    .await
            })
        };

        old_health.wait_for_requests(1).await;

        {
            let mut cached = tokio::time::timeout(Duration::from_millis(100), handle.cached.lock())
                .await
                .expect("validation should not hold the handle mutex");
            *cached = Some(Arc::clone(&replacement));
        }

        old_health.release();

        let snapshot = acquire.await.unwrap().unwrap();

        assert!(!Arc::ptr_eq(&snapshot, &original));
        assert!(Arc::ptr_eq(&snapshot, &replacement));
    }

    #[tokio::test]
    async fn handle_rebuilds_without_invalidating_held_snapshot() {
        let stale = Arc::new(managed_server_with_exited_child("http://127.0.0.1:9").await);
        let handle = OrchestratorServerHandle {
            cached: AsyncMutex::new(Some(Arc::clone(&stale))),
        };
        let mock = health_mock_server().await;
        let base_url = mock.uri();
        let starts = Arc::new(AtomicUsize::new(0));

        let rebuilt = handle
            .get_or_recover_with(|| {
                let starts = Arc::clone(&starts);
                let base_url = base_url.clone();
                async move {
                    starts.fetch_add(1, Ordering::SeqCst);
                    Ok(external_server(&base_url))
                }
            })
            .await
            .unwrap();

        assert_eq!(starts.load(Ordering::SeqCst), 1);
        assert!(!Arc::ptr_eq(&stale, &rebuilt));
        assert_eq!(stale.base_url(), "http://127.0.0.1:9");
        assert_eq!(rebuilt.base_url(), base_url.trim_end_matches('/'));
    }

    #[test]
    #[serial(env)]
    fn managed_guard_disabled_when_env_not_set() {
        let _env = ManagedEnvGuard::new();
        // Ensure the env var is not set
        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::remove_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV) };
        assert!(!managed_guard_enabled());
    }

    #[test]
    #[serial(env)]
    fn managed_guard_enabled_when_env_is_1() {
        let _env = ManagedEnvGuard::new();
        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV, "1") };
        assert!(managed_guard_enabled());
    }

    #[test]
    #[serial(env)]
    fn managed_guard_disabled_when_env_is_0() {
        let _env = ManagedEnvGuard::new();
        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV, "0") };
        assert!(!managed_guard_enabled());
    }

    #[test]
    #[serial(env)]
    fn managed_guard_disabled_when_env_is_empty() {
        let _env = ManagedEnvGuard::new();
        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV, "") };
        assert!(!managed_guard_enabled());
    }

    #[test]
    #[serial(env)]
    fn managed_guard_disabled_when_env_is_whitespace() {
        let _env = ManagedEnvGuard::new();
        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV, "   ") };
        assert!(!managed_guard_enabled());
    }

    #[test]
    #[serial(env)]
    fn managed_guard_enabled_when_env_is_truthy() {
        let _env = ManagedEnvGuard::new();
        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV, "true") };
        assert!(managed_guard_enabled());
    }

    #[tokio::test]
    #[serial(env)]
    async fn recursion_guard_only_blocks_real_startup_paths() {
        let _env = ManagedEnvGuard::new();
        // SAFETY: Test serialized by #[serial(env)], preventing concurrent env access.
        unsafe { std::env::set_var(OPENCODE_ORCHESTRATOR_MANAGED_ENV, "1") };

        let mock = health_mock_server().await;
        let handle = OrchestratorServerHandle::from_server_unshared(external_server(&mock.uri()));
        let reused = handle
            .get_or_recover_with(|| async { anyhow::bail!("should not start") })
            .await
            .unwrap();
        assert_eq!(reused.base_url(), mock.uri().trim_end_matches('/'));

        let fresh_handle = OrchestratorServerHandle::new();
        let err = match fresh_handle.acquire().await {
            Ok(_server) => panic!("expected recursion guard to block fresh startup"),
            Err(error) => error,
        };
        assert!(err.to_string().contains(ORCHESTRATOR_MANAGED_GUARD_MESSAGE));
    }
}
