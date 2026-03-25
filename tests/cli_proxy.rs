//! Integration tests for `stacker proxy`.

use assert_cmd::Command;
use predicates::prelude::*;

fn stacker_cmd() -> Command {
    Command::cargo_bin("stacker-cli").expect("stacker-cli binary not found")
}

#[test]
fn test_proxy_add_generates_nginx_block() {
    stacker_cmd()
        .args(["proxy", "add", "example.com", "--upstream", "http://app:3000"])
        .assert()
        .success()
        .stdout(predicate::str::contains("server_name").or(predicate::str::contains("example.com")));
}

#[test]
fn test_proxy_add_with_ssl() {
    stacker_cmd()
        .args([
            "proxy", "add", "secure.example.com",
            "--upstream", "http://app:3000",
            "--ssl", "auto",
        ])
        .assert()
        .success();
}

#[test]
fn test_proxy_detect_help() {
    stacker_cmd()
        .args(["proxy", "detect", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Detect"));
}

#[test]
fn test_proxy_add_help_shows_options() {
    stacker_cmd()
        .args(["proxy", "add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--upstream"))
        .stdout(predicate::str::contains("--ssl"));
}
