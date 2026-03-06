//! justfile autogen utilities.
//!
//! Syncs root justfile variables that should be derived from workspace metadata.

use crate::autogen::replace_named_block_toml;
use anyhow::{Context, Result};
use cargo_metadata::{Metadata, Package};
use std::fs;

/// Extract metadata.repo.role from a package, defaulting to "unknown".
fn get_role(pkg: &Package) -> &str {
    pkg.metadata
        .get("repo")
        .and_then(|r| r.get("role"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
}

/// Extract metadata.repo.integrations.<key> as bool.
fn get_integration(pkg: &Package, key: &str) -> bool {
    pkg.metadata
        .get("repo")
        .and_then(|r| r.get("integrations"))
        .and_then(|i| i.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Render MCP_SERVERS justfile variable from workspace packages that are MCP server apps.
///
/// Filters for packages with:
/// - `metadata.repo.role = "app"` (binary applications)
/// - `metadata.repo.integrations.mcp = true` (MCP integration enabled)
fn render_mcp_servers(metadata: &Metadata) -> String {
    let mut servers: Vec<String> = Vec::new();

    for pkg in &metadata.packages {
        if !metadata.workspace_members.contains(&pkg.id) {
            continue;
        }
        // Only include app packages (binaries) that have MCP integration
        if get_role(pkg) == "app" && get_integration(pkg, "mcp") {
            servers.push(pkg.name.clone());
        }
    }

    servers.sort();
    let joined = servers.join(" ");

    // Include blank lines around the variable for justfile formatter compatibility
    format!("\nMCP_SERVERS := \"{}\"\n", joined)
}

/// Sync root justfile autogen blocks.
///
/// Returns true if the file was changed.
pub fn sync_justfile(path: &str, metadata: &Metadata, dry_run: bool, check: bool) -> Result<bool> {
    let original = fs::read_to_string(path).with_context(|| format!("Failed to read {path}"))?;

    let body = render_mcp_servers(metadata);
    let (updated, changed) = replace_named_block_toml(&original, "justfile:mcp-servers", &body)?;

    if changed {
        if check {
            anyhow::bail!("justfile MCP_SERVERS is out of date; run `cargo run -p xtask -- sync`");
        }
        if !dry_run {
            fs::write(path, &updated).with_context(|| format!("Failed to write {path}"))?;
            eprintln!("[sync] Updated {path}");
        } else {
            eprintln!("[sync] Would update {path} (dry-run)");
        }
    }

    Ok(changed)
}
