mod support;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[ignore = "integration test - run with: just test-integration"]
#[test]
fn sync_returns_non_zero_when_any_mount_fails() {
    let td = TempDir::new().unwrap();
    support::git_ok(td.path(), &["init"]);

    let config_dir = td.path().join(".thoughts");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("config.json"),
        r#"{
  "version": "2.0",
  "mount_dirs": {},
  "thoughts_mount": {
    "remote": "https://example.invalid/not-cloned-thoughts.git",
    "sync": "auto"
  },
  "context_mounts": [],
  "references": []
}"#,
    )
    .unwrap();

    cargo_bin_cmd!("thoughts")
        .current_dir(td.path())
        .args(["sync", "--all"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "One or more mounts failed to sync",
        ));
}
