mod support;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[test]
fn sync_returns_non_zero_when_any_mount_fails() {
    if std::env::var("THOUGHTS_INTEGRATION_TESTS").ok().as_deref() != Some("1") {
        return;
    }

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
