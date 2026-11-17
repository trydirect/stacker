//! Integration tests for the deployment-resolution surface (BUGS.md #55).
//!
//! `stacker deployment use` pins the deployment this directory works with,
//! and `explain topology` accepts the same hash explicitly.

use assert_cmd::Command;
use predicates::prelude::*;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_deployment_use_help_shows_hash_and_clear() {
    stacker_cmd()
        .args(["deployment", "use", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("deploy.deployment_hash"))
        .stdout(predicate::str::contains("--clear"));
}

#[test]
fn test_deployment_use_rejects_hash_together_with_clear() {
    stacker_cmd()
        .args(["deployment", "use", "deployment_1545", "--clear"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

#[test]
fn test_deployment_use_requires_a_hash() {
    stacker_cmd()
        .args(["deployment", "use"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_explain_topology_help_shows_deployment() {
    stacker_cmd()
        .args(["explain", "topology", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--deployment"));
}
