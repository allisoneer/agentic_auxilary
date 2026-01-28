//! Verify command implementation.
//!
//! Validates metadata, policy rules, and generated file freshness.

use crate::policy::{IntegrationRule, Policy, TodoPolicy};
use crate::sync;
use anyhow::{Context, Result, bail};
use cargo_metadata::{Metadata, MetadataCommand, Package};
use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
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

/// Validate an integration's dependency rule for a package.
///
/// This helper is intentionally dual-purpose:
/// - It *guards* on the integration being enabled in `package.metadata.repo.integrations.<key>`
///   (and on a policy rule existing), and
/// - It validates both `any_of` and `all_of` dependency constraints when enabled.
///
/// Returns `Some(error_message)` on failure; otherwise `None` (disabled, no rule, or satisfied).
fn validate_integration(
    pkg: &Package,
    key: &str,
    label: &str,
    rule: Option<&IntegrationRule>,
) -> Option<String> {
    if !get_integration(pkg, key) {
        return None;
    }
    let rule = rule?;

    let has_any = if rule.any_of.is_empty() {
        true
    } else {
        rule.any_of.iter().any(|n| has_dep(pkg, n))
    };
    let has_all = rule.all_of.iter().all(|n| has_dep(pkg, n));

    if !has_any || !has_all {
        Some(format!(
            "{}: {} integration enabled but missing required dependencies.\n  {}",
            pkg.name,
            label,
            rule.message
                .as_deref()
                .unwrap_or("Check policy.toml for requirements.")
        ))
    } else {
        None
    }
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
        if let Some(err) = validate_integration(pkg, "mcp", "MCP", policy.integrations.mcp.as_ref())
        {
            errors.push(err);
        }

        // Check logging integration
        if let Some(err) = validate_integration(
            pkg,
            "logging",
            "Logging",
            policy.integrations.logging.as_ref(),
        ) {
            errors.push(err);
        }

        // Check NAPI integration
        if let Some(err) =
            validate_integration(pkg, "napi", "NAPI", policy.integrations.napi.as_ref())
        {
            errors.push(err);
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
        // TODO(2): Implement path enforcement logic using policy.paths.allow patterns
        // (planned in Plan 5, deferred). Flip enforce = true in policy.toml when ready.
        eprintln!("[verify] NOTE: Path constraints are not enforced.");
        return Ok(());
    }

    // TODO(2): Validate workspace crate paths against policy.paths.allow glob patterns
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

/// Check TODO annotation conventions across all git-tracked files.
///
/// Two validations in one scan:
/// 1. **Blocked severity**: any `TODO(N)` where N is in `blocked_severities` fails.
/// 2. **Format enforcement**: any uppercase `TODO` not in `TODO(0-3)` form fails.
fn check_todo_annotations(ws_root: &Path, todos: &TodoPolicy) -> Result<()> {
    let tagged_re = Regex::new(r"TODO\s*\(\s*([0-3])\s*\)").expect("valid regex");
    let any_todo_re = Regex::new(r"\bTODO\b").expect("valid regex");

    // Enumerate git-tracked files via `git ls-files -z`.
    // Uses output() to read both stdout/stderr concurrently (avoids pipe deadlock).
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(ws_root)
        .output()
        .context("Failed to run `git ls-files -z`")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("`git ls-files -z` failed with {}: {stderr}", output.status);
    }

    let raw = output.stdout;
    let mut blocked: Vec<String> = Vec::new();
    let mut format_violations: Vec<String> = Vec::new();

    for path_bytes in raw.split(|&b| b == 0) {
        if path_bytes.is_empty() {
            continue;
        }
        let rel_path = String::from_utf8_lossy(path_bytes);

        // Check ignore_paths prefixes.
        if todos
            .ignore_paths
            .iter()
            .any(|prefix| rel_path.starts_with(prefix.as_str()))
        {
            continue;
        }

        let abs_path = ws_root.join(rel_path.as_ref());

        // Skip symlinks: avoids blocking on FUSE mounts (e.g. thoughts three-space
        // symlinks like context/, personal/, references/ that point to .thoughts-data/).
        // symlink_metadata() uses lstat which does NOT follow the symlink.
        match abs_path.symlink_metadata() {
            Ok(meta) if meta.file_type().is_symlink() => continue,
            Ok(meta) if !meta.file_type().is_file() => continue,
            Err(_) => continue,
            _ => {}
        }

        let file = match File::open(&abs_path) {
            Ok(f) => f,
            Err(_) => continue, // file may have been deleted since ls-files
        };

        // Binary detection: check first 512 bytes for NUL.
        let mut reader = BufReader::new(file);
        let mut header = [0u8; 512];
        let n = reader.read(&mut header).unwrap_or(0);
        if header[..n].contains(&0) {
            continue; // binary file
        }

        // Seek back to start by re-opening (BufReader consumed bytes).
        let file = match File::open(&abs_path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = BufReader::new(file);

        for (line_idx, line_result) in reader.lines().enumerate() {
            let line = match line_result {
                Ok(l) => l,
                Err(_) => continue, // skip decode errors
            };
            let line_num = line_idx + 1;

            // Check for tagged TODOs with blocked severities.
            for cap in tagged_re.captures_iter(&line) {
                if let Some(m) = cap.get(1)
                    && let Ok(severity) = m.as_str().parse::<u8>()
                    && todos.blocked_severities.contains(&severity)
                {
                    blocked.push(format!("{}:{}: {}", rel_path, line_num, line.trim()));
                }
            }

            // Check format enforcement: every \bTODO\b must have a tagged match at the same offset.
            for m in any_todo_re.find_iter(&line) {
                let start = m.start();
                let has_tag = tagged_re.find_iter(&line).any(|tm| tm.start() == start);
                if !has_tag {
                    format_violations.push(format!("{}:{}: {}", rel_path, line_num, line.trim()));
                }
            }
        }
    }

    if blocked.is_empty() && format_violations.is_empty() {
        return Ok(());
    }

    let mut msg = String::new();
    if !blocked.is_empty() {
        msg.push_str(&format!(
            "Blocked TODO severities {:?} found ({} occurrence{}):\n  {}",
            todos.blocked_severities,
            blocked.len(),
            if blocked.len() == 1 { "" } else { "s" },
            blocked.join("\n  ")
        ));
    }
    if !format_violations.is_empty() {
        if !msg.is_empty() {
            msg.push_str("\n\n");
        }
        msg.push_str(&format!(
            "TODO format violations ({} occurrence{}): TODO must be tagged as TODO(0), TODO(1), TODO(2), or TODO(3).\n  {}",
            format_violations.len(),
            if format_violations.len() == 1 { "" } else { "s" },
            format_violations.join("\n  ")
        ));
    }

    bail!("{msg}");
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

    // 4) TODO annotations
    eprintln!("[verify] Checking TODO annotations...");
    check_todo_annotations(metadata.workspace_root.as_std_path(), &policy.todos)?;

    // 5) Generated files freshness
    if check {
        eprintln!("[verify] Checking generated files are up to date...");
        // This will fail if any files need updating
        sync::run(true, true)?;
    }

    // 6) Generated files not gitignored
    eprintln!("[verify] Checking generated files are not gitignored...");
    let gen_paths = collect_generated_paths(&metadata);
    let gen_path_refs: Vec<&str> = gen_paths.iter().map(|s| s.as_str()).collect();
    check_generated_not_gitignored(&gen_path_refs)?;

    eprintln!("[verify] All checks passed!");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::TodoPolicy;
    use std::process::Command;

    /// Create a temporary git repo, write files, stage them, and return the temp dir.
    fn setup_git_repo(files: &[(&str, &[u8])]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create temp dir");
        Command::new("git")
            .args(["init", "--initial-branch=main"])
            .current_dir(dir.path())
            .output()
            .expect("git init");

        // Configure git user for commits (required by some git versions).
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .expect("git config email");
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir.path())
            .output()
            .expect("git config name");

        for (name, content) in files {
            let path = dir.path().join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create parent dirs");
            }
            std::fs::write(&path, content).expect("write file");
        }

        Command::new("git")
            .args(["add", "-A"])
            .current_dir(dir.path())
            .output()
            .expect("git add");

        dir
    }

    #[test]
    fn blocks_todo_0() {
        let dir = setup_git_repo(&[("a.rs", b"// TODO(0): nope\n")]);
        let policy = TodoPolicy {
            blocked_severities: vec![0],
            ignore_paths: vec![],
        };
        let result = check_todo_annotations(dir.path(), &policy);
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Blocked TODO severities"),
            "expected blocked message, got: {err}"
        );
        assert!(err.contains("a.rs:1"), "expected file:line ref, got: {err}");
    }

    #[test]
    fn flags_untagged_todo() {
        let dir = setup_git_repo(&[("a.rs", b"// TODO: tag me\n")]);
        let policy = TodoPolicy {
            blocked_severities: vec![0],
            ignore_paths: vec![],
        };
        let result = check_todo_annotations(dir.path(), &policy);
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("TODO format violations"),
            "expected format violation, got: {err}"
        );
        assert!(err.contains("a.rs:1"), "expected file:line ref, got: {err}");
    }

    #[test]
    fn does_not_match_lowercase_todo_calls() {
        let dir = setup_git_repo(&[("a.rs", b"sessions.todo(\"s1\")\n")]);
        let policy = TodoPolicy {
            blocked_severities: vec![0],
            ignore_paths: vec![],
        };
        let result = check_todo_annotations(dir.path(), &policy);
        assert!(
            result.is_ok(),
            "lowercase todo should not trigger: {result:?}"
        );
    }

    #[test]
    fn ignores_configured_paths() {
        let dir = setup_git_repo(&[("CLAUDE.md", b"TODO(0): convention definition\n")]);
        let policy = TodoPolicy {
            blocked_severities: vec![0],
            ignore_paths: vec!["CLAUDE.md".to_string()],
        };
        let result = check_todo_annotations(dir.path(), &policy);
        assert!(
            result.is_ok(),
            "ignored path should not trigger: {result:?}"
        );
    }

    #[test]
    fn skips_binary_files_by_nul_detection() {
        // Put a NUL byte in the first 512 bytes, then a TODO(0) later.
        let mut content = vec![0u8; 100];
        content.extend_from_slice(b"TODO(0): should be skipped\n");
        let dir = setup_git_repo(&[("binary.bin", &content)]);
        let policy = TodoPolicy {
            blocked_severities: vec![0],
            ignore_paths: vec![],
        };
        let result = check_todo_annotations(dir.path(), &policy);
        assert!(result.is_ok(), "binary file should be skipped: {result:?}");
    }
}
