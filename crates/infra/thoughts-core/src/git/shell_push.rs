use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::process::Command;
use std::process::Stdio;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushFailureKind {
    Race,
    Auth,
    Network,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushResult {
    pub success: bool,
    pub failure_kind: Option<PushFailureKind>,
    pub stderr: String,
}

pub fn build_push_command(repo_path: &Path, remote: &str, branch: &str) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(repo_path)
        .arg("push")
        .arg("--progress")
        .arg(remote)
        .arg(format!("HEAD:refs/heads/{branch}"));
    cmd
}

fn print_progress_line(line: &str) {
    if line.starts_with("To ")
        || line.starts_with("Everything up-to-date")
        || line.contains('%')
        || line.starts_with("remote:")
        || line.contains("Counting objects")
    {
        println!("    {}", line);
    }
}

fn classify_push_failure(stderr: &str) -> PushFailureKind {
    let stderr = stderr.to_ascii_lowercase();

    if stderr.contains("[rejected]")
        || stderr.contains("non-fast-forward")
        || stderr.contains("fetch first")
        || stderr.contains("failed to push some refs")
    {
        return PushFailureKind::Race;
    }

    if stderr.contains("authentication failed")
        || stderr.contains("permission denied")
        || stderr.contains("could not read from remote repository")
        || stderr.contains("repository not found")
    {
        return PushFailureKind::Auth;
    }

    if stderr.contains("could not resolve host")
        || stderr.contains("temporary failure in name resolution")
        || stderr.contains("connection timed out")
        || stderr.contains("operation timed out")
        || stderr.contains("network is unreachable")
        || stderr.contains("no route to host")
        || stderr.contains("connection refused")
        || stderr.contains("connection reset")
    {
        return PushFailureKind::Network;
    }

    PushFailureKind::Other
}

pub fn push_current_branch_with_result(
    repo_path: &Path,
    remote: &str,
    branch: &str,
) -> Result<PushResult> {
    which::which("git").context("git executable not found in PATH")?;

    let mut cmd = build_push_command(repo_path, remote, branch);
    let mut child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn git push")?;

    let mut stderr_lines = Vec::new();
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    print_progress_line(&line);
                    stderr_lines.push(line);
                }
                Err(_) => break,
            }
        }
    }

    let status = child.wait().context("Failed to wait for git push")?;
    let stderr = stderr_lines.join("\n");

    Ok(PushResult {
        success: status.success(),
        failure_kind: (!status.success()).then(|| classify_push_failure(&stderr)),
        stderr,
    })
}

pub fn push_current_branch(repo_path: &Path, remote: &str, branch: &str) -> Result<()> {
    let result = push_current_branch_with_result(repo_path, remote, branch)?;
    if !result.success {
        let kind = result.failure_kind.unwrap_or(PushFailureKind::Other);
        let stderr = result.stderr.trim();
        if stderr.is_empty() {
            bail!("git push failed ({kind:?})");
        }
        bail!("git push failed ({kind:?}): {stderr}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_push_cmd_has_expected_args() {
        let cmd = build_push_command(Path::new("/tmp/repo"), "origin", "main");
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            args,
            vec!["push", "--progress", "origin", "HEAD:refs/heads/main"]
        );
        assert_eq!(cmd.get_current_dir(), Some(Path::new("/tmp/repo")));
    }

    #[test]
    fn classify_rejected_push_as_race() {
        let stderr =
            "! [rejected]        HEAD -> main (fetch first)\nerror: failed to push some refs";
        assert_eq!(classify_push_failure(stderr), PushFailureKind::Race);
    }

    #[test]
    fn classify_auth_push_failure() {
        let stderr = "remote: Permission denied\nfatal: Could not read from remote repository.";
        assert_eq!(classify_push_failure(stderr), PushFailureKind::Auth);
    }

    #[test]
    fn classify_network_push_failure() {
        let stderr = "fatal: unable to access 'https://example.com/repo.git/': Could not resolve host: example.com";
        assert_eq!(classify_push_failure(stderr), PushFailureKind::Network);
    }

    #[test]
    fn classify_unknown_push_failure_as_other() {
        let stderr = "fatal: unexpected server failure";
        assert_eq!(classify_push_failure(stderr), PushFailureKind::Other);
    }
}
