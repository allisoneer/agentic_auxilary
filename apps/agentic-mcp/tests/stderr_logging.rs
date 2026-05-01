use std::path::Path;
use std::process::Command;

#[test]
fn tracing_setup_explicitly_uses_stderr_writer() {
    let main_rs_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs");
    let main_rs = std::fs::read_to_string(&main_rs_path).expect("read src/main.rs");

    let main_start = main_rs
        .find("async fn main() -> anyhow::Result<()>")
        .expect("main function should exist");
    let startup = &main_rs[main_start..];
    let rustls_start = startup
        .find("// Install the rustls CryptoProvider")
        .expect("rustls setup should follow tracing setup");
    let tracing_setup = &startup[..rustls_start];

    assert!(
        tracing_setup.contains("tracing_subscriber::fmt()"),
        "tracing setup should use the configurable fmt builder; found:\n{tracing_setup}"
    );
    assert!(
        tracing_setup.contains(".with_writer(std::io::stderr)"),
        "agentic-mcp tracing must explicitly write to stderr; found:\n{tracing_setup}"
    );
    assert!(
        !tracing_setup.contains("tracing_subscriber::fmt::init()"),
        "fmt::init() uses the default writer instead of explicitly selecting stderr; found:\n{tracing_setup}"
    );
}

#[test]
fn list_tools_keeps_stdout_empty_for_protocol_safety() {
    let output = Command::new(env!("CARGO_BIN_EXE_agentic-mcp"))
        .arg("--list-tools")
        .output()
        .expect("run agentic-mcp --list-tools");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "agentic-mcp --list-tools failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.is_empty(),
        "stdout must stay reserved for MCP protocol messages; got:\n{stdout}"
    );
    assert!(
        stderr.contains("Available tools"),
        "expected --list-tools output on stderr; got:\n{stderr}"
    );
}
