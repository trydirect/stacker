//! Regression for BUGS.md #50: the readiness result decides a local deploy's
//! headline.
//!
//! `docker compose up` accepting the files is not a deployed stack. A deploy
//! whose containers never became ready is a failed deploy: the verdict comes
//! from the readiness check, the next line says how to see why (`stacker
//! logs`), and there is no "deploy to production" upsell after a failure.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

/// A `docker` that reports a healthy daemon and accepts `compose up`, while
/// `compose ps` reports containers in the state named by `FAKE_DOCKER_STATE`.
fn fake_docker(root: &TempDir) -> PathBuf {
    let bin = root.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let script = bin.join("docker");
    fs::write(
        &script,
        r#"#!/bin/sh
case "$*" in
  *" ps "*|*" ps")
    if [ "$FAKE_DOCKER_STATE" = "running" ]; then
      echo '[{"State":"running","Name":"app","Service":"app"}]'
    else
      echo '[{"State":"exited","Name":"app","Service":"app"}]'
    fi
    ;;
  *)
    echo fake
    ;;
esac
exit 0
"#,
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn path_with(bin: &Path) -> String {
    let old = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}", bin.display(), old)
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn setup_project(dir: &TempDir) {
    let config = format!(
        r#"
name: readiness-app
version: "1.0"
app:
  type: static
  path: "."
  ports:
    - "{}:80"
deploy:
  target: local
"#,
        free_port()
    );
    fs::write(dir.path().join("stacker.yml"), config).unwrap();
    fs::write(dir.path().join("index.html"), "<h1>Hello</h1>").unwrap();
}

#[test]
fn a_local_deploy_whose_containers_never_become_ready_is_a_failed_deploy() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);
    let bin = fake_docker(&dir);

    stacker_cmd()
        .current_dir(dir.path())
        .env("PATH", path_with(&bin))
        .env("FAKE_DOCKER_STATE", "exited")
        .env("STACKER_LOCAL_WATCH_TIMEOUT_SECS", "2")
        .args(["deploy", "--target", "local"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Local deployment failed"))
        .stderr(predicate::str::contains("stacker logs"))
        .stderr(predicate::str::contains("Deploy to production").not())
        .stderr(predicate::str::contains("Create a free account").not())
        .stderr(predicate::str::contains("stacker status").not());
}

#[test]
fn a_local_deploy_whose_containers_run_still_reports_success() {
    let dir = TempDir::new().unwrap();
    setup_project(&dir);
    let bin = fake_docker(&dir);

    stacker_cmd()
        .current_dir(dir.path())
        .env("PATH", path_with(&bin))
        .env("FAKE_DOCKER_STATE", "running")
        .env("STACKER_LOCAL_WATCH_TIMEOUT_SECS", "30")
        .args(["deploy", "--target", "local"])
        .assert()
        .success()
        .stderr(predicate::str::contains("containers running"))
        .stderr(predicate::str::contains("Local deployment started"));
}
