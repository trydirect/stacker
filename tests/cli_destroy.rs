//! Integration tests for `stacker destroy`.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_destroy_requires_confirmation() {
    let dir = TempDir::new().unwrap();
    let stacker_dir = dir.path().join(".stacker");
    fs::create_dir_all(&stacker_dir).unwrap();
    fs::write(stacker_dir.join("docker-compose.yml"), "version: '3.8'\n").unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .arg("destroy")
        .assert()
        .failure()
        .stderr(predicate::str::contains("confirm").or(predicate::str::contains("Destroy")));
}

#[test]
fn test_destroy_no_deployment_returns_error() {
    let dir = TempDir::new().unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["destroy", "--confirm"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("No deployment").or(predicate::str::contains("Nothing to destroy")));
}

#[test]
fn test_destroy_help_shows_options() {
    stacker_cmd()
        .args(["destroy", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--volumes"))
        .stdout(predicate::str::contains("--confirm"));
}
