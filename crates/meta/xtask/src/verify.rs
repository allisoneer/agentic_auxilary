//! Verify command implementation.
//!
//! Validates metadata, policy rules, and generated file freshness.

use crate::policy::Policy;
use crate::sync;
use anyhow::{Context, Result, bail};
use cargo_metadata::{Metadata, MetadataCommand, Package};
use std::process::Command;

/// Check if a package has a dependency (direct or renamed).
fn has_dep(pkg: &Package, needle: &str) -> bool {
    pkg.dependencies
        .iter()
        .any(|d| d.name == needle || d.rename.as_deref() == Some(needle))
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

/// Validate metadata presence and enum values.
fn check_metadata(metadata: &Metadata, policy: &Policy) -> Result<()> {
    let mut errors = Vec::new();

    for pkg in &metadata.packages {
        if !metadata.workspace_members.contains(&pkg.id) {
            continue;
        }

        let repo = pkg.metadata.get("repo");
        if repo.is_none() {
            errors.push(format!(
                "{}: missing [package.metadata.repo]\n\
                 Add the following to {}:\n\n\
                 [package.metadata.repo]\n\
                 role = \"lib\"\n\
                 family = \"tools\"\n\n\
                 [package.metadata.repo.integrations]\n\
                 mcp = false\n\
                 logging = false\n\
                 napi = false\n",
                pkg.name, pkg.manifest_path
            ));
            continue;
        }

        let repo = repo.unwrap();

        // Validate role
        let role = repo.get("role").and_then(|v| v.as_str()).unwrap_or("");
        if !policy.is_valid_role(role) {
            errors.push(format!(
                "{}: invalid role '{}'. Allowed: {:?}",
                pkg.name, role, policy.enums.role
            ));
        }

        // Validate family
        let family = repo.get("family").and_then(|v| v.as_str()).unwrap_or("");
        if !policy.is_valid_family(family) {
            errors.push(format!(
                "{}: invalid family '{}'. Allowed: {:?}",
                pkg.name, family, policy.enums.family
            ));
        }
    }

    if !errors.is_empty() {
        bail!("Metadata validation failed:\n\n{}", errors.join("\n"));
    }

    Ok(())
}

/// Validate integration dependency rules.
fn check_integrations(metadata: &Metadata, policy: &Policy) -> Result<()> {
    let mut errors = Vec::new();

    for pkg in &metadata.packages {
        if !metadata.workspace_members.contains(&pkg.id) {
            continue;
        }

        // Check MCP integration
        if get_integration(pkg, "mcp")
            && let Some(rule) = &policy.integrations.mcp
        {
            let has_any = if rule.any_of.is_empty() {
                true
            } else {
                rule.any_of.iter().any(|n| has_dep(pkg, n))
            };
            let has_all = rule.all_of.iter().all(|n| has_dep(pkg, n));

            if !has_any || !has_all {
                errors.push(format!(
                    "{}: MCP integration enabled but missing required dependencies.\n  {}",
                    pkg.name,
                    rule.message
                        .as_deref()
                        .unwrap_or("Check policy.toml for requirements.")
                ));
            }
        }

        // Check logging integration
        if get_integration(pkg, "logging")
            && let Some(rule) = &policy.integrations.logging
        {
            let has_all = rule.all_of.iter().all(|n| has_dep(pkg, n));
            if !has_all {
                errors.push(format!(
                    "{}: Logging integration enabled but missing required dependencies.\n  {}",
                    pkg.name,
                    rule.message
                        .as_deref()
                        .unwrap_or("Check policy.toml for requirements.")
                ));
            }
        }

        // Check NAPI integration
        if get_integration(pkg, "napi")
            && let Some(rule) = &policy.integrations.napi
        {
            let has_any = if rule.any_of.is_empty() {
                true
            } else {
                rule.any_of.iter().any(|n| has_dep(pkg, n))
            };
            if !has_any {
                errors.push(format!(
                    "{}: NAPI integration enabled but missing required dependencies.\n  {}",
                    pkg.name,
                    rule.message
                        .as_deref()
                        .unwrap_or("Check policy.toml for requirements.")
                ));
            }
        }
    }

    if !errors.is_empty() {
        bail!("Integration validation failed:\n\n{}", errors.join("\n\n"));
    }

    Ok(())
}

/// Check path constraints (when enforced).
fn check_paths(policy: &Policy) -> Result<()> {
    if !policy.paths.enforce {
        eprintln!("[verify] NOTE: Path constraints are not enforced (planned for Plan 5).");
        return Ok(());
    }

    // Path enforcement will be implemented in Plan 5
    Ok(())
}

/// Check that generated files are not gitignored.
fn check_generated_not_gitignored(paths: &[&str]) -> Result<()> {
    for p in paths {
        let status = Command::new("git").args(["check-ignore", "-q", p]).status();

        if let Ok(s) = status
            && s.success()
        {
            bail!(
                "Generated file {} is gitignored; remove ignore rule to comply with policy.",
                p
            );
        }
    }
    Ok(())
}

/// Collect all generated paths including per-crate CLAUDE.md files.
fn collect_generated_paths(metadata: &Metadata) -> Vec<String> {
    let mut paths = vec!["CLAUDE.md".to_string(), "release-plz.toml".to_string()];
    let ws_root = metadata.workspace_root.as_std_path();

    for pkg in &metadata.packages {
        if !metadata.workspace_members.contains(&pkg.id) {
            continue;
        }
        if let Some(dir) = pkg.manifest_path.as_std_path().parent() {
            let path = dir.join("CLAUDE.md");
            let rel = path
                .strip_prefix(ws_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let rel = rel.trim_start_matches('/').to_string();
            paths.push(rel);
        }
    }
    paths
}

/// Run the verify command.
pub fn run(check: bool) -> Result<()> {
    eprintln!("[verify] Loading workspace metadata...");
    let metadata = MetadataCommand::new()
        .no_deps()
        .exec()
        .context("Failed to run `cargo metadata`")?;

    eprintln!("[verify] Loading policy from tools/policy.toml...");
    let policy = Policy::load()?;

    // 1) Metadata validation
    eprintln!("[verify] Checking metadata presence and validity...");
    check_metadata(&metadata, &policy)?;

    // 2) Integration rules
    eprintln!("[verify] Checking integration dependencies...");
    check_integrations(&metadata, &policy)?;

    // 3) Path constraints
    eprintln!("[verify] Checking path constraints...");
    check_paths(&policy)?;

    // 4) Generated files freshness
    if check {
        eprintln!("[verify] Checking generated files are up to date...");
        // This will fail if any files need updating
        sync::run(true, true)?;
    }

    // 5) Generated files not gitignored
    eprintln!("[verify] Checking generated files are not gitignored...");
    let gen_paths = collect_generated_paths(&metadata);
    let gen_path_refs: Vec<&str> = gen_paths.iter().map(|s| s.as_str()).collect();
    check_generated_not_gitignored(&gen_path_refs)?;

    eprintln!("[verify] All checks passed!");
    Ok(())
}
