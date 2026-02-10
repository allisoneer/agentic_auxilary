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
use std::path::Path;
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
        /// true if ALL duplicate entries are auto-managed (used for messaging nuance)
        #[allow(dead_code)]
        auto_managed: bool,
    },
    /// Mapped path does not exist
    MissingPath {
        url: String,
        path: String,
        auto_managed: bool,
    },
    /// Mapped path exists but is not a directory
    NotADirectory {
        url: String,
        path: String,
        auto_managed: bool,
    },
    /// Mapped path is not a git repository
    NotAGitRepo {
        url: String,
        path: String,
        auto_managed: bool,
    },
    /// Origin remote doesn't match the mapped URL's identity
    OriginMismatch {
        url: String,
        path: String,
        expected: String,
        actual: String,
        auto_managed: bool,
    },
}

impl Issue {
    fn print(&self) {
        match self {
            Issue::DuplicateIdentity {
                canonical_key,
                urls,
                auto_managed: _,
            } => {
                println!(
                    "{} Duplicate canonical identity: {}",
                    "⚠".yellow(),
                    canonical_key
                );
                for url in urls {
                    println!("    - {}", url);
                }
                println!(
                    "    Fix: Run 'thoughts references doctor --fix' to consolidate mappings deterministically."
                );
            }
            Issue::MissingPath {
                url,
                path,
                auto_managed,
            } => {
                println!("{} Missing path for {}", "✗".red(), url);
                println!("    Path: {}", path);
                if *auto_managed {
                    println!(
                        "    Fix: Run 'thoughts references doctor --fix' to remove the stale auto-managed mapping."
                    );
                } else {
                    println!(
                        "    Fix: This mapping is user-managed; update/remove it in repos.json (doctor --fix will not modify user-managed entries)."
                    );
                }
            }
            Issue::NotADirectory {
                url,
                path,
                auto_managed,
            } => {
                println!("{} Path is not a directory for {}", "✗".red(), url);
                println!("    Path: {}", path);
                if *auto_managed {
                    println!(
                        "    Fix: Run 'thoughts references doctor --fix' to remove the stale auto-managed mapping."
                    );
                } else {
                    println!(
                        "    Fix: This mapping is user-managed; update/remove it in repos.json (doctor --fix will not modify user-managed entries)."
                    );
                }
            }
            Issue::NotAGitRepo {
                url,
                path,
                auto_managed,
            } => {
                println!("{} Path is not a git repository for {}", "⚠".yellow(), url);
                println!("    Path: {}", path);
                if *auto_managed {
                    println!(
                        "    Fix: Run 'thoughts references doctor --fix' to remove the stale auto-managed mapping."
                    );
                } else {
                    println!(
                        "    Fix: This mapping is user-managed; update/remove it in repos.json (doctor --fix will not modify user-managed entries)."
                    );
                }
            }
            Issue::OriginMismatch {
                url,
                path,
                expected,
                actual,
                auto_managed,
            } => {
                println!("{} Origin mismatch for {}", "⚠".yellow(), url);
                println!("    Path: {}", path);
                println!("    Expected identity: {}", expected);
                println!("    Actual origin: {}", actual);
                if *auto_managed {
                    println!(
                        "    Fix: Run 'thoughts references doctor --fix' to remove the auto-managed mapping."
                    );
                    println!(
                        "         Then re-clone the correct repo (doctor does not delete directories; you may need to remove/rename the existing directory)."
                    );
                } else {
                    println!(
                        "    Fix: This mapping is user-managed; update the mapping URL or fix the repository origin manually."
                    );
                }
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
                auto_managed: location.auto_managed,
            });
            continue;
        }

        // Check if path is a directory
        if !path.is_dir() {
            issues.push(Issue::NotADirectory {
                url: url.clone(),
                path: path_str.clone(),
                auto_managed: location.auto_managed,
            });
            continue;
        }

        // Check if path is a git repo
        if !is_git_repo(path) {
            issues.push(Issue::NotAGitRepo {
                url: url.clone(),
                path: path_str.clone(),
                auto_managed: location.auto_managed,
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
                            auto_managed: location.auto_managed,
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
            // Determine if ALL duplicates are auto-managed
            let all_auto = urls.iter().all(|u| {
                mapping
                    .mappings
                    .get(u)
                    .map(|loc| loc.auto_managed)
                    .unwrap_or(false)
            });
            issues.push(Issue::DuplicateIdentity {
                canonical_key: format!("{}/{}/{}", key.host, key.org_path, key.repo),
                urls: urls.clone(),
                auto_managed: all_auto,
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

/// Compute the health ranking for a mapping entry.
///
/// Higher ranks are healthier:
/// - 4: Healthy (origin matches expected identity)
/// - 3: Origin unknown (missing or unparsable)
/// - 2: Origin mismatch
/// - 1: Directory exists but not a git repo
/// - 0: Missing or not a directory
fn health_rank(url: &str, path: &Path) -> u8 {
    // Rank 0: missing or not a directory
    if !path.exists() || !path.is_dir() {
        return 0;
    }

    // Rank 1: directory but not a git repo
    if !is_git_repo(path) {
        return 1;
    }

    // Git repo: determine origin status
    let (base_url, _) = parse_url_and_subpath(url);
    let expected = match RepoIdentity::parse(&base_url) {
        Ok(id) => id.canonical_key(),
        Err(_) => return 3, // treat as origin unknown
    };

    match try_get_origin_identity(path) {
        Ok(Some(actual)) => {
            if actual.canonical_key() == expected {
                4 // healthy
            } else {
                2 // origin mismatch
            }
        }
        Ok(None) => 3, // origin unknown (missing or unparsable)
        Err(_) => 3,   // origin unknown on error
    }
}

fn apply_fixes(issues: &[Issue]) -> Result<()> {
    let mapping_mgr = RepoMappingManager::new()?;
    // Use load_locked() to prevent concurrent RMW races with sync operations
    let (mut mapping, _lock) = mapping_mgr.load_locked()?;
    let mut fixed_count = 0;

    for issue in issues {
        match issue {
            Issue::MissingPath { url, .. }
            | Issue::NotADirectory { url, .. }
            | Issue::NotAGitRepo { url, .. }
            | Issue::OriginMismatch { url, .. } => {
                // Only remove auto-managed entries
                if let Some(loc) = mapping.mappings.get(url)
                    && loc.auto_managed
                {
                    println!(
                        "{} Removing broken auto-managed entry: {}",
                        "↻".green(),
                        url
                    );
                    mapping.mappings.remove(url);
                    fixed_count += 1;
                }
            }
            Issue::DuplicateIdentity { urls, .. } => {
                // Build candidates with sorting criteria:
                // (url, health_rank, user_specified, last_sync)
                let mut candidates: Vec<(String, u8, bool, Option<chrono::DateTime<chrono::Utc>>)> =
                    urls.iter()
                        .filter_map(|u| {
                            let loc = mapping.mappings.get(u)?;
                            let rank = health_rank(u, &loc.path);
                            let user_specified = !loc.auto_managed;
                            Some((u.clone(), rank, user_specified, loc.last_sync))
                        })
                        .collect();

                if candidates.len() <= 1 {
                    continue;
                }

                // Preserve the best last_sync across all duplicates
                let best_last_sync = candidates.iter().filter_map(|c| c.3).max();

                // Sort: health_rank DESC, user_specified DESC, last_sync DESC, url ASC
                candidates.sort_by(|a, b| {
                    b.1.cmp(&a.1) // health_rank DESC
                        .then_with(|| b.2.cmp(&a.2)) // user_specified DESC
                        .then_with(|| b.3.cmp(&a.3)) // last_sync DESC
                        .then_with(|| a.0.cmp(&b.0)) // url ASC (deterministic tiebreaker)
                });

                let winner_url = candidates[0].0.clone();
                println!("{} Keeping winner entry: {}", "✓".green(), winner_url);

                // Remove all losers
                for (loser_url, ..) in candidates.iter().skip(1) {
                    println!("{} Removing duplicate entry: {}", "↻".green(), loser_url);
                    mapping.mappings.remove(loser_url);
                    fixed_count += 1;
                }

                // Preserve best last_sync on winner
                if let Some(ts) = best_last_sync
                    && let Some(loc) = mapping.mappings.get_mut(&winner_url)
                {
                    loc.last_sync = Some(ts);
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    // ===== health_rank tests =====

    #[test]
    fn test_health_rank_missing_path_returns_0() {
        let path = std::path::Path::new("/nonexistent/path/that/does/not/exist");
        let rank = health_rank("https://github.com/org/repo", path);
        assert_eq!(rank, 0);
    }

    #[test]
    fn test_health_rank_file_not_dir_returns_0() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("not_a_dir.txt");
        std::fs::write(&file_path, "content").unwrap();
        let rank = health_rank("https://github.com/org/repo", &file_path);
        assert_eq!(rank, 0);
    }

    #[test]
    fn test_health_rank_dir_not_git_returns_1() {
        let dir = tempdir().unwrap();
        let rank = health_rank("https://github.com/org/repo", dir.path());
        assert_eq!(rank, 1);
    }

    #[test]
    fn test_health_rank_git_repo_matching_origin_returns_4() {
        let dir = tempdir().unwrap();
        let repo_path = dir.path();

        // Initialize a git repo with matching origin
        let repo = git2::Repository::init(repo_path).unwrap();
        repo.remote("origin", "https://github.com/org/repo.git")
            .unwrap();

        let rank = health_rank("https://github.com/org/repo", repo_path);
        assert_eq!(rank, 4);
    }

    #[test]
    fn test_health_rank_git_repo_mismatched_origin_returns_2() {
        let dir = tempdir().unwrap();
        let repo_path = dir.path();

        // Initialize a git repo with mismatched origin
        let repo = git2::Repository::init(repo_path).unwrap();
        repo.remote("origin", "https://github.com/other-org/other-repo.git")
            .unwrap();

        let rank = health_rank("https://github.com/org/repo", repo_path);
        assert_eq!(rank, 2);
    }

    #[test]
    fn test_health_rank_git_repo_no_origin_returns_3() {
        let dir = tempdir().unwrap();
        let repo_path = dir.path();

        // Initialize a git repo without setting origin
        git2::Repository::init(repo_path).unwrap();

        let rank = health_rank("https://github.com/org/repo", repo_path);
        assert_eq!(rank, 3);
    }

    #[test]
    fn test_health_rank_unparseable_url_returns_3() {
        let dir = tempdir().unwrap();
        let repo_path = dir.path();
        git2::Repository::init(repo_path).unwrap();

        // Use an unparseable URL
        let rank = health_rank("not-a-valid-url", repo_path);
        assert_eq!(rank, 3);
    }

    // ===== Duplicate sorting tests =====

    #[test]
    fn test_duplicate_sorting_prefers_higher_health() {
        let t1 = chrono::Utc::now();
        let mut candidates = [
            ("url_a".to_string(), 2u8, true, Some(t1)), // lower health
            ("url_b".to_string(), 4u8, true, Some(t1)), // higher health
        ];

        candidates.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| b.3.cmp(&a.3))
                .then_with(|| a.0.cmp(&b.0))
        });

        assert_eq!(candidates[0].0, "url_b");
    }

    #[test]
    fn test_duplicate_sorting_prefers_user_specified_when_health_equal() {
        let t1 = chrono::Utc::now();
        let mut candidates = [
            ("url_a".to_string(), 4u8, false, Some(t1)), // auto-managed
            ("url_b".to_string(), 4u8, true, Some(t1)),  // user-specified
        ];

        candidates.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| b.3.cmp(&a.3))
                .then_with(|| a.0.cmp(&b.0))
        });

        assert_eq!(candidates[0].0, "url_b");
    }

    #[test]
    fn test_duplicate_sorting_prefers_newer_sync_when_tied() {
        let t1 = chrono::Utc::now();
        let t2 = t1 + chrono::Duration::hours(1);
        let mut candidates = [
            ("url_a".to_string(), 4u8, true, Some(t1)), // older sync
            ("url_b".to_string(), 4u8, true, Some(t2)), // newer sync
        ];

        candidates.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| b.3.cmp(&a.3))
                .then_with(|| a.0.cmp(&b.0))
        });

        assert_eq!(candidates[0].0, "url_b");
    }

    #[test]
    fn test_duplicate_sorting_uses_url_alphabetical_as_tiebreaker() {
        let t1 = chrono::Utc::now();
        let mut candidates = [
            ("url_z".to_string(), 4u8, true, Some(t1)),
            ("url_a".to_string(), 4u8, true, Some(t1)),
        ];

        candidates.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| b.3.cmp(&a.3))
                .then_with(|| a.0.cmp(&b.0))
        });

        assert_eq!(candidates[0].0, "url_a");
    }

    #[test]
    fn test_duplicate_sorting_full_chain() {
        // Test that health > user_specified > last_sync > url_alphabetical
        let t1 = chrono::Utc::now();
        let t2 = t1 + chrono::Duration::hours(1);
        let mut candidates = [
            ("url_z".to_string(), 2u8, true, Some(t2)),  // low health
            ("url_a".to_string(), 4u8, false, Some(t2)), // high health, auto
            ("url_b".to_string(), 4u8, true, Some(t1)),  // high health, user, old sync
            ("url_c".to_string(), 4u8, true, Some(t2)),  // high health, user, new sync (winner)
        ];

        candidates.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| b.3.cmp(&a.3))
                .then_with(|| a.0.cmp(&b.0))
        });

        assert_eq!(candidates[0].0, "url_c");
        assert_eq!(candidates[1].0, "url_b");
        assert_eq!(candidates[2].0, "url_a");
        assert_eq!(candidates[3].0, "url_z");
    }
}
