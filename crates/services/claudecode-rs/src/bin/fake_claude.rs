use serde_json::json;
use std::io::BufRead;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

static TERMINATED: AtomicBool = AtomicBool::new(false);

extern "C" fn record_termination(_signal: libc::c_int) {
    TERMINATED.store(true, Ordering::SeqCst);
}

#[expect(
    clippy::exit,
    reason = "fixture must emulate an immediate nonzero Claude process exit"
)]
fn run_lifecycle_fixture(pid_file: &str) {
    let mut child = std::process::Command::new("sh")
        .args(["-c", "trap 'exit 0' TERM INT; while :; do sleep 1; done"])
        .spawn()
        .unwrap_or_else(|error| panic!("failed to spawn lifecycle helper: {error}"));
    let child_pid = child.id();
    // SAFETY: record_termination is an async-signal-safe handler that only stores an atomic bool.
    unsafe {
        libc::signal(
            libc::SIGTERM,
            record_termination as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            record_termination as *const () as libc::sighandler_t,
        );
    }
    let pid_bytes = serde_json::to_vec(&json!({
        "parent_pid": std::process::id(),
        "child_pid": child_pid,
    }))
    .unwrap_or_default();
    let temporary_pid_file = format!("{pid_file}.tmp");
    std::fs::write(&temporary_pid_file, pid_bytes)
        .and_then(|()| std::fs::rename(&temporary_pid_file, pid_file))
        .unwrap_or_else(|error| panic!("failed to write lifecycle pid file: {error}"));

    if std::env::var_os("FAKE_CLAUDE_FORCE_ERROR_AFTER_SPAWN").is_some() {
        // SAFETY: child_pid was returned by a live child process spawned above.
        unsafe { libc::kill(child_pid.cast_signed(), libc::SIGTERM) };
        let _ = child.wait();
        eprintln!("forced fake_claude error after helper spawn");
        std::process::exit(1);
    }

    while !TERMINATED.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(10));
    }
    // SAFETY: child_pid was returned by a live child process spawned above.
    unsafe { libc::kill(child_pid.cast_signed(), libc::SIGTERM) };
    let _ = child.wait();
    print!("fake text output");
}

fn rpc_call(
    stdin: &mut impl Write,
    stdout: &mut impl BufRead,
    request: &serde_json::Value,
) -> serde_json::Value {
    writeln!(stdin, "{request}")
        .and_then(|()| stdin.flush())
        .unwrap_or_else(|error| panic!("failed to send fake MCP request: {error}"));
    let mut line = String::new();
    stdout
        .read_line(&mut line)
        .unwrap_or_else(|error| panic!("failed to read fake MCP response: {error}"));
    serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("invalid fake MCP response {line:?}: {error}"))
}

fn invoke_configured_mcp(args: &[String], nonce: &str) -> Option<String> {
    let config_path = args
        .windows(2)
        .find(|pair| pair[0] == "--mcp-config")
        .map(|pair| &pair[1])?;
    let config: serde_json::Value = serde_json::from_slice(
        &std::fs::read(config_path)
            .unwrap_or_else(|error| panic!("failed to read fake MCP config: {error}")),
    )
    .unwrap_or_else(|error| panic!("invalid fake MCP config: {error}"));
    let server = &config["mcpServers"]["nonce"];
    let command = server["command"]
        .as_str()
        .unwrap_or_else(|| panic!("fake MCP config lacks nonce command"));
    let mut child_command = std::process::Command::new(command);
    if let Some(server_args) = server["args"].as_array() {
        child_command.args(server_args.iter().filter_map(serde_json::Value::as_str));
    }
    if let Some(environment) = server["env"].as_object() {
        child_command.envs(
            environment
                .iter()
                .filter_map(|(key, value)| value.as_str().map(|value| (key, value))),
        );
    }
    let mut child = child_command
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("failed to spawn configured fake MCP server: {error}"));
    let mut stdin = child
        .stdin
        .take()
        .unwrap_or_else(|| panic!("missing MCP stdin"));
    let stdout = child
        .stdout
        .take()
        .unwrap_or_else(|| panic!("missing MCP stdout"));
    let mut stdout = std::io::BufReader::new(stdout);

    let initialized = rpc_call(
        &mut stdin,
        &mut stdout,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":"fake-claude","version":"1.0.0"}
        }}),
    );
    assert!(initialized.get("error").is_none(), "{initialized}");
    let tools = rpc_call(
        &mut stdin,
        &mut stdout,
        &json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    );
    assert_eq!(tools["result"]["tools"][0]["name"], "echo_nonce");
    let called = rpc_call(
        &mut stdin,
        &mut stdout,
        &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
            "name":"echo_nonce","arguments":{"nonce":nonce}
        }}),
    );
    drop(stdin);
    let _ = child.wait();
    called
        .pointer("/result/content/0/text")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
}

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--version") {
        if args
            .first()
            .and_then(|path| std::path::Path::new(path).file_name())
            .is_some_and(|name| name == "fake_claude_hanging_version")
        {
            let pid_file = std::path::Path::new(&args[0]).with_extension("pid");
            std::fs::write(pid_file, std::process::id().to_string())
                .unwrap_or_else(|error| panic!("failed to write version probe pid: {error}"));
            loop {
                std::thread::sleep(Duration::from_mins(1));
            }
        }
        println!("2.1.220 (fake)");
        return;
    }

    if let Ok(pid_file) = std::env::var("FAKE_CLAUDE_PID_FILE") {
        run_lifecycle_fixture(&pid_file);
        return;
    }

    let scenario = std::env::var("FAKE_CLAUDE_SCENARIO").unwrap_or_else(|_| "stream".to_string());
    if scenario.contains("warning") {
        eprintln!("fake warning without credential values");
    }
    if scenario.contains("leak-configured-secret")
        && let Ok(secret) = std::env::var("SESSION_SECRET")
    {
        eprintln!("generic stderr content: {secret}");
    }
    if scenario.contains("oversized-stderr") {
        eprintln!(
            "stderr-early-sentinel{}stderr-final-tail",
            "stderr-body".repeat(100_000)
        );
    }

    let mcp_server_errors = if scenario.contains("mcp-error") {
        vec![json!({"server": "nonce", "error": "fixture initialization failure"})]
    } else {
        Vec::new()
    };
    let terminal_error = scenario.contains("structured-error");
    let nonce =
        std::env::var("FAKE_CLAUDE_NONCE").unwrap_or_else(|_| "nonce-from-fixture".to_string());
    let mcp_result = invoke_configured_mcp(&args, &nonce);
    let mut events = vec![
        json!({
            "type": "system",
            "subtype": "init",
            "session_id": "fake-session",
            "cwd": std::env::current_dir().ok(),
            "tools": mcp_result.as_ref().map(|_| vec!["mcp__nonce__echo_nonce"]),
            "mcp_servers": mcp_result.as_ref().map(|_| vec![json!({"name": "nonce", "status": "connected"})]),
            "mcp_server_errors": mcp_server_errors
        }),
        json!({"type": "future_event", "nonce": "raw-preserved"}),
    ];
    let setting_sources_are_empty = args
        .windows(2)
        .any(|pair| pair[0] == "--setting-sources" && pair[1].is_empty());
    if scenario.contains("ambient-home") && !setting_sources_are_empty {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let paths = [
            ".claude/settings.json",
            ".claude/CLAUDE.md",
            ".claude/plugins/sentinel.txt",
            ".claude/projects/fixture/memory/MEMORY.md",
        ];
        let ambient = home
            .into_iter()
            .flat_map(|home| paths.map(|path| home.join(path)))
            .filter_map(|path| std::fs::read_to_string(path).ok())
            .collect::<Vec<_>>();
        events.push(json!({"type":"future_event","ambient_home":ambient}));
    }
    if let Some(mcp_result) = mcp_result {
        let result_content = if scenario.contains("leak-configured-secret") {
            format!(
                "{mcp_result} generic tool result {}",
                std::env::var("SESSION_SECRET").unwrap_or_default()
            )
        } else {
            mcp_result
        };
        events.extend([
            json!({
                "type": "assistant",
                "session_id": "fake-session",
                "parent_tool_use_id": null,
                "message": {"role": "assistant", "content": [{
                    "type": "tool_use", "id": "call-1", "name": "mcp__nonce__echo_nonce",
                    "input": {"nonce": nonce}
                }]}
            }),
            json!({
                "type": "user",
                "session_id": "fake-session",
                "message": {"role": "user", "content": [{
                    "type": "tool_result", "tool_use_id": "call-1",
                    "content": [{"type": "text", "text": result_content}], "is_error": false
                }]}
            }),
        ]);
    }
    if scenario.contains("oversized-event") {
        events.push(json!({
            "type":"future_event",
            "payload":format!(
                "early-oversized-event-sentinel{}oversized-event-tail",
                "x".repeat(2 * 1024 * 1024)
            )
        }));
    }
    if scenario.contains("oversized-transcript") {
        events.extend((0..3_000).map(|index| json!({"type":"future_event","index":index})));
    }
    if scenario.contains("top-level-error") {
        events.push(
            json!({"type":"error","session_id":"fake-session","error":"top-level terminal error"}),
        );
    } else {
        events.push(json!({
            "type": "result", "subtype": "success", "session_id": "fake-session",
            "result": "fixture complete", "is_error": terminal_error,
            "error": terminal_error.then_some("fixture structured error"), "num_turns": 1
        }));
    }

    let output_format = args
        .windows(2)
        .find(|pair| pair[0] == "--output-format")
        .map_or("stream-json", |pair| pair[1].as_str());
    if output_format == "json" && scenario.contains("untagged") {
        println!(
            "{}",
            json!({"session_id":"fake-session","result":"untagged fixture"})
        );
    } else if output_format == "json" || scenario.contains("array") {
        println!("{}", serde_json::Value::Array(events));
    } else {
        for event in events {
            println!("{event}");
        }
    }
    let _ = std::io::stdout().flush();

    if scenario.contains("nonzero") {
        std::process::exit(17);
    }
}
