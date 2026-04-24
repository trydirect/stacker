//! Integration tests for `stacker update`.

use assert_cmd::Command;
use mockito::{Server, ServerGuard};
use predicates::prelude::*;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

fn mock_releases_server() -> ServerGuard {
    let mut server = Server::new();
    server
        .mock("GET", "/releases")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body("[]")
        .create();
    server
}

#[test]
fn test_update_default_channel() {
    let server = mock_releases_server();
    stacker_cmd()
        .env(
            "STACKER_UPDATE_RELEASES_URL",
            format!("{}/releases", server.url()),
        )
        .arg("update")
        .assert()
        .success()
        .stderr(predicate::str::contains("stable"));
}

#[test]
fn test_update_beta_channel() {
    let server = mock_releases_server();
    stacker_cmd()
        .env(
            "STACKER_UPDATE_RELEASES_URL",
            format!("{}/releases", server.url()),
        )
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
        .stderr(
            predicate::str::contains("Unknown channel").or(predicate::str::contains("nightly")),
        );
}

#[test]
fn test_update_help() {
    stacker_cmd()
        .args(["update", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--channel"));
}
