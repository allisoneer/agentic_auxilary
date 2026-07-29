use claudecode::Client;
use claudecode::MCPConfig;
use claudecode::MCPServer;
use claudecode::Model;
use claudecode::OutputFormat;
use claudecode::SessionConfig;
use std::collections::HashMap;
use std::collections::HashSet;
use std::io::BufRead;
use std::io::Write;
use std::process::Stdio;

#[tokio::test]
async fn test_client_creation() {
    // Skip if claude not available
    if which::which("claude").is_err() {
        eprintln!("Skipping test: claude not found in PATH");
        return;
    }

    let client = Client::new().await;
    assert!(client.is_ok());
}

#[tokio::test]
#[ignore = "requires claude CLI to be installed"]
async fn test_simple_query() {
    if which::which("claude").is_err() {
        return;
    }

    let client = Client::new().await.unwrap();

    let config = SessionConfig::builder("Say 'Hello, Rust!' and nothing else")
        .output_format(OutputFormat::Text)
        .build()
        .unwrap();

    let result = client.launch_and_wait(config).await.unwrap();
    assert!(!result.is_error);
    assert!(result.content.is_some());
}

#[tokio::test]
#[ignore = "requires claude CLI to be installed"]
async fn test_session_cancellation() {
    if which::which("claude").is_err() {
        return;
    }

    let client = Client::new().await.unwrap();

    let config = SessionConfig::builder("Count to 1000 slowly")
        .output_format(OutputFormat::StreamingJson)
        .build()
        .unwrap();

    let mut session = client.launch(config).await.unwrap();

    // Let it run briefly
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Kill it
    assert!(session.kill().await.is_ok());
}

#[tokio::test]
async fn deterministic_mcp_preflight_requires_exact_nonce_tool() {
    let mut servers = HashMap::new();
    servers.insert(
        "nonce".to_string(),
        MCPServer::stdio(env!("CARGO_BIN_EXE_fake_mcp_server"), vec![]),
    );
    let mcp_config = MCPConfig {
        mcp_servers: servers,
    };
    let opts = claudecode::mcp::validate::ValidateOptions {
        expected_tools: HashMap::from([(
            "nonce".to_string(),
            HashSet::from(["echo_nonce".to_string()]),
        )]),
        working_dir: Some(std::env::current_dir().unwrap()),
        ..Default::default()
    };
    let report = claudecode::mcp::validate::ensure_valid_mcp_config(&mcp_config, &opts)
        .await
        .unwrap();
    assert!(report.all_ok());
}

#[test]
fn deterministic_mcp_fixture_executes_real_nonce_call() {
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_fake_mcp_server"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "echo_nonce",
                "arguments": {"nonce": "nonce-through-real-server"}
            }
        })
    )
    .unwrap();
    stdin.flush().unwrap();
    drop(stdin);

    let response: serde_json::Value = serde_json::from_str(
        &std::io::BufReader::new(stdout)
            .lines()
            .next()
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        response
            .pointer("/result/content/0/text")
            .and_then(serde_json::Value::as_str),
        Some("nonce-through-real-server")
    );
    assert_eq!(
        response.pointer("/result/isError"),
        Some(&serde_json::json!(false))
    );
    assert!(child.wait().unwrap().success());
}

#[test]
fn deterministic_mcp_fixture_rejects_unknown_tools_and_invalid_nonce_arguments() {
    for (name, arguments, expected_code) in [
        ("unknown", serde_json::json!({"nonce":"value"}), -32601),
        ("echo_nonce", serde_json::json!({}), -32602),
        ("echo_nonce", serde_json::json!({"nonce":7}), -32602),
        (
            "echo_nonce",
            serde_json::json!({"nonce":"value","extra":true}),
            -32602,
        ),
    ] {
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_fake_mcp_server"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        writeln!(
            stdin,
            "{}",
            serde_json::json!({
                "jsonrpc":"2.0","id":1,"method":"tools/call",
                "params":{"name":name,"arguments":arguments}
            })
        )
        .unwrap();
        drop(stdin);
        let response: serde_json::Value = serde_json::from_str(
            &std::io::BufReader::new(stdout)
                .lines()
                .next()
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(response["error"]["code"], expected_code);
        assert!(child.wait().unwrap().success());
    }
}

#[tokio::test]
async fn deterministic_mcp_preflight_reports_exact_tool_mismatch() {
    let mcp_config = MCPConfig {
        mcp_servers: HashMap::from([(
            "nonce".to_string(),
            MCPServer::stdio(env!("CARGO_BIN_EXE_fake_mcp_server"), vec![]),
        )]),
    };
    let opts = claudecode::mcp::validate::ValidateOptions {
        expected_tools: HashMap::from([(
            "nonce".to_string(),
            HashSet::from(["wrong_tool".to_string()]),
        )]),
        ..Default::default()
    };
    let error = claudecode::mcp::validate::ensure_valid_mcp_config(&mcp_config, &opts)
        .await
        .unwrap_err();
    let rendered = format!("{:?}", error.errors);
    assert!(rendered.contains("wrong_tool"), "{rendered}");
    assert!(rendered.contains("echo_nonce"), "{rendered}");
}

#[cfg(unix)]
#[tokio::test]
async fn deterministic_mcp_preflight_honors_working_directory() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::TempDir::new().unwrap();
    symlink(
        env!("CARGO_BIN_EXE_fake_mcp_server"),
        directory.path().join("fake_mcp_server"),
    )
    .unwrap();
    let mcp_config = MCPConfig {
        mcp_servers: HashMap::from([(
            "nonce".to_string(),
            MCPServer::stdio("./fake_mcp_server", vec![]),
        )]),
    };
    let opts = claudecode::mcp::validate::ValidateOptions {
        expected_tools: HashMap::from([(
            "nonce".to_string(),
            HashSet::from(["echo_nonce".to_string()]),
        )]),
        working_dir: Some(directory.path().to_path_buf()),
        ..Default::default()
    };
    assert!(
        claudecode::mcp::validate::ensure_valid_mcp_config(&mcp_config, &opts)
            .await
            .unwrap()
            .all_ok()
    );
}

#[tokio::test]
async fn fake_claude_retains_raw_transcript_and_warning_diagnostics() {
    let client = Client::with_path(env!("CARGO_BIN_EXE_fake_claude"))
        .await
        .unwrap();
    let evidence_dir = tempfile::TempDir::new().unwrap();
    let evidence_file = evidence_dir.path().join("nonce-evidence");
    let mcp_config = MCPConfig {
        mcp_servers: HashMap::from([(
            "nonce".to_string(),
            MCPServer::stdio_with_env(
                env!("CARGO_BIN_EXE_fake_mcp_server"),
                vec![],
                HashMap::from([(
                    "FAKE_MCP_EVIDENCE_FILE".to_string(),
                    evidence_file.display().to_string(),
                )]),
            ),
        )]),
    };
    let config = SessionConfig::builder("fixture secret query")
        .output_format(OutputFormat::StreamingJson)
        .tools(vec![])
        .setting_sources(vec![])
        .system_prompt("fixture secret system prompt")
        .settings("{\"secret\":\"fixture secret settings\"}")
        .mcp_config(mcp_config)
        .mcp_server_always_load("nonce", true)
        .env(HashMap::from([
            ("FAKE_CLAUDE_SCENARIO".to_string(), "warning".to_string()),
            (
                "FAKE_CLAUDE_NONCE".to_string(),
                "nonce-through-fake-claude".to_string(),
            ),
        ]))
        .build()
        .unwrap();
    let session = client.launch(config).await.unwrap();
    let outcome = session.complete().await.unwrap();
    assert_eq!(outcome.result.content.as_deref(), Some("fixture complete"));
    assert_eq!(outcome.exit_code, Some(0));
    assert!(outcome.stderr.contains("fake warning"));
    assert!(
        outcome
            .transcript
            .iter()
            .any(|event| event.raw["nonce"] == "raw-preserved")
    );
    assert!(outcome.transcript.iter().any(|event| {
        matches!(&event.event, claudecode::Event::User(user)
        if user.message.content.iter().any(|content| {
            matches!(content.tool_result(), Some(("call-1", value, false))
                if value.pointer("/0/text").and_then(serde_json::Value::as_str)
                    == Some("nonce-through-fake-claude"))
        }))
    }));
    assert!(
        outcome
            .invocation
            .argv
            .windows(2)
            .any(|pair| { pair[0] == "--tools" && pair[1].is_empty() })
    );
    let invocation = format!("{:?}", outcome.invocation.argv);
    assert!(!invocation.contains("fixture secret query"));
    assert!(!invocation.contains("fixture secret system prompt"));
    assert!(!invocation.contains("fixture secret settings"));
    assert_eq!(
        std::fs::read_to_string(evidence_file).unwrap(),
        "nonce-through-fake-claude"
    );
    assert_eq!(
        outcome.invocation.claude_version.as_deref(),
        Some("2.1.220 (fake)")
    );
    assert_eq!(
        outcome
            .invocation
            .mcp_config
            .as_ref()
            .and_then(|config| config.pointer("/mcpServers/nonce/alwaysLoad")),
        Some(&serde_json::Value::Bool(true))
    );
}

#[tokio::test]
async fn fake_claude_invocation_metadata_lists_env_keys_and_redacts_mcp_secrets() {
    let client = Client::with_path(env!("CARGO_BIN_EXE_fake_claude"))
        .await
        .unwrap();
    let mcp_config = MCPConfig {
        mcp_servers: HashMap::from([(
            "nonce".to_string(),
            MCPServer::stdio_with_env(
                env!("CARGO_BIN_EXE_fake_mcp_server"),
                vec![],
                HashMap::from([
                    ("API_TOKEN".to_string(), "mcp-super-secret".to_string()),
                    ("VISIBLE_MODE".to_string(), "fixture".to_string()),
                ]),
            ),
        )]),
    };
    let config = SessionConfig::builder("fixture")
        .output_format(OutputFormat::StreamingJson)
        .env(HashMap::from([
            ("FAKE_CLAUDE_SCENARIO".to_string(), "warning".to_string()),
            (
                "SESSION_SECRET".to_string(),
                "session-super-secret".to_string(),
            ),
        ]))
        .mcp_config(mcp_config)
        .build()
        .unwrap();
    let session = client.launch(config).await.unwrap();
    let outcome = session.complete().await.unwrap();

    assert_eq!(
        outcome.invocation.environment_keys,
        vec!["FAKE_CLAUDE_SCENARIO", "SESSION_SECRET"]
    );
    let rendered = outcome.invocation.mcp_config.unwrap().to_string();
    assert!(rendered.contains("<redacted>"), "{rendered}");
    assert!(rendered.contains("fixture"), "{rendered}");
    assert!(!rendered.contains("mcp-super-secret"), "{rendered}");
    assert!(!rendered.contains("session-super-secret"), "{rendered}");
}

#[tokio::test]
async fn fake_claude_redacts_configured_secrets_from_tool_results_and_stderr() {
    let client = Client::with_path(env!("CARGO_BIN_EXE_fake_claude"))
        .await
        .unwrap();
    let secret = "configured-generic-content-secret";
    let config = SessionConfig::builder("fixture")
        .output_format(OutputFormat::StreamingJson)
        .env(HashMap::from([
            (
                "FAKE_CLAUDE_SCENARIO".to_string(),
                "leak-configured-secret".to_string(),
            ),
            ("SESSION_SECRET".to_string(), secret.to_string()),
        ]))
        .mcp_config(MCPConfig {
            mcp_servers: HashMap::from([(
                "nonce".to_string(),
                MCPServer::stdio(env!("CARGO_BIN_EXE_fake_mcp_server"), vec![]),
            )]),
        })
        .build()
        .unwrap();
    let outcome = client
        .launch(config)
        .await
        .unwrap()
        .complete()
        .await
        .unwrap();
    let rendered = format!(
        "{} {} {:?}",
        outcome.stderr, outcome.raw_stdout, outcome.transcript
    );
    assert!(
        !rendered.contains(secret),
        "raw secret persisted: {rendered}"
    );
    assert!(rendered.contains("<redacted>"), "redaction marker absent");
}

#[tokio::test]
async fn fake_claude_empty_setting_sources_exclude_user_home_ambient_state() {
    let workspace = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap();
    let fixture_root = workspace.join("target/claudecode-hermetic-fixtures");
    std::fs::create_dir_all(&fixture_root).unwrap();
    let home = tempfile::Builder::new()
        .prefix("home-")
        .tempdir_in(fixture_root)
        .unwrap();
    let sentinels = [
        (
            ".claude/settings.json",
            "USER-SETTINGS-HOOK-PLUGIN-SENTINEL",
        ),
        (".claude/CLAUDE.md", "USER-CLAUDE-GUIDANCE-SENTINEL"),
        (".claude/plugins/sentinel.txt", "USER-PLUGIN-SENTINEL"),
        (
            ".claude/projects/fixture/memory/MEMORY.md",
            "USER-AUTO-MEMORY-SENTINEL",
        ),
    ];
    for (relative, value) in sentinels {
        let path = home.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, value).unwrap();
    }
    let client = Client::with_path(env!("CARGO_BIN_EXE_fake_claude"))
        .await
        .unwrap();
    let run = |setting_sources: Vec<String>| {
        SessionConfig::builder("fixture")
            .output_format(OutputFormat::StreamingJson)
            .setting_sources(setting_sources)
            .env(HashMap::from([
                (
                    "FAKE_CLAUDE_SCENARIO".to_string(),
                    "ambient-home".to_string(),
                ),
                ("HOME".to_string(), home.path().display().to_string()),
            ]))
            .build()
            .unwrap()
    };

    let inherited = client
        .launch(run(vec!["user".to_string()]))
        .await
        .unwrap()
        .complete()
        .await
        .unwrap();
    let inherited = format!("{:?}", inherited.transcript);
    for (_, sentinel) in sentinels {
        assert!(inherited.contains(sentinel), "control missed {sentinel}");
    }

    let isolated = client
        .launch(run(vec![]))
        .await
        .unwrap()
        .complete()
        .await
        .unwrap();
    let isolated = format!("{:?}", isolated.transcript);
    for (_, sentinel) in sentinels {
        assert!(
            !isolated.contains(sentinel),
            "ambient state leaked: {sentinel}"
        );
    }
}

#[tokio::test]
async fn fake_claude_structured_error_fails_with_zero_exit() {
    let client = Client::with_path(env!("CARGO_BIN_EXE_fake_claude"))
        .await
        .unwrap();
    let config = SessionConfig::builder("fixture")
        .output_format(OutputFormat::StreamingJson)
        .env(HashMap::from([(
            "FAKE_CLAUDE_SCENARIO".to_string(),
            "structured-error".to_string(),
        )]))
        .build()
        .unwrap();
    let error = client.launch_and_wait(config).await.unwrap_err();
    assert!(error.to_string().contains("fixture structured error"));
}

#[tokio::test]
async fn fake_claude_retains_mcp_server_errors() {
    let client = Client::with_path(env!("CARGO_BIN_EXE_fake_claude"))
        .await
        .unwrap();
    let config = SessionConfig::builder("fixture")
        .output_format(OutputFormat::StreamingJson)
        .env(HashMap::from([(
            "FAKE_CLAUDE_SCENARIO".to_string(),
            "mcp-error".to_string(),
        )]))
        .build()
        .unwrap();
    let session = client.launch(config).await.unwrap();
    let outcome = session.complete().await.unwrap();
    let init = outcome
        .transcript
        .iter()
        .find_map(|event| match &event.event {
            claudecode::Event::System(system) if system.subtype.as_deref() == Some("init") => {
                Some(system)
            }
            _ => None,
        });
    assert_eq!(init.unwrap().mcp_server_errors.len(), 1);
}

#[tokio::test]
async fn fake_claude_nonzero_exit_is_reported_with_diagnostics() {
    let client = Client::with_path(env!("CARGO_BIN_EXE_fake_claude"))
        .await
        .unwrap();
    let config = SessionConfig::builder("fixture")
        .output_format(OutputFormat::StreamingJson)
        .env(HashMap::from([(
            "FAKE_CLAUDE_SCENARIO".to_string(),
            "warning-nonzero".to_string(),
        )]))
        .build()
        .unwrap();
    let error = client.launch_and_wait(config).await.unwrap_err();
    let claudecode::ClaudeError::ExecutionFailed { failure } = error else {
        panic!("expected diagnostics-bearing execution failure");
    };
    assert_eq!(failure.exit_code, Some(17));
    assert!(failure.message.contains("status 17"));
    assert!(failure.stderr.contains("fake warning"));
    assert!(!failure.transcript.is_empty());
    assert!(!failure.raw_stdout.is_empty());
    assert!(!failure.invocation.argv.join(" ").contains("fixture"));
}

#[tokio::test]
async fn fake_claude_top_level_error_event_is_terminal_and_diagnostic() {
    let client = Client::with_path(env!("CARGO_BIN_EXE_fake_claude"))
        .await
        .unwrap();
    let config = SessionConfig::builder("secret error query")
        .output_format(OutputFormat::StreamingJson)
        .env_var("FAKE_CLAUDE_SCENARIO", "top-level-error")
        .build()
        .unwrap();
    let error = client.launch_and_wait(config).await.unwrap_err();
    let claudecode::ClaudeError::ExecutionFailed { failure } = error else {
        panic!("expected diagnostics-bearing execution failure");
    };
    assert_eq!(failure.message, "top-level terminal error");
    assert_eq!(failure.exit_code, Some(0));
    assert!(failure.transcript.iter().any(|event| {
        matches!(&event.event, claudecode::Event::Error(error)
            if error.error == "top-level terminal error")
    }));
    assert!(
        !failure
            .invocation
            .argv
            .join(" ")
            .contains("secret error query")
    );
}

#[tokio::test]
async fn fake_claude_bounds_oversized_stream_diagnostics_and_transcript() {
    let client = Client::with_path(env!("CARGO_BIN_EXE_fake_claude"))
        .await
        .unwrap();
    let config = SessionConfig::builder("fixture")
        .output_format(OutputFormat::StreamingJson)
        .env_var(
            "FAKE_CLAUDE_SCENARIO",
            "oversized-event-oversized-transcript-oversized-stderr",
        )
        .build()
        .unwrap();
    let mut session = client.launch(config).await.unwrap();
    let outcome = session.complete().await.unwrap();
    assert!(outcome.raw_stdout.len() <= 256 * 1024);
    assert!(outcome.stderr.len() <= 256 * 1024);
    assert!(!outcome.raw_stdout.is_empty());
    assert!(!outcome.stderr.is_empty());
    assert!(!outcome.transcript.is_empty());
    assert!(outcome.transcript.len() <= 2_048);
    let transcript_bytes = outcome
        .transcript
        .iter()
        .map(|event| event.raw.to_string().len())
        .sum::<usize>();
    assert!(transcript_bytes <= 2 * 1024 * 1024);
    assert!(
        !outcome
            .raw_stdout
            .contains("early-oversized-event-sentinel")
    );
    assert!(outcome.raw_stdout.contains("fixture complete"));
    assert!(!outcome.stderr.contains("stderr-early-sentinel"));
    assert!(outcome.stderr.ends_with("stderr-final-tail\n"));
    let first_retained_index = outcome
        .transcript
        .iter()
        .find_map(|event| event.raw.get("index").and_then(serde_json::Value::as_u64))
        .unwrap();
    assert!(first_retained_index > 0);
    assert!(matches!(
        outcome.transcript.last().map(|event| &event.event),
        Some(claudecode::Event::Result(_))
    ));
    assert_eq!(outcome.result.content.as_deref(), Some("fixture complete"));
    let mut events = session.take_event_stream().unwrap();
    let mut event_count = 0;
    let mut terminal_delivered = false;
    while let Ok(event) = events.try_recv() {
        event_count += 1;
        terminal_delivered |= matches!(event, claudecode::Event::Result(_));
    }
    assert!(event_count <= 2_048);
    assert!(terminal_delivered);
}

#[tokio::test]
async fn fake_claude_restores_untagged_single_json_results() {
    let client = Client::with_path(env!("CARGO_BIN_EXE_fake_claude"))
        .await
        .unwrap();
    let config = SessionConfig::builder("fixture")
        .output_format(OutputFormat::Json)
        .env_var("FAKE_CLAUDE_SCENARIO", "untagged")
        .build()
        .unwrap();
    let outcome = client
        .launch(config)
        .await
        .unwrap()
        .complete()
        .await
        .unwrap();
    assert!(outcome.transcript.is_empty());
    assert_eq!(outcome.result.result.as_deref(), Some("untagged fixture"));
    assert_eq!(outcome.result.content.as_deref(), Some("untagged fixture"));
}

#[tokio::test]
async fn fake_claude_observes_canonical_effective_working_directory() {
    let client = Client::with_path(env!("CARGO_BIN_EXE_fake_claude"))
        .await
        .unwrap();
    for configured in [None, Some(tempfile::TempDir::new().unwrap())] {
        let mut builder =
            SessionConfig::builder("fixture").output_format(OutputFormat::StreamingJson);
        if let Some(directory) = &configured {
            builder = builder.working_dir(directory.path());
        }
        let outcome = client
            .launch(builder.build().unwrap())
            .await
            .unwrap()
            .complete()
            .await
            .unwrap();
        let expected = std::fs::canonicalize(configured.as_ref().map_or_else(
            || std::env::current_dir().unwrap(),
            |dir| dir.path().to_path_buf(),
        ))
        .unwrap();
        assert_eq!(
            outcome.invocation.working_dir.as_deref(),
            Some(expected.as_path())
        );
        let observed = outcome
            .transcript
            .iter()
            .find_map(|event| match &event.event {
                claudecode::Event::System(system) => system.cwd.as_deref(),
                _ => None,
            });
        assert_eq!(observed, Some(expected.to_string_lossy().as_ref()));
    }
}

#[tokio::test]
#[ignore = "requires claude CLI to be installed and support haiku model"]
async fn test_haiku_model() {
    if which::which("claude").is_err() {
        return;
    }

    let client = Client::new().await.unwrap();

    let config = SessionConfig::builder("Say 'Hello from Haiku!' and nothing else")
        .model(Model::Haiku)
        .output_format(OutputFormat::Text)
        .build()
        .unwrap();

    let result = client.launch_and_wait(config).await.unwrap();
    assert!(!result.is_error);
    assert!(result.content.is_some());
}
