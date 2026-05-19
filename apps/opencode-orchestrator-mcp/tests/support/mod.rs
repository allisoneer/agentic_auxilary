//! Test support utilities for wiremock-based integration tests.
//!
//! Provides helpers for constructing mock `OpenCode` API responses and
//! sequenced responders for simulating stale-then-fresh scenarios.

#![allow(clippy::unwrap_used)]
#![allow(dead_code)]

use agentic_config::types::OrchestratorConfig;
use opencode_orchestrator_mcp::server::OrchestratorServer;
use opencode_orchestrator_mcp::server::OrchestratorServerHandle;
use opencode_orchestrator_mcp::server::RecoveryMode;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

/// Build an `OrchestratorServerHandle` connected to a wiremock `MockServer`.
///
/// The handle is pre-initialized with a server backed by the mock.
/// Uses a short 5-second timeout suitable for tests.
pub async fn test_orchestrator_server(mock: &MockServer) -> Arc<OrchestratorServerHandle> {
    test_orchestrator_server_with_config(mock, OrchestratorConfig::default()).await
}

/// Build an `OrchestratorServerHandle` connected to a wiremock `MockServer`
/// with explicit orchestrator config.
pub async fn test_orchestrator_server_with_config(
    mock: &MockServer,
    config: OrchestratorConfig,
) -> Arc<OrchestratorServerHandle> {
    Mock::given(method("GET"))
        .and(path("/global/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "healthy": true,
            "version": "test",
        })))
        .mount(mock)
        .await;

    let base_url = mock.uri().trim_end_matches('/').to_string();
    let client = opencode_rs::ClientBuilder::new()
        .base_url(&base_url)
        .timeout_secs(5) // Short timeout for tests
        .build()
        .unwrap();

    Arc::new(OrchestratorServerHandle::from_server_unshared(
        OrchestratorServer::from_client_unshared_with_config(
            client,
            base_url,
            RecoveryMode::External,
            config,
        ),
    ))
}

/// Respond with different responses in sequence; after exhausting, repeat last.
///
/// This is useful for simulating scenarios like:
/// - First call returns stale data, second call returns fresh data
/// - First call times out, second call succeeds
///
/// # Usage
///
/// ```ignore
/// let responder = SequenceResponder::new(vec![...]);
/// let call_counter = responder.call_counter();  // Get shared counter before mounting
/// Mock::given(...).respond_with(responder).mount(&mock).await;
/// // Later...
/// assert!(call_counter.get() >= 2);
/// ```
#[derive(Clone)]
pub struct SequenceResponder {
    responders: Vec<ResponseTemplate>,
    calls: Arc<AtomicUsize>,
}

impl SequenceResponder {
    /// Create a new sequence responder with the given response templates.
    ///
    /// # Panics
    ///
    /// Panics if `responders` is empty.
    pub fn new(responders: Vec<ResponseTemplate>) -> Self {
        assert!(!responders.is_empty(), "responders must not be empty");
        Self {
            responders,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Get a handle to the call counter that can be checked after the responder is consumed.
    ///
    /// Call this before passing the responder to `respond_with`.
    pub fn call_counter(&self) -> CallCounter {
        CallCounter {
            inner: Arc::clone(&self.calls),
        }
    }
}

/// Handle to a shared call counter for checking how many times a responder was invoked.
#[derive(Clone)]
pub struct CallCounter {
    inner: Arc<AtomicUsize>,
}

impl CallCounter {
    /// Get the current call count.
    pub fn get(&self) -> usize {
        self.inner.load(Ordering::SeqCst)
    }
}

/// Respond with one template until a call threshold is reached, then switch templates.
#[derive(Clone)]
pub struct SwitchAfterCallsResponder {
    counter: CallCounter,
    min_calls: usize,
    before: ResponseTemplate,
    after: ResponseTemplate,
}

impl SwitchAfterCallsResponder {
    /// Create a responder that switches to `after` once `counter.get() >= min_calls`.
    pub fn new(
        counter: CallCounter,
        min_calls: usize,
        before: ResponseTemplate,
        after: ResponseTemplate,
    ) -> Self {
        Self {
            counter,
            min_calls,
            before,
            after,
        }
    }
}

impl Respond for SwitchAfterCallsResponder {
    fn respond(&self, _req: &Request) -> ResponseTemplate {
        if self.counter.get() >= self.min_calls {
            self.after.clone()
        } else {
            self.before.clone()
        }
    }
}

impl Respond for SequenceResponder {
    fn respond(&self, _req: &Request) -> ResponseTemplate {
        let idx = self.calls.fetch_add(1, Ordering::SeqCst);
        self.responders
            .get(idx)
            .cloned()
            .unwrap_or_else(|| self.responders.last().cloned().expect("non-empty"))
    }
}

// ============================================================================
// JSON Fixtures matching upstream v1.14.33 `...ID` wire casing.
// ============================================================================

/// Create a session fixture with the given session ID.
pub fn session_fixture(session_id: &str) -> serde_json::Value {
    session_fixture_with_path(session_id, None)
}

/// Create a session fixture with an optional project-relative path.
pub fn session_fixture_with_path(session_id: &str, path: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "id": session_id,
        "slug": session_id,
        "projectId": "proj1",
        "directory": "/tmp",
        "path": path,
        "title": "Test Session",
        "version": "1.0",
        "time": { "created": 1_234_567_890, "updated": 1_234_567_890 }
    })
}

/// Create a v2 session status fixture (idle map).
pub fn status_v2_idle() -> serde_json::Value {
    serde_json::json!({})
}

/// Create a v2 session status fixture (busy map).
pub fn status_v2_busy(session_id: &str) -> serde_json::Value {
    serde_json::json!({
        session_id: { "type": "busy" }
    })
}

/// Create a v2 session status fixture (retry map).
pub fn status_v2_retry(session_id: &str, attempt: u64) -> serde_json::Value {
    serde_json::json!({
        session_id: {
            "type": "retry",
            "attempt": attempt,
            "message": "retrying",
            "next": 0
        }
    })
}

/// Create a modern session status map fixture from explicit entries.
pub fn session_status_fixture(statuses: &[(&str, Value)]) -> Value {
    let mut map = serde_json::Map::new();
    for (session_id, status) in statuses {
        map.insert((*session_id).to_string(), status.clone());
    }
    Value::Object(map)
}

/// Create a busy status fixture entry.
pub fn busy_status_fixture() -> Value {
    serde_json::json!({ "type": "busy" })
}

/// Create a retry status fixture entry.
pub fn retry_status_fixture(attempt: u64, message: &str, next: u64) -> Value {
    serde_json::json!({
        "type": "retry",
        "attempt": attempt,
        "message": message,
        "next": next,
    })
}

/// Create a permission fixture.
pub fn permission_fixture(
    id: &str,
    session_id: &str,
    permission: &str,
    patterns: &[&str],
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "sessionID": session_id,  // Note: sessionID not sessionId (matches opencode-rs types)
        "permission": permission,
        "patterns": patterns,
        "always": [],
        "tool": null,
        "metadata": null
    })
}

/// Create a question fixture.
pub fn question_fixture(
    id: &str,
    session_id: &str,
    questions: &[serde_json::Value],
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "sessionID": session_id,
        "questions": questions,
        "tool": null,
    })
}

/// Create a messages fixture with optional assistant text.
///
/// If `assistant_text` is `Some`, includes an assistant message with that text.
/// If `None`, only includes a user message (simulating stale/not-yet-persisted state).
pub fn messages_fixture(session_id: &str, assistant_text: Option<&str>) -> serde_json::Value {
    let mut msgs = vec![serde_json::json!({
        "info": {"id": "u1", "sessionID": session_id, "role": "user", "time": {"created": 1}},
        "parts": []
    })];

    if let Some(text) = assistant_text {
        msgs.push(serde_json::json!({
            "info": {"id": "a1", "sessionID": session_id, "role": "assistant", "time": {"created": 2}},
            "parts": [{"type": "text", "text": text}]
        }));
    }

    serde_json::Value::Array(msgs)
}

/// Create a message fixture with explicit parts and timestamps.
pub fn message_fixture(
    session_id: &str,
    message_id: &str,
    role: &str,
    created: i64,
    completed: Option<i64>,
    parts: Vec<Value>,
) -> Value {
    let mut time = serde_json::json!({ "created": created });
    if let Some(completed) = completed {
        time["completed"] = serde_json::json!(completed);
    }
    let parts = Value::Array(parts);

    serde_json::json!({
        "info": {
            "id": message_id,
            "sessionID": session_id,
            "role": role,
            "time": time,
        },
        "parts": parts,
    })
}

/// Create a tool part fixture with an optional state payload.
pub fn tool_part_fixture(call_id: &str, tool: &str, state: Option<Value>) -> Value {
    let mut part = serde_json::json!({
        "type": "tool",
        "callID": call_id,
        "tool": tool,
        "input": {},
    });

    if let Some(state) = state {
        part["state"] = state;
    }

    part
}

/// Create a message history response fixture.
pub fn message_history_fixture(messages: Vec<Value>) -> Value {
    Value::Array(messages)
}

/// Create a sessions list response fixture.
pub fn sessions_list_fixture(session_ids: &[&str]) -> serde_json::Value {
    serde_json::json!(
        session_ids
            .iter()
            .map(|id| session_fixture(id))
            .collect::<Vec<_>>()
    )
}

/// Create a commands list response fixture.
pub fn commands_list_fixture() -> serde_json::Value {
    serde_json::json!([
        {"name": "test", "description": "Run tests"},
        {"name": "build", "description": "Build project"},
        {"name": "lint", "description": "Run linter"}
    ])
}

/// Seed launched sessions on the in-memory test server.
pub async fn seed_spawned_sessions(server: &Arc<OrchestratorServerHandle>, session_ids: &[&str]) {
    let srv = server
        .acquire()
        .await
        .expect("test server should be initialized");
    let mut spawned = srv.spawned_sessions().write().await;
    for session_id in session_ids {
        spawned.insert((*session_id).to_string());
    }
}
