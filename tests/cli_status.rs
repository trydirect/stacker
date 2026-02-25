//! Integration tests for `stacker status`.

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_status_no_deployment_returns_error() {
    let dir = TempDir::new().unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .arg("status")
        .assert()
        .failure()
        .stderr(predicate::str::contains("No deployment").or(predicate::str::contains("docker-compose")));
}

#[test]
fn test_status_help_shows_json_flag() {
    stacker_cmd()
        .args(["status", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--json"));
}
