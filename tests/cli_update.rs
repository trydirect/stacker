//! Integration tests for `stacker update`.

use assert_cmd::Command;
use predicates::prelude::*;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_update_default_channel() {
    stacker_cmd()
        .arg("update")
        .assert()
        .success()
        .stderr(predicate::str::contains("stable"));
}

#[test]
fn test_update_beta_channel() {
    stacker_cmd()
        .args(["update", "--channel", "beta"])
        .assert()
        .success()
        .stderr(predicate::str::contains("beta"));
}

#[test]
fn test_update_invalid_channel_fails() {
    stacker_cmd()
        .args(["update", "--channel", "nightly"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown channel").or(predicate::str::contains("nightly")));
}

#[test]
fn test_update_help() {
    stacker_cmd()
        .args(["update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--channel"));
}
