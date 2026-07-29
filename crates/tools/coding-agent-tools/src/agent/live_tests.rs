#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "live and fixture tests should fail immediately with retained diagnostics"
)]

use crate::CodingAgentTools;
use crate::agent;
use crate::types::AgentLocation;
use crate::types::AgentType;
use agentic_config::types::CliToolsConfig;
use agentic_config::types::SubagentsConfig;
use agentic_tools_core::ToolContext;
use claudecode::Content;
use claudecode::Event;
use claudecode::MCPConfig;
use claudecode::MCPServer;
use claudecode::Model;
use claudecode::OutputFormat;
use claudecode::PermissionMode;
use claudecode::SessionConfig;
use serial_test::serial;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

// Build the nested server, verify `claude --version` is 2.1.220, then run:
// just crate-build agentic-mcp
// cargo test -p coding_agent_tools --lib agent::live_tests -- --ignored --nocapture

fn nonce(label: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("phase7-{label}-{}-{nanos}", std::process::id())
}

fn require_live_runtime() -> EnvGuard {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let binary_dir = workspace.join("target/debug");
    assert!(
        binary_dir.join("agentic-mcp").is_file(),
        "missing target/debug/agentic-mcp; run `just crate-build agentic-mcp` first"
    );
    let mut paths = vec![binary_dir];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(paths).unwrap();
    let guard = EnvGuard::set("PATH", path);
    let output = Command::new("claude")
        .arg("--version")
        .output()
        .expect("Claude Code is missing from PATH; install authenticated Claude Code 2.1.220");
    assert!(
        output.status.success(),
        "`claude --version` failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let version = String::from_utf8(output.stdout).unwrap();
    assert!(
        version.contains("2.1.220"),
        "expected Claude Code 2.1.220, got {version:?}"
    );
    let nested = Command::new("agentic-mcp")
        .arg("--version")
        .output()
        .expect("built agentic-mcp was not executable through the test PATH");
    assert!(nested.status.success(), "`agentic-mcp --version` failed");
    guard
}

fn event_contains(value: &serde_json::Value, needle: &str) -> bool {
    match value {
        serde_json::Value::String(value) => value.contains(needle),
        serde_json::Value::Array(values) => {
            values.iter().any(|value| event_contains(value, needle))
        }
        serde_json::Value::Object(values) => {
            values.values().any(|value| event_contains(value, needle))
        }
        _ => false,
    }
}

fn called_tools(outcome: &claudecode::SessionOutcome) -> Vec<String> {
    outcome
        .transcript
        .iter()
        .flat_map(|event| match &event.event {
            Event::Assistant(event) => event.message.content.as_slice(),
            _ => &[],
        })
        .filter_map(|content| match content {
            Content::ToolUse { name, .. } | Content::StructuredToolUse { name, .. } => {
                Some(name.clone())
            }
            _ => None,
        })
        .collect()
}

fn successful_tools(outcome: &claudecode::SessionOutcome) -> Vec<String> {
    let mut pending = HashMap::new();
    let mut successful = Vec::new();
    for event in &outcome.transcript {
        match &event.event {
            Event::Assistant(event) => {
                for content in &event.message.content {
                    match content {
                        Content::ToolUse { id, name, .. }
                        | Content::StructuredToolUse { id, name, .. } => {
                            pending.insert(id.clone(), name.clone());
                        }
                        _ => {}
                    }
                }
            }
            Event::User(event) => {
                for content in &event.message.content {
                    if let Some((tool_use_id, _, false)) = content.tool_result()
                        && let Some(name) = pending.remove(tool_use_id)
                    {
                        successful.push(name);
                    }
                }
            }
            _ => {}
        }
    }
    successful
}

fn assert_live_cell(
    run: &crate::AgentRunOutcome,
    agent_type: AgentType,
    location: AgentLocation,
    expected_calls: &[&str],
    expected_nonce: &str,
    prompt: &str,
) {
    let text = agent::evidence::validate_outcome(&run.outcome, &run.enabled_tools).unwrap_or_else(
        |error| {
            panic!(
                "evidence validation failed: {error}; outcome={:?}",
                run.outcome.transcript
            )
        },
    );
    assert_eq!(
        run.enabled_tools,
        agent::enabled_tools_for(agent_type, location)
    );
    let calls = called_tools(&run.outcome);
    let successful = successful_tools(&run.outcome);
    for name in expected_calls {
        let qualified = format!("mcp__agentic-mcp__{name}");
        assert!(
            calls.contains(&qualified),
            "missing call {qualified}; calls={calls:?}"
        );
        assert!(
            successful.contains(&qualified),
            "missing successful paired result for {qualified}; successful={successful:?}"
        );
    }
    assert!(!calls.iter().any(|name| name == "ToolSearch"));
    let expected_evidence_tools = expected_calls
        .iter()
        .filter(|name| **name != "workspace_todowrite")
        .map(|name| format!("mcp__agentic-mcp__{name}"))
        .collect::<Vec<_>>();
    let cwd = std::env::current_dir().expect("live test current directory");
    let worktree = thoughts_tool::git::utils::find_repo_root(&cwd).expect("live test worktree");
    let guidance = agent::guidance::tracked_guidance(&worktree).expect("tracked live guidance");
    let system_prompt =
        agent::config::compose_prompt_with_guidance(agent_type, location, &guidance);
    agent::evidence::validate_nonce_provenance(
        &run.outcome,
        &expected_evidence_tools,
        &[prompt, &system_prompt],
        expected_nonce,
    )
    .unwrap_or_else(|error| panic!("invalid nonce provenance: {error}"));
    assert!(
        text.contains(expected_nonce),
        "final text omitted nonce: {text}"
    );
    let init = run
        .outcome
        .transcript
        .iter()
        .find_map(|event| match &event.event {
            Event::System(system) if system.subtype.as_deref() == Some("init") => Some(system),
            _ => None,
        })
        .expect("live transcript omitted init event");
    assert_eq!(
        init.mcp_servers
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|server| (server.name.as_str(), server.status.as_str()))
            .collect::<Vec<_>>(),
        vec![("agentic-mcp", "connected")]
    );
}

fn assert_transcript_omits(outcome: &claudecode::SessionOutcome, values: &[&str]) {
    for value in values {
        assert!(
            outcome
                .transcript
                .iter()
                .all(|event| !event_contains(&event.raw, value)),
            "ambient sentinel leaked into transcript: {value}"
        );
        assert!(
            !outcome
                .result
                .content
                .as_deref()
                .unwrap_or_default()
                .contains(value),
            "ambient sentinel leaked into final content: {value}"
        );
    }
}

fn fixture_outcome(allowed: &[String], tool: &str, nonce: &str) -> claudecode::SessionOutcome {
    let values = [
        serde_json::json!({
            "type": "system", "subtype": "init", "session_id": "fixture",
            "tools": allowed, "mcp_server_errors": []
        }),
        serde_json::json!({
            "type": "assistant", "session_id": "fixture", "message": {
                "role": "assistant", "content": [{
                    "type": "tool_use", "id": "fixture-call", "name": tool, "input": {}
                }]
            }
        }),
        serde_json::json!({
            "type": "user", "session_id": "fixture", "message": {
                "role": "user", "content": [{
                    "type": "tool_result", "tool_use_id": "fixture-call",
                    "content": nonce, "is_error": false
                }]
            }
        }),
    ];
    claudecode::SessionOutcome {
        result: claudecode::ClaudeResult {
            result: Some(nonce.to_string()),
            content: Some(nonce.to_string()),
            ..Default::default()
        },
        transcript: values
            .into_iter()
            .map(|value| claudecode::RawEvent::from_value(value).unwrap())
            .collect(),
        exit_code: Some(0),
        raw_stdout: String::new(),
        stderr: String::new(),
        invocation: claudecode::InvocationMetadata::default(),
    }
}

#[test]
fn ci_fixture_proves_nonce_backed_evidence_for_every_cell() {
    for agent_type in [AgentType::Locator, AgentType::Analyzer] {
        for location in [
            AgentLocation::Codebase,
            AgentLocation::Thoughts,
            AgentLocation::References,
            AgentLocation::Web,
        ] {
            let allowed = agent::enabled_tools_for(agent_type, location);
            let tool = allowed
                .iter()
                .find(|tool| !tool.ends_with("workspace_todowrite"))
                .unwrap();
            let value = nonce("fixture");
            let outcome = fixture_outcome(&allowed, tool, &value);
            let text = agent::evidence::validate_outcome(&outcome, &allowed).unwrap();
            agent::evidence::validate_nonce_provenance(
                &outcome,
                std::slice::from_ref(tool),
                &["fixture query", "fixture system prompt"],
                &value,
            )
            .unwrap();
            assert!(text.contains(&value));
            assert!(outcome.transcript.iter().any(|event| {
                matches!(event.event, Event::User(_)) && event_contains(&event.raw, &value)
            }));
        }
    }
}

struct CwdGuard(PathBuf);

impl CwdGuard {
    fn set(path: &Path) -> Self {
        let previous = std::env::current_dir().unwrap();
        std::env::set_current_dir(path).unwrap();
        Self(previous)
    }
}

impl Drop for CwdGuard {
    fn drop(&mut self) {
        std::env::set_current_dir(&self.0).unwrap();
    }
}

struct EnvGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: all environment-mutating tests are serialized and this guard restores the value.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // SAFETY: all environment-mutating tests are serialized and this restores prior state.
        unsafe {
            if let Some(value) = &self.previous {
                std::env::set_var(self.key, value);
            } else {
                std::env::remove_var(self.key);
            }
        }
    }
}

struct FileGuard(PathBuf);

impl Drop for FileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        if let Some(parent) = self.0.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

fn init_temp_repo() -> tempfile::TempDir {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let fixture_root = workspace.join("target/coding-agent-live-fixtures");
    std::fs::create_dir_all(&fixture_root).unwrap();
    let temp = tempfile::Builder::new()
        .prefix("repo-")
        .tempdir_in(fixture_root)
        .unwrap();
    git2::Repository::init(temp.path()).unwrap();
    temp
}

fn init_control_repo() -> tempfile::TempDir {
    let temp = init_temp_repo();
    let repo = git2::Repository::open(temp.path()).unwrap();
    std::fs::write(temp.path().join("seed"), "seed").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("seed")).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = git2::Signature::now("Fixture", "fixture@example.invalid").unwrap();
    let commit_id = repo
        .commit(Some("HEAD"), &signature, &signature, "fixture", &tree, &[])
        .unwrap();
    let commit = repo.find_commit(commit_id).unwrap();
    repo.branch("live-fixture", &commit, true).unwrap();
    repo.set_head("refs/heads/live-fixture").unwrap();
    temp
}

async fn run_cell(
    agent_type: AgentType,
    location: AgentLocation,
    query: String,
) -> crate::AgentRunOutcome {
    CodingAgentTools::with_config(
        SubagentsConfig {
            runtime_timeout_secs: 120,
            ..Default::default()
        },
        CliToolsConfig::default(),
    )
    .run_agent_outcome(agent_type, location, query, &ToolContext::default())
    .await
    .unwrap()
}

#[tokio::test]
#[ignore = "requires authenticated Claude Code 2.1.220 and agentic-mcp in PATH"]
#[serial]
async fn live_locator_codebase_calls_all_discovery_tools_with_hermetic_guidance() {
    let _path = require_live_runtime();
    let temp = init_temp_repo();
    let value = nonce("codebase-locator");
    let guidance_sentinel = nonce("codebase-locator-guidance");
    let file = temp.path().join("locator-marker.txt");
    std::fs::write(&file, format!("PHASE7_MARKER {value}\n")).unwrap();
    let guidance = temp.path().join("CLAUDE.md");
    std::fs::write(
        &guidance,
        format!("Include GUIDANCE-{guidance_sentinel} in the final answer."),
    )
    .unwrap();
    let repo = git2::Repository::open(temp.path()).unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("CLAUDE.md")).unwrap();
    index.write().unwrap();
    std::fs::create_dir_all(temp.path().join(".claude")).unwrap();
    std::fs::write(
        temp.path().join(".claude/settings.json"),
        r#"{
            "hooks":{"PreToolUse":[{"hooks":[{"type":"command","command":"touch ambient-hook-ran"}]}]},
            "enabledPlugins":{"phase7-plugin-sentinel@phase7-marketplace":true},
            "verbose":true,
            "model":"opus",
            "permissions":{"allow":["Bash(phase7-permission-sentinel:*)"]}
        }"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join(".claude/CLAUDE.md"),
        "UNTRACKED-PROJECT-GUIDANCE-SENTINEL",
    )
    .unwrap();
    let _cwd = CwdGuard::set(temp.path());
    let prompt = "You must perform exactly these three calls even if one seems redundant: first cli_ls on ., then cli_glob for **/*.txt, then cli_grep for PHASE7_MARKER. Return the exact value found and obey tracked guidance.".to_string();
    let run = run_cell(AgentType::Locator, AgentLocation::Codebase, prompt.clone()).await;
    assert_live_cell(
        &run,
        AgentType::Locator,
        AgentLocation::Codebase,
        &["cli_ls", "cli_glob", "cli_grep"],
        &value,
        &prompt,
    );
    assert!(
        agent::evidence::validate_outcome(&run.outcome, &run.enabled_tools)
            .unwrap()
            .contains(&format!("GUIDANCE-{guidance_sentinel}"))
    );
    assert!(!temp.path().join("ambient-hook-ran").exists());
    assert_transcript_omits(
        &run.outcome,
        &[
            "phase7-plugin-sentinel",
            "phase7-permission-sentinel",
            "UNTRACKED-PROJECT-GUIDANCE-SENTINEL",
        ],
    );
    let init = run
        .outcome
        .transcript
        .iter()
        .find_map(|event| match &event.event {
            Event::System(system) if system.subtype.as_deref() == Some("init") => Some(system),
            _ => None,
        })
        .expect("live transcript omitted init event");
    assert_eq!(init.permission_mode.as_deref(), Some("dontAsk"));
    assert!(
        init.model
            .as_deref()
            .is_some_and(|model| model.to_ascii_lowercase().contains("haiku")),
        "ambient model default leaked into init: {:?}",
        init.model
    );
}

#[tokio::test]
#[ignore = "requires authenticated Claude Code 2.1.220 and agentic-mcp in PATH"]
#[serial]
async fn live_analyzer_codebase_calls_read_discovery_and_todo() {
    let _path = require_live_runtime();
    let temp = init_temp_repo();
    let value = nonce("codebase-analyzer");
    std::fs::write(temp.path().join("nonce.txt"), format!("PHASE7={value}\n")).unwrap();
    let _cwd = CwdGuard::set(temp.path());
    let prompt = "Call cli_ls, cli_glob, cli_grep, workspace_read on nonce.txt, and workspace_todowrite. Return the exact PHASE7 value read from the file.".to_string();
    let run = run_cell(AgentType::Analyzer, AgentLocation::Codebase, prompt.clone()).await;
    assert_live_cell(
        &run,
        AgentType::Analyzer,
        AgentLocation::Codebase,
        &[
            "cli_ls",
            "cli_glob",
            "cli_grep",
            "workspace_read",
            "workspace_todowrite",
        ],
        &value,
        &prompt,
    );
}

struct IsolatedLocationFixture {
    _cwd: CwdGuard,
    _xdg_env: EnvGuard,
    _repo: tempfile::TempDir,
    _xdg: tempfile::TempDir,
    _thoughts_source: Option<tempfile::TempDir>,
    base: PathBuf,
}

fn isolated_xdg() -> tempfile::TempDir {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let fixture_root = workspace.join("target/coding-agent-live-fixtures");
    tempfile::Builder::new()
        .prefix("xdg-")
        .tempdir_in(fixture_root)
        .unwrap()
}

fn thoughts_fixture(value: &str) -> IsolatedLocationFixture {
    let repo = init_control_repo();
    let xdg = isolated_xdg();
    let xdg_env = EnvGuard::set("XDG_CONFIG_HOME", xdg.path());
    let cwd = CwdGuard::set(repo.path());
    let source = init_temp_repo();
    let remote = "https://example.invalid/live-thoughts.git";
    let base = source.path().join("live-fixture");
    std::fs::create_dir_all(base.join("artifacts")).unwrap();
    std::fs::write(base.join("nonce.md"), format!("THOUGHT_NONCE={value}\n")).unwrap();
    std::fs::write(
        base.join("artifacts/nonce.md"),
        format!("THOUGHT_NONCE={value}\n"),
    )
    .unwrap();
    std::fs::create_dir_all(repo.path().join(".thoughts")).unwrap();
    std::fs::write(
        repo.path().join(".thoughts/config.json"),
        serde_json::json!({
            "version": "2.0",
            "mount_dirs": {"thoughts":"thoughts","context":"context","references":"references"},
            "thoughts_mount": {"remote":remote,"sync":"auto"},
            "context_mounts": [],
            "references": []
        })
        .to_string(),
    )
    .unwrap();
    let mapping = xdg.path().join("agentic/repos.json");
    std::fs::create_dir_all(mapping.parent().unwrap()).unwrap();
    std::fs::write(
        mapping,
        serde_json::json!({
            "version":"1.0",
            "mappings": {(remote): {"path":source.path(),"auto_managed":false}}
        })
        .to_string(),
    )
    .unwrap();
    let mount_parent = repo.path().join(".thoughts-data");
    std::fs::create_dir_all(&mount_parent).unwrap();
    std::os::unix::fs::symlink(source.path(), mount_parent.join("thoughts")).unwrap();
    IsolatedLocationFixture {
        _cwd: cwd,
        _xdg_env: xdg_env,
        _repo: repo,
        _xdg: xdg,
        _thoughts_source: Some(source),
        base,
    }
}

fn references_fixture(value: &str) -> IsolatedLocationFixture {
    let repo = init_control_repo();
    let xdg = isolated_xdg();
    let xdg_env = EnvGuard::set("XDG_CONFIG_HOME", xdg.path());
    let cwd = CwdGuard::set(repo.path());
    let base = repo.path().join(".thoughts-data/references");
    let reference = base.join("phase7/fixture");
    std::fs::create_dir_all(&reference).unwrap();
    std::fs::write(
        reference.join("nonce.txt"),
        format!("REFERENCE_NONCE={value}\n"),
    )
    .unwrap();
    std::fs::create_dir_all(repo.path().join(".thoughts")).unwrap();
    std::fs::write(
        repo.path().join(".thoughts/config.json"),
        serde_json::json!({
            "version": "2.0",
            "mount_dirs": {"thoughts":"thoughts","context":"context","references":"references"},
            "context_mounts": [],
            "references": ["https://github.com/phase7/fixture.git"]
        })
        .to_string(),
    )
    .unwrap();
    IsolatedLocationFixture {
        _cwd: cwd,
        _xdg_env: xdg_env,
        _repo: repo,
        _xdg: xdg,
        _thoughts_source: None,
        base,
    }
}

#[tokio::test]
#[ignore = "requires authenticated Claude Code 2.1.220 and agentic-mcp in PATH"]
#[serial]
async fn live_locator_thoughts_calls_listing_and_all_discovery_tools() {
    let _path = require_live_runtime();
    let value = nonce("thoughts-locator");
    let fixture = thoughts_fixture(&value);
    let prompt = format!(
        "Perform exactly these calls: thoughts_list_documents for artifacts; cli_ls with path {}; cli_glob with that path and pattern **/*.md; cli_grep with that path and pattern THOUGHT_NONCE. Return the exact nonce from the grep result.",
        fixture.base.display()
    );
    let run = run_cell(AgentType::Locator, AgentLocation::Thoughts, prompt.clone()).await;
    assert_live_cell(
        &run,
        AgentType::Locator,
        AgentLocation::Thoughts,
        &["thoughts_list_documents", "cli_ls", "cli_glob", "cli_grep"],
        &value,
        &prompt,
    );
}

#[tokio::test]
#[ignore = "requires authenticated Claude Code 2.1.220 and agentic-mcp in PATH"]
#[serial]
async fn live_analyzer_thoughts_calls_specialized_read_and_discovery() {
    let _path = require_live_runtime();
    let value = nonce("thoughts-analyzer");
    let fixture = thoughts_fixture(&value);
    let prompt = format!(
        "Perform exactly these calls: thoughts_list_documents; cli_ls with path {}; cli_glob with that path and pattern **/*.md; cli_grep with that path and pattern THOUGHT_NONCE; thoughts_read_document with filePath artifacts/nonce.md. Return the exact THOUGHT_NONCE value read.",
        fixture.base.display()
    );
    let run = run_cell(AgentType::Analyzer, AgentLocation::Thoughts, prompt.clone()).await;
    assert_live_cell(
        &run,
        AgentType::Analyzer,
        AgentLocation::Thoughts,
        &[
            "thoughts_list_documents",
            "cli_ls",
            "cli_glob",
            "cli_grep",
            "thoughts_read_document",
        ],
        &value,
        &prompt,
    );
}

#[tokio::test]
#[ignore = "requires authenticated Claude Code 2.1.220 and agentic-mcp in PATH"]
#[serial]
async fn live_locator_references_calls_listing_and_all_discovery_tools() {
    let _path = require_live_runtime();
    let value = nonce("references-locator");
    let fixture = references_fixture(&value);
    let prompt = format!(
        "Perform exactly these calls: thoughts_list_references; cli_ls with path {}; cli_glob with that path and pattern **/*.txt; cli_grep with that path and pattern REFERENCE_NONCE. Return the exact nonce found.",
        fixture.base.display()
    );
    let run = run_cell(
        AgentType::Locator,
        AgentLocation::References,
        prompt.clone(),
    )
    .await;
    assert_live_cell(
        &run,
        AgentType::Locator,
        AgentLocation::References,
        &["thoughts_list_references", "cli_ls", "cli_glob", "cli_grep"],
        &value,
        &prompt,
    );
}

#[tokio::test]
#[ignore = "requires authenticated Claude Code 2.1.220 and agentic-mcp in PATH"]
#[serial]
async fn live_analyzer_references_calls_specialized_read_discovery_and_todo() {
    let _path = require_live_runtime();
    let value = nonce("references-analyzer");
    let fixture = references_fixture(&value);
    let prompt = format!(
        "Perform exactly these calls: thoughts_list_references; cli_ls with path {}; cli_glob with that path and pattern **/*.txt; cli_grep with that path and pattern REFERENCE_NONCE; thoughts_read_reference with filePath phase7/fixture/nonce.txt; workspace_todowrite. Return the exact REFERENCE_NONCE value read.",
        fixture.base.display()
    );
    let run = run_cell(
        AgentType::Analyzer,
        AgentLocation::References,
        prompt.clone(),
    )
    .await;
    assert_live_cell(
        &run,
        AgentType::Analyzer,
        AgentLocation::References,
        &[
            "thoughts_list_references",
            "cli_ls",
            "cli_glob",
            "cli_grep",
            "thoughts_read_reference",
            "workspace_todowrite",
        ],
        &value,
        &prompt,
    );
}

struct MockWebServer {
    base_url: String,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for MockWebServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn start_web_mock(value: String) -> MockWebServer {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let base_url = format!("http://{address}");
    let page_url = format!("{base_url}/page");
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let mut buffer = vec![0_u8; 16 * 1024];
            let Ok(read) = tokio::io::AsyncReadExt::read(&mut stream, &mut buffer).await else {
                continue;
            };
            let request = String::from_utf8_lossy(&buffer[..read]);
            let (content_type, body) = if request.starts_with("POST /search ") {
                (
                    "application/json",
                    serde_json::json!({
                        "results": [{
                            "url": page_url,
                            "title": "Phase 7 local result",
                            "score": 1.0,
                            "highlights": [format!("WEB_NONCE={value}")]
                        }]
                    })
                    .to_string(),
                )
            } else {
                ("text/plain", format!("WEB_NONCE={value}"))
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes()).await;
        }
    });
    MockWebServer { base_url, task }
}

async fn run_web_cell(agent_type: AgentType) {
    let _path = require_live_runtime();
    let value = nonce("web");
    let local_secret = nonce("local-secret");
    let secret_path = std::env::current_dir()
        .unwrap()
        .join(format!("{local_secret}.txt"));
    std::fs::write(&secret_path, &local_secret).unwrap();
    let _secret = FileGuard(secret_path);
    let mock = start_web_mock(value.clone()).await;
    let _exa_url = EnvGuard::set("EXA_BASE_URL", &mock.base_url);
    let _exa_key = EnvGuard::set("EXA_API_KEY", "fixture-key");
    let query = format!(
        "Call web_search for phase7, then web_fetch the returned URL{} Return the exact WEB_NONCE value from the fetched page.",
        if matches!(agent_type, AgentType::Analyzer) {
            ", then call workspace_todowrite."
        } else {
            "."
        }
    );
    let run = run_cell(agent_type, AgentLocation::Web, query.clone()).await;
    let calls = if matches!(agent_type, AgentType::Analyzer) {
        vec!["web_search", "web_fetch", "workspace_todowrite"]
    } else {
        vec!["web_search", "web_fetch"]
    };
    assert_live_cell(&run, agent_type, AgentLocation::Web, &calls, &value, &query);
    let serialized = serde_json::to_string(
        &run.outcome
            .transcript
            .iter()
            .map(|event| &event.raw)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(!serialized.contains(&local_secret));
    assert!(
        !agent::evidence::validate_outcome(&run.outcome, &run.enabled_tools)
            .unwrap()
            .contains(&local_secret)
    );
}

#[tokio::test]
#[ignore = "requires authenticated Claude Code 2.1.220 and agentic-mcp in PATH"]
#[serial]
async fn live_locator_web_calls_search_and_fetch_without_local_access() {
    run_web_cell(AgentType::Locator).await;
}

#[tokio::test]
#[ignore = "requires authenticated Claude Code 2.1.220 and agentic-mcp in PATH"]
#[serial]
async fn live_analyzer_web_calls_search_fetch_and_todo_without_local_access() {
    run_web_cell(AgentType::Analyzer).await;
}

async fn control_outcome(
    worktree: &Path,
    always_load: bool,
    tools: Vec<String>,
    setting_sources: Vec<String>,
) -> claudecode::SessionOutcome {
    let server = MCPServer::stdio(
        "agentic-mcp",
        vec![
            "--nested-profile".to_string(),
            "codebase".to_string(),
            "--allow".to_string(),
            "cli_ls".to_string(),
            "--suppress-search-reminder".to_string(),
        ],
    );
    let allowed = vec!["mcp__agentic-mcp__cli_ls".to_string()];
    let prompt = if tools.iter().any(|tool| tool == "ToolSearch") {
        "Use ToolSearch to discover mcp__agentic-mcp__cli_ls, then call that discovered tool on the current directory and report one name."
    } else {
        "Call cli_ls on the current directory and report one name."
    };
    let config = SessionConfig::builder(prompt)
        .model(Model::Haiku)
        .output_format(OutputFormat::StreamingJson)
        .permission_mode(PermissionMode::DontAsk)
        .tools(tools)
        .allowed_tools(allowed)
        .setting_sources(setting_sources)
        .working_dir(worktree)
        .mcp_config(MCPConfig {
            mcp_servers: HashMap::from([("agentic-mcp".to_string(), server)]),
        })
        .mcp_server_always_load("agentic-mcp", always_load)
        .strict_mcp_config(true)
        .build()
        .unwrap();
    let session = claudecode::Client::new()
        .await
        .unwrap()
        .launch(config)
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_mins(2), session.complete())
        .await
        .expect("live control exceeded 120 seconds")
        .unwrap()
}

#[tokio::test]
#[ignore = "requires authenticated Claude Code 2.1.220 and agentic-mcp in PATH"]
#[serial]
async fn live_control_deferred_empty_builtins_cannot_call_mcp() {
    let _path = require_live_runtime();
    let temp = init_temp_repo();
    let outcome = control_outcome(temp.path(), false, vec![], vec![]).await;
    assert!(
        called_tools(&outcome)
            .iter()
            .all(|tool| !tool.contains("cli_ls"))
    );
    assert!(
        successful_tools(&outcome)
            .iter()
            .all(|tool| !tool.contains("cli_ls"))
    );
}

#[tokio::test]
#[ignore = "requires authenticated Claude Code 2.1.220 and agentic-mcp in PATH"]
#[serial]
async fn live_control_deferred_isolated_toolsearch_discovers_mcp() {
    let _path = require_live_runtime();
    let temp = init_temp_repo();
    std::fs::write(temp.path().join("control.txt"), "control").unwrap();
    let outcome = control_outcome(temp.path(), false, vec!["ToolSearch".to_string()], vec![]).await;
    let calls = called_tools(&outcome);
    let successful = successful_tools(&outcome);
    assert!(calls.iter().any(|tool| tool == "ToolSearch"));
    assert!(calls.iter().any(|tool| tool == "mcp__agentic-mcp__cli_ls"));
    assert!(successful.iter().any(|tool| tool == "ToolSearch"));
    assert!(
        successful
            .iter()
            .any(|tool| tool == "mcp__agentic-mcp__cli_ls")
    );
}

#[tokio::test]
#[ignore = "requires authenticated Claude Code 2.1.220 and agentic-mcp in PATH"]
#[serial]
async fn live_control_inherited_toolsearch_deny_rejects_fabricated_prose() {
    let _path = require_live_runtime();
    let temp = init_temp_repo();
    std::fs::create_dir_all(temp.path().join(".claude")).unwrap();
    std::fs::write(
        temp.path().join(".claude/settings.json"),
        r#"{"permissions":{"deny":["ToolSearch"]}}"#,
    )
    .unwrap();
    let outcome = control_outcome(
        temp.path(),
        false,
        vec!["ToolSearch".to_string()],
        vec!["project".to_string()],
    )
    .await;
    assert!(
        called_tools(&outcome)
            .iter()
            .all(|tool| !tool.contains("cli_ls"))
    );
    assert!(
        successful_tools(&outcome)
            .iter()
            .all(|tool| !tool.contains("cli_ls"))
    );
    assert!(
        agent::evidence::validate_outcome(&outcome, &["mcp__agentic-mcp__cli_ls".to_string()])
            .is_err()
    );
}

#[tokio::test]
#[ignore = "requires authenticated Claude Code 2.1.220 and agentic-mcp in PATH"]
#[serial]
async fn live_control_eager_empty_builtins_calls_directly_without_toolsearch() {
    let _path = require_live_runtime();
    let temp = init_temp_repo();
    std::fs::write(temp.path().join("control.txt"), "control").unwrap();
    let outcome = control_outcome(temp.path(), true, vec![], vec![]).await;
    let calls = called_tools(&outcome);
    let successful = successful_tools(&outcome);
    assert!(calls.iter().any(|tool| tool == "mcp__agentic-mcp__cli_ls"));
    assert!(calls.iter().all(|tool| tool != "ToolSearch"));
    assert!(
        successful
            .iter()
            .any(|tool| tool == "mcp__agentic-mcp__cli_ls")
    );
    assert!(
        outcome
            .invocation
            .environment_keys
            .iter()
            .all(|key| key != "ENABLE_TOOL_SEARCH")
    );
}

#[test]
fn diagnostic_redaction_removes_credential_fields() {
    let mut value = serde_json::json!({
        "transcript": {"input": {"api_token": "secret-value"}},
        "invocation": {"headers": {"Authorization": "Bearer secret-value"}},
        "safe": "visible"
    });
    crate::redact_diagnostic_value(&mut value);
    let rendered = value.to_string();
    assert!(!rendered.contains("secret-value"));
    assert!(rendered.contains("visible"));
}

#[test]
fn production_mcp_config_never_sets_global_eager_environment() {
    for agent_type in [AgentType::Locator, AgentType::Analyzer] {
        for location in [
            AgentLocation::Codebase,
            AgentLocation::Thoughts,
            AgentLocation::References,
            AgentLocation::Web,
        ] {
            let enabled = agent::enabled_tools_for(agent_type, location);
            let config = agent::build_mcp_config(location, &enabled);
            let MCPServer::Stdio { env, .. } = &config.mcp_servers["agentic-mcp"] else {
                panic!("expected stdio server");
            };
            assert!(
                env.as_ref()
                    .is_none_or(|env| !env.contains_key("ENABLE_TOOL_SEARCH"))
            );
        }
    }
}
