//! The `--new-app` surface of `stacker agent deploy-app` (BUGS.md #57).
//!
//! Adding a service with a name the deployment does not know yet is a real
//! use; reaching that path by guessing a name after an unhelpful error is
//! what cost a user their running app. The flag is how the caller says which
//! of the two this is, so it has to stay wired and documented.

use assert_cmd::Command;
use predicates::prelude::*;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_deploy_app_help_documents_new_app() {
    stacker_cmd()
        .args(["agent", "deploy-app", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--new-app"))
        .stdout(predicate::str::contains("does not have yet"));
}

#[test]
fn test_deploy_app_accepts_new_app_flag() {
    // Parsing must accept the flag; the command then fails on credentials or
    // connectivity, never on the flag itself.
    let output = stacker_cmd()
        .args([
            "agent",
            "deploy-app",
            "redis",
            "--image",
            "redis:7",
            "--new-app",
        ])
        .output()
        .expect("command runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("unexpected argument"),
        "--new-app must parse: {stderr}"
    );
}
