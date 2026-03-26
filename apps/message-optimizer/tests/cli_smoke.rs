use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

#[test]
fn help_shows_flags() {
    cargo_bin_cmd!("message-optimizer")
        .arg("--help")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("--supplemental-context")
                .and(predicate::str::contains("--json"))
                .and(predicate::str::contains("--pretty")),
        );
}

#[test]
fn pretty_requires_json_flag() {
    cargo_bin_cmd!("message-optimizer")
        .args(["--pretty", "hello"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--json"));
}
