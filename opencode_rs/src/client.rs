//! High-level client API for OpenCode.
//!
//! This module provides the ergonomic `Client` and `ClientBuilder` types.

#[cfg(not(feature = "http"))]
use crate::error::OpencodeError;
use crate::error::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[cfg(feature = "http")]
use crate::http::{HttpClient, HttpConfig};

/// OpenCode client for interacting with the server.
#[derive(Clone)]
pub struct Client {
    #[cfg(feature = "http")]
    http: HttpClient,
    // Used by SSE subscriber for reconnection (Phase 5)
    #[allow(dead_code)]
    last_event_id: Arc<RwLock<Option<String>>>,
}

/// Builder for creating a [`Client`].
#[derive(Clone)]
pub struct ClientBuilder {
    base_url: String,
    directory: Option<String>,
    timeout: Duration,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:4096".to_string(),
            directory: None,
            timeout: Duration::from_secs(300), // 5 min for long AI requests
        }
    }
}

impl ClientBuilder {
    /// Create a new client builder with default settings.
    ///
    /// Default settings:
    /// - Base URL: `http://127.0.0.1:4096`
    /// - Timeout: 300 seconds (5 minutes)
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the base URL for the OpenCode server.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Set the directory context for requests.
    ///
    /// This sets the `x-opencode-directory` header on all requests.
    pub fn directory(mut self, dir: impl Into<String>) -> Self {
        self.directory = Some(dir.into());
        self
    }

    /// Set the request timeout in seconds.
    pub fn timeout_secs(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    /// Build the client.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be built or if the
    /// `http` feature is not enabled.
    #[cfg(feature = "http")]
    pub fn build(self) -> Result<Client> {
        let http = HttpClient::new(HttpConfig {
            base_url: self.base_url,
            directory: self.directory,
            timeout: self.timeout,
        })?;

        Ok(Client {
            http,
            last_event_id: Arc::new(RwLock::new(None)),
        })
    }

    /// Build the client.
    ///
    /// # Errors
    ///
    /// Returns an error because the `http` feature is required.
    #[cfg(not(feature = "http"))]
    pub fn build(self) -> Result<Client> {
        Err(OpencodeError::InvalidConfig(
            "http feature required to build client".into(),
        ))
    }
}

impl Client {
    /// Create a new client builder.
    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    /// Get the sessions API.
    #[cfg(feature = "http")]
    pub fn sessions(&self) -> crate::http::sessions::SessionsApi {
        crate::http::sessions::SessionsApi::new(self.http.clone())
    }

    /// Get the messages API.
    #[cfg(feature = "http")]
    pub fn messages(&self) -> crate::http::messages::MessagesApi {
        crate::http::messages::MessagesApi::new(self.http.clone())
    }

    /// Get the parts API.
    #[cfg(feature = "http")]
    pub fn parts(&self) -> crate::http::parts::PartsApi {
        crate::http::parts::PartsApi::new(self.http.clone())
    }

    /// Get the permissions API.
    #[cfg(feature = "http")]
    pub fn permissions(&self) -> crate::http::permissions::PermissionsApi {
        crate::http::permissions::PermissionsApi::new(self.http.clone())
    }

    /// Get the files API.
    #[cfg(feature = "http")]
    pub fn files(&self) -> crate::http::files::FilesApi {
        crate::http::files::FilesApi::new(self.http.clone())
    }

    /// Get the find API.
    #[cfg(feature = "http")]
    pub fn find(&self) -> crate::http::find::FindApi {
        crate::http::find::FindApi::new(self.http.clone())
    }

    /// Get the providers API.
    #[cfg(feature = "http")]
    pub fn providers(&self) -> crate::http::providers::ProvidersApi {
        crate::http::providers::ProvidersApi::new(self.http.clone())
    }

    /// Get the MCP API.
    #[cfg(feature = "http")]
    pub fn mcp(&self) -> crate::http::mcp::McpApi {
        crate::http::mcp::McpApi::new(self.http.clone())
    }

    /// Get the PTY API.
    #[cfg(feature = "http")]
    pub fn pty(&self) -> crate::http::pty::PtyApi {
        crate::http::pty::PtyApi::new(self.http.clone())
    }

    /// Get the config API.
    #[cfg(feature = "http")]
    pub fn config(&self) -> crate::http::config::ConfigApi {
        crate::http::config::ConfigApi::new(self.http.clone())
    }

    /// Get the tools API.
    #[cfg(feature = "http")]
    pub fn tools(&self) -> crate::http::tools::ToolsApi {
        crate::http::tools::ToolsApi::new(self.http.clone())
    }

    /// Get the project API.
    #[cfg(feature = "http")]
    pub fn project(&self) -> crate::http::project::ProjectApi {
        crate::http::project::ProjectApi::new(self.http.clone())
    }

    /// Get the worktree API.
    #[cfg(feature = "http")]
    pub fn worktree(&self) -> crate::http::worktree::WorktreeApi {
        crate::http::worktree::WorktreeApi::new(self.http.clone())
    }

    /// Get the misc API.
    #[cfg(feature = "http")]
    pub fn misc(&self) -> crate::http::misc::MiscApi {
        crate::http::misc::MiscApi::new(self.http.clone())
    }

    /// Simple helper to create session and send a text prompt.
    ///
    /// Note: This method returns immediately after sending the prompt.
    /// The AI response will arrive asynchronously via SSE events.
    /// Use [`subscribe_session`] to receive the response.
    ///
    /// # Errors
    ///
    /// Returns an error if session creation or prompt fails.
    #[cfg(feature = "http")]
    pub async fn run_simple_text(
        &self,
        text: impl Into<String>,
    ) -> Result<crate::types::session::Session> {
        use crate::types::message::{PromptPart, PromptRequest};
        use crate::types::session::CreateSessionRequest;

        let session = self
            .sessions()
            .create(&CreateSessionRequest::default())
            .await?;

        let _ = self
            .messages()
            .prompt(
                &session.id,
                &PromptRequest {
                    parts: vec![PromptPart::Text {
                        text: text.into(),
                        synthetic: None,
                        ignored: None,
                        metadata: None,
                    }],
                    message_id: None,
                    model: None,
                    agent: None,
                    no_reply: None,
                    system: None,
                    variant: None,
                },
            )
            .await?;

        Ok(session)
    }

    /// Set the last event ID (for SSE reconnection).
    #[cfg(feature = "sse")]
    #[allow(dead_code)] // Used by SSE subscriber in Phase 5
    pub(crate) async fn set_last_event_id(&self, id: Option<String>) {
        *self.last_event_id.write().await = id;
    }

    /// Get the last event ID.
    #[cfg(feature = "sse")]
    #[allow(dead_code)] // Used by SSE subscriber in Phase 5
    pub(crate) async fn last_event_id(&self) -> Option<String> {
        self.last_event_id.read().await.clone()
    }

    /// Get the HTTP client.
    #[cfg(feature = "http")]
    #[allow(dead_code)] // May be used by external crates
    pub(crate) fn http(&self) -> &HttpClient {
        &self.http
    }

    /// Get the last event ID handle for SSE.
    #[cfg(feature = "sse")]
    #[allow(dead_code)] // May be used by external crates
    pub(crate) fn last_event_id_handle(&self) -> Arc<RwLock<Option<String>>> {
        self.last_event_id.clone()
    }
}

#[cfg(all(feature = "http", feature = "sse"))]
impl Client {
    /// Get an SSE subscriber for streaming events.
    pub fn sse_subscriber(&self) -> crate::sse::SseSubscriber {
        crate::sse::SseSubscriber::new(
            self.http.base().to_string(),
            self.http.directory().map(|s| s.to_string()),
            self.last_event_id.clone(),
        )
    }

    /// Subscribe to all events for the configured directory with default options.
    ///
    /// This subscribes to the `/event` endpoint which streams all events
    /// for the directory specified in the client configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe(&self) -> Result<crate::sse::SseSubscription> {
        self.sse_subscriber()
            .subscribe(crate::sse::SseOptions::default())
            .await
    }

    /// Subscribe to events filtered by session ID with default options.
    ///
    /// Events are filtered client-side to only include events matching
    /// the specified session ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe_session(&self, session_id: &str) -> Result<crate::sse::SseSubscription> {
        self.sse_subscriber()
            .subscribe_session(session_id, crate::sse::SseOptions::default())
            .await
    }

    /// Subscribe to global events with default options (all directories).
    ///
    /// # Errors
    ///
    /// Returns an error if the subscription cannot be created.
    pub async fn subscribe_global(&self) -> Result<crate::sse::SseSubscription> {
        self.sse_subscriber()
            .subscribe_global(crate::sse::SseOptions::default())
            .await
    }
}

#[cfg(test)]
mod tests {
    // TODO(3): Add integration tests with mocked HTTP/SSE backends for Client API methods
    use super::*;

    #[test]
    fn test_client_builder_defaults() {
        let builder = ClientBuilder::new();
        assert_eq!(builder.base_url, "http://127.0.0.1:4096");
        assert_eq!(builder.timeout, Duration::from_secs(300));
        assert!(builder.directory.is_none());
    }

    #[test]
    fn test_client_builder_customization() {
        let builder = ClientBuilder::new()
            .base_url("http://localhost:8080")
            .directory("/my/project")
            .timeout_secs(60);

        assert_eq!(builder.base_url, "http://localhost:8080");
        assert_eq!(builder.directory, Some("/my/project".to_string()));
        assert_eq!(builder.timeout, Duration::from_secs(60));
    }

    #[cfg(feature = "http")]
    #[test]
    fn test_client_build() {
        let client = ClientBuilder::new().build();
        assert!(client.is_ok());
    }
}
