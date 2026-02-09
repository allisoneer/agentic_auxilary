//! Diagnose reference repository mapping and clone issues.
//!
//! Detects:
//! - Duplicate mappings by canonical identity
//! - Missing/non-dir paths
//! - Non-git paths
//! - Origin mismatch vs mapped URL identity
//!
//! Optional `--fix` applies safe repairs:
//! - Dedupe identical-path entries
//! - Prune missing auto-managed entries

use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;
use thoughts_tool::config::repo_mapping_manager::parse_url_and_subpath;
use thoughts_tool::config::{RepoMapping, RepoMappingManager};
use thoughts_tool::git::utils::{is_git_repo, try_get_origin_identity};
use thoughts_tool::repo_identity::{RepoIdentity, RepoIdentityKey};
use thoughts_tool::utils::paths::get_repo_mapping_path;

/// Diagnostic issue found during doctor check.
#[derive(Debug)]
enum Issue {
    /// Multiple URLs map to the same canonical identity
    DuplicateIdentity {
        canonical_key: String,
        urls: Vec<String>,
    },
    /// Mapped path does not exist
    MissingPath { url: String, path: String },
    /// Mapped path exists but is not a directory
    NotADirectory { url: String, path: String },
    /// Mapped path is not a git repository
    NotAGitRepo { url: String, path: String },
    /// Origin remote doesn't match the mapped URL's identity
    OriginMismatch {
        url: String,
        path: String,
        expected: String,
        actual: String,
    },
}

impl Issue {
    fn print(&self) {
        match self {
            Issue::DuplicateIdentity {
                canonical_key,
                urls,
            } => {
                println!(
                    "{} Duplicate canonical identity: {}",
                    "⚠".yellow(),
                    canonical_key
                );
                for url in urls {
                    println!("    - {}", url);
                }
                println!("    Fix: Run 'thoughts references sync' to consolidate mappings");
            }
            Issue::MissingPath { url, path } => {
                println!("{} Missing path for {}", "✗".red(), url);
                println!("    Path: {}", path);
                println!("    Fix: Run 'thoughts references sync' or remove the reference");
            }
            Issue::NotADirectory { url, path } => {
                println!("{} Path is not a directory for {}", "✗".red(), url);
                println!("    Path: {}", path);
                println!("    Fix: Remove the file and run 'thoughts references sync'");
            }
            Issue::NotAGitRepo { url, path } => {
                println!("{} Path is not a git repository for {}", "⚠".yellow(), url);
                println!("    Path: {}", path);
                println!("    Fix: Remove the directory and run 'thoughts references sync'");
            }
            Issue::OriginMismatch {
                url,
                path,
                expected,
                actual,
            } => {
                println!("{} Origin mismatch for {}", "⚠".yellow(), url);
                println!("    Path: {}", path);
                println!("    Expected identity: {}", expected);
                println!("    Actual origin: {}", actual);
                println!("    Fix: Update the mapping URL or remove and re-sync");
            }
        }
    }
}

pub async fn execute(fix: bool) -> Result<()> {
    let mapping_path = get_repo_mapping_path()?;

    if !mapping_path.exists() {
        println!("{} No repos.json found - nothing to diagnose", "✓".green());
        return Ok(());
    }

    let contents = std::fs::read_to_string(&mapping_path)?;
    let mapping: RepoMapping = serde_json::from_str(&contents)?;

    if mapping.mappings.is_empty() {
        println!("{} repos.json is empty - nothing to diagnose", "✓".green());
        return Ok(());
    }

    println!("Checking {} mappings...\n", mapping.mappings.len());

    let mut issues: Vec<Issue> = Vec::new();

    // Build canonical identity map to detect duplicates
    let mut identity_map: HashMap<RepoIdentityKey, Vec<String>> = HashMap::new();

    for (url, location) in &mapping.mappings {
        let path = &location.path;
        let path_str = path.display().to_string();
        let (base_url, _) = parse_url_and_subpath(url);

        // Check if URL parses
        let identity = match RepoIdentity::parse(&base_url) {
            Ok(id) => Some(id),
            Err(_) => {
                println!("{} Cannot parse URL: {} (skipping)", "⚠".yellow(), url);
                None
            }
        };

        // Track canonical identity for duplicate detection
        if let Some(ref id) = identity {
            let key = id.canonical_key();
            identity_map.entry(key).or_default().push(url.clone());
        }

        // Check if path exists
        if !path.exists() {
            issues.push(Issue::MissingPath {
                url: url.clone(),
                path: path_str.clone(),
            });
            continue;
        }

        // Check if path is a directory
        if !path.is_dir() {
            issues.push(Issue::NotADirectory {
                url: url.clone(),
                path: path_str.clone(),
            });
            continue;
        }

        // Check if path is a git repo
        if !is_git_repo(path) {
            issues.push(Issue::NotAGitRepo {
                url: url.clone(),
                path: path_str.clone(),
            });
            continue;
        }

        // Check if origin matches
        if let Some(ref expected_id) = identity {
            match try_get_origin_identity(path) {
                Ok(Some(actual_id)) => {
                    let expected_key = expected_id.canonical_key();
                    let actual_key = actual_id.canonical_key();

                    if expected_key != actual_key {
                        issues.push(Issue::OriginMismatch {
                            url: url.clone(),
                            path: path_str.clone(),
                            expected: format!(
                                "{}/{}/{}",
                                expected_key.host, expected_key.org_path, expected_key.repo
                            ),
                            actual: format!(
                                "{}/{}/{}",
                                actual_key.host, actual_key.org_path, actual_key.repo
                            ),
                        });
                    }
                }
                Ok(None) => {
                    // No origin (or origin URL doesn't parse) — skip mismatch check.
                }
                Err(e) => {
                    tracing::warn!(
                        path = %path_str,
                        error = ?e,
                        "Could not verify origin identity (skipping origin check)"
                    );
                }
            }
        }
    }

    // Check for duplicate canonical identities
    for (key, urls) in &identity_map {
        if urls.len() > 1 {
            issues.push(Issue::DuplicateIdentity {
                canonical_key: format!("{}/{}/{}", key.host, key.org_path, key.repo),
                urls: urls.clone(),
            });
        }
    }

    // Print issues
    if issues.is_empty() {
        println!("{} All mappings are healthy!", "✓".green());
        return Ok(());
    }

    println!("Found {} issue(s):\n", issues.len());
    for issue in &issues {
        issue.print();
        println!();
    }

    // Apply fixes if requested
    if fix {
        apply_fixes(&issues)?;
    } else {
        println!(
            "Run with {} to apply safe automatic repairs.",
            "--fix".cyan()
        );
    }

    Ok(())
}

fn apply_fixes(issues: &[Issue]) -> Result<()> {
    let mapping_mgr = RepoMappingManager::new()?;
    let mut mapping = mapping_mgr.load()?;
    let mut fixed_count = 0;

    for issue in issues {
        match issue {
            Issue::MissingPath { url, .. } => {
                // Only remove auto-managed entries
                if let Some(loc) = mapping.mappings.get(url)
                    && loc.auto_managed
                {
                    println!(
                        "{} Removing missing auto-managed entry: {}",
                        "↻".green(),
                        url
                    );
                    mapping.mappings.remove(url);
                    fixed_count += 1;
                }
            }
            Issue::DuplicateIdentity { urls, .. } => {
                // If all duplicates point to the same path, keep only one
                let paths: Vec<_> = urls
                    .iter()
                    .filter_map(|u| mapping.mappings.get(u).map(|l| l.path.clone()))
                    .collect();

                if !paths.is_empty() && paths.iter().all(|p| p == &paths[0]) {
                    // All point to same path - remove all but first
                    for url in urls.iter().skip(1) {
                        println!("{} Removing duplicate entry: {}", "↻".green(), url);
                        mapping.mappings.remove(url);
                        fixed_count += 1;
                    }
                }
            }
            // Other issues require manual intervention
            _ => {}
        }
    }

    if fixed_count > 0 {
        mapping_mgr.save(&mapping)?;
        println!("\n{} Applied {} fix(es)", "✓".green(), fixed_count);
    } else {
        println!("\n{} No automatic fixes could be applied", "⚠".yellow());
        println!("Manual intervention required for remaining issues.");
    }

    Ok(())
}
