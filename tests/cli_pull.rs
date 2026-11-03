use assert_cmd::Command;
use predicates::prelude::*;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_pull_help() {
    stacker_cmd()
        .args(["pull", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("PROJECT_REF")
                .and(predicate::str::contains("--force"))
                .and(predicate::str::contains("--dir"))
                .and(predicate::str::contains("--json")),
        );
}

#[test]
fn test_pull_requires_project_ref() {
    stacker_cmd().args(["pull"]).assert().failure();
}

#[test]
fn test_pull_shows_in_main_help() {
    stacker_cmd()
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pull"));
}
