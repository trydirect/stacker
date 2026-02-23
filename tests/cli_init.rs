//! Integration tests for `stacker init`.
//!
//! Uses `assert_cmd` to invoke the stacker-cli binary.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_init_creates_stacker_yml() {
    let dir = TempDir::new().unwrap();
    // Create an index.html so detector picks up "static"
    fs::write(dir.path().join("index.html"), "<h1>Hello</h1>").unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();

    assert!(dir.path().join("stacker.yml").exists());
    let content = fs::read_to_string(dir.path().join("stacker.yml")).unwrap();
    assert!(content.contains("name:"));
    assert!(content.contains("app:"));
}

#[test]
fn test_init_with_app_type_flag() {
    let dir = TempDir::new().unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["init", "--app-type", "node"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join("stacker.yml")).unwrap();
    assert!(content.contains("node"));
}

#[test]
fn test_init_with_proxy_flag() {
    let dir = TempDir::new().unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["init", "--with-proxy"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join("stacker.yml")).unwrap();
    assert!(content.contains("proxy"));
}

#[test]
fn test_init_with_ai_flag() {
    let dir = TempDir::new().unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["init", "--with-ai"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join("stacker.yml")).unwrap();
    assert!(content.contains("ai"));
}

#[test]
fn test_init_refuses_overwrite() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("stacker.yml"), "existing").unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn test_init_detects_static_project() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("index.html"), "<h1>Test</h1>").unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join("stacker.yml")).unwrap();
    assert!(content.contains("static"));
}

#[test]
fn test_init_detects_node_project() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test", "version": "1.0.0"}"#,
    )
    .unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join("stacker.yml")).unwrap();
    assert!(content.contains("node"));
}

#[test]
fn test_init_detects_python_project() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("requirements.txt"), "flask\n").unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join("stacker.yml")).unwrap();
    assert!(content.contains("python"));
}
