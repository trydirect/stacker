//! Integration tests for `stacker logs`.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_logs_no_deployment_returns_error() {
    let dir = TempDir::new().unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .arg("logs")
        .assert()
        .failure()
        .stderr(predicate::str::contains("No deployment found").or(predicate::str::contains("docker-compose")));
}

#[test]
fn test_logs_help_shows_options() {
    stacker_cmd()
        .args(["logs", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--follow"))
        .stdout(predicate::str::contains("--service"))
        .stdout(predicate::str::contains("--tail"))
        .stdout(predicate::str::contains("--since"));
}
