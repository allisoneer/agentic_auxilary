use std::fmt::Write;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

const MAX_GUIDANCE_FILE_BYTES: u64 = 128 * 1024;
const MAX_GUIDANCE_TOTAL_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
struct GuidanceFile {
    path: String,
    scope: String,
    depth: usize,
    content: String,
}

pub fn tracked_guidance(worktree: &Path) -> Result<String, String> {
    let worktree = std::fs::canonicalize(worktree)
        .map_err(|error| format!("Failed to canonicalize worktree: {error}"))?;
    let repository = git2::Repository::open(&worktree)
        .map_err(|error| format!("Failed to open worktree Git repository: {error}"))?;
    let index = repository
        .index()
        .map_err(|error| format!("Failed to read Git index: {error}"))?;
    let mut files = Vec::new();
    let mut total = 0_usize;

    for entry in index.iter() {
        let relative = std::str::from_utf8(&entry.path)
            .map_err(|_| "Git index contains a non-UTF-8 path".to_string())?;
        let relative_path = Path::new(relative);
        if relative_path.file_name().and_then(|name| name.to_str()) != Some("CLAUDE.md") {
            continue;
        }
        if entry.mode == 0o120_000 {
            return Err(format!("Tracked guidance `{relative}` is a symlink"));
        }
        let joined = worktree.join(relative_path);
        reject_symlink_components(&worktree, relative_path, relative)?;
        let mut file = std::fs::File::open(&joined)
            .map_err(|error| format!("Failed to open tracked guidance `{relative}`: {error}"))?;
        reject_symlink_components(&worktree, relative_path, relative)?;
        let canonical = std::fs::canonicalize(&joined)
            .map_err(|error| format!("Failed to resolve tracked guidance `{relative}`: {error}"))?;
        if !canonical.starts_with(&worktree) {
            return Err(format!(
                "Tracked guidance `{relative}` resolves outside the worktree"
            ));
        }
        let metadata = std::fs::metadata(&canonical)
            .map_err(|error| format!("Failed to inspect tracked guidance `{relative}`: {error}"))?;
        let opened_metadata = file.metadata().map_err(|error| {
            format!("Failed to inspect opened tracked guidance `{relative}`: {error}")
        })?;
        if metadata.dev() != opened_metadata.dev() || metadata.ino() != opened_metadata.ino() {
            return Err(format!(
                "Tracked guidance `{relative}` changed while it was being validated"
            ));
        }
        if !metadata.is_file() {
            return Err(format!(
                "Tracked guidance `{relative}` is not a regular file"
            ));
        }
        if metadata.len() > MAX_GUIDANCE_FILE_BYTES {
            return Err(format!("Tracked guidance `{relative}` exceeds 128 KiB"));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut bytes)
            .map_err(|error| format!("Failed to read tracked guidance `{relative}`: {error}"))?;
        let content = String::from_utf8(bytes)
            .map_err(|_| format!("Tracked guidance `{relative}` is not valid UTF-8"))?;
        total = total
            .checked_add(content.len())
            .ok_or_else(|| "Tracked guidance aggregate size overflowed".to_string())?;
        if total > MAX_GUIDANCE_TOTAL_BYTES {
            return Err("Tracked guidance exceeds the 1 MiB aggregate limit".to_string());
        }
        let scope_path = if relative_path.parent().and_then(Path::file_name)
            == Some(std::ffi::OsStr::new(".claude"))
        {
            relative_path
                .parent()
                .and_then(Path::parent)
                .unwrap_or_else(|| Path::new(""))
        } else {
            relative_path.parent().unwrap_or_else(|| Path::new(""))
        };
        let scope = if scope_path.as_os_str().is_empty() {
            ".".to_string()
        } else {
            scope_path.to_string_lossy().replace('\\', "/")
        };
        files.push(GuidanceFile {
            path: relative.replace('\\', "/"),
            depth: scope_path.components().count(),
            scope,
            content,
        });
    }

    files.sort_by(|left, right| {
        left.depth
            .cmp(&right.depth)
            .then_with(|| left.path.cmp(&right.path))
    });
    if files.is_empty() {
        return Ok(String::new());
    }
    let mut output = String::from(
        "# Trusted tracked repository guidance\nApply each block only within its labeled subtree; narrower scopes override broader scopes only inside that subtree.\n",
    );
    for file in files {
        let _ = write!(
            output,
            "\n## Guidance `{}` (scope: `{}`)\n{}\n",
            file.path, file.scope, file.content
        );
    }
    Ok(output)
}

fn reject_symlink_components(
    worktree: &Path,
    relative: &Path,
    display: &str,
) -> Result<(), String> {
    let mut current = worktree.to_path_buf();
    for component in relative.components() {
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current)
            .map_err(|error| format!("Failed to inspect tracked guidance `{display}`: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Tracked guidance `{display}` contains a symlink at {}",
                current.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "tests should fail immediately on fixture and assertion errors"
)]
mod tests {
    use super::*;

    fn repository() -> (tempfile::TempDir, git2::Repository) {
        let temp = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(temp.path()).unwrap();
        (temp, repo)
    }

    fn track(repo: &git2::Repository, relative: &str, bytes: &[u8]) {
        let root = repo.workdir().unwrap();
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new(relative)).unwrap();
        index.write().unwrap();
    }

    #[test]
    fn includes_only_tracked_guidance_in_deterministic_scope_order() {
        let (temp, repo) = repository();
        track(&repo, "nested/CLAUDE.md", b"nested");
        track(&repo, "CLAUDE.md", b"root");
        track(&repo, "nested/.claude/CLAUDE.md", b"nested-dot");
        std::fs::create_dir_all(temp.path().join("untracked")).unwrap();
        std::fs::write(temp.path().join("untracked/CLAUDE.md"), "untrusted").unwrap();

        let guidance = tracked_guidance(temp.path()).unwrap();
        assert!(guidance.find("`CLAUDE.md`").unwrap() < guidance.find("nested/CLAUDE.md").unwrap());
        assert!(!guidance.contains("untrusted"));
        assert!(guidance.contains("scope: `nested`"));
    }

    #[test]
    fn rejects_invalid_utf8_and_per_file_limit() {
        let (temp, repo) = repository();
        track(&repo, "CLAUDE.md", &[0xFF]);
        assert!(tracked_guidance(temp.path()).unwrap_err().contains("UTF-8"));

        track(&repo, "CLAUDE.md", &vec![b'x'; 128 * 1024 + 1]);
        assert!(
            tracked_guidance(temp.path())
                .unwrap_err()
                .contains("128 KiB")
        );
    }

    #[test]
    fn empty_bundle_is_valid_and_aggregate_limit_is_enforced() {
        let (empty, _) = repository();
        assert_eq!(tracked_guidance(empty.path()).unwrap(), "");

        let (temp, repo) = repository();
        for index in 0..9 {
            track(
                &repo,
                &format!("scope-{index}/CLAUDE.md"),
                &vec![b'x'; 128 * 1024],
            );
        }
        assert!(
            tracked_guidance(temp.path())
                .unwrap_err()
                .contains("aggregate")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_tracked_symlink_guidance() {
        use std::os::unix::fs::symlink;

        let (temp, repo) = repository();
        let outside = tempfile::NamedTempFile::new().unwrap();
        symlink(outside.path(), temp.path().join("CLAUDE.md")).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("CLAUDE.md")).unwrap();
        index.write().unwrap();
        assert!(
            tracked_guidance(temp.path())
                .unwrap_err()
                .contains("symlink")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_tracked_regular_file_replaced_by_final_symlink() {
        use std::os::unix::fs::symlink;

        let (temp, repo) = repository();
        track(&repo, "CLAUDE.md", b"tracked regular file");
        std::fs::remove_file(temp.path().join("CLAUDE.md")).unwrap();
        let replacement = temp.path().join("replacement.md");
        std::fs::write(&replacement, "replacement").unwrap();
        symlink(&replacement, temp.path().join("CLAUDE.md")).unwrap();

        assert!(
            tracked_guidance(temp.path())
                .unwrap_err()
                .contains("symlink")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_guidance_through_intermediate_symlink_inside_worktree() {
        use std::os::unix::fs::symlink;

        let (temp, repo) = repository();
        track(&repo, "nested/CLAUDE.md", b"tracked");
        std::fs::rename(temp.path().join("nested"), temp.path().join("actual")).unwrap();
        symlink("actual", temp.path().join("nested")).unwrap();

        assert!(
            tracked_guidance(temp.path())
                .unwrap_err()
                .contains("symlink")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_guidance_escaping_through_symlinked_directory() {
        use std::os::unix::fs::symlink;

        let (temp, repo) = repository();
        track(&repo, "nested/CLAUDE.md", b"tracked");
        std::fs::remove_dir_all(temp.path().join("nested")).unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        std::fs::write(outside.path().join("CLAUDE.md"), "outside").unwrap();
        symlink(outside.path(), temp.path().join("nested")).unwrap();

        assert!(
            tracked_guidance(temp.path())
                .unwrap_err()
                .contains("symlink")
        );
    }
}
