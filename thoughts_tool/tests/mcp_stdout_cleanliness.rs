//! Test that MCP mode produces no stdout before handshake.
//!
//! MCP protocol reserves stdout exclusively for JSON-RPC frames.
//! Any stdout content before the handshake will corrupt the protocol stream.

use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

#[test]
fn mcp_produces_no_stdout_before_handshake() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_thoughts"))
        .arg("mcp")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Stress logs; still should be stderr-only in MCP mode
        .env("RUST_LOG", "trace")
        .spawn()
        .expect("failed to spawn thoughts mcp");

    sleep(Duration::from_millis(150));

    // Kill and collect output
    let _ = child.kill();
    let output = child.wait_with_output().expect("failed to wait on child");

    assert!(
        output.stdout.is_empty(),
        "stdout should be empty before MCP handshake, got: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
