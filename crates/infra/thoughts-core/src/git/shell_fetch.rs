use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::{Command, Stdio};

/// Build a git fetch command for the given repo and remote
pub fn build_fetch_command(repo_path: &Path, remote: &str) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_path).arg("fetch").arg(remote);
    cmd
}

/// Fetch from remote using system git (uses system SSH, triggers 1Password prompts)
pub fn fetch(repo_path: &Path, remote: &str) -> Result<()> {
    which::which("git").context("git executable not found in PATH")?;

    let mut cmd = build_fetch_command(repo_path, remote);
    let status = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| {
            format!(
                "Failed to spawn git fetch for remote '{}' in '{}'",
                remote,
                repo_path.display()
            )
        })?;

    if !status.success() {
        bail!(
            "git fetch failed for remote '{}' in '{}' with exit code {:?}",
            remote,
            repo_path.display(),
            status.code()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn build_fetch_cmd_has_expected_args() {
        let cmd = build_fetch_command(Path::new("/tmp/repo"), "origin");
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, vec!["fetch", "origin"]);
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/tmp/repo")));
    }
}
