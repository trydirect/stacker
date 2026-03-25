//! Integration tests for `stacker deploy`.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

fn setup_project(dir: &TempDir) {
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
    fs::write(dir.path().join("index.html"), "<h1>Hello</h1>").unwrap();
}

#[test]
fn test_deploy_dry_run_generates_artifacts() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    stacker_cmd()
        .current_dir(dir.path())
        .args(["deploy", "--target", "local", "--dry-run"])
        .assert()
        .success();

    // Dry-run should generate the Dockerfile and compose file
    assert!(dir.path().join(".stacker").exists());
    assert!(dir.path().join(".stacker/Dockerfile").exists());
    assert!(dir.path().join(".stacker/docker-compose.yml").exists());
}

#[test]
fn test_deploy_dry_run_dockerfile_content() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    stacker_cmd()
        .current_dir(dir.path())
        .args(["deploy", "--target", "local", "--dry-run"])
        .assert()
        .success();

    let dockerfile = fs::read_to_string(dir.path().join(".stacker/Dockerfile")).unwrap();
    assert!(dockerfile.contains("FROM"));
}

#[test]
fn test_deploy_dry_run_compose_content() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    stacker_cmd()
        .current_dir(dir.path())
        .args(["deploy", "--target", "local", "--dry-run"])
        .assert()
        .success();

    let compose = fs::read_to_string(dir.path().join(".stacker/docker-compose.yml")).unwrap();
    assert!(compose.contains("services"));
}

#[test]
fn test_deploy_no_config_returns_error() {
    let dir = TempDir::new().unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["deploy", "--target", "local"])
        .assert()
        .failure();
}

#[test]
fn test_deploy_custom_file_flag() {
    let dir = TempDir::new().unwrap();
    let config = r#"
name: custom-app
version: "1.0"
app:
  type: static
  path: "."
deploy:
  target: local
"#;
    fs::write(dir.path().join("custom.yml"), config).unwrap();
    fs::write(dir.path().join("index.html"), "<h1>Hi</h1>").unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["deploy", "--target", "local", "--file", "custom.yml", "--dry-run"])
        .assert()
        .success();
}

#[test]
fn test_deploy_cloud_without_credentials_fails() {
    let dir = TempDir::new().unwrap();
    let config = r#"
name: cloud-app
version: "1.0"
app:
  type: static
  path: "."
deploy:
  target: cloud
  cloud:
    provider: hetzner
    region: fsn1
    size: cx21
    ssh_key: ~/.ssh/id_ed25519
"#;
    fs::write(dir.path().join("stacker.yml"), config).unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["deploy", "--target", "cloud"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("login").or(predicate::str::contains("credential").or(predicate::str::contains("Login"))));
}

#[test]
fn test_deploy_invalid_target_fails() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);

    stacker_cmd()
        .current_dir(dir.path())
        .args(["deploy", "--target", "mars"])
        .assert()
        .failure();
}
