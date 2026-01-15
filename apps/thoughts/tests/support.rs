use std::path::Path;
use std::process::Output;

/// Run a git command and assert it succeeds. On failure, print stdout/stderr for diagnostics.
pub fn git_ok(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to spawn git");
    assert!(
        out.status.success(),
        "git {:?} in {} failed (status: {}):\nstdout:\n{}\nstderr:\n{}",
        args,
        dir.display(),
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}

/// Like git_ok, but returns the Output for callers that need stdout/stderr.
pub fn git_ok_out(dir: &Path, args: &[&str]) -> Output {
    let out = std::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to spawn git");
    assert!(
        out.status.success(),
        "git {:?} in {} failed (status: {}):\nstdout:\n{}\nstderr:\n{}",
        args,
        dir.display(),
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    out
}

/// Convenience: return stdout (trimmed) from a successful git invocation.
pub fn git_stdout(dir: &Path, args: &[&str]) -> String {
    let out = git_ok_out(dir, args);
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises all helper functions to verify they work correctly.
    #[test]
    fn helpers_work_with_git_version() {
        let tmp = tempfile::TempDir::new().unwrap();
        // git_ok: run a simple command
        git_ok(tmp.path(), &["init"]);
        // git_ok_out: run and get output
        let out = git_ok_out(tmp.path(), &["--version"]);
        assert!(out.status.success());
        // git_stdout: get trimmed stdout
        let version = git_stdout(tmp.path(), &["--version"]);
        assert!(version.starts_with("git version"));
    }
}
