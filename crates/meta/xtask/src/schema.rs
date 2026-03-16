//! agentic.schema.json sync utilities.
//!
//! Important: xtask shells out to the canonical generator to preserve xtask's
//! "no workspace deps" boundary.

use anyhow::{Context, Result, bail};
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

fn generate_schema_json() -> Result<String> {
    let output = Command::new("cargo")
        .args(["run", "-p", "agentic-bin", "--", "config", "schema"])
        .output()
        .context("Failed to run `cargo run -p agentic-bin -- config schema`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "`cargo run -p agentic-bin -- config schema` failed with {}: {stderr}",
            output.status
        );
    }

    let stdout =
        String::from_utf8(output.stdout).context("Schema generator stdout was not valid UTF-8")?;
    Ok(stdout)
}

/// Sync a full file at `path` to the canonical agentic config schema.
///
/// Returns true if the file was/would be changed.
pub fn sync_schema(path: &str, dry_run: bool, check: bool) -> Result<bool> {
    let desired = generate_schema_json()?;

    // Missing file is treated as stale.
    let existed = Path::new(path).exists();

    let current = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("Failed to read {path}")),
    };

    let changed = !existed || current != desired;

    if changed {
        if check {
            // Canonical guidance only.
            bail!("{path} is out of date; run `cargo run -p xtask -- sync`");
        }

        if !dry_run {
            fs::write(path, desired).with_context(|| format!("Failed to write {path}"))?;
            eprintln!("[sync] Updated {path}");
        } else {
            eprintln!("[sync] Would update {path} (dry-run)");
        }
    }

    Ok(changed)
}
