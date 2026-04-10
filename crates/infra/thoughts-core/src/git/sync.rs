use crate::git::shell_fetch;
use crate::git::shell_push::PushFailureKind;
use crate::git::shell_push::push_current_branch_with_result;
use crate::git::utils::ensure_repo_ready_for_sync;
use crate::git::utils::get_sync_branch;
use crate::git::utils::is_worktree_dirty;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use chrono::DateTime;
use chrono::Utc;
use colored::Colorize;
use git2::Commit;
use git2::ErrorCode;
use git2::Index;
use git2::IndexAddOption;
use git2::Oid;
use git2::Repository;
use git2::Signature;
use git2::Tree;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;

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
        after_logs.starts_with("tool_logs_")
            && std::path::Path::new(path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("jsonl"))
    } else {
        false
    }
}

/// Merge two JSONL log files by deduplicating on `call_id` and sorting by `started_at`.
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
pub(crate) struct DivergenceAnalysis {
    /// Local and remote have diverged (both have unique commits)
    pub(crate) is_diverged: bool,
    /// Local is ahead of remote (has commits not on remote)
    pub(crate) is_ahead: bool,
    /// Local is behind remote (remote has commits not on local)
    pub(crate) is_behind: bool,
}

const MAX_PUSH_RETRIES: u32 = 3;
const RETRY_BASE_MS: u64 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncRelation {
    NoUpstream,
    UpToDate,
    AheadOnly,
    BehindOnly,
    Diverged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncAttemptOutcome {
    NoHeadChange,
    FastForwarded,
    Committed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PushRaceResetMode {
    Mixed,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitParentPlan {
    None,
    HeadOnly,
    UpstreamOnly,
    HeadAndUpstream,
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

    #[expect(
        clippy::future_not_send,
        reason = "git2::Repository is Send but not Sync; this is a known limitation"
    )]
    pub async fn sync(&self, mount_name: &str) -> Result<()> {
        println!("  {} {}", "Syncing".cyan(), mount_name);

        ensure_repo_ready_for_sync(&self.repo_path)?;

        // Check for remote before get_sync_branch() so unborn branches work without remotes
        if self.repo.find_remote("origin").is_err() {
            println!(
                "    {} No remote 'origin' configured (local-only)",
                "Info".dimmed()
            );
            self.sync_without_remote(mount_name)?;
            return Ok(());
        }

        // get_sync_branch will fail for unborn/detached with a helpful error message
        let branch_name = get_sync_branch(&self.repo_path)?;

        for attempt in 0..MAX_PUSH_RETRIES {
            let attempt_head = self.head_commit_oid()?;
            let sync_outcome = self.sync_once(mount_name, &branch_name)?;

            let push_result =
                push_current_branch_with_result(&self.repo_path, "origin", &branch_name)?;
            if push_result.success {
                println!("    {} Pushed to remote", "✓".green());
                return Ok(());
            }

            let failure_kind = push_result.failure_kind.unwrap_or(PushFailureKind::Other);
            if failure_kind == PushFailureKind::Race && attempt < MAX_PUSH_RETRIES - 1 {
                println!(
                    "    {} Push race detected; retrying after re-fetch",
                    "Info".dimmed()
                );
                let reset_mode = match sync_outcome {
                    SyncAttemptOutcome::FastForwarded => PushRaceResetMode::Hard,
                    SyncAttemptOutcome::NoHeadChange | SyncAttemptOutcome::Committed => {
                        PushRaceResetMode::Mixed
                    }
                };
                self.reset_after_push_race(attempt_head, reset_mode)?;
                sleep(Duration::from_millis(RETRY_BASE_MS * 2u64.pow(attempt))).await;
                continue;
            }

            let stderr = push_result.stderr.trim();
            if stderr.is_empty() {
                bail!("git push failed ({failure_kind:?})");
            }
            bail!("git push failed ({failure_kind:?}): {stderr}");
        }

        bail!("git push race retry budget exhausted after {MAX_PUSH_RETRIES} attempts")
    }

    fn sync_without_remote(&self, mount_name: &str) -> Result<()> {
        let changes_staged = self.stage_changes()?;
        if !changes_staged {
            println!("    {} No changes to commit", "○".dimmed());
            return Ok(());
        }

        let head_commit = self.head_commit()?;
        let local_tree = self.local_tree_from_index()?;
        let commit_oid = self.create_commit_from_relation(
            mount_name,
            &local_tree,
            head_commit.as_ref(),
            None,
            SyncRelation::NoUpstream,
        )?;
        self.refresh_worktree_after_commit(commit_oid)?;
        println!("    {} Committed changes", "✓".green());
        Ok(())
    }

    fn sync_once(&self, mount_name: &str, branch_name: &str) -> Result<SyncAttemptOutcome> {
        shell_fetch::fetch(&self.repo_path, "origin").with_context(|| {
            format!(
                "Fetch from origin failed for repo '{}'",
                self.repo_path.display()
            )
        })?;

        let head_commit = self.head_commit()?;
        let upstream_commit = self.find_upstream_commit(branch_name)?;
        let relation =
            self.sync_relation(head_commit.as_ref(), upstream_commit.as_ref(), branch_name)?;

        if let Some(upstream_commit) = upstream_commit.as_ref()
            && self.should_premerge_before_staging(relation)?
        {
            self.premerge_jsonl_files(&upstream_commit.tree()?)?;
        }

        let changes_staged = self.stage_changes()?;
        let local_tree = self.local_tree_from_index()?;

        match relation {
            SyncRelation::NoUpstream => {
                if changes_staged {
                    let commit_oid = self.create_commit_from_relation(
                        mount_name,
                        &local_tree,
                        head_commit.as_ref(),
                        None,
                        relation,
                    )?;
                    self.refresh_worktree_after_commit(commit_oid)?;
                    println!("    {} Committed changes", "✓".green());
                    return Ok(SyncAttemptOutcome::Committed);
                }
                return Ok(SyncAttemptOutcome::NoHeadChange);
            }
            SyncRelation::UpToDate | SyncRelation::AheadOnly => {
                if changes_staged {
                    let commit_oid = self.create_commit_from_relation(
                        mount_name,
                        &local_tree,
                        head_commit.as_ref(),
                        upstream_commit.as_ref(),
                        relation,
                    )?;
                    self.refresh_worktree_after_commit(commit_oid)?;
                    println!("    {} Committed changes", "✓".green());
                    return Ok(SyncAttemptOutcome::Committed);
                }
                println!("    {} No changes to commit", "○".dimmed());
                return Ok(SyncAttemptOutcome::NoHeadChange);
            }
            SyncRelation::BehindOnly => {
                let upstream_commit = upstream_commit.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("Missing upstream commit for behind-only sync")
                })?;
                if !changes_staged {
                    self.fast_forward_to_commit(branch_name, upstream_commit)?;
                    println!("    {} Pulled remote changes", "✓".green());
                    return Ok(SyncAttemptOutcome::FastForwarded);
                }
            }
            SyncRelation::Diverged => {
                println!(
                    "    {} Detected divergence from remote - merging before commit",
                    "Info".dimmed()
                );
            }
        }

        let upstream_commit = upstream_commit
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing upstream commit for merge integration"))?;
        let merged_tree = self.integrate_local_tree(
            head_commit.as_ref(),
            &local_tree,
            upstream_commit,
            relation,
        )?;
        let commit_oid = self.create_commit_from_relation(
            mount_name,
            &merged_tree,
            head_commit.as_ref(),
            Some(upstream_commit),
            relation,
        )?;
        self.refresh_worktree_after_commit(commit_oid)?;
        println!("    {} Integrated remote changes", "✓".green());

        Ok(SyncAttemptOutcome::Committed)
    }

    fn should_premerge_before_staging(&self, relation: SyncRelation) -> Result<bool> {
        Ok(match relation {
            SyncRelation::Diverged => true,
            SyncRelation::BehindOnly => is_worktree_dirty(&self.repo)?,
            SyncRelation::NoUpstream | SyncRelation::UpToDate | SyncRelation::AheadOnly => false,
        })
    }

    /// Check if local and remote branches have diverged.
    pub(crate) fn check_divergence(&self, branch_name: &str) -> Result<DivergenceAnalysis> {
        let head = self.repo.head()?;
        let upstream_ref = format!("refs/remotes/origin/{branch_name}");

        let local_oid = head
            .target()
            .ok_or_else(|| anyhow::anyhow!("No HEAD target"))?;

        let Ok(upstream_oid) = self.repo.refname_to_id(&upstream_ref) else {
            // No upstream branch yet - local is ahead
            return Ok(DivergenceAnalysis {
                is_diverged: false,
                is_ahead: true,
                is_behind: false,
            });
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

    fn sync_relation(
        &self,
        head_commit: Option<&Commit<'_>>,
        upstream_commit: Option<&Commit<'_>>,
        branch_name: &str,
    ) -> Result<SyncRelation> {
        match (head_commit, upstream_commit) {
            (_, None) => Ok(SyncRelation::NoUpstream),
            (None, Some(_)) => Ok(SyncRelation::BehindOnly),
            (Some(_), Some(_)) => {
                let analysis = self.check_divergence(branch_name)?;
                Ok(
                    match (analysis.is_diverged, analysis.is_ahead, analysis.is_behind) {
                        (false, true, false) => SyncRelation::AheadOnly,
                        (false, false, true) => SyncRelation::BehindOnly,
                        (false, false, false) => SyncRelation::UpToDate,
                        // diverged (true, _, _) or any other combination
                        _ => SyncRelation::Diverged,
                    },
                )
            }
        }
    }

    fn head_commit(&self) -> Result<Option<Commit<'_>>> {
        match self.repo.head() {
            Ok(head) => {
                let target = head
                    .target()
                    .ok_or_else(|| anyhow::anyhow!("No HEAD target"))?;
                Ok(Some(self.repo.find_commit(target)?))
            }
            Err(e) if e.code() == ErrorCode::UnbornBranch => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn head_commit_oid(&self) -> Result<Option<Oid>> {
        Ok(self.head_commit()?.map(|commit| commit.id()))
    }

    fn find_upstream_commit(&self, branch_name: &str) -> Result<Option<Commit<'_>>> {
        match self
            .repo
            .refname_to_id(&format!("refs/remotes/origin/{branch_name}"))
        {
            Ok(oid) => Ok(Some(self.repo.find_commit(oid)?)),
            Err(_) => Ok(None),
        }
    }

    fn local_tree_from_index(&self) -> Result<Tree<'_>> {
        let mut index = self.repo.index()?;
        let tree_id = index.write_tree()?;
        self.repo.find_tree(tree_id).map_err(Into::into)
    }

    fn integrate_local_tree(
        &self,
        head_commit: Option<&Commit<'_>>,
        local_tree: &Tree<'_>,
        upstream_commit: &Commit<'_>,
        relation: SyncRelation,
    ) -> Result<Tree<'_>> {
        let ancestor_tree_id =
            self.ancestor_tree_for_merge(head_commit, upstream_commit, relation)?;
        let ancestor_tree = self.repo.find_tree(ancestor_tree_id)?;
        let upstream_tree = upstream_commit.tree()?;
        let mut merged_index =
            self.repo
                .merge_trees(&ancestor_tree, local_tree, &upstream_tree, None)?;

        if merged_index.has_conflicts() {
            self.resolve_merge_conflicts(&mut merged_index)?;
        }
        if merged_index.has_conflicts() {
            bail!("Failed to resolve merge conflicts before final commit");
        }

        let tree_id = merged_index.write_tree_to(&self.repo)?;
        self.repo.find_tree(tree_id).map_err(Into::into)
    }

    fn ancestor_tree_for_merge(
        &self,
        head_commit: Option<&Commit<'_>>,
        upstream_commit: &Commit<'_>,
        relation: SyncRelation,
    ) -> Result<Oid> {
        match relation {
            SyncRelation::BehindOnly => match head_commit {
                Some(head_commit) => Ok(head_commit.tree_id()),
                None => self.empty_tree().map(|tree| tree.id()),
            },
            SyncRelation::Diverged => {
                let head_commit = head_commit
                    .ok_or_else(|| anyhow::anyhow!("Missing HEAD commit for diverged merge"))?;
                match self.repo.merge_base(head_commit.id(), upstream_commit.id()) {
                    Ok(merge_base_oid) => Ok(self.repo.find_commit(merge_base_oid)?.tree_id()),
                    Err(_) => self.empty_tree().map(|tree| tree.id()),
                }
            }
            _ => self.empty_tree().map(|tree| tree.id()),
        }
    }

    fn empty_tree(&self) -> Result<Tree<'_>> {
        let mut index = Index::new()?;
        let tree_id = index.write_tree_to(&self.repo)?;
        self.repo.find_tree(tree_id).map_err(Into::into)
    }

    fn resolve_merge_conflicts(&self, index: &mut Index) -> Result<()> {
        // Stage bits are in flags bits 12-13. Clear them to make stage-0 (resolved) entries.
        const GIT_INDEX_ENTRY_STAGEMASK: u16 = 0x3000;

        let conflicts: Vec<_> = index
            .conflicts()?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        for conflict in conflicts {
            let path = conflict
                .our
                .as_ref()
                .or(conflict.their.as_ref())
                .or(conflict.ancestor.as_ref())
                .map(|entry| String::from_utf8_lossy(&entry.path).to_string())
                .unwrap_or_default();

            if is_tool_log_file(&path)
                && let (Some(local), Some(remote)) = (&conflict.our, &conflict.their)
            {
                let local_blob = self.repo.find_blob(local.id)?;
                let remote_blob = self.repo.find_blob(remote.id)?;
                let merged = merge_jsonl_logs(remote_blob.content(), local_blob.content());

                // Write merged content to disk for worktree consistency
                let file_path = self.repo_path.join(&path);
                if let Some(parent) = file_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&file_path, &merged)?;

                // Write merged bytes to ODB and add via IndexEntry (not add_path)
                // merge_trees() returns an in-memory index without workdir backing,
                // so add_path() would fail. We create the blob manually and add the entry.
                let blob_oid = self.repo.blob(&merged)?;

                // Remove conflict entries (stage 1, 2, 3) before adding the resolved entry.
                // Without this, index.add() only replaces the matching stage slot,
                // leaving other conflict entries and has_conflicts() still returns true.
                index.conflict_remove(Path::new(&path))?;

                let entry = git2::IndexEntry {
                    id: blob_oid,
                    file_size: u32::try_from(merged.len()).unwrap_or(u32::MAX),
                    // Copy other fields from the local entry
                    ctime: local.ctime,
                    mtime: local.mtime,
                    dev: local.dev,
                    ino: local.ino,
                    mode: local.mode,
                    uid: local.uid,
                    gid: local.gid,
                    flags: local.flags & !GIT_INDEX_ENTRY_STAGEMASK,
                    flags_extended: local.flags_extended,
                    path: local.path.clone(),
                };
                index.add(&entry)?;
                continue;
            }

            // Non-JSONL conflict resolution: prefer remote (theirs) version
            match (&conflict.our, &conflict.their) {
                (_, Some(remote)) => {
                    // Remove conflict entries first, then add resolved stage-0 entry
                    index.conflict_remove(Path::new(&path))?;
                    let resolved = git2::IndexEntry {
                        ctime: remote.ctime,
                        mtime: remote.mtime,
                        dev: remote.dev,
                        ino: remote.ino,
                        mode: remote.mode,
                        uid: remote.uid,
                        gid: remote.gid,
                        file_size: remote.file_size,
                        id: remote.id,
                        flags: remote.flags & !GIT_INDEX_ENTRY_STAGEMASK,
                        flags_extended: remote.flags_extended,
                        path: remote.path.clone(),
                    };
                    index.add(&resolved)?;
                }
                (Some(_), None) => {
                    // File deleted on remote - remove it
                    index.conflict_remove(Path::new(&path))?;
                }
                (None, None) => {}
            }
        }

        // Note: Don't call index.write() - this is an in-memory index from merge_trees()
        // with no backing file. The caller uses write_tree_to(&self.repo) to persist.
        Ok(())
    }

    fn create_commit_from_relation(
        &self,
        mount_name: &str,
        tree: &Tree<'_>,
        head_commit: Option<&Commit<'_>>,
        upstream_commit: Option<&Commit<'_>>,
        relation: SyncRelation,
    ) -> Result<Oid> {
        match commit_parent_plan(relation, head_commit.is_some(), upstream_commit.is_some())? {
            CommitParentPlan::None => self.create_commit_for_tree(mount_name, tree, &[]),
            CommitParentPlan::HeadOnly => {
                let parents = head_commit.map(|commit| vec![commit]).unwrap_or_default();
                self.create_commit_for_tree(mount_name, tree, &parents)
            }
            CommitParentPlan::UpstreamOnly => {
                let upstream_commit = upstream_commit.ok_or_else(|| {
                    anyhow::anyhow!("Missing upstream commit for behind-only commit")
                })?;
                self.create_commit_for_tree(mount_name, tree, &[upstream_commit])
            }
            CommitParentPlan::HeadAndUpstream => {
                let head_commit = head_commit
                    .ok_or_else(|| anyhow::anyhow!("Missing HEAD commit for diverged commit"))?;
                let upstream_commit = upstream_commit.ok_or_else(|| {
                    anyhow::anyhow!("Missing upstream commit for diverged commit")
                })?;
                self.create_commit_for_tree(mount_name, tree, &[head_commit, upstream_commit])
            }
        }
    }

    fn create_commit_for_tree(
        &self,
        mount_name: &str,
        tree: &Tree<'_>,
        parents: &[&Commit<'_>],
    ) -> Result<Oid> {
        let sig = Signature::now("thoughts-sync", "thoughts@sync.local")?;
        let message = if let Some(subpath) = &self.subpath {
            format!("Auto-sync thoughts for {mount_name} (subpath: {subpath})")
        } else {
            format!("Auto-sync thoughts for {mount_name}")
        };

        // Create commit object without updating any ref.
        // This bypasses libgit2's parent validation which would fail when
        // parents[0] != HEAD.target() (e.g., for UpstreamOnly commits).
        let commit_oid = self
            .repo
            .commit(None, &sig, &sig, &message, tree, parents)?;

        // Update the branch ref to point to the new commit
        // Handle unborn branches (empty repo with no commits) by extracting
        // the target branch name from the symbolic HEAD reference.
        let (refname, is_branch) = match self.repo.head() {
            Ok(head_ref) => {
                let name = head_ref
                    .name()
                    .ok_or_else(|| anyhow::anyhow!("HEAD has no name"))?;
                (name.to_string(), head_ref.is_branch())
            }
            Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {
                // For unborn branches, HEAD is a symbolic ref pointing to a branch
                // that doesn't exist yet (e.g., refs/heads/main). We need to create it.
                let head_ref = self.repo.find_reference("HEAD")?;
                let symbolic_target = head_ref
                    .symbolic_target()
                    .ok_or_else(|| anyhow::anyhow!("HEAD has no symbolic target"))?;
                (symbolic_target.to_string(), true)
            }
            Err(e) => return Err(e.into()),
        };

        // For symbolic HEAD (normal case) or unborn branch, update/create the target branch
        // For detached HEAD, update HEAD directly
        if is_branch {
            self.repo.reference(
                &refname,
                commit_oid,
                true, // force
                &format!("thoughts-sync: {message}"),
            )?;
        } else {
            self.repo.set_head_detached(commit_oid)?;
        }

        Ok(commit_oid)
    }

    fn refresh_worktree_after_commit(&self, commit_oid: Oid) -> Result<()> {
        if self.subpath.is_some() {
            let commit = self.repo.find_commit(commit_oid)?;
            self.refresh_subpath_after_commit(&commit)?;
            return Ok(());
        }

        let obj = self.repo.find_object(commit_oid, None)?;
        self.repo.reset(
            &obj,
            git2::ResetType::Hard,
            Some(git2::build::CheckoutBuilder::default().force()),
        )?;
        Ok(())
    }

    fn refresh_subpath_after_commit(&self, commit: &Commit<'_>) -> Result<()> {
        let subpath = self
            .subpath
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Missing subpath for subpath refresh"))?;
        let tree = commit.tree()?;
        let mut checkout = git2::build::CheckoutBuilder::default();
        checkout.force().path(subpath);
        self.repo
            .checkout_tree(tree.as_object(), Some(&mut checkout))?;
        self.refresh_index_in_scope()
    }

    fn fast_forward_to_commit(
        &self,
        branch_name: &str,
        upstream_commit: &Commit<'_>,
    ) -> Result<()> {
        if is_worktree_dirty(&self.repo)? {
            bail!(
                "Cannot fast-forward: working tree has uncommitted changes. Please commit or stash before syncing."
            );
        }

        self.repo.set_head(&format!("refs/heads/{branch_name}"))?;
        let obj = self.repo.find_object(upstream_commit.id(), None)?;
        self.repo.reset(
            &obj,
            git2::ResetType::Hard,
            Some(git2::build::CheckoutBuilder::default().force()),
        )?;
        Ok(())
    }

    fn reset_after_push_race(
        &self,
        original_head: Option<Oid>,
        reset_mode: PushRaceResetMode,
    ) -> Result<()> {
        if let Some(original_head) = original_head {
            let obj = self.repo.find_object(original_head, None)?;
            match reset_mode {
                PushRaceResetMode::Mixed => {
                    self.repo.reset(&obj, git2::ResetType::Mixed, None)?;
                }
                PushRaceResetMode::Hard => {
                    self.repo.reset(
                        &obj,
                        git2::ResetType::Hard,
                        Some(git2::build::CheckoutBuilder::default().force()),
                    )?;
                }
            }
        } else {
            let branch_name = get_sync_branch(&self.repo_path)?;
            self.repo.set_head(&format!("refs/heads/{branch_name}"))?;
            self.repo.cleanup_state()?;
        }
        Ok(())
    }

    fn premerge_jsonl_files(&self, upstream_tree: &Tree<'_>) -> Result<()> {
        for rel_path in self.tool_log_files_in_scope()? {
            let Some(upstream_content) = self.read_tree_blob(upstream_tree, &rel_path)? else {
                continue;
            };

            let local_path = self.repo_path.join(&rel_path);
            let local_content = std::fs::read(&local_path)?;
            let merged = merge_jsonl_logs(&upstream_content, &local_content);
            if merged != local_content {
                std::fs::write(local_path, merged)?;
            }
        }
        Ok(())
    }

    fn tool_log_files_in_scope(&self) -> Result<Vec<String>> {
        let root = self.subpath.as_ref().map_or_else(
            || self.repo_path.clone(),
            |subpath| self.repo_path.join(subpath),
        );
        let mut files = Vec::new();
        self.collect_tool_log_files(&root, &mut files)?;
        files.sort();
        Ok(files)
    }

    fn collect_tool_log_files(&self, dir: &Path, files: &mut Vec<String>) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }

            if path.is_dir() {
                self.collect_tool_log_files(&path, files)?;
                continue;
            }

            let rel_path = path
                .strip_prefix(&self.repo_path)
                .with_context(|| format!("Failed to strip repo prefix from {}", path.display()))?;
            let rel_path = rel_path.to_string_lossy().replace('\\', "/");
            if is_tool_log_file(&rel_path) {
                files.push(rel_path);
            }
        }

        Ok(())
    }

    fn read_tree_blob(&self, tree: &Tree<'_>, rel_path: &str) -> Result<Option<Vec<u8>>> {
        let entry = match tree.get_path(Path::new(rel_path)) {
            Ok(entry) => entry,
            Err(err) if err.code() == ErrorCode::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };

        let blob = self.repo.find_blob(entry.id())?;
        Ok(Some(blob.content().to_vec()))
    }

    fn stage_changes(&self) -> Result<bool> {
        self.refresh_index_in_scope()?;

        let index = self.repo.index()?;

        // Check if we actually have changes to commit
        // Handle empty repo case where HEAD doesn't exist yet
        let diff = match self.repo.head() {
            Ok(head) => {
                let head_oid = head
                    .target()
                    .ok_or_else(|| anyhow::anyhow!("HEAD reference has no target"))?;
                let head_tree = self.repo.find_commit(head_oid)?.tree()?;
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

    fn refresh_index_in_scope(&self) -> Result<()> {
        let mut index = self.repo.index()?;
        let pathspecs = self.scoped_pathspecs();

        index.add_all(pathspecs.iter(), IndexAddOption::DEFAULT, None)?;

        // Update index to catch deletions in the pathspec
        index.update_all(pathspecs.iter(), None)?;

        index.write()?;

        Ok(())
    }

    fn scoped_pathspecs(&self) -> Vec<String> {
        if let Some(subpath) = &self.subpath {
            vec![format!("{}/*", subpath), format!("{}/**/*", subpath)]
        } else {
            vec![".".to_string()]
        }
    }
}

fn commit_parent_plan(
    relation: SyncRelation,
    has_head: bool,
    has_upstream: bool,
) -> Result<CommitParentPlan> {
    Ok(match relation {
        SyncRelation::NoUpstream | SyncRelation::UpToDate | SyncRelation::AheadOnly => {
            if has_head {
                CommitParentPlan::HeadOnly
            } else {
                CommitParentPlan::None
            }
        }
        SyncRelation::BehindOnly => {
            if !has_upstream {
                bail!("Missing upstream commit for behind-only commit");
            }
            CommitParentPlan::UpstreamOnly
        }
        SyncRelation::Diverged => {
            if !has_head || !has_upstream {
                bail!("Missing head or upstream commit for diverged commit");
            }
            CommitParentPlan::HeadAndUpstream
        }
    })
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
    fn test_merge_context_jsonl_keeps_local_on_collision() {
        let remote = br#"{"call_id":"same","started_at":"2025-01-01T10:00:00Z","tool":"remote"}"#;
        let local = br#"{"call_id":"same","started_at":"2025-01-01T10:00:00Z","tool":"local"}"#;

        let merged = merge_jsonl_logs(remote, local);
        let merged_str = String::from_utf8_lossy(&merged);

        assert!(merged_str.contains("local"));
        assert!(!merged_str.contains("remote"));
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

    #[test]
    fn commit_parent_plan_selects_expected_parents() {
        assert_eq!(
            commit_parent_plan(SyncRelation::NoUpstream, false, false).unwrap(),
            CommitParentPlan::None
        );
        assert_eq!(
            commit_parent_plan(SyncRelation::UpToDate, true, true).unwrap(),
            CommitParentPlan::HeadOnly
        );
        assert_eq!(
            commit_parent_plan(SyncRelation::AheadOnly, true, false).unwrap(),
            CommitParentPlan::HeadOnly
        );
        assert_eq!(
            commit_parent_plan(SyncRelation::BehindOnly, true, true).unwrap(),
            CommitParentPlan::UpstreamOnly
        );
        assert_eq!(
            commit_parent_plan(SyncRelation::Diverged, true, true).unwrap(),
            CommitParentPlan::HeadAndUpstream
        );
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
    /// Expected: `is_diverged=false`, `is_ahead=true`, `is_behind=false`
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
        let analysis = sync.check_divergence("main").unwrap();

        assert!(!analysis.is_diverged, "should not be diverged");
        assert!(analysis.is_ahead, "should be ahead (no upstream)");
        assert!(!analysis.is_behind, "should not be behind");
    }

    /// Test: Local and remote are at the same commit.
    /// Expected: `is_diverged=false`, `is_ahead=false`, `is_behind=false`
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
        let analysis = sync.check_divergence("main").unwrap();

        assert!(!analysis.is_diverged, "should not be diverged");
        assert!(!analysis.is_ahead, "should not be ahead");
        assert!(!analysis.is_behind, "should not be behind");
    }

    /// Test: Local has commits that remote doesn't (local ahead only).
    /// Expected: `is_diverged=false`, `is_ahead=true`, `is_behind=false`
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
        let analysis = sync.check_divergence("main").unwrap();

        assert!(!analysis.is_diverged, "should not be diverged");
        assert!(analysis.is_ahead, "should be ahead");
        assert!(!analysis.is_behind, "should not be behind");
    }

    /// Test: Remote has commits that local doesn't (local behind only).
    /// Expected: `is_diverged=false`, `is_ahead=false`, `is_behind=true`
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
        let analysis = sync.check_divergence("main").unwrap();

        assert!(!analysis.is_diverged, "should not be diverged");
        assert!(!analysis.is_ahead, "should not be ahead");
        assert!(analysis.is_behind, "should be behind");
    }

    /// Test: Both local and remote have unique commits (diverged).
    /// Expected: `is_diverged=true`, `is_ahead=true`, `is_behind=true`
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
        let analysis = sync.check_divergence("main").unwrap();

        assert!(analysis.is_diverged, "should be diverged");
        assert!(analysis.is_ahead, "should be ahead");
        assert!(analysis.is_behind, "should be behind");
    }

    #[test]
    fn refresh_worktree_after_commit_refreshes_only_subpath() {
        let repo = tempfile::TempDir::new().unwrap();
        git_ok(repo.path(), &["init"]);
        std::fs::create_dir_all(repo.path().join("branch")).unwrap();
        std::fs::write(repo.path().join("branch/data.txt"), "committed\n").unwrap();
        std::fs::write(repo.path().join("outside.txt"), "outside\n").unwrap();
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
        git_ok(repo.path(), &["branch", "-M", "main"]);

        std::fs::write(repo.path().join("branch/data.txt"), "stale branch\n").unwrap();
        std::fs::write(repo.path().join("outside.txt"), "stale outside\n").unwrap();
        git_ok(repo.path(), &["add", "branch/data.txt", "outside.txt"]);

        let sync = GitSync::new(repo.path(), Some("branch".to_string())).unwrap();
        let head_oid = Oid::from_str(&git_stdout(repo.path(), &["rev-parse", "HEAD"])).unwrap();

        sync.refresh_worktree_after_commit(head_oid).unwrap();

        assert_eq!(
            std::fs::read_to_string(repo.path().join("branch/data.txt")).unwrap(),
            "committed\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.path().join("outside.txt")).unwrap(),
            "stale outside\n"
        );

        let status = git_stdout(repo.path(), &["status", "--short"]);
        assert!(!status.contains("branch/data.txt"), "status was: {status}");
        assert!(status.contains("outside.txt"), "status was: {status}");
    }

    #[test]
    fn reset_after_push_race_hard_restores_fast_forwarded_worktree() {
        let repo = tempfile::TempDir::new().unwrap();
        git_ok(repo.path(), &["init"]);
        std::fs::write(repo.path().join("base.txt"), "one\n").unwrap();
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
                "c1",
            ],
        );
        git_ok(repo.path(), &["branch", "-M", "main"]);
        let c1 = git_stdout(repo.path(), &["rev-parse", "HEAD"]);

        std::fs::write(repo.path().join("base.txt"), "two\n").unwrap();
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
                "c2",
            ],
        );
        let c2 = git_stdout(repo.path(), &["rev-parse", "HEAD"]);
        git_ok(repo.path(), &["reset", "--hard", &c1]);

        let sync = GitSync::new(repo.path(), None).unwrap();
        let c2_commit = sync.repo.find_commit(Oid::from_str(&c2).unwrap()).unwrap();

        sync.fast_forward_to_commit("main", &c2_commit).unwrap();
        assert_eq!(
            std::fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "two\n"
        );

        sync.reset_after_push_race(Some(Oid::from_str(&c1).unwrap()), PushRaceResetMode::Hard)
            .unwrap();

        assert_eq!(git_stdout(repo.path(), &["rev-parse", "HEAD"]), c1);
        assert_eq!(
            std::fs::read_to_string(repo.path().join("base.txt")).unwrap(),
            "one\n"
        );
        assert!(git_stdout(repo.path(), &["status", "--short"]).is_empty());
    }
}
