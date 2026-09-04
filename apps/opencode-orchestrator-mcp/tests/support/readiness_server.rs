#![allow(clippy::expect_used, clippy::unwrap_used, dead_code)]

use agentic_config::types::OrchestratorConfig;
use opencode_orchestrator_mcp::server::OrchestratorServer;
use opencode_orchestrator_mcp::server::OrchestratorServerHandle;
use opencode_orchestrator_mcp::server::RecoveryMode;
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU16;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio::time::timeout;

const DEADLOCK_GUARD: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub enum ResponseAction {
    Json { status: u16, body: Value },
    Close,
}

impl ResponseAction {
    pub fn json(status: u16, body: Value) -> Self {
        Self::Json { status, body }
    }
}

#[derive(Default)]
struct ResponseSequences {
    permissions: VecDeque<ResponseAction>,
    questions: VecDeque<ResponseAction>,
    statuses: VecDeque<ResponseAction>,
    messages: VecDeque<ResponseAction>,
}

#[derive(Default)]
struct RequestCounters {
    event: AtomicUsize,
    command: AtomicUsize,
    prompt: AtomicUsize,
    permission_response: AtomicUsize,
    question_response: AtomicUsize,
}

struct Shared {
    session_id: String,
    sequences: Mutex<ResponseSequences>,
    counters: RequestCounters,
    stream_connected: Notify,
    sentinel_released: AtomicBool,
    release_sentinel: Notify,
    permission_response: Notify,
    question_response: Notify,
    command_request: Notify,
    prompt_request: Notify,
    permission_response_status: AtomicU16,
    question_response_status: AtomicU16,
    close_stream_before_ready: AtomicBool,
    events: mpsc::UnboundedSender<Value>,
    event_rx: Mutex<Option<mpsc::UnboundedReceiver<Value>>>,
}

pub struct ReadinessServer {
    base_url: String,
    shared: Arc<Shared>,
    shutdown: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl ReadinessServer {
    pub async fn start(session_id: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("readiness server should bind");
        let address = listener.local_addr().expect("listener should have address");
        let (events, event_rx) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            session_id: session_id.to_string(),
            sequences: Mutex::new(ResponseSequences::default()),
            counters: RequestCounters::default(),
            stream_connected: Notify::new(),
            sentinel_released: AtomicBool::new(false),
            release_sentinel: Notify::new(),
            permission_response: Notify::new(),
            question_response: Notify::new(),
            command_request: Notify::new(),
            prompt_request: Notify::new(),
            permission_response_status: AtomicU16::new(200),
            question_response_status: AtomicU16::new(200),
            close_stream_before_ready: AtomicBool::new(false),
            events,
            event_rx: Mutex::new(Some(event_rx)),
        });
        let (shutdown, mut shutdown_rx) = watch::channel(false);
        let task_shared = Arc::clone(&shared);
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    changed = shutdown_rx.changed() => {
                        if changed.is_err() || *shutdown_rx.borrow() {
                            break;
                        }
                    }
                    accepted = listener.accept() => {
                        let (stream, _) = accepted.expect("readiness server should accept connections");
                        let connection_shared = Arc::clone(&task_shared);
                        let connection_shutdown = shutdown_rx.clone();
                        tokio::spawn(async move {
                            handle_connection(stream, connection_shared, connection_shutdown).await;
                        });
                    }
                }
            }
        });

        Self {
            base_url: format!("http://{address}"),
            shared,
            shutdown,
            task,
        }
    }

    pub fn orchestrator_server(&self) -> Arc<OrchestratorServerHandle> {
        let client = opencode_rs::Client::builder()
            .base_url(&self.base_url)
            .directory("/tmp")
            .timeout_secs(30)
            .build()
            .expect("readiness test client should build");
        Arc::new(OrchestratorServerHandle::from_server_unshared(
            OrchestratorServer::from_client_unshared_with_config(
                client,
                self.base_url.clone(),
                RecoveryMode::External,
                OrchestratorConfig::default(),
            ),
        ))
    }

    pub fn push_permissions(&self, action: ResponseAction) {
        self.shared
            .sequences
            .lock()
            .unwrap()
            .permissions
            .push_back(action);
    }

    pub fn push_questions(&self, action: ResponseAction) {
        self.shared
            .sequences
            .lock()
            .unwrap()
            .questions
            .push_back(action);
    }

    pub fn push_status(&self, action: ResponseAction) {
        self.shared
            .sequences
            .lock()
            .unwrap()
            .statuses
            .push_back(action);
    }

    pub fn push_messages(&self, action: ResponseAction) {
        self.shared
            .sequences
            .lock()
            .unwrap()
            .messages
            .push_back(action);
    }

    pub fn set_permission_response_status(&self, status: u16) {
        self.shared
            .permission_response_status
            .store(status, Ordering::SeqCst);
    }

    pub fn set_question_response_status(&self, status: u16) {
        self.shared
            .question_response_status
            .store(status, Ordering::SeqCst);
    }

    pub fn close_stream_before_ready(&self) {
        self.shared
            .close_stream_before_ready
            .store(true, Ordering::SeqCst);
        self.release_readiness();
    }

    pub async fn wait_stream_connected(&self) {
        wait_for_counter(&self.shared.counters.event, &self.shared.stream_connected).await;
    }

    pub fn release_readiness(&self) {
        self.shared.sentinel_released.store(true, Ordering::SeqCst);
        self.shared.release_sentinel.notify_waiters();
    }

    pub fn append_event(&self, event: Value) {
        self.shared
            .events
            .send(event)
            .expect("SSE connection should remain active");
    }

    pub fn command_count(&self) -> usize {
        self.shared.counters.command.load(Ordering::SeqCst)
    }

    pub fn prompt_count(&self) -> usize {
        self.shared.counters.prompt.load(Ordering::SeqCst)
    }

    pub fn permission_response_count(&self) -> usize {
        self.shared
            .counters
            .permission_response
            .load(Ordering::SeqCst)
    }

    pub fn question_response_count(&self) -> usize {
        self.shared
            .counters
            .question_response
            .load(Ordering::SeqCst)
    }

    pub async fn wait_command(&self) {
        wait_for_counter(&self.shared.counters.command, &self.shared.command_request).await;
    }

    pub async fn wait_prompt(&self) {
        wait_for_counter(&self.shared.counters.prompt, &self.shared.prompt_request).await;
    }

    pub async fn wait_permission_response(&self) {
        wait_for_counter(
            &self.shared.counters.permission_response,
            &self.shared.permission_response,
        )
        .await;
    }

    pub async fn wait_question_response(&self) {
        wait_for_counter(
            &self.shared.counters.question_response,
            &self.shared.question_response,
        )
        .await;
    }
}

impl Drop for ReadinessServer {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
        self.task.abort();
    }
}

async fn wait_for_counter(counter: &AtomicUsize, notification: &Notify) {
    timeout(DEADLOCK_GUARD, async {
        loop {
            let notified = notification.notified();
            if counter.load(Ordering::SeqCst) > 0 {
                return;
            }
            notified.await;
        }
    })
    .await
    .expect("request notification should not deadlock");
}

async fn handle_connection(
    mut stream: TcpStream,
    shared: Arc<Shared>,
    shutdown: watch::Receiver<bool>,
) {
    let Some((method, path)) = read_request(&mut stream).await else {
        return;
    };
    if method == "GET" && path == "/event" {
        handle_event_stream(stream, shared, shutdown).await;
        return;
    }

    let action = route_request(&method, &path, &shared);
    match action {
        ResponseAction::Json { status, body } => write_json(&mut stream, status, &body).await,
        ResponseAction::Close => {}
    }
}

async fn read_request(stream: &mut TcpStream) -> Option<(String, String)> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 2048];
    loop {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        request.extend_from_slice(&chunk[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let request = String::from_utf8(request).ok()?;
    let mut parts = request.lines().next()?.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.split('?').next()?.to_string();
    Some((method, path))
}

fn route_request(method: &str, path: &str, shared: &Shared) -> ResponseAction {
    if method == "GET" && path == "/global/health" {
        return ResponseAction::json(200, serde_json::json!({"healthy": true, "version": "test"}));
    }
    if method == "GET" && path == format!("/session/{}", shared.session_id) {
        return ResponseAction::json(
            200,
            serde_json::json!({
                "id": shared.session_id,
                "slug": shared.session_id,
                "projectId": "project-1",
                "directory": "/tmp",
                "title": "Readiness test",
                "version": "test",
                "time": {"created": 1, "updated": 1}
            }),
        );
    }
    if method == "GET" && path == "/permission" {
        return next_action(
            &mut shared.sequences.lock().unwrap().permissions,
            Value::Array(vec![]),
        );
    }
    if method == "GET" && path == "/question" {
        return next_action(
            &mut shared.sequences.lock().unwrap().questions,
            Value::Array(vec![]),
        );
    }
    if method == "GET" && path == "/session/status" {
        return next_action(
            &mut shared.sequences.lock().unwrap().statuses,
            serde_json::json!({}),
        );
    }
    if method == "GET" && path.ends_with("/message") {
        return next_action(
            &mut shared.sequences.lock().unwrap().messages,
            Value::Array(vec![]),
        );
    }
    if method == "POST" && path.ends_with("/command") {
        shared.counters.command.fetch_add(1, Ordering::SeqCst);
        shared.command_request.notify_waiters();
        return ResponseAction::json(200, serde_json::json!({}));
    }
    if method == "POST" && path.ends_with("/prompt_async") {
        shared.counters.prompt.fetch_add(1, Ordering::SeqCst);
        shared.prompt_request.notify_waiters();
        return ResponseAction::json(204, Value::Null);
    }
    if method == "POST" && path.starts_with("/permission/") && path.ends_with("/reply") {
        shared
            .counters
            .permission_response
            .fetch_add(1, Ordering::SeqCst);
        shared.permission_response.notify_waiters();
        let status = shared.permission_response_status.load(Ordering::SeqCst);
        return ResponseAction::json(status, serde_json::json!(status < 400));
    }
    if method == "POST"
        && path.starts_with("/question/")
        && (path.ends_with("/reply") || path.ends_with("/reject"))
    {
        shared
            .counters
            .question_response
            .fetch_add(1, Ordering::SeqCst);
        shared.question_response.notify_waiters();
        let status = shared.question_response_status.load(Ordering::SeqCst);
        return ResponseAction::json(status, serde_json::json!(status < 400));
    }
    ResponseAction::json(404, serde_json::json!({"message": "not found"}))
}

fn next_action(queue: &mut VecDeque<ResponseAction>, default_body: Value) -> ResponseAction {
    queue
        .pop_front()
        .unwrap_or_else(|| ResponseAction::json(200, default_body))
}

async fn write_json(stream: &mut TcpStream, status: u16, body: &Value) {
    let body = if status == 204 {
        String::new()
    } else {
        body.to_string()
    };
    let reason = match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Test Response",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(response.as_bytes())
        .await
        .expect("JSON response should be writable");
}

async fn handle_event_stream(
    mut stream: TcpStream,
    shared: Arc<Shared>,
    mut shutdown: watch::Receiver<bool>,
) {
    shared.counters.event.fetch_add(1, Ordering::SeqCst);
    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncache-control: no-cache\r\nconnection: close\r\n\r\n",
        )
        .await
        .expect("SSE headers should be writable");
    stream.flush().await.expect("SSE headers should flush");
    shared.stream_connected.notify_waiters();

    while !shared.sentinel_released.load(Ordering::SeqCst) {
        tokio::select! {
            () = shared.release_sentinel.notified() => {}
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return;
                }
            }
        }
    }
    if shared.close_stream_before_ready.load(Ordering::SeqCst) {
        return;
    }
    write_sse_event(
        &mut stream,
        &serde_json::json!({"type": "server.connected", "properties": {}}),
    )
    .await;

    let mut events = shared
        .event_rx
        .lock()
        .unwrap()
        .take()
        .expect("only one active SSE stream is expected");
    loop {
        tokio::select! {
            event = events.recv() => {
                let Some(event) = event else {
                    return;
                };
                write_sse_event(&mut stream, &event).await;
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    return;
                }
            }
        }
    }
}

async fn write_sse_event(stream: &mut TcpStream, event: &Value) {
    stream
        .write_all(format!("data: {event}\n\n").as_bytes())
        .await
        .expect("SSE event should be writable");
    stream.flush().await.expect("SSE event should flush");
}
