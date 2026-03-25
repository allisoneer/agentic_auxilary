//! Query-time working tree overlay helpers.

use anyhow::Result;
use git2::{Repository, Status, StatusOptions};
use std::path::{Path, PathBuf};

/// Dirty/untracked working-tree files to verify against the base index.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct OverlayPaths {
    pub dirty_tracked: Vec<PathBuf>,
    pub untracked: Vec<PathBuf>,
}

pub fn overlay_paths(repo_root: &Path) -> Result<OverlayPaths> {
    let repo = Repository::open(repo_root)?;
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true)
        .exclude_submodules(true);

    let statuses = repo.statuses(Some(&mut opts))?;
    let mut dirty_tracked = Vec::new();
    let mut untracked = Vec::new();

    for entry in statuses.iter() {
        let Some(rel_path) = entry.path() else {
            continue;
        };
        let path = repo_root.join(rel_path);
        let status = entry.status();
        if status.contains(Status::WT_NEW) {
            untracked.push(path);
        } else if status != Status::CURRENT {
            dirty_tracked.push(path);
        }
    }

    dirty_tracked.sort();
    dirty_tracked.dedup();
    untracked.sort();
    untracked.dedup();

    Ok(OverlayPaths {
        dirty_tracked,
        untracked,
    })
}

#[cfg(test)]
#[expect(clippy::unwrap_used)]
mod tests {
    use super::*;
    use git2::{Repository, Signature};
    use tempfile::TempDir;

    fn init_repo() -> (TempDir, Repository) {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        std::fs::write(tmp.path().join("tracked.txt"), "hello\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("tracked.txt")).unwrap();
        index.write().unwrap();
        let sig = Signature::now("Test", "test@example.com").unwrap();
        {
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
                .unwrap();
        }
        (tmp, repo)
    }

    #[test]
    fn overlay_paths_reports_dirty_and_untracked_files() {
        let (tmp, _repo) = init_repo();
        std::fs::write(tmp.path().join("tracked.txt"), "changed\n").unwrap();
        std::fs::write(tmp.path().join("new.txt"), "new\n").unwrap();

        let overlay = overlay_paths(tmp.path()).unwrap();
        assert!(
            overlay
                .dirty_tracked
                .iter()
                .any(|p| p.ends_with("tracked.txt"))
        );
        assert!(overlay.untracked.iter().any(|p| p.ends_with("new.txt")));
    }
}
