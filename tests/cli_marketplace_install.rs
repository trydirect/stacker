//! Integration tests for `stacker install <template>`.
//!
//! These test the guard that prevents `stacker install` from running
//! inside an already-deployed project directory.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

fn write_stacker_yml(dir: &TempDir, deploy_target: &str) {
    let config = format!(
        r#"
name: test-project
version: "1.0"
app:
  type: static
  path: "."
deploy:
  target: {}
"#,
        deploy_target
    );
    fs::write(dir.path().join("stacker.yml"), config).unwrap();
}

fn write_cloud_lock(dir: &TempDir) {
    let stacker_dir = dir.path().join(".stacker");
    fs::create_dir_all(&stacker_dir).unwrap();
    let lock = r#"
target: cloud
server_ip: 203.0.113.42
server_name: test-server
deployment_id: 1
project_id: 1
cloud_id: 1
project_name: test-project
stacker_email: test@example.com
deployed_at: 2026-07-01T12:00:00+00:00
"#;
    fs::write(stacker_dir.join("deployment-cloud.lock"), lock).unwrap();
}

#[test]
fn test_install_refuses_existing_deployed_project() {
    let dir = TempDir::new().unwrap();
    write_stacker_yml(&dir, "cloud");
    write_cloud_lock(&dir);

    stacker_cmd()
        .current_dir(dir.path())
        .args(["install", "some-template"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "already contains a deployed project",
        ))
        .stderr(predicate::str::contains("stacker service add"))
        .stderr(predicate::str::contains("stacker agent deploy-app"));
}

#[test]
fn test_install_refuses_existing_local_deployed_project() {
    let dir = TempDir::new().unwrap();
    write_stacker_yml(&dir, "local");
    let stacker_dir = dir.path().join(".stacker");
    fs::create_dir_all(&stacker_dir).unwrap();
    let lock = r#"
target: local
server_ip: 127.0.0.1
deployed_at: 2026-07-01T12:00:00+00:00
"#;
    fs::write(stacker_dir.join("deployment-local.lock"), lock).unwrap();

    stacker_cmd()
        .current_dir(dir.path())
        .args(["install", "some-template"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "already contains a deployed project",
        ));
}

#[test]
fn test_install_allowed_with_stacker_yml_but_no_lock() {
    let dir = TempDir::new().unwrap();
    write_stacker_yml(&dir, "cloud");

    // Should NOT hit the "already contains a deployed project" guard.
    // It will fail for a different reason (no credentials / needs login),
    // but crucially not with our guard message.
    stacker_cmd()
        .current_dir(dir.path())
        .args(["install", "some-template"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already contains a deployed project").not());
}

#[test]
fn test_install_allowed_in_empty_directory() {
    let dir = TempDir::new().unwrap();

    // No stacker.yml, no lock — should proceed past our guard
    // and fail for a different reason (needs login / no credentials).
    stacker_cmd()
        .current_dir(dir.path())
        .args(["install", "some-template"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already contains a deployed project").not());
}
