use std::process::Command;
use std::process::Stdio;

#[test]
fn serving_diagnostics_never_use_protocol_stdout() {
    let home = tempfile::TempDir::new().expect("temporary home");
    let mut child = Command::new(env!("CARGO_BIN_EXE_agentic-mcp"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", home.path().join("xdg"))
        .args(["--allow", "cli_ls"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn agentic-mcp");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("wait for agentic-mcp");
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Starting agentic-mcp"));
}
