//! Test that MCP mode produces no stdout before handshake.
//!
//! MCP protocol reserves stdout exclusively for JSON-RPC frames.
//! Any stdout content before the handshake will corrupt the protocol stream.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

/// Get the path to the coding-agent-tools binary.
/// Works for both `cargo test` and direct test execution.
fn get_bin_path() -> PathBuf {
    // First try the env var that cargo test sets
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_coding-agent-tools") {
        return PathBuf::from(path);
    }

    // Fall back to constructing from manifest dir
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let target_dir = PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .join("target")
        .join("debug")
        .join("coding-agent-tools");

    assert!(
        target_dir.exists(),
        "Binary not found at {:?}. Run `cargo build -p coding_agent_tools` first.",
        target_dir
    );
    target_dir
}

#[test]
fn mcp_produces_no_stdout_before_handshake() {
    let bin_path = get_bin_path();

    let mut child = Command::new(&bin_path)
        .arg("mcp")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Targeted logs only; coding_agent_tools does not log before handshake
        .env("RUST_LOG", "coding_agent_tools=trace")
        .spawn()
        .expect("failed to spawn coding-agent-tools mcp");

    sleep(Duration::from_millis(150));

    let _ = child.kill();
    let output = child.wait_with_output().expect("failed to wait on child");

    assert!(
        output.stdout.is_empty(),
        "stdout should be empty before MCP handshake, got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
