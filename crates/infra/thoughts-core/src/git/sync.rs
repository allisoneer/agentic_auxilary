use crate::git::shell_fetch;
use crate::git::shell_push::push_current_branch;
use crate::git::utils::is_worktree_dirty;
use anyhow::Context;
use anyhow::Result;
use chrono::DateTime;
use chrono::Utc;
use colored::*;
use git2::IndexAddOption;
use git2::Repository;
use git2::Signature;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// Minimal struct for parsing log entries during merge.
/// Only fields needed for deduplication and sorting.
#[derive(Debug, Deserialize, Serialize)]
struct LogEntryForMerge {
    call_id: String,
    started_at: DateTime<Utc>,
    #[serde(flatten)]
    rest: serde_json::Value,
}

/// Check if a path matches the tool logs pattern.
///
/// Tool log files are in `*/logs/tool_logs_*.jsonl` paths.
/// The `tool_logs_` prefix must appear immediately after `/logs/` to prevent
/// false positives on paths like `tool_logs_config/logs/readme.md`.
fn is_tool_log_file(path: &str) -> bool {
    if let Some(logs_idx) = path.find("/logs/") {
        let after_logs = &path[logs_idx + 6..]; // Skip "/logs/"
        after_logs.starts_with("tool_logs_") && path.ends_with(".jsonl")
    } else {
        false
    }
}

/// Merge two JSONL log files by deduplicating on call_id and sorting by started_at.
///
/// - Records are deduplicated by `call_id` (local/theirs wins on collision)
/// - Records are sorted chronologically by `started_at`
/// - Unparseable lines are preserved at the end of the merged output
fn merge_jsonl_logs(ours_content: &[u8], theirs_content: &[u8]) -> Vec<u8> {
    let mut records: HashMap<String, (DateTime<Utc>, String)> = HashMap::new();
    let mut unparseable_lines: Vec<String> = Vec::new();

    // Parse "ours" (remote/upstream) first
    for line in String::from_utf8_lossy(ours_content).lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<LogEntryForMerge>(line) {
            Ok(entry) => {
                records.insert(entry.call_id.clone(), (entry.started_at, line.to_string()));
            }
            Err(_) => {
                unparseable_lines.push(line.to_string());
            }
        }
    }

    // Parse "theirs" (local) - wins on collision since it's the newer version being replayed
    for line in String::from_utf8_lossy(theirs_content).lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<LogEntryForMerge>(line) {
            Ok(entry) => {
                // Local wins on collision (overwrite)
                records.insert(entry.call_id.clone(), (entry.started_at, line.to_string()));
            }
            Err(_) => {
                // Only add if not already in unparseable (avoid duplicates)
                if !unparseable_lines.contains(&line.to_string()) {
                    unparseable_lines.push(line.to_string());
                }
            }
        }
    }

    // Sort by started_at
    let mut sorted: Vec<_> = records.into_values().collect();
    sorted.sort_by_key(|(ts, _)| *ts);

    // Build output: sorted records, then unparseable lines
    let mut output = sorted
        .into_iter()
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n");

    if !unparseable_lines.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&unparseable_lines.join("\n"));
    }

    if !output.is_empty() {
        output.push('\n');
    }

    output.into_bytes()
}

/// Result of analyzing divergence between local and remote branches.
#[allow(dead_code)] // is_ahead and is_behind are tested in unit tests (divergence_* tests below)
pub(crate) struct DivergenceAnalysis {
    /// Local and remote have diverged (both have unique commits)
    pub(crate) is_diverged: bool,
    /// Local is ahead of remote (has commits not on remote)
    pub(crate) is_ahead: bool,
    /// Local is behind remote (remote has commits not on local)
    pub(crate) is_behind: bool,
}

pub struct GitSync {
    repo: Repository,
    repo_path: std::path::PathBuf,
    subpath: Option<String>,
}

impl GitSync {
    pub fn new(repo_path: &Path, subpath: Option<String>) -> Result<Self> {
        let repo = Repository::open(repo_path)?;
        Ok(Self {
            repo,
            repo_path: repo_path.to_path_buf(),
            subpath,
        })
    }

    pub async fn sync(&self, mount_name: &str) -> Result<()> {
        println!("  {} {}", "Syncing".cyan(), mount_name);

        // 1. PRE-FLIGHT: Fetch first to know remote state before committing
        if let Err(e) = self.preflight_fetch() {
            println!("    {} Pre-flight fetch failed: {}", "⚠".yellow(), e);
            // Continue anyway - we'll try to sync what we can
        }

        // 2. Stage changes (respecting subpath)
        let changes_staged = self.stage_changes().await?;

        // 3. Commit if there are changes
        if changes_staged {
            self.commit(mount_name).await?;
            println!("    {} Committed changes", "✓".green());
        } else {
            println!("    {} No changes to commit", "○".dimmed());
        }

        // 4. Pull with rebase (may be fast-forward now if no local changes)
        match self.pull_rebase().await {
            Ok(pulled) => {
                if pulled {
                    println!("    {} Pulled remote changes", "✓".green());
                }
            }
            Err(e) => {
                println!("    {} Pull failed: {}", "⚠".yellow(), e);
                // Continue anyway - will try to push local changes
            }
        }

        // 5. Push (non-fatal)
        match self.push().await {
            Ok(_) => println!("    {} Pushed to remote", "✓".green()),
            Err(e) => {
                println!("    {} Push failed: {}", "⚠".yellow(), e);
                println!("      {} Changes saved locally only", "Info".dimmed());
            }
        }

        Ok(())
    }

    /// Pre-flight fetch to update remote refs before committing.
    ///
    /// This enables early divergence detection and cleaner error messages.
    fn preflight_fetch(&self) -> Result<()> {
        // Check if origin exists
        if self.repo.find_remote("origin").is_err() {
            return Ok(()); // No remote, nothing to fetch
        }

        // Fetch using shell git (uses system SSH, triggers 1Password)
        shell_fetch::fetch(&self.repo_path, "origin")?;

        // Log divergence status for visibility
        if let Ok(analysis) = self.check_divergence()
            && analysis.is_diverged
        {
            println!(
                "    {} Detected divergence from remote - will attempt rebase",
                "Info".dimmed()
            );
        }

        Ok(())
    }

    /// Check if local and remote branches have diverged.
    pub(crate) fn check_divergence(&self) -> Result<DivergenceAnalysis> {
        let head = self.repo.head()?;
        let branch_name = head.shorthand().unwrap_or("HEAD");
        let upstream_ref = format!("refs/remotes/origin/{}", branch_name);

        let local_oid = head
            .target()
            .ok_or_else(|| anyhow::anyhow!("No HEAD target"))?;

        let upstream_oid = match self.repo.refname_to_id(&upstream_ref) {
            Ok(oid) => oid,
            Err(_) => {
                // No upstream branch yet - local is ahead
                return Ok(DivergenceAnalysis {
                    is_diverged: false,
                    is_ahead: true,
                    is_behind: false,
                });
            }
        };

        // Use graph_ahead_behind for accurate commit counts instead of merge_analysis
        // which doesn't distinguish between ahead-only, behind-only, and diverged states
        let (ahead, behind) = self.repo.graph_ahead_behind(local_oid, upstream_oid)?;

        Ok(DivergenceAnalysis {
            is_diverged: ahead > 0 && behind > 0,
            is_ahead: ahead > 0,
            is_behind: behind > 0,
        })
    }

    async fn stage_changes(&self) -> Result<bool> {
        let mut index = self.repo.index()?;

        // Get the pathspec for staging
        let pathspecs: Vec<String> = if let Some(subpath) = &self.subpath {
            // Only stage files within subpath
            // Use glob pattern to match all files recursively
            vec![
                format!("{}/*", subpath),    // Files directly in subpath
                format!("{}/**/*", subpath), // Files in subdirectories
            ]
        } else {
            // Stage all changes in repo
            vec![".".to_string()]
        };

        // Configure flags for proper subpath handling
        let flags = IndexAddOption::DEFAULT;

        // Track if we staged anything
        let mut staged_files = 0;

        // Stage new and modified files with callback to track what we're staging
        let cb = &mut |_path: &std::path::Path, _matched_spec: &[u8]| -> i32 {
            staged_files += 1;
            0 // Include this file
        };

        // Add all matching files
        index.add_all(
            pathspecs.iter(),
            flags,
            Some(cb as &mut git2::IndexMatchedPath),
        )?;

        // Update index to catch deletions in the pathspec
        index.update_all(pathspecs.iter(), None)?;

        index.write()?;

        // Check if we actually have changes to commit
        // Handle empty repo case where HEAD doesn't exist yet
        let diff = match self.repo.head() {
            Ok(head) => {
                let head_tree = self.repo.find_commit(head.target().unwrap())?.tree()?;
                self.repo
                    .diff_tree_to_index(Some(&head_tree), Some(&index), None)?
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
                // Empty repo - no HEAD yet, so everything in index is new
                self.repo.diff_tree_to_index(None, Some(&index), None)?
            }
            Err(e) => return Err(e.into()),
        };

        Ok(diff.stats()?.files_changed() > 0)
    }

    async fn commit(&self, mount_name: &str) -> Result<()> {
        let sig = Signature::now("thoughts-sync", "thoughts@sync.local")?;
        let tree_id = self.repo.index()?.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;

        // Create descriptive commit message
        let message = if let Some(subpath) = &self.subpath {
            format!("Auto-sync thoughts for {mount_name} (subpath: {subpath})")
        } else {
            format!("Auto-sync thoughts for {mount_name}")
        };

        // Handle both initial commit and subsequent commits
        match self.repo.head() {
            Ok(head) => {
                // Normal commit with parent
                let parent = self.repo.find_commit(head.target().unwrap())?;
                self.repo
                    .commit(Some("HEAD"), &sig, &sig, &message, &tree, &[&parent])?;
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
                // Initial commit - no parents
                self.repo.commit(
                    Some("HEAD"),
                    &sig,
                    &sig,
                    &message,
                    &tree,
                    &[], // No parents for initial commit
                )?;
            }
            Err(e) => return Err(e.into()),
        }

        Ok(())
    }

    async fn pull_rebase(&self) -> Result<bool> {
        // Check if origin exists
        if self.repo.find_remote("origin").is_err() {
            println!(
                "    {} No remote 'origin' configured (local-only)",
                "Info".dimmed()
            );
            return Ok(false);
        }

        // Fetch using shell git (uses system SSH, triggers 1Password)
        shell_fetch::fetch(&self.repo_path, "origin").with_context(|| {
            format!(
                "Fetch from origin failed for repo '{}'",
                self.repo_path.display()
            )
        })?;

        // Get current branch
        let head = self.repo.head()?;
        let branch_name = head.shorthand().unwrap_or("main");

        // Try to find the upstream commit
        let upstream_oid = match self
            .repo
            .refname_to_id(&format!("refs/remotes/origin/{branch_name}"))
        {
            Ok(oid) => oid,
            Err(_) => {
                // No upstream branch yet
                return Ok(false);
            }
        };

        let upstream_commit = self.repo.find_annotated_commit(upstream_oid)?;
        let head_commit = self.repo.find_annotated_commit(head.target().unwrap())?;

        // Check if we need to rebase
        let analysis = self.repo.merge_analysis(&[&upstream_commit])?;

        if analysis.0.is_up_to_date() {
            return Ok(false);
        }

        if analysis.0.is_fast_forward() {
            // Safety gate: never force-checkout over local changes
            if is_worktree_dirty(&self.repo)? {
                anyhow::bail!(
                    "Cannot fast-forward: working tree has uncommitted changes. Please commit or stash before syncing."
                );
            }
            // TODO(3): Migrate to gitoxide when worktree update support is added upstream
            // (currently marked incomplete in gitoxide README)
            // Fast-forward: update ref, index, and working tree atomically
            let obj = self.repo.find_object(upstream_oid, None)?;
            self.repo.reset(
                &obj,
                git2::ResetType::Hard,
                Some(git2::build::CheckoutBuilder::default().force()),
            )?;
            return Ok(true);
        }

        // Need to rebase - wrap in closure with abort safety net
        let mut rebase =
            self.repo
                .rebase(Some(&head_commit), Some(&upstream_commit), None, None)?;

        let rebase_result: Result<bool> = (|| {
            while let Some(operation) = rebase.next() {
                // Fix: properly handle errors instead of silent discard
                let _op =
                    operation.map_err(|e| anyhow::anyhow!("Rebase operation failed: {}", e))?;

                if self.repo.index()?.has_conflicts() {
                    // Resolve conflicts by preferring remote
                    self.resolve_conflicts_prefer_remote()?;
                }
                rebase.commit(
                    None,
                    &Signature::now("thoughts-sync", "thoughts@sync.local")?,
                    None,
                )?;
            }
            rebase.finish(None)?;
            Ok(true)
        })();

        // Safety net: abort rebase on any failure to prevent stuck state
        if rebase_result.is_err() {
            let _ = rebase.abort(); // Best-effort cleanup, ignore abort errors
        }

        rebase_result
    }

    async fn push(&self) -> Result<()> {
        if self.repo.find_remote("origin").is_err() {
            println!(
                "    {} No remote 'origin' configured (local-only)",
                "Info".dimmed()
            );
            return Ok(());
        }

        let head = self.repo.head()?;
        let branch = head.shorthand().unwrap_or("main");

        // Use shell git push (triggers 1Password SSH prompts)
        push_current_branch(&self.repo_path, "origin", branch)?;
        Ok(())
    }

    /// Resolve conflicts by preferring the remote/upstream version.
    ///
    /// IMPORTANT: During rebase, libgit2 inverts ours/theirs semantics:
    /// - `conflict.our` = upstream commit (what we're rebasing onto) = REMOTE
    /// - `conflict.their` = local commit being replayed = LOCAL
    ///
    /// So to prefer remote, we use `conflict.our`, not `conflict.their`.
    ///
    /// Special handling for tool log files (`*/logs/tool_logs_*.jsonl`):
    /// - Parse both sides as JSONL
    /// - Deduplicate by `call_id` (local wins on collision)
    /// - Sort by `started_at` timestamp
    /// - Preserve unparseable lines at the end
    fn resolve_conflicts_prefer_remote(&self) -> Result<()> {
        let mut index = self.repo.index()?;
        let conflicts: Vec<_> = index.conflicts()?.collect::<Result<Vec<_>, _>>()?;

        for conflict in conflicts {
            // Get the path from whichever side exists
            let path = conflict
                .our
                .as_ref()
                .or(conflict.their.as_ref())
                .map(|e| String::from_utf8_lossy(&e.path).to_string());

            let path_str = path.as_deref().unwrap_or("");

            // Smart merge for tool log files
            if is_tool_log_file(path_str)
                && let (Some(our), Some(their)) = (&conflict.our, &conflict.their)
            {
                let our_blob = self.repo.find_blob(our.id)?;
                let their_blob = self.repo.find_blob(their.id)?;

                let merged = merge_jsonl_logs(our_blob.content(), their_blob.content());

                // Write merged content to working tree file
                let file_path = self.repo_path.join(path_str);
                std::fs::write(&file_path, &merged)?;

                // Write merged content back to index
                index.add_frombuffer(our, &merged)?;
                continue;
            }

            // Standard resolution: prefer upstream/remote (conflict.our during rebase)
            if let Some(our) = conflict.our {
                index.add(&our)?;
            } else if let Some(their) = conflict.their {
                // Fallback to local if no remote version exists
                index.add(&their)?;
            }
            // If both are None, the file was deleted on both sides - nothing to add
        }

        index.write()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_jsonl_deduplicates_by_call_id() {
        let ours = br#"{"call_id":"abc","started_at":"2025-01-01T10:00:00Z","tool":"foo"}
{"call_id":"def","started_at":"2025-01-01T11:00:00Z","tool":"bar"}"#;
        let theirs = br#"{"call_id":"abc","started_at":"2025-01-01T10:00:00Z","tool":"foo_updated"}
{"call_id":"ghi","started_at":"2025-01-01T12:00:00Z","tool":"baz"}"#;

        let merged = merge_jsonl_logs(ours, theirs);
        let merged_str = String::from_utf8_lossy(&merged);

        // Should have 3 unique records, abc should have "foo_updated" (theirs wins)
        assert!(merged_str.contains("foo_updated"));
        assert!(!merged_str.contains(r#""tool":"foo""#)); // Original overwritten
        assert!(merged_str.contains("def"));
        assert!(merged_str.contains("ghi"));
    }

    #[test]
    fn test_merge_jsonl_preserves_unparseable() {
        let ours = b"not valid json\n";
        let theirs = br#"{"call_id":"abc","started_at":"2025-01-01T10:00:00Z","tool":"foo"}"#;

        let merged = merge_jsonl_logs(ours, theirs);
        let merged_str = String::from_utf8_lossy(&merged);

        assert!(merged_str.contains("not valid json"));
        assert!(merged_str.contains("call_id"));
    }

    #[test]
    fn test_merge_jsonl_sorts_by_timestamp() {
        let ours = br#"{"call_id":"late","started_at":"2025-01-01T15:00:00Z","tool":"c"}"#;
        let theirs = br#"{"call_id":"early","started_at":"2025-01-01T09:00:00Z","tool":"a"}
{"call_id":"mid","started_at":"2025-01-01T12:00:00Z","tool":"b"}"#;

        let merged = merge_jsonl_logs(ours, theirs);
        let merged_str = String::from_utf8_lossy(&merged);
        let lines: Vec<_> = merged_str.lines().collect();

        assert!(lines[0].contains("early"));
        assert!(lines[1].contains("mid"));
        assert!(lines[2].contains("late"));
    }

    #[test]
    fn test_merge_jsonl_empty_files() {
        let merged = merge_jsonl_logs(b"", b"");
        assert!(merged.is_empty());
    }

    #[test]
    fn test_merge_jsonl_one_side_empty() {
        let content = br#"{"call_id":"abc","started_at":"2025-01-01T10:00:00Z","tool":"foo"}"#;

        let merged_ours_empty = merge_jsonl_logs(b"", content);
        assert!(String::from_utf8_lossy(&merged_ours_empty).contains("abc"));

        let merged_theirs_empty = merge_jsonl_logs(content, b"");
        assert!(String::from_utf8_lossy(&merged_theirs_empty).contains("abc"));
    }

    #[test]
    fn test_is_tool_log_file() {
        // Valid tool log paths
        assert!(is_tool_log_file("branch/logs/tool_logs_2025-01-01.jsonl"));
        assert!(is_tool_log_file(
            "foo/logs/tool_logs_2025-01-01_abc123.jsonl"
        ));
        assert!(is_tool_log_file("a/b/c/logs/tool_logs_whatever.jsonl"));

        // Invalid: wrong filename in logs directory
        assert!(!is_tool_log_file("branch/logs/other.jsonl"));

        // Invalid: tool_logs_ in wrong directory
        assert!(!is_tool_log_file(
            "branch/research/tool_logs_2025-01-01.jsonl"
        ));

        // Invalid: wrong extension
        assert!(!is_tool_log_file("branch/logs/tool_logs_2025-01-01.json"));

        // Invalid: tool_logs_ appears BEFORE /logs/ (false positive that tighter check prevents)
        assert!(!is_tool_log_file("tool_logs_config/logs/readme.jsonl"));
        assert!(!is_tool_log_file("tool_logs_foo/logs/bar.jsonl"));

        // Invalid: no /logs/ directory at all
        assert!(!is_tool_log_file("tool_logs_2025-01-01.jsonl"));
    }

    // -------------------------------------------------------------------------
    // Divergence detection unit tests
    // These test check_divergence() return values for various git graph states.
    // -------------------------------------------------------------------------

    /// Helper: run git command and assert success
    fn git_ok(dir: &std::path::Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("failed to spawn git");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Helper: get trimmed stdout from git command
    fn git_stdout(dir: &std::path::Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("failed to spawn git");
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Test: No upstream ref exists (fresh local repo, no remote tracking branch).
    /// Expected: is_diverged=false, is_ahead=true, is_behind=false
    #[test]
    fn divergence_no_upstream_ref() {
        let repo = tempfile::TempDir::new().unwrap();
        git_ok(repo.path(), &["init"]);
        std::fs::write(repo.path().join("a.txt"), "a").unwrap();
        git_ok(repo.path(), &["add", "."]);
        git_ok(
            repo.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "initial",
            ],
        );

        let sync = GitSync::new(repo.path(), None).unwrap();
        let analysis = sync.check_divergence().unwrap();

        assert!(!analysis.is_diverged, "should not be diverged");
        assert!(analysis.is_ahead, "should be ahead (no upstream)");
        assert!(!analysis.is_behind, "should not be behind");
    }

    /// Test: Local and remote are at the same commit.
    /// Expected: is_diverged=false, is_ahead=false, is_behind=false
    #[test]
    fn divergence_up_to_date() {
        let repo = tempfile::TempDir::new().unwrap();
        git_ok(repo.path(), &["init"]);
        std::fs::write(repo.path().join("a.txt"), "a").unwrap();
        git_ok(repo.path(), &["add", "."]);
        git_ok(
            repo.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "initial",
            ],
        );
        // Normalize branch name (git init may create master or main depending on config)
        git_ok(repo.path(), &["branch", "-M", "main"]);

        let head_oid = git_stdout(repo.path(), &["rev-parse", "HEAD"]);
        git_ok(
            repo.path(),
            &["update-ref", "refs/remotes/origin/main", &head_oid],
        );

        let sync = GitSync::new(repo.path(), None).unwrap();
        let analysis = sync.check_divergence().unwrap();

        assert!(!analysis.is_diverged, "should not be diverged");
        assert!(!analysis.is_ahead, "should not be ahead");
        assert!(!analysis.is_behind, "should not be behind");
    }

    /// Test: Local has commits that remote doesn't (local ahead only).
    /// Expected: is_diverged=false, is_ahead=true, is_behind=false
    #[test]
    fn divergence_local_ahead_only() {
        let repo = tempfile::TempDir::new().unwrap();
        git_ok(repo.path(), &["init"]);
        std::fs::write(repo.path().join("a.txt"), "a").unwrap();
        git_ok(repo.path(), &["add", "."]);
        git_ok(
            repo.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "C1",
            ],
        );
        // Normalize branch name (git init may create master or main depending on config)
        git_ok(repo.path(), &["branch", "-M", "main"]);

        let c1_oid = git_stdout(repo.path(), &["rev-parse", "HEAD"]);
        git_ok(
            repo.path(),
            &["update-ref", "refs/remotes/origin/main", &c1_oid],
        );

        std::fs::write(repo.path().join("b.txt"), "b").unwrap();
        git_ok(repo.path(), &["add", "."]);
        git_ok(
            repo.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "C2",
            ],
        );

        let sync = GitSync::new(repo.path(), None).unwrap();
        let analysis = sync.check_divergence().unwrap();

        assert!(!analysis.is_diverged, "should not be diverged");
        assert!(analysis.is_ahead, "should be ahead");
        assert!(!analysis.is_behind, "should not be behind");
    }

    /// Test: Remote has commits that local doesn't (local behind only).
    /// Expected: is_diverged=false, is_ahead=false, is_behind=true
    #[test]
    fn divergence_local_behind_only() {
        let repo = tempfile::TempDir::new().unwrap();
        git_ok(repo.path(), &["init"]);
        std::fs::write(repo.path().join("a.txt"), "a").unwrap();
        git_ok(repo.path(), &["add", "."]);
        git_ok(
            repo.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "C1",
            ],
        );
        // Normalize branch name (git init may create master or main depending on config)
        git_ok(repo.path(), &["branch", "-M", "main"]);

        std::fs::write(repo.path().join("b.txt"), "b").unwrap();
        git_ok(repo.path(), &["add", "."]);
        git_ok(
            repo.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "C2",
            ],
        );

        let c2_oid = git_stdout(repo.path(), &["rev-parse", "HEAD"]);
        git_ok(repo.path(), &["reset", "--hard", "HEAD~1"]);
        git_ok(
            repo.path(),
            &["update-ref", "refs/remotes/origin/main", &c2_oid],
        );

        let sync = GitSync::new(repo.path(), None).unwrap();
        let analysis = sync.check_divergence().unwrap();

        assert!(!analysis.is_diverged, "should not be diverged");
        assert!(!analysis.is_ahead, "should not be ahead");
        assert!(analysis.is_behind, "should be behind");
    }

    /// Test: Both local and remote have unique commits (diverged).
    /// Expected: is_diverged=true, is_ahead=true, is_behind=true
    #[test]
    fn divergence_diverged() {
        let repo = tempfile::TempDir::new().unwrap();
        git_ok(repo.path(), &["init"]);
        std::fs::write(repo.path().join("a.txt"), "a").unwrap();
        git_ok(repo.path(), &["add", "."]);
        git_ok(
            repo.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "C1",
            ],
        );
        // Normalize branch name (git init may create master or main depending on config)
        git_ok(repo.path(), &["branch", "-M", "main"]);

        let c1_oid = git_stdout(repo.path(), &["rev-parse", "HEAD"]);

        std::fs::write(repo.path().join("b.txt"), "b").unwrap();
        git_ok(repo.path(), &["add", "."]);
        git_ok(
            repo.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "C2-local",
            ],
        );

        git_ok(repo.path(), &["branch", "remote-sim", &c1_oid]);
        git_ok(repo.path(), &["checkout", "remote-sim"]);
        std::fs::write(repo.path().join("c.txt"), "c").unwrap();
        git_ok(repo.path(), &["add", "."]);
        git_ok(
            repo.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "C3-remote",
            ],
        );

        let c3_oid = git_stdout(repo.path(), &["rev-parse", "HEAD"]);
        git_ok(repo.path(), &["checkout", "main"]);
        git_ok(
            repo.path(),
            &["update-ref", "refs/remotes/origin/main", &c3_oid],
        );

        let sync = GitSync::new(repo.path(), None).unwrap();
        let analysis = sync.check_divergence().unwrap();

        assert!(analysis.is_diverged, "should be diverged");
        assert!(analysis.is_ahead, "should be ahead");
        assert!(analysis.is_behind, "should be behind");
    }
}
