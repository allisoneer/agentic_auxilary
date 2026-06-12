//! Endpoint coverage verification for opencode-rs SDK.
//!
//! Shells out to the SDK's `endpoint_coverage` example to perform the analysis.

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Deserialize)]
struct CoverageReport {
    comparison_mode: String,
    live_diff_performed: bool,
    duplicate_sdk_endpoints: Vec<String>,
    duplicate_skip_endpoints: Vec<String>,
    overlapping_endpoints: Vec<String>,
    missing_endpoints: Vec<String>,
}

fn validate_report(report: &CoverageReport) -> Result<()> {
    if !report.duplicate_sdk_endpoints.is_empty() {
        bail!(
            "duplicate SDK endpoints found: {}",
            report.duplicate_sdk_endpoints.join(", ")
        );
    }

    if !report.duplicate_skip_endpoints.is_empty() {
        bail!(
            "duplicate skipped endpoints found: {}",
            report.duplicate_skip_endpoints.join(", ")
        );
    }

    if !report.overlapping_endpoints.is_empty() {
        bail!(
            "endpoint(s) listed in both SDK and skip sets: {}",
            report.overlapping_endpoints.join(", ")
        );
    }

    if report.live_diff_performed && !report.missing_endpoints.is_empty() {
        bail!(
            "missing live endpoints found: {}",
            report.missing_endpoints.join(", ")
        );
    }

    Ok(())
}

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

    if json || check {
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

    if !check {
        println!("{stdout}");
    }

    if !stderr.is_empty() {
        eprintln!("{stderr}");
    }

    if check {
        let report: CoverageReport = serde_json::from_str(&stdout)
            .context("Failed to parse endpoint coverage JSON output")?;
        validate_report(&report)?;

        if report.live_diff_performed {
            println!(
                "[endpoint-coverage] Check mode: validated {} report",
                report.comparison_mode
            );
        } else {
            println!(
                "[endpoint-coverage] Check mode: inventory-only validation passed (no live OpenAPI diff performed)"
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_report_accepts_inventory_only_without_live_missing_gate() {
        let report = CoverageReport {
            comparison_mode: "inventory-only".to_string(),
            live_diff_performed: false,
            duplicate_sdk_endpoints: vec![],
            duplicate_skip_endpoints: vec![],
            overlapping_endpoints: vec![],
            missing_endpoints: vec!["GET /api/example".to_string()],
        };

        validate_report(&report).unwrap();
    }

    #[test]
    fn validate_report_rejects_overlaps() {
        let report = CoverageReport {
            comparison_mode: "inventory-only".to_string(),
            live_diff_performed: false,
            duplicate_sdk_endpoints: vec![],
            duplicate_skip_endpoints: vec![],
            overlapping_endpoints: vec!["GET /api/example".to_string()],
            missing_endpoints: vec![],
        };

        assert!(validate_report(&report).is_err());
    }
}
