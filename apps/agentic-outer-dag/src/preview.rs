use crate::dag::engine::PlannedAction;
use crate::dag::engine::planned_actions_for_start;
use crate::state;
use crate::worktree;
use anyhow::Result;
use serde::Serialize;
use std::path::Path;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct DryRunStartPreview {
    pub ticket: String,
    pub branch: String,
    pub worktree: Option<String>,
    pub stage: state::StageKind,
    pub state_file: String,
    pub would_create: bool,
    pub blocked_without_force: bool,
    pub planned_actions: Vec<PlannedAction>,
}

pub fn build_dry_run_start_preview(
    ticket: &str,
    branch: Option<&str>,
    worktree_path: Option<&Path>,
    force: bool,
) -> Result<DryRunStartPreview> {
    let plan = worktree::preview_resolve(branch, worktree_path)?;
    let state_file = format!(
        "./thoughts/{}/artifacts/{}",
        plan.branch,
        state::STATE_FILENAME
    );
    let blocked_without_force = state_file_path(&plan, worktree_path)?.exists() && !force;

    Ok(DryRunStartPreview {
        ticket: ticket.to_string(),
        branch: plan.branch,
        worktree: plan.path.as_ref().map(|path| path.display().to_string()),
        stage: state::StageKind::FreshnessBeforeTicketToPr,
        state_file,
        would_create: plan.would_create,
        blocked_without_force,
        planned_actions: planned_actions_for_start(),
    })
}

fn state_file_path(
    plan: &worktree::WorktreePreview,
    worktree_path: Option<&Path>,
) -> Result<PathBuf> {
    let anchor = if let Some(path) = plan.path.as_ref() {
        path.clone()
    } else if let Some(path) = worktree_path {
        path.to_path_buf()
    } else {
        std::env::current_dir()?
    };

    Ok(anchor
        .join("thoughts")
        .join(&plan.branch)
        .join("artifacts")
        .join(state::STATE_FILENAME))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use std::env;
    use std::fs;
    use std::sync::Mutex;
    use std::sync::OnceLock;
    use tempfile::TempDir;

    static CWD_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    #[test]
    fn serializes_nullable_worktree_field_as_null() {
        let preview = DryRunStartPreview {
            ticket: "ENG-992".to_string(),
            branch: "feature/eng-992".to_string(),
            worktree: None,
            stage: state::StageKind::FreshnessBeforeTicketToPr,
            state_file: "./thoughts/feature/eng-992/artifacts/agentic-outer-dag-state.json"
                .to_string(),
            would_create: true,
            blocked_without_force: false,
            planned_actions: planned_actions_for_start(),
        };

        let json = serde_json::to_value(&preview).unwrap();

        assert!(json.get("worktree").unwrap().is_null());
        assert_eq!(
            json.get("stage").unwrap(),
            &serde_json::Value::String("freshness_before_ticket_to_pr".to_string())
        );
    }

    #[test]
    fn reports_blocked_without_force_when_state_file_exists() {
        let fixture = GitFixture::new().unwrap();
        let branch = fixture.current_branch().unwrap();
        let state_dir = fixture
            .repo
            .join("thoughts")
            .join(&branch)
            .join("artifacts");
        fs::create_dir_all(&state_dir).unwrap();
        fs::write(state_dir.join(state::STATE_FILENAME), "{}\n").unwrap();

        let preview =
            build_dry_run_start_preview("ENG-992", Some(&branch), Some(&fixture.repo), false)
                .unwrap();

        assert!(preview.blocked_without_force);
        assert!(!preview.would_create);
        assert_eq!(
            preview.state_file,
            format!("./thoughts/{branch}/artifacts/{}", state::STATE_FILENAME)
        );
    }

    #[test]
    fn reports_would_create_for_branch_without_existing_worktree() {
        let _guard = cwd_lock().lock().unwrap();
        let fixture = GitFixture::new().unwrap();
        let saved = env::current_dir().unwrap();
        env::set_current_dir(&fixture.repo).unwrap();

        let preview =
            build_dry_run_start_preview("ENG-992", Some("feature/preview-only"), None, false)
                .unwrap();

        env::set_current_dir(saved).unwrap();

        assert!(preview.would_create);
        assert_eq!(preview.branch, "feature/preview-only");
        assert_eq!(
            preview.planned_actions.first().map(|action| action.id),
            Some("worktree.resolve")
        );
    }

    fn cwd_lock() -> &'static Mutex<()> {
        CWD_LOCK.get_or_init(|| Mutex::new(()))
    }

    struct GitFixture {
        _temp: TempDir,
        repo: PathBuf,
    }

    impl GitFixture {
        fn new() -> Result<Self> {
            let temp = TempDir::new()?;
            let repo = temp.path().join("repo");

            run_git(temp.path(), ["init", repo.to_str().unwrap()])?;
            run_git(&repo, ["config", "user.name", "Test User"])?;
            run_git(&repo, ["config", "user.email", "test@example.com"])?;
            fs::write(repo.join("README.md"), "base\n")?;
            run_git(&repo, ["add", "README.md"])?;
            run_git(&repo, ["commit", "-m", "initial"])?;
            run_git(&repo, ["branch", "feature/preview-only"])?;

            Ok(Self { _temp: temp, repo })
        }

        fn current_branch(&self) -> Result<String> {
            git_output(&self.repo, ["branch", "--show-current"])
        }
    }

    fn run_git<const N: usize>(cwd: &Path, args: [&str; N]) -> Result<()> {
        let output = std::process::Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()?;
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
        let output = std::process::Command::new("git")
            .current_dir(cwd)
            .args(args)
            .output()?;
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
