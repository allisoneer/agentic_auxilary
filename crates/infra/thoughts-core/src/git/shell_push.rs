use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

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

pub fn push_current_branch(repo_path: &Path, remote: &str, branch: &str) -> Result<()> {
    which::which("git").context("git executable not found in PATH")?;

    let mut cmd = build_push_command(repo_path, remote, branch);
    let mut child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn git push")?;

    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(l) => print_progress_line(&l),
                Err(_) => break,
            }
        }
    }

    let status = child.wait().context("Failed to wait for git push")?;
    if !status.success() {
        bail!("git push failed with exit code {:?}", status.code());
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
}
