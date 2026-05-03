use agentic_config::types::ReasoningConfig;
use agentic_tools_core::ToolContext;
use agentic_tools_core::ToolError;
use gpt5_reasoner::PromptType;
use gpt5_reasoner::gpt5_reasoner_impl;
use serial_test::serial;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use wiremock::Match;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::Request;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

const PARTIAL_REASONING_MARKER: &str = "> **Warning:** Partial response (executor stream interrupted). Content below may be incomplete.\n\n";
const PARTIAL_PLAN_MARKER: &str = "**WARNING: INCOMPLETE PLAN**\nThe plan below may be incomplete because the executor stream ended unexpectedly.\n\n---\n\n";

#[derive(Debug)]
struct ModelMatcher(&'static str);

impl Match for ModelMatcher {
    fn matches(&self, request: &Request) -> bool {
        serde_json::from_slice::<serde_json::Value>(&request.body)
            .ok()
            .and_then(|body| {
                body.get("model")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .as_deref()
            == Some(self.0)
    }
}

struct EnvVarGuard {
    key: String,
    prev: Option<String>,
}

impl EnvVarGuard {
    fn set(key: &str, value: &str) -> Self {
        let prev = std::env::var(key).ok();
        unsafe { std::env::set_var(key, value) };
        Self {
            key: key.to_string(),
            prev,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.prev {
            Some(prev) => unsafe { std::env::set_var(&self.key, prev) },
            None => unsafe { std::env::remove_var(&self.key) },
        }
    }
}

struct CwdGuard {
    prev: PathBuf,
}

impl CwdGuard {
    fn set(path: &Path) -> Self {
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(path).unwrap();
        Self { prev }
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.prev).unwrap();
    }
}

fn optimizer_response_body() -> String {
    let content = r#"
FILE_GROUPING
```yaml
file_groups:
  - name: implementation_targets
    files: []
```

OPTIMIZED_TEMPLATE
```xml
<context>
  <!-- GROUP: implementation_targets -->
</context>
```
"#;

    format!(
        r#"{{
  "id":"chatcmpl-opt",
  "object":"chat.completion",
  "created":0,
  "model":"opt",
  "choices":[{{"index":0,"message":{{"role":"assistant","content":{content_json}}},"finish_reason":"stop"}}]
}}"#,
        content_json = serde_json::to_string(content).unwrap()
    )
}

fn sse(lines: &[&str]) -> String {
    lines.join("\n")
}

fn base_cfg(server: &MockServer) -> ReasoningConfig {
    base_cfg_from_uri(format!("{}/api/v1", server.uri()))
}

fn base_cfg_from_uri(api_base_url: String) -> ReasoningConfig {
    ReasoningConfig {
        api_base_url: Some(api_base_url),
        optimizer_model: "opt".into(),
        executor_model: "exec".into(),
        executor_timeout_secs: 5,
        stream_heartbeat_secs: 1,
        ..ReasoningConfig::default()
    }
}

fn mount_optimizer_mock(server: &MockServer) -> impl std::future::Future<Output = ()> + '_ {
    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(ModelMatcher("opt"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(optimizer_response_body(), "application/json"),
        )
        .mount(server)
}

fn git_ok(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed:\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn setup_plan_write_env(filename: &str) -> (TempDir, EnvVarGuard, CwdGuard, PathBuf, String) {
    let temp = TempDir::new().unwrap();
    let code_repo = temp.path().join("code_repo");
    let thoughts_repo = temp.path().join("thoughts_repo");
    let xdg_config_home = temp.path().join("xdg");
    std::fs::create_dir_all(&code_repo).unwrap();
    std::fs::create_dir_all(&thoughts_repo).unwrap();
    std::fs::create_dir_all(xdg_config_home.join("agentic")).unwrap();

    git_ok(&code_repo, &["init"]);
    git_ok(&code_repo, &["config", "user.email", "test@example.com"]);
    git_ok(&code_repo, &["config", "user.name", "Test User"]);
    std::fs::write(code_repo.join("README.md"), "test repo\n").unwrap();
    git_ok(&code_repo, &["add", "README.md"]);
    git_ok(&code_repo, &["commit", "-m", "init"]);
    git_ok(&code_repo, &["checkout", "-b", "feature-streaming-test"]);

    std::fs::create_dir_all(code_repo.join(".thoughts")).unwrap();
    std::fs::write(
        code_repo.join(".thoughts/config.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "version": "2.0",
            "mount_dirs": {},
            "context_mounts": [],
            "references": [],
            "thoughts_mount": {
                "remote": "https://github.com/example/thoughts.git",
                "sync": "auto"
            }
        }))
        .unwrap(),
    )
    .unwrap();

    std::fs::write(
        xdg_config_home.join("agentic/repos.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "version": "1.0",
            "mappings": {
                "https://github.com/example/thoughts.git": {
                    "path": thoughts_repo.clone(),
                    "auto_managed": false
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();

    let xdg_guard = EnvVarGuard::set("XDG_CONFIG_HOME", xdg_config_home.to_str().unwrap());
    let cwd_guard = CwdGuard::set(&code_repo);

    let expected_relative = "./thoughts/feature-streaming-test/plans/".to_string() + filename;
    let expected_absolute = thoughts_repo
        .join("feature-streaming-test")
        .join("plans")
        .join(filename);

    (
        temp,
        xdg_guard,
        cwd_guard,
        expected_absolute,
        expected_relative,
    )
}

async fn received_model_requests(server: &MockServer, model: &str) -> Vec<serde_json::Value> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter_map(|request| serde_json::from_slice::<serde_json::Value>(&request.body).ok())
        .filter(|body| body.get("model").and_then(serde_json::Value::as_str) == Some(model))
        .collect()
}

struct RawChatServer {
    uri: String,
    task: JoinHandle<()>,
    requests: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl RawChatServer {
    async fn start_partial_failure(first_event: String) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let requests_for_task = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let requests = Arc::clone(&requests_for_task);
                let first_event = first_event.clone();
                tokio::spawn(async move {
                    let body = read_request_json(&mut socket).await.unwrap();
                    requests.lock().unwrap().push(body.clone());
                    match body.get("model").and_then(serde_json::Value::as_str) {
                        Some("opt") => {
                            write_json_response(&mut socket, &optimizer_response_body())
                                .await
                                .unwrap();
                        }
                        Some("exec") => {
                            write_partial_failure_stream(&mut socket, &first_event)
                                .await
                                .unwrap();
                        }
                        other => panic!("unexpected model in raw server: {other:?}"),
                    }
                });
            }
        });

        Self {
            uri: format!("http://{addr}"),
            task,
            requests,
        }
    }

    fn api_base_url(&self) -> String {
        format!("{}/api/v1", self.uri)
    }

    fn received_model_requests(&self, model: &str) -> Vec<serde_json::Value> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|body| body.get("model").and_then(serde_json::Value::as_str) == Some(model))
            .cloned()
            .collect()
    }
}

impl Drop for RawChatServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_request_json(
    stream: &mut tokio::net::TcpStream,
) -> std::io::Result<serde_json::Value> {
    let mut buf = Vec::new();
    let mut scratch = [0_u8; 4096];

    loop {
        let n = stream.read(&mut scratch).await?;
        if n == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "request ended before headers completed",
            ));
        }
        buf.extend_from_slice(&scratch[..n]);

        if let Some(headers_end) = buf.windows(4).position(|window| window == b"\r\n\r\n") {
            let body_start = headers_end + 4;
            let headers = std::str::from_utf8(&buf[..body_start]).unwrap();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length: ")
                        .or_else(|| line.strip_prefix("content-length: "))
                })
                .unwrap()
                .trim()
                .parse::<usize>()
                .unwrap();

            while buf.len() < body_start + content_length {
                let n = stream.read(&mut scratch).await?;
                if n == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "request ended before body completed",
                    ));
                }
                buf.extend_from_slice(&scratch[..n]);
            }

            return Ok(
                serde_json::from_slice(&buf[body_start..body_start + content_length]).unwrap(),
            );
        }
    }
}

async fn write_json_response(
    stream: &mut tokio::net::TcpStream,
    body: &str,
) -> std::io::Result<()> {
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(headers.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.flush().await
}

async fn write_chunk(stream: &mut tokio::net::TcpStream, bytes: &[u8]) -> std::io::Result<()> {
    let header = format!("{:X}\r\n", bytes.len());
    stream.write_all(header.as_bytes()).await?;
    stream.write_all(bytes).await?;
    stream.write_all(b"\r\n").await?;
    stream.flush().await
}

async fn write_partial_failure_stream(
    stream: &mut tokio::net::TcpStream,
    first_event: &str,
) -> std::io::Result<()> {
    stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
        )
        .await?;
    write_chunk(stream, first_event.as_bytes()).await?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    stream.write_all(b"Z\r\n").await?;
    stream.flush().await?;
    stream.shutdown().await
}

#[tokio::test]
#[serial(env)]
async fn executor_stream_success_accumulates_and_uses_usage() {
    let server = MockServer::start().await;
    let _api_key = EnvVarGuard::set("OPENROUTER_API_KEY", "test");
    mount_optimizer_mock(&server).await;

    let sse_body = sse(&[
        r#"data: {"id":"chatcmpl-exec","object":"chat.completion.chunk","created":0,"model":"exec","choices":[{"index":0,"delta":{"content":"Hello "}}],"usage":null}"#,
        "",
        r#"data: {"id":"chatcmpl-exec","object":"chat.completion.chunk","created":0,"model":"exec","choices":[{"index":0,"delta":{"content":"world"}}],"usage":null}"#,
        "",
        r#"data: {"id":"chatcmpl-exec","object":"chat.completion.chunk","created":0,"model":"exec","choices":[],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,"completion_tokens_details":{"reasoning_tokens":1}}}"#,
        "",
        "data: [DONE]",
        "",
    ]);

    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(ModelMatcher("exec"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_raw(sse_body, "text/event-stream"),
        )
        .mount(&server)
        .await;

    let cfg = base_cfg(&server);
    let out = gpt5_reasoner_impl(
        "ignored".into(),
        vec![],
        None,
        &cfg,
        PromptType::Reasoning,
        None,
        &ToolContext::default(),
    )
    .await
    .unwrap();

    assert_eq!(out, "Hello world");

    let exec_requests = received_model_requests(&server, "exec").await;
    assert_eq!(exec_requests.len(), 1);
    assert_eq!(
        exec_requests[0]["stream_options"]["include_usage"].as_bool(),
        Some(true)
    );
}

#[tokio::test]
#[serial(env)]
async fn executor_stream_error_after_content_salvages_reasoning_with_prepend_marker() {
    let server = RawChatServer::start_partial_failure(
        sse(&[
            r#"data: {"id":"chatcmpl-exec","object":"chat.completion.chunk","created":0,"model":"exec","choices":[{"index":0,"delta":{"content":"Partial answer"}}],"usage":null}"#,
            "",
            "",
        ]),
    )
    .await;
    let _api_key = EnvVarGuard::set("OPENROUTER_API_KEY", "test");
    let cfg = base_cfg_from_uri(server.api_base_url());
    let out = gpt5_reasoner_impl(
        "ignored".into(),
        vec![],
        None,
        &cfg,
        PromptType::Reasoning,
        None,
        &ToolContext::default(),
    )
    .await
    .unwrap();

    assert!(out.starts_with(PARTIAL_REASONING_MARKER));
    assert!(out.contains("Partial answer"));
    assert!(!out.contains("StreamError"));
    assert!(!out.contains("executor_timeout"));
    assert_eq!(server.received_model_requests("exec").len(), 1);
}

#[tokio::test]
#[serial(env)]
async fn plan_mode_output_filename_stream_error_writes_partial_file_and_returns_path() {
    let server = RawChatServer::start_partial_failure(
        sse(&[
            r##"data: {"id":"chatcmpl-exec","object":"chat.completion.chunk","created":0,"model":"exec","choices":[{"index":0,"delta":{"content":"# Partial plan\n"}}],"usage":null}"##,
            "",
            "",
        ]),
    )
    .await;
    let _api_key = EnvVarGuard::set("OPENROUTER_API_KEY", "test");
    let (_temp, _xdg, _cwd, expected_absolute, expected_relative) =
        setup_plan_write_env("partial_plan.md");
    let cfg = base_cfg_from_uri(server.api_base_url());
    let out = gpt5_reasoner_impl(
        "ignored".into(),
        vec![],
        None,
        &cfg,
        PromptType::Plan,
        Some("partial_plan.md".into()),
        &ToolContext::default(),
    )
    .await
    .unwrap();

    assert_eq!(out, expected_relative);
    assert!(expected_absolute.exists());
    let written = std::fs::read_to_string(expected_absolute).unwrap();
    assert!(written.starts_with(PARTIAL_PLAN_MARKER));
    assert!(written.contains("# Partial plan"));
    assert!(!written.contains("StreamError"));
    assert_eq!(server.received_model_requests("exec").len(), 1);
}

#[tokio::test]
#[serial(env)]
async fn cancellation_while_waiting_for_stream_next_returns_cancelled_no_salvage() {
    let server = MockServer::start().await;
    let _api_key = EnvVarGuard::set("OPENROUTER_API_KEY", "test");
    let (_temp, _xdg, _cwd, expected_absolute, _expected_relative) =
        setup_plan_write_env("cancelled_plan.md");
    mount_optimizer_mock(&server).await;

    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(ModelMatcher("exec"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(30)),
        )
        .mount(&server)
        .await;

    let cfg = base_cfg(&server);
    let ctx = ToolContext::default();
    let cancel = ctx.cancellation_token();
    let task = tokio::spawn({
        let ctx = ctx.clone();
        let cfg = cfg.clone();
        async move {
            gpt5_reasoner_impl(
                "ignored".into(),
                vec![],
                None,
                &cfg,
                PromptType::Plan,
                Some("cancelled_plan.md".into()),
                &ctx,
            )
            .await
        }
    });

    tokio::time::sleep(Duration::from_millis(150)).await;
    cancel.cancel();

    let err = task.await.unwrap().unwrap_err();
    assert!(matches!(err, ToolError::Cancelled { .. }));
    assert!(!expected_absolute.exists());
}

#[tokio::test]
#[serial(env)]
async fn empty_clean_completion_after_long_attempt_does_not_retry() {
    let server = MockServer::start().await;
    let _api_key = EnvVarGuard::set("OPENROUTER_API_KEY", "test");
    mount_optimizer_mock(&server).await;

    let done_only = sse(&["data: [DONE]", ""]);

    Mock::given(method("POST"))
        .and(path("/api/v1/chat/completions"))
        .and(ModelMatcher("exec"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_delay(Duration::from_secs(1))
                .set_body_raw(done_only, "text/event-stream"),
        )
        .mount(&server)
        .await;

    let mut cfg = base_cfg(&server);
    cfg.empty_response_no_retry_after_secs = 0;

    let err = gpt5_reasoner_impl(
        "ignored".into(),
        vec![],
        None,
        &cfg,
        PromptType::Reasoning,
        None,
        &ToolContext::default(),
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("retry suppressed"));

    let exec_requests = received_model_requests(&server, "exec").await;
    assert_eq!(exec_requests.len(), 1);
}
