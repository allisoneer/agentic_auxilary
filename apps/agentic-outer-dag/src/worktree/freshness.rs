use anyhow::Context;
use anyhow::Result;
use git2::Repository;
use gwt_worktree::worktree::is_worktree_dirty;
use std::process::Command;
use std::process::ExitStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FreshnessOutcome {
    UpToDate,
    Rebases { old_head: String, new_head: String },
    Conflict,
    DirtyTree,
}

pub fn run(base_ref: &str, dry_run: bool) -> Result<FreshnessOutcome> {
    if is_dirty()? {
        return Ok(FreshnessOutcome::DirtyTree);
    }

    if dry_run {
        return Ok(FreshnessOutcome::UpToDate);
    }

    git(["fetch", "origin", "--prune"])?;

    if !needs_rebase(base_ref)? {
        return Ok(FreshnessOutcome::UpToDate);
    }

    let old_head = rev_parse("HEAD")?;
    let status = git_status(["rebase", base_ref])?;
    if !status.success() {
        git(["rebase", "--abort"])?;
        return Ok(FreshnessOutcome::Conflict);
    }
    let new_head = rev_parse("HEAD")?;

    Ok(FreshnessOutcome::Rebases { old_head, new_head })
}

fn is_dirty() -> Result<bool> {
    let repo =
        Repository::discover(".").context("failed to discover repository for freshness check")?;
    is_worktree_dirty(&repo).context("failed to inspect worktree dirtiness")
}

fn needs_rebase(base_ref: &str) -> Result<bool> {
    let status = git_status(["merge-base", "--is-ancestor", base_ref, "HEAD"])?;
    Ok(!status.success())
}

fn rev_parse(rev: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", rev])
        .output()
        .with_context(|| format!("failed to run git rev-parse for {rev}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "git rev-parse failed for {rev}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git<const N: usize>(args: [&str; N]) -> Result<()> {
    let status = git_status(args)?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("git command failed: git {}", args.join(" "))
    }
}

fn git_status<const N: usize>(args: [&str; N]) -> Result<ExitStatus> {
    Command::new("git")
        .args(args)
        .status()
        .with_context(|| format!("failed to run git {}", args.join(" ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use tempfile::TempDir;

    static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn returns_up_to_date_when_branch_is_current() {
        let _guard = cwd_lock().lock().unwrap();
        let fixture = GitFixture::new().unwrap();
        let saved = env::current_dir().unwrap();
        env::set_current_dir(&fixture.feature_clone).unwrap();

        let outcome = run("origin/main", false).unwrap();

        env::set_current_dir(saved).unwrap();
        assert_eq!(outcome, FreshnessOutcome::UpToDate);
    }

    #[test]
    fn rebases_when_origin_main_moves_forward() {
        let _guard = cwd_lock().lock().unwrap();
        let fixture = GitFixture::new().unwrap();
        fixture.advance_main("main update\n").unwrap();
        let saved = env::current_dir().unwrap();
        env::set_current_dir(&fixture.feature_clone).unwrap();

        let outcome = run("origin/main", false).unwrap();

        env::set_current_dir(saved).unwrap();
        match outcome {
            FreshnessOutcome::Rebases { old_head, new_head } => assert_ne!(old_head, new_head),
            other => panic!("expected rebase outcome, got {other:?}"),
        }
    }

    #[test]
    fn returns_conflict_when_rebase_hits_merge_conflict() {
        let _guard = cwd_lock().lock().unwrap();
        let fixture = GitFixture::new().unwrap();
        fixture.write_shared_file("feature change\n").unwrap();
        fixture.commit_feature("feature change").unwrap();
        fixture.advance_main("main change\n").unwrap();
        let saved = env::current_dir().unwrap();
        env::set_current_dir(&fixture.feature_clone).unwrap();

        let outcome = run("origin/main", false).unwrap();

        assert!(!fixture.rebase_in_progress().unwrap());
        let rerun_outcome = run("origin/main", false).unwrap();

        env::set_current_dir(saved).unwrap();
        assert_eq!(outcome, FreshnessOutcome::Conflict);
        assert_eq!(rerun_outcome, FreshnessOutcome::Conflict);
    }

    fn cwd_lock() -> &'static Mutex<()> {
        CWD_LOCK.get_or_init(|| Mutex::new(()))
    }

    struct GitFixture {
        _temp: TempDir,
        main_clone: PathBuf,
        feature_clone: PathBuf,
    }

    impl GitFixture {
        fn new() -> Result<Self> {
            let temp = TempDir::new()?;
            let origin = temp.path().join("origin.git");
            let main_clone = temp.path().join("main");
            let feature_clone = temp.path().join("feature");

            run_git(temp.path(), ["init", "--bare", origin.to_str().unwrap()])?;
            run_git(&origin, ["symbolic-ref", "HEAD", "refs/heads/main"])?;
            run_git(
                temp.path(),
                [
                    "clone",
                    origin.to_str().unwrap(),
                    main_clone.to_str().unwrap(),
                ],
            )?;
            configure_repo(&main_clone)?;
            run_git(&main_clone, ["checkout", "-b", "main"])?;
            fs::write(main_clone.join("shared.txt"), "base\n")?;
            run_git(&main_clone, ["add", "shared.txt"])?;
            run_git(&main_clone, ["commit", "-m", "initial"])?;
            run_git(&main_clone, ["push", "-u", "origin", "main"])?;

            run_git(
                temp.path(),
                [
                    "clone",
                    origin.to_str().unwrap(),
                    feature_clone.to_str().unwrap(),
                ],
            )?;
            configure_repo(&feature_clone)?;
            run_git(&feature_clone, ["checkout", "-b", "feature/test"])?;

            Ok(Self {
                _temp: temp,
                main_clone,
                feature_clone,
            })
        }

        fn advance_main(&self, contents: &str) -> Result<()> {
            fs::write(self.main_clone.join("shared.txt"), contents)?;
            run_git(&self.main_clone, ["add", "shared.txt"])?;
            run_git(&self.main_clone, ["commit", "-m", "main update"])?;
            run_git(&self.main_clone, ["push", "origin", "main"])?;
            Ok(())
        }

        fn write_shared_file(&self, contents: &str) -> Result<()> {
            fs::write(self.feature_clone.join("shared.txt"), contents)?;
            Ok(())
        }

        fn commit_feature(&self, message: &str) -> Result<()> {
            run_git(&self.feature_clone, ["add", "shared.txt"])?;
            run_git(&self.feature_clone, ["commit", "-m", message])?;
            Ok(())
        }

        fn rebase_in_progress(&self) -> Result<bool> {
            let git_dir = git_output(&self.feature_clone, ["rev-parse", "--git-dir"])?;
            let git_dir = self.feature_clone.join(git_dir);
            Ok(git_dir.join("rebase-apply").exists() || git_dir.join("rebase-merge").exists())
        }
    }

    fn configure_repo(path: &Path) -> Result<()> {
        run_git(path, ["config", "user.name", "Test User"])?;
        run_git(path, ["config", "user.email", "test@example.com"])?;
        Ok(())
    }

    fn run_git<const N: usize>(cwd: &Path, args: [&str; N]) -> Result<()> {
        let output = Command::new("git").current_dir(cwd).args(args).output()?;
        if output.status.success() {
            Ok(())
        } else {
            anyhow::bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )
        }
    }

    fn git_output<const N: usize>(cwd: &Path, args: [&str; N]) -> Result<String> {
        let output = Command::new("git").current_dir(cwd).args(args).output()?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            anyhow::bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )
        }
    }
}
