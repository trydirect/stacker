//! Integration tests for `stacker config`.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_config_validate_valid_returns_success() {
    let dir = TempDir::new().unwrap();
    let config = r#"
name: test-app
version: "1.0"
app:
  type: static
  path: "."
deploy:
  target: local
"#;
    fs::write(dir.path().join("stacker.yml"), config).unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["config", "validate"])
        .assert()
        .success();
}

#[test]
fn test_config_validate_missing_file_returns_error() {
    let dir = TempDir::new().unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["config", "validate"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Configuration file not found"));
}

#[test]
fn test_config_validate_custom_file() {
    let dir = TempDir::new().unwrap();
    let config = r#"
name: custom
version: "1.0"
app:
  type: node
  path: "."
deploy:
  target: local
"#;
    fs::write(dir.path().join("my-config.yml"), config).unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["config", "validate", "--file", "my-config.yml"])
        .assert()
        .success();
}

#[test]
fn test_config_show_displays_config() {
    let dir = TempDir::new().unwrap();
    let config = r#"
name: show-test
version: "1.0"
app:
  type: static
  path: "."
deploy:
  target: local
"#;
    fs::write(dir.path().join("stacker.yml"), config).unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["config", "show"])
        .assert()
        .success()
        .stdout(predicate::str::contains("show-test"));
}

#[test]
fn test_config_show_resolved_displays_paths_without_values() {
    let dir = TempDir::new().unwrap();
    let config = r#"
name: resolved-test
version: "1.0"
env_file: docker/prod/.env
env:
  S3_BUCKET: superbucket
app:
  type: static
  path: "."
deploy:
  target: server
"#;
    fs::write(dir.path().join("stacker.yml"), config).unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["config", "show", "--resolved"])
        .assert()
        .success()
        .stdout(predicate::str::contains("local_env_file: docker/prod/.env"))
        .stdout(predicate::str::contains(
            "remote_runtime_env_file: /home/trydirect/project/.env",
        ))
        .stdout(predicate::str::contains("compose_env_file: .env"))
        .stdout(predicate::str::contains(
            "config_hash: unavailable_until_deploy",
        ))
        .stdout(predicate::str::contains("runtime_env_contract_version: v1"))
        .stdout(predicate::str::contains(
            "runtime_env_contract_order: lowest_to_highest",
        ))
        .stdout(predicate::str::contains("name: base"))
        .stdout(predicate::str::contains("name: compose"))
        .stdout(predicate::str::contains("superbucket").not());
}

#[test]
fn test_config_show_missing_file_returns_error() {
    let dir = TempDir::new().unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["config", "show"])
        .assert()
        .failure();
}

#[test]
fn test_config_example_prints_full_reference() {
    let dir = TempDir::new().unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["config", "example"])
        .assert()
        .success()
        .stdout(predicate::str::contains("FULL COMMENTED REFERENCE"))
        .stdout(predicate::str::contains("monitoring:"))
        .stdout(predicate::str::contains("hooks:"))
        .stdout(predicate::str::contains("deploy:"));
}
