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
use std::sync::Condvar;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
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
    build_test_orchestrator_server(mock, 5, Some(config)).await
}

/// Build an `OrchestratorServerHandle` connected to a wiremock `MockServer`
/// with a one-second client timeout for transport-failure tests.
pub async fn short_timeout_test_orchestrator_server(
    mock: &MockServer,
) -> Arc<OrchestratorServerHandle> {
    build_test_orchestrator_server(mock, 1, None).await
}

async fn build_test_orchestrator_server(
    mock: &MockServer,
    timeout_secs: u64,
    config: Option<OrchestratorConfig>,
) -> Arc<OrchestratorServerHandle> {
    mount_test_health(mock).await;

    let base_url = mock.uri().trim_end_matches('/').to_string();
    let client = build_test_client(&base_url, timeout_secs);

    let server = if let Some(config) = config {
        OrchestratorServer::from_client_unshared_with_config(
            client,
            base_url,
            RecoveryMode::External,
            config,
        )
    } else {
        OrchestratorServer::from_client_unshared(client, &base_url, RecoveryMode::External)
    };

    Arc::new(OrchestratorServerHandle::from_server_unshared(server))
}

async fn mount_test_health(mock: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/global/health"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "healthy": true,
            "version": "test",
        })))
        .mount(mock)
        .await;
}

fn build_test_client(base_url: &str, timeout_secs: u64) -> opencode_rs::Client {
    match opencode_rs::ClientBuilder::new()
        .base_url(base_url)
        .directory("/tmp".to_string())
        .timeout_secs(timeout_secs)
        .build()
    {
        Ok(client) => client,
        Err(error) => panic!("failed to build test OpenCode client for {base_url}: {error}"),
    }
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

#[derive(Clone, Default)]
pub struct CommandCorrelationFixture {
    command_message_id: Arc<(Mutex<Option<String>>, Condvar)>,
}

impl CommandCorrelationFixture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn command_responder(&self) -> CommandMessageIdResponder {
        CommandMessageIdResponder {
            command_message_id: Arc::clone(&self.command_message_id),
        }
    }

    pub fn sse_responder(
        &self,
        session_id: &str,
        assistant_message_id: &str,
        delta: &str,
        interruption: Value,
    ) -> CommandCorrelatedSseResponder {
        CommandCorrelatedSseResponder {
            command_message_id: Arc::clone(&self.command_message_id),
            session_id: session_id.to_string(),
            assistant_message_id: assistant_message_id.to_string(),
            delta: delta.to_string(),
            interruption,
        }
    }
}

#[derive(Clone)]
pub struct CommandMessageIdResponder {
    command_message_id: Arc<(Mutex<Option<String>>, Condvar)>,
}

impl Respond for CommandMessageIdResponder {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        let message_id = body
            .get("messageID")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let (lock, ready) = &*self.command_message_id;
        *lock.lock().unwrap() = Some(message_id);
        ready.notify_all();

        ResponseTemplate::new(200).set_body_json(serde_json::json!({"status": "ok"}))
    }
}

#[derive(Clone)]
pub struct CommandCorrelatedSseResponder {
    command_message_id: Arc<(Mutex<Option<String>>, Condvar)>,
    session_id: String,
    assistant_message_id: String,
    delta: String,
    interruption: Value,
}

impl Respond for CommandCorrelatedSseResponder {
    fn respond(&self, _request: &Request) -> ResponseTemplate {
        let (lock, ready) = &*self.command_message_id;
        let message_id = lock.lock().unwrap();
        let (message_id, wait) = ready
            .wait_timeout_while(message_id, Duration::from_secs(5), |id| id.is_none())
            .unwrap();
        assert!(!wait.timed_out(), "command POST did not publish messageID");
        let command_message_id = message_id.clone().unwrap();
        drop(message_id);

        let events = [
            serde_json::json!({
                "type": "message.part.delta",
                "properties": {
                    "sessionID": self.session_id,
                    "messageID": self.assistant_message_id,
                    "delta": self.delta,
                }
            }),
            serde_json::json!({
                "type": "message.updated",
                "properties": {
                    "info": {
                        "id": self.assistant_message_id,
                        "sessionID": self.session_id,
                        "role": "assistant",
                        "parentID": command_message_id,
                        "time": { "created": 1 },
                    }
                }
            }),
            self.interruption.clone(),
        ];
        ResponseTemplate::new(200).set_body_raw(sse_body(&events), "text/event-stream")
    }
}

// ============================================================================
// JSON Fixtures matching upstream v1.17.4 `...ID` wire casing.
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

/// Create a session fixture with an optional parent using upstream `parentID` casing.
pub fn session_fixture_with_parent(session_id: &str, parent_id: Option<&str>) -> serde_json::Value {
    let mut session = session_fixture(session_id);
    if let Some(parent_id) = parent_id {
        session["parentID"] = serde_json::json!(parent_id);
    }
    session
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

/// Create an unknown status fixture entry.
pub fn unknown_status_fixture() -> Value {
    serde_json::json!({ "type": "paused" })
}

/// Create a permission fixture.
pub fn permission_fixture(
    id: &str,
    session_id: &str,
    permission: &str,
    patterns: &[&str],
) -> serde_json::Value {
    permission_fixture_with_metadata(id, session_id, permission, patterns, &Value::Null)
}

/// Create a realistic patch-file metadata entry.
pub fn patch_file_metadata_fixture() -> Value {
    serde_json::json!({
        "filePath": "/tmp/src/lib.rs",
        "relativePath": "src/lib.rs",
        "type": "update",
        "patch": "Index: src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n",
        "additions": 1,
        "deletions": 1,
        "movePath": null,
    })
}

/// Create a permission fixture with explicit metadata.
pub fn permission_fixture_with_metadata(
    id: &str,
    session_id: &str,
    permission: &str,
    patterns: &[&str],
    metadata: &Value,
) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "sessionID": session_id,  // Note: sessionID not sessionId (matches opencode-rs types)
        "permission": permission,
        "patterns": patterns,
        "always": [],
        "tool": null,
        "metadata": metadata
    })
}

/// Create the live-confirmed 400 response body array for patch-file metadata.
pub fn permission_patch_file_array_bad_request_fixture() -> Value {
    serde_json::json!([patch_file_metadata_fixture()])
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

/// Encode typed event JSON values as an SSE response body.
pub fn sse_body(events: &[Value]) -> String {
    let mut body = String::new();
    for event in events {
        body.push_str("data: ");
        body.push_str(&event.to_string());
        body.push_str("\n\n");
    }
    body
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
