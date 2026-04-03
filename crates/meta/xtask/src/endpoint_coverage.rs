//! Endpoint coverage verification for opencode-rs SDK.
//!
//! Shells out to the SDK's endpoint_coverage example to perform the analysis.

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use std::process::Command;

pub fn run(json: bool, check: bool) -> Result<()> {
    println!("[endpoint-coverage] Running SDK endpoint coverage analysis...");

    let mut cmd = Command::new("cargo");
    cmd.args([
        "run",
        "-p",
        "opencode_rs",
        "--example",
        "endpoint_coverage",
        "--features",
        "full",
        "--",
    ]);

    if json {
        cmd.arg("--json");
    }

    let output = cmd
        .output()
        .context("Failed to run endpoint_coverage example")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        eprintln!("{stderr}");
        bail!(
            "endpoint_coverage example failed with status: {}",
            output.status
        );
    }

    println!("{stdout}");

    if !stderr.is_empty() {
        eprintln!("{stderr}");
    }

    // In check mode, could parse output and fail if coverage is incomplete
    if check {
        // For now, just succeed if the tool ran
        // Future: parse JSON output and check for missing endpoints
        println!("[endpoint-coverage] Check mode: tool ran successfully");
    }

    Ok(())
}
