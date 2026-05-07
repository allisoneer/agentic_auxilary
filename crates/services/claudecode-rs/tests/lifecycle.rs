#![expect(clippy::unwrap_used, reason = "Tests should panic on failure")]

use claudecode::Client;
use claudecode::OutputFormat;
use claudecode::SessionConfig;
use nix::unistd::Pid;
use serial_test::serial;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;
use tempfile::TempDir;
use tokio::process::Child;
use tokio::process::Command;

#[derive(Debug, serde::Deserialize)]
struct PidInfo {
    parent_pid: u32,
    child_pid: u32,
}

fn fake_claude_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake_claude"))
}

fn pid_file(temp_dir: &TempDir) -> PathBuf {
    temp_dir.path().join("fake-clause-pids.json")
}

fn config(output_format: OutputFormat, query: &str, pid_file: &Path) -> SessionConfig {
    SessionConfig::builder(query)
        .output_format(output_format)
        .env_var("FAKE_CLAUDE_PID_FILE", pid_file.display().to_string())
        .build()
        .unwrap()
}

fn spawn_fake_claude(query: &str, pid_file: &Path, force_error_after_spawn: bool) -> Child {
    let mut cmd = Command::new(fake_claude_path());
    cmd.arg("--output-format")
        .arg("text")
        .arg("--")
        .arg(query)
        .env("FAKE_CLAUDE_PID_FILE", pid_file.display().to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if force_error_after_spawn {
        cmd.env("FAKE_CLAUDE_FORCE_ERROR_AFTER_SPAWN", "1");
    }

    cmd.spawn().unwrap()
}

async fn wait_for_pid_info(pid_file: &PathBuf) -> PidInfo {
    for _ in 0..100 {
        if let Ok(bytes) = tokio::fs::read(pid_file).await {
            return serde_json::from_slice(&bytes).unwrap();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for fake claude pid file");
}

async fn wait_for_process_exit(pid: u32) {
    for _ in 0..200 {
        let exists = match nix::sys::signal::kill(Pid::from_raw(pid.cast_signed()), None) {
            Err(nix::errno::Errno::ESRCH) => false,
            Ok(()) | Err(_) => true,
        };
        if !exists {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("process {pid} did not exit in time");
}

#[tokio::test]
#[serial]
async fn dropping_launch_and_wait_kills_fake_claude_and_child() {
    let temp_dir = TempDir::new().unwrap();
    let pid_path = pid_file(&temp_dir);
    let client = Client::with_path(fake_claude_path()).await.unwrap();
    let cfg = config(OutputFormat::Text, "drop test", &pid_path);

    let task = tokio::spawn(async move { client.launch_and_wait(cfg).await });
    let pids = wait_for_pid_info(&pid_path).await;
    task.abort();
    let _ = task.await;

    wait_for_process_exit(pids.parent_pid).await;
    wait_for_process_exit(pids.child_pid).await;
}

#[tokio::test]
#[serial]
async fn session_kill_works_after_worker_startup() {
    let temp_dir = TempDir::new().unwrap();
    let pid_path = pid_file(&temp_dir);
    let client = Client::with_path(fake_claude_path()).await.unwrap();
    let cfg = config(OutputFormat::Text, "kill test", &pid_path);

    let mut session = client.launch(cfg).await.unwrap();
    let pids = wait_for_pid_info(&pid_path).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    session.kill().await.unwrap();

    wait_for_process_exit(pids.parent_pid).await;
    wait_for_process_exit(pids.child_pid).await;
}

#[tokio::test]
#[serial]
async fn fake_claude_reaps_helper_child_after_normal_exit() {
    let temp_dir = TempDir::new().unwrap();
    let pid_path = pid_file(&temp_dir);
    let child = spawn_fake_claude("normal exit test", &pid_path, false);
    let pids = wait_for_pid_info(&pid_path).await;

    nix::sys::signal::kill(
        Pid::from_raw(pids.parent_pid.cast_signed()),
        Some(nix::sys::signal::Signal::SIGTERM),
    )
    .unwrap();

    let output = child.wait_with_output().await.unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "fake text output"
    );

    wait_for_process_exit(pids.parent_pid).await;
    wait_for_process_exit(pids.child_pid).await;
}

#[tokio::test]
#[serial]
async fn fake_claude_reaps_helper_child_after_forced_error_exit() {
    let temp_dir = TempDir::new().unwrap();
    let pid_path = pid_file(&temp_dir);
    let child = spawn_fake_claude("forced error test", &pid_path, true);
    let pids = wait_for_pid_info(&pid_path).await;

    let output = child.wait_with_output().await.unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("forced fake_claude error after helper spawn")
    );

    wait_for_process_exit(pids.parent_pid).await;
    wait_for_process_exit(pids.child_pid).await;
}

#[tokio::test]
#[serial]
async fn fake_claude_runs_in_separate_process_group() {
    let temp_dir = TempDir::new().unwrap();
    let pid_path = pid_file(&temp_dir);
    let client = Client::with_path(fake_claude_path()).await.unwrap();
    let cfg = config(OutputFormat::Text, "pgid test", &pid_path);

    let session = client.launch(cfg).await.unwrap();
    let pids = wait_for_pid_info(&pid_path).await;

    let test_pgid = nix::unistd::getpgid(None).unwrap();
    let fake_pgid =
        nix::unistd::getpgid(Some(Pid::from_raw(pids.parent_pid.cast_signed()))).unwrap();
    assert_ne!(fake_pgid, test_pgid);

    session.cancel().await.unwrap();
    wait_for_process_exit(pids.parent_pid).await;
    wait_for_process_exit(pids.child_pid).await;
}

#[tokio::test]
#[serial]
async fn fake_claude_child_shares_parent_process_group() {
    let temp_dir = TempDir::new().unwrap();
    let pid_path = pid_file(&temp_dir);
    let client = Client::with_path(fake_claude_path()).await.unwrap();
    let cfg = config(OutputFormat::Text, "child pgid test", &pid_path);

    let session = client.launch(cfg).await.unwrap();
    let pids = wait_for_pid_info(&pid_path).await;

    let parent_pgid =
        nix::unistd::getpgid(Some(Pid::from_raw(pids.parent_pid.cast_signed()))).unwrap();
    let child_pgid =
        nix::unistd::getpgid(Some(Pid::from_raw(pids.child_pid.cast_signed()))).unwrap();
    assert_eq!(parent_pgid, child_pgid);

    session.cancel().await.unwrap();
    wait_for_process_exit(pids.parent_pid).await;
    wait_for_process_exit(pids.child_pid).await;
}

#[tokio::test]
#[serial]
async fn session_cancel_completes_within_bound_for_hung_process() {
    let temp_dir = TempDir::new().unwrap();
    let pid_path = pid_file(&temp_dir);
    let client = Client::with_path(fake_claude_path()).await.unwrap();
    let cfg = config(OutputFormat::Text, "[hang] cancel test", &pid_path);

    let session = client.launch(cfg).await.unwrap();
    let pids = wait_for_pid_info(&pid_path).await;

    let started = Instant::now();
    session.cancel().await.unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));

    wait_for_process_exit(pids.parent_pid).await;
    wait_for_process_exit(pids.child_pid).await;
}
