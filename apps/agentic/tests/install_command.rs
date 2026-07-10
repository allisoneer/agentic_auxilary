use assert_cmd::Command;
use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use serde_json::Value;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use tempfile::TempDir;

fn agentic_cmd() -> Command {
    cargo_bin_cmd!("agentic")
}

fn init_fake_git_repo(dir: &std::path::Path) {
    assert!(std::fs::create_dir_all(dir.join(".git")).is_ok());
}

const EXPECTED_FILES: &[&str] = &[
    ".opencode/sysprompt.md",
    ".opencode/sysprompt_gpt54.md",
    ".opencode/orchestrator_sysprompt_gpt54.md",
    ".opencode/review_sysprompt_gpt54.md",
    ".opencode/command/bash.md",
    ".opencode/command/capture_pr_comments_openai.md",
    ".opencode/command/commit.md",
    ".opencode/command/create_plan_final.md",
    ".opencode/command/create_plan_init.md",
    ".opencode/command/decide_findings_openai.md",
    ".opencode/command/describe_pr.md",
    ".opencode/command/discord.md",
    ".opencode/command/frame_openai.md",
    ".opencode/command/implement_plan.md",
    ".opencode/command/linear.md",
    ".opencode/command/linear_ticket_2_pr.md",
    ".opencode/command/linear_ticket_design_brief.md",
    ".opencode/command/openai.md",
    ".opencode/command/playwright.md",
    ".opencode/command/research.md",
    ".opencode/command/resolve_pr_ci_failures.md",
    ".opencode/command/resolve_pr_comments.md",
    ".opencode/command/resume_work_openai.md",
    ".opencode/command/review.md",
    ".opencode/command/review_pr_comments.md",
    ".opencode/command/sync_with_main_and_resolve_conflicts.md",
    ".opencode/command/unwind_openai.md",
    "opencode.json",
];

#[test]
fn install_succeeds_into_git_repo_root() {
    let temp = TempDir::new().unwrap();
    init_fake_git_repo(temp.path());

    let mut cmd = agentic_cmd();
    cmd.args(["install", "--path", temp.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Installed"));

    for rel_path in EXPECTED_FILES {
        assert!(temp.path().join(rel_path).exists(), "missing {rel_path}");
    }

    assert_installed_file_refs_resolve(temp.path());
}

#[test]
fn install_fails_outside_git_repo() {
    let temp = TempDir::new().unwrap();

    let mut cmd = agentic_cmd();
    cmd.args(["install", "--path", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Not in a git repository"));
}

#[test]
fn install_from_nested_dir_targets_repo_root() {
    let temp = TempDir::new().unwrap();
    init_fake_git_repo(temp.path());

    let nested = temp.path().join("a/b/c");
    std::fs::create_dir_all(&nested).unwrap();

    let mut cmd = agentic_cmd();
    cmd.current_dir(&nested)
        .args(["install"])
        .assert()
        .success();

    assert!(temp.path().join("opencode.json").exists());
    assert!(!nested.join("opencode.json").exists());
}

#[test]
fn install_preflight_blocks_without_force_and_writes_nothing() {
    let temp = TempDir::new().unwrap();
    init_fake_git_repo(temp.path());

    std::fs::create_dir_all(temp.path().join(".opencode")).unwrap();
    std::fs::write(temp.path().join(".opencode/sysprompt.md"), "user version").unwrap();

    let mut cmd = agentic_cmd();
    cmd.args(["install", "--path", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Managed file(s) already exist"))
        .stderr(predicate::str::contains(".opencode/sysprompt.md"));

    assert!(!temp.path().join("opencode.json").exists());
    assert!(!temp.path().join(".opencode/command/bash.md").exists());
}

#[test]
fn install_force_overwrites_managed_preserves_unmanaged() {
    let temp = TempDir::new().unwrap();
    init_fake_git_repo(temp.path());

    std::fs::create_dir_all(temp.path().join(".opencode")).unwrap();

    let unmanaged = temp.path().join(".opencode/custom.md");
    std::fs::write(&unmanaged, "keep me").unwrap();

    let managed = temp.path().join(".opencode/sysprompt.md");
    std::fs::write(&managed, "old").unwrap();

    let mut cmd = agentic_cmd();
    cmd.args([
        "install",
        "--force",
        "--path",
        temp.path().to_str().unwrap(),
    ])
    .assert()
    .success();

    let new_managed = std::fs::read_to_string(&managed).unwrap();
    assert!(new_managed.contains("# Agent System Prompt"));
    assert_eq!(std::fs::read_to_string(&unmanaged).unwrap(), "keep me");
}

#[cfg(unix)]
#[test]
fn install_refuses_dangling_symlink_at_managed_destination() {
    let temp = TempDir::new().unwrap();
    init_fake_git_repo(temp.path());

    std::fs::create_dir_all(temp.path().join(".opencode")).unwrap();
    symlink(
        temp.path().join("missing-target.md"),
        temp.path().join(".opencode/sysprompt.md"),
    )
    .unwrap();

    let mut cmd = agentic_cmd();
    cmd.args(["install", "--path", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("refusing to write to symlink"))
        .stderr(predicate::str::contains(".opencode/sysprompt.md"));
}

#[cfg(unix)]
#[test]
fn install_refuses_dangling_symlink_in_parent_directory() {
    let temp = TempDir::new().unwrap();
    init_fake_git_repo(temp.path());
    let symlink_path = temp.path().join(".opencode");

    symlink(temp.path().join("missing-dir"), &symlink_path).unwrap();

    let mut cmd = agentic_cmd();
    cmd.args(["install", "--path", temp.path().to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "refusing to traverse symlink directory",
        ))
        .stderr(predicate::str::contains(
            symlink_path.to_string_lossy().as_ref(),
        ));
}

fn assert_installed_file_refs_resolve(repo_root: &std::path::Path) {
    let opencode = match std::fs::read_to_string(repo_root.join("opencode.json")) {
        Ok(contents) => contents,
        Err(error) => panic!("failed to read installed opencode.json: {error}"),
    };
    let parsed: Value = match serde_json::from_str(&opencode) {
        Ok(parsed) => parsed,
        Err(error) => panic!("failed to parse installed opencode.json: {error}"),
    };
    let mut file_refs = Vec::new();
    collect_file_refs(&parsed, &mut file_refs);

    for file_ref in file_refs {
        let normalized = file_ref.strip_prefix("./").unwrap_or(&file_ref);
        assert!(
            repo_root.join(normalized).exists(),
            "missing file ref target {normalized}"
        );
    }
}

fn collect_file_refs(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(string) => {
            if let Some(inner) = string
                .strip_prefix("{file:")
                .and_then(|value| value.strip_suffix('}'))
            {
                output.push(inner.to_string());
            }
        }
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_file_refs(value, output)),
        Value::Object(object) => object
            .values()
            .for_each(|value| collect_file_refs(value, output)),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}
