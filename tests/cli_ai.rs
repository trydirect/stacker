//! Integration tests for `stacker ai`.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_ai_ask_no_config_returns_error() {
    let dir = TempDir::new().unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["ai", "ask", "How to optimize my Dockerfile?"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ConfigNotFound"));
}

#[test]
fn test_ai_ask_disabled_returns_error() {
    let dir = TempDir::new().unwrap();
    let config = r#"
name: test-app
version: "1.0"
app:
  type: static
  path: "."
deploy:
  target: local
ai:
  enabled: false
  provider: openai
"#;
    fs::write(dir.path().join("stacker.yml"), config).unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["ai", "ask", "Question?"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("AiNotConfigured"));
}

#[test]
fn test_ai_help_shows_usage() {
    stacker_cmd()
        .args(["ai", "ask", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--context"));
}
