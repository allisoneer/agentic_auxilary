//! Test support utilities for wiremock-based integration tests.
//!
//! Provides helpers for constructing mock `OpenCode` API responses and
//! sequenced responders for simulating stale-then-fresh scenarios.

#![allow(clippy::unwrap_used)]
#![allow(dead_code)]

use opencode_orchestrator_mcp::server::OrchestratorServer;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tokio::sync::OnceCell;
use wiremock::MockServer;
use wiremock::Request;
use wiremock::Respond;
use wiremock::ResponseTemplate;

/// Build an `OrchestratorServer` cell connected to a wiremock `MockServer`.
///
/// The cell is pre-initialized with a server backed by the mock.
/// Uses a short 5-second timeout suitable for tests.
pub fn test_orchestrator_server(mock: &MockServer) -> Arc<OnceCell<OrchestratorServer>> {
    let base_url = mock.uri().trim_end_matches('/').to_string();
    let client = opencode_rs::ClientBuilder::new()
        .base_url(&base_url)
        .timeout_secs(5) // Short timeout for tests
        .build()
        .unwrap();

    let cell = Arc::new(OnceCell::new());
    // Pre-initialize with the mock-backed server (bypasses managed server spawn)
    cell.set(OrchestratorServer::from_client_unshared(client, base_url))
        .unwrap_or_else(|_| panic!("cell should be empty"));
    cell
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
// JSON Fixtures matching opencode-rs field names (camelCase)
// ============================================================================

/// Create a session fixture with the given session ID.
pub fn session_fixture(session_id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": session_id,
        "slug": session_id,
        "projectId": "proj1",
        "directory": "/tmp",
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
        "sessionId": session_id,
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
        "info": {"id": "u1", "sessionId": session_id, "role": "user", "time": {"created": 1}},
        "parts": []
    })];

    if let Some(text) = assistant_text {
        msgs.push(serde_json::json!({
            "info": {"id": "a1", "sessionId": session_id, "role": "assistant", "time": {"created": 2}},
            "parts": [{"type": "text", "text": text}]
        }));
    }

    serde_json::Value::Array(msgs)
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
