//! Integration tests for the `stacker logs` / `stacker agent logs` interface.
//!
//! BUGS.md #40: the two commands disagreed on their own interface — `logs`
//! took `--service` + `--tail` with no positional app, `agent logs` took a
//! positional `<APP>` and rejected `--lines`/`--tail`, and the documented form
//! `stacker agent logs my-app --lines 200` did not exist. Both forms must parse.

use assert_cmd::Command;
use predicates::prelude::*;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_agent_logs_accepts_documented_lines_form() {
    stacker_cmd()
        .args(["agent", "logs", "my-app", "--lines", "200", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--lines"));
}

#[test]
fn test_agent_logs_accepts_tail_alias() {
    stacker_cmd()
        .args(["agent", "logs", "my-app", "--tail", "25", "--help"])
        .assert()
        .success();
}

#[test]
fn test_logs_accepts_positional_app_and_lines() {
    stacker_cmd()
        .args(["logs", "n8n", "--lines", "25", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--lines"));
}

#[test]
fn test_logs_still_accepts_service_and_tail() {
    stacker_cmd()
        .args(["logs", "--service", "n8n", "--tail", "25", "--help"])
        .assert()
        .success();
}

#[test]
fn test_logs_rejects_positional_app_together_with_service() {
    stacker_cmd()
        .args(["logs", "n8n", "--service", "other"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}
