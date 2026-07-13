use anyhow::Context;
use anyhow::Result;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;

fn bin_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_agentic-outer-dag") {
        return Ok(PathBuf::from(path));
    }

    let current_exe = std::env::current_exe().context("current test binary path should resolve")?;
    current_exe
        .parent()
        .and_then(Path::parent)
        .map(|dir| dir.join("agentic-outer-dag"))
        .context("test binary should live under target/<profile>/deps")
}

#[test]
fn start_emits_progress_on_stderr_and_final_status_on_stdout() -> Result<()> {
    let fixture = GitFixture::new()?;

    let output = Command::new(bin_path()?)
        .current_dir(fixture.repo_path())
        .env("HOME", fixture.home_dir())
        .env("XDG_CONFIG_HOME", fixture.xdg_config_home())
        .env_remove("RUST_LOG")
        .args(base_start_args(&fixture))
        .output()
        .context("process should run")?;

    anyhow::ensure!(
        output.status.success(),
        "stderr was: {}",
        lossy(&output.stderr)
    );

    let stdout = lossy(&output.stdout);
    let stderr = lossy(&output.stderr);

    let _: serde_json::Value = serde_json::from_str(&stdout).context("stdout should be JSON")?;
    anyhow::ensure!(stderr.contains("outer-dag:"), "stderr was: {stderr}");
    Ok(())
}

#[test]
fn final_only_suppresses_progress_lines() -> Result<()> {
    let fixture = GitFixture::new()?;

    let output = Command::new(bin_path()?)
        .current_dir(fixture.repo_path())
        .env("HOME", fixture.home_dir())
        .env("XDG_CONFIG_HOME", fixture.xdg_config_home())
        .env_remove("RUST_LOG")
        .args(final_only_start_args(&fixture))
        .output()
        .context("process should run")?;

    anyhow::ensure!(
        output.status.success(),
        "stderr was: {}",
        lossy(&output.stderr)
    );

    let stdout = lossy(&output.stdout);
    let stderr = lossy(&output.stderr);

    let _: serde_json::Value = serde_json::from_str(&stdout).context("stdout should be JSON")?;
    anyhow::ensure!(!stderr.contains("outer-dag:"), "stderr was: {stderr}");
    Ok(())
}

fn base_start_args(fixture: &GitFixture) -> Vec<String> {
    vec![
        "--quiet".to_string(),
        "start".to_string(),
        "--ticket".to_string(),
        "ENG-992".to_string(),
        "--branch".to_string(),
        fixture.branch().to_string(),
        "--worktree".to_string(),
        fixture.repo_path().display().to_string(),
        "--stop-after".to_string(),
        "freshness_before_ticket_to_pr".to_string(),
        "--no-opencode-dispatch".to_string(),
        "--no-linear-handoff".to_string(),
        "--force".to_string(),
    ]
}

fn final_only_start_args(fixture: &GitFixture) -> Vec<String> {
    let mut args = vec!["--quiet".to_string(), "--final-only".to_string()];
    args.extend(base_start_args(fixture).into_iter().skip(1));
    args
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

struct GitFixture {
    _temp: TempDir,
    repo: PathBuf,
    branch: String,
    home: PathBuf,
    xdg_config_home: PathBuf,
}

impl GitFixture {
    fn new() -> Result<Self> {
        let temp = TempDir::new().context("tempdir should create")?;
        let origin = temp.path().join("origin.git");
        let repo = temp.path().join("repo");
        let home = temp.path().join("home");
        let xdg_config_home = home.join(".config");
        let thoughts_repo = temp.path().join("thoughts-data");
        let branch = "feature/progress-test".to_string();
        let remote = "https://github.com/example/thoughts-data.git".to_string();

        run_git(temp.path(), ["init", "--bare", path_str(&origin)?])?;
        run_git(temp.path(), ["clone", path_str(&origin)?, path_str(&repo)?])?;
        std::fs::create_dir_all(repo.join(".thoughts"))?;
        std::fs::create_dir_all(&thoughts_repo)?;
        std::fs::create_dir_all(xdg_config_home.join("agentic"))?;

        run_git(&repo, ["config", "user.name", "Test User"])?;
        run_git(&repo, ["config", "user.email", "test@example.com"])?;
        std::fs::write(repo.join("README.md"), "base\n")?;
        run_git(&repo, ["add", "README.md"])?;
        run_git(&repo, ["commit", "-m", "initial"])?;
        run_git(&repo, ["branch", "-M", "main"])?;
        run_git(&repo, ["push", "-u", "origin", "main"])?;
        run_git(&repo, ["remote", "set-url", "origin", &remote])?;
        run_git(&repo, ["checkout", "-b", &branch])?;

        std::fs::write(
            repo.join(".thoughts").join("config.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": "2.0",
                "mount_dirs": {
                    "thoughts": "thoughts",
                    "context": "context",
                    "references": "references"
                },
                "thoughts_mount": {
                    "remote": &remote,
                    "sync": "none"
                },
                "context_mounts": [],
                "references": []
            }))?,
        )?;
        std::fs::write(
            xdg_config_home.join("agentic").join("repos.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": "1.0",
                "mappings": {
                    remote: {
                        "path": thoughts_repo,
                        "auto_managed": false
                    }
                }
            }))?,
        )?;

        Ok(Self {
            _temp: temp,
            repo,
            branch,
            home,
            xdg_config_home,
        })
    }

    fn repo_path(&self) -> &Path {
        &self.repo
    }

    fn branch(&self) -> &str {
        &self.branch
    }

    fn home_dir(&self) -> &Path {
        &self.home
    }

    fn xdg_config_home(&self) -> &Path {
        &self.xdg_config_home
    }
}

fn path_str(path: &Path) -> Result<&str> {
    path.to_str()
        .context("fixture path should be valid UTF-8 for git CLI")
}

fn run_git<const N: usize>(cwd: &Path, args: [&str; N]) -> Result<()> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .context("git command should start")?;

    anyhow::ensure!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        lossy(&output.stderr)
    );
    Ok(())
}
